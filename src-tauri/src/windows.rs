use crate::{BrowserChoice, LaunchError, LaunchRequest};
use std::{collections::HashSet, io, process::Command};
use tauri::Url;
use windows::{
    core::{Interface, HSTRING},
    Foundation::Uri,
    System::{Launcher, LauncherOptions},
    Win32::{
        Foundation::HWND,
        System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
        UI::Shell::IInitializeWithWindow,
    },
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Storage::Packaging::Appx::{PackageFamilyNameFromFullName, PACKAGE_FAMILY_NAME_MAX_LENGTH},
    System::Environment::ExpandEnvironmentStringsW,
    UI::{
        Shell::{
            AssocQueryStringW, CommandLineToArgvW, ShellExecuteW, ASSOCF_INIT_FIXED_PROGID,
            ASSOCSTR_FRIENDLYAPPNAME,
        },
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};
use winreg::{
    enums::{
        HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY,
        KEY_WOW64_64KEY,
    },
    RegKey,
};

const OWN_PROG_ID: &str = "LinkSelector.Url";
const REGISTERED_APPS: &str = "Software\\RegisteredApplications";
const BROWSER_CLIENTS: &str = "Software\\Clients\\StartMenuInternet";
const PACKAGES: &str =
    r"Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
const PACKAGED_CHOICE: &str = "msix:";

pub fn browser_choices(url: &Url, _: &str) -> Vec<BrowserChoice> {
    let mut seen = HashSet::new();
    let mut choices = Vec::new();

    let classes = RegKey::predef(HKEY_CLASSES_ROOT);
    if let Ok(packages) = classes.open_subkey(PACKAGES) {
        add_packaged_choices(&packages, &classes, url.scheme(), &mut seen, &mut choices);
    }

    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let root = RegKey::predef(hive);
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            if let Ok(registered) = root.open_subkey_with_flags(REGISTERED_APPS, KEY_READ | view) {
                for (name, _) in registered.enum_values().filter_map(Result::ok) {
                    if let Ok(path) = registered.get_value::<String, _>(&name) {
                        add_choice(
                            &root,
                            view,
                            &path,
                            &name,
                            url.scheme(),
                            &mut seen,
                            &mut choices,
                        );
                    }
                }
            }
            if let Ok(clients) = root.open_subkey_with_flags(BROWSER_CLIENTS, KEY_READ | view) {
                for name in clients.enum_keys().filter_map(Result::ok) {
                    let path = format!(r"{BROWSER_CLIENTS}\{name}\Capabilities");
                    add_choice(
                        &root,
                        view,
                        &path,
                        &name,
                        url.scheme(),
                        &mut seen,
                        &mut choices,
                    );
                }
            }
        }
    }

    choices.sort_by_cached_key(|choice| choice.name.to_lowercase());
    choices
}

fn add_packaged_choices(
    packages: &RegKey,
    classes: &RegKey,
    scheme: &str,
    seen: &mut HashSet<String>,
    choices: &mut Vec<BrowserChoice>,
) {
    // Windows creates an AppX ProgID for each protocol declared in the package manifest.
    for package in packages.enum_keys().filter_map(Result::ok) {
        let Ok(applications) = packages.open_subkey(&package) else {
            continue;
        };
        let Some(family) = package_family(&package) else {
            continue;
        };
        for application in applications.enum_keys().filter_map(Result::ok) {
            let path = format!(r"{application}\Capabilities\URLAssociations");
            let Ok(associations) = applications.open_subkey(path) else {
                continue;
            };
            let Ok(id) = associations.get_value::<String, _>(scheme) else {
                continue;
            };
            let normalized = id.to_ascii_lowercase();
            if !normalized.starts_with("appx")
                || !id.bytes().all(|byte| byte.is_ascii_alphanumeric())
                || classes.open_subkey(&id).is_err()
                || !seen.insert(normalized)
            {
                continue;
            }
            let name = association_name(&id)
                .unwrap_or_else(|| package.split('_').next().unwrap_or(&package).to_owned());
            choices.push(BrowserChoice {
                id: format!("{PACKAGED_CHOICE}{id}"),
                name,
                package_family: Some(family.clone()),
            });
        }
    }
}

fn package_family(full_name: &str) -> Option<String> {
    let full_name = wide(full_name);
    let mut family = [0u16; PACKAGE_FAMILY_NAME_MAX_LENGTH as usize + 1];
    let mut length = family.len() as u32;
    let result = unsafe {
        PackageFamilyNameFromFullName(full_name.as_ptr(), &mut length, family.as_mut_ptr())
    };
    if result != 0 || length == 0 {
        return None;
    }
    String::from_utf16(&family[..length as usize - 1]).ok()
}

fn association_name(id: &str) -> Option<String> {
    let id = wide(id);
    let mut name = [0u16; 512];
    let mut length = name.len() as u32;
    let result = unsafe {
        AssocQueryStringW(
            ASSOCF_INIT_FIXED_PROGID,
            ASSOCSTR_FRIENDLYAPPNAME,
            id.as_ptr(),
            std::ptr::null(),
            name.as_mut_ptr(),
            &mut length,
        )
    };
    let end = name.iter().position(|&character| character == 0)?;
    (result == 0 && end > 0).then(|| String::from_utf16_lossy(&name[..end]))
}

fn add_choice(
    root: &RegKey,
    view: u32,
    path: &str,
    fallback_name: &str,
    scheme: &str,
    seen: &mut HashSet<String>,
    choices: &mut Vec<BrowserChoice>,
) {
    let Ok(capabilities) = root.open_subkey_with_flags(path, KEY_READ | view) else {
        return;
    };
    let Ok(associations) = capabilities.open_subkey_with_flags("URLAssociations", KEY_READ | view)
    else {
        return;
    };
    let Ok(id) = associations.get_value::<String, _>(scheme) else {
        return;
    };
    if id.is_empty()
        || id.eq_ignore_ascii_case(OWN_PROG_ID)
        || id.eq_ignore_ascii_case("http")
        || id.eq_ignore_ascii_case("https")
        || registered_command(&id).is_err()
        || !seen.insert(id.to_lowercase())
    {
        return;
    }
    let name = capabilities
        .get_value::<String, _>("ApplicationName")
        .ok()
        .filter(|name| !name.is_empty() && !name.starts_with('@'))
        .unwrap_or_else(|| fallback_name.to_owned());
    choices.push(BrowserChoice {
        id,
        name,
        package_family: None,
    });
}

pub fn browser_icons(_: &Url, _: &str) -> std::collections::HashMap<String, String> {
    std::collections::HashMap::new()
}

fn registered_command(id: &str) -> io::Result<String> {
    let key = RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(format!(r"{id}\shell\open\command"))?;
    let template: String = key.get_value("")?;
    if template.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Empty browser command",
        ));
    }
    Ok(template)
}

pub fn open_in_browser(
    url: &Url,
    browser: &BrowserChoice,
    request: &mut LaunchRequest<'_>,
) -> Result<(), LaunchError> {
    spawn_browser(url, browser, request).map(|_| ())
}

fn spawn_browser(
    url: &Url,
    browser: &BrowserChoice,
    request: &mut LaunchRequest<'_>,
) -> Result<std::process::Child, LaunchError> {
    let template = registered_command(&browser.id)?;
    let expanded = expand_environment(&template)?;
    let mut arguments = split_command(&expanded)?.into_iter();
    let program = arguments
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing browser executable"))?;

    let mut command = Command::new(program);
    let mut used_url = false;
    for argument in arguments {
        // This invocation contains just the URL, with no additional shell arguments.
        if argument != "%*" {
            command.arg(insert_url(&argument, url.as_str(), &mut used_url));
        }
    }
    if !used_url {
        command.arg(url.as_str());
    }

    request.submit(|| command.spawn())
}

pub fn open_packaged_browser(
    url: &Url,
    family: &str,
    owner: Option<isize>,
    request: &mut LaunchRequest<'_>,
) -> Result<(), LaunchError> {
    // This runs on a worker so Windows can complete activation without blocking the picker UI.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(io::Error::other)?;
    let launched = (|| -> Result<bool, LaunchError> {
        let link = Uri::CreateUri(&HSTRING::from(url.as_str())).map_err(io::Error::other)?;
        let options = LauncherOptions::new().map_err(io::Error::other)?;
        options
            .SetTargetApplicationPackageFamilyName(&HSTRING::from(family))
            .map_err(io::Error::other)?;
        if let Some(owner) = owner {
            unsafe {
                options
                    .cast::<IInitializeWithWindow>()
                    .map_err(io::Error::other)?
                    .Initialize(HWND(owner as _))
                    .map_err(io::Error::other)?
            };
        }
        let operation = request.submit(|| {
            Launcher::LaunchUriWithOptionsAsync(&link, &options).map_err(io::Error::other)
        })?;
        Ok(operation.join().map_err(io::Error::other)?)
    })();
    unsafe { RoUninitialize() };
    if launched? {
        Ok(())
    } else {
        Err(io::Error::other("Windows did not launch the selected package").into())
    }
}

fn expand_environment(template: &str) -> io::Result<Vec<u16>> {
    let template = wide(template);
    // The first call returns the required size including the terminating null.
    let required = unsafe { ExpandEnvironmentStringsW(template.as_ptr(), std::ptr::null_mut(), 0) };
    if required == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut expanded = vec![0; required as usize];
    let written =
        unsafe { ExpandEnvironmentStringsW(template.as_ptr(), expanded.as_mut_ptr(), required) };
    if written == 0 {
        return Err(io::Error::last_os_error());
    }
    if written > required {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Expanded browser command grew",
        ));
    }
    Ok(expanded)
}

fn split_command(command: &[u16]) -> io::Result<Vec<String>> {
    let mut count = 0;
    // CommandLineToArgvW returns a LocalAlloc block containing null-terminated arguments.
    let arguments = unsafe { CommandLineToArgvW(command.as_ptr(), &mut count) };
    if arguments.is_null() {
        return Err(io::Error::last_os_error());
    }
    let parts = unsafe {
        let parts = std::slice::from_raw_parts(arguments, count as usize)
            .iter()
            .map(|&argument| {
                let length = (0..)
                    .take_while(|offset| *argument.add(*offset) != 0)
                    .count();
                String::from_utf16_lossy(std::slice::from_raw_parts(argument, length))
            })
            .collect();
        LocalFree(arguments.cast());
        parts
    };
    Ok(parts)
}

fn insert_url(template: &str, url: &str, used_url: &mut bool) -> String {
    let mut result = String::new();
    let mut remaining = template;
    while let Some((prefix, suffix)) = remaining.split_once('%') {
        result.push_str(prefix);
        if let Some(rest) = suffix.strip_prefix(['1', 'l', 'L']) {
            result.push_str(url);
            *used_url = true;
            remaining = rest;
        } else {
            result.push('%');
            remaining = suffix;
        }
    }
    result.push_str(remaining);
    result
}

pub fn open_settings() -> Result<(), ()> {
    let settings = wide("ms-settings:defaultapps?registeredAppUser=Link%20Selector");
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            settings.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    (result as isize > 32).then_some(()).ok_or(())
}

pub fn copy(url: &str) -> Result<(), ()> {
    clipboard_win::set_clipboard_string(url).map_err(|_| ())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, thread, time::Duration};

    struct TestClass {
        root: RegKey,
        path: String,
    }

    impl Drop for TestClass {
        fn drop(&mut self) {
            let _ = self.root.delete_subkey_all(&self.path);
        }
    }

    #[test]
    fn inserts_the_url_without_reinterpreting_percent_sequences() {
        let mut used = false;
        let url = "https://example.com/test%20page?q=%25L&x=1";
        assert_eq!(
            insert_url("--url=%L", url, &mut used),
            format!("--url={url}")
        );
        assert!(used);
        assert_eq!(insert_url("--other=20%", url, &mut false), "--other=20%");
    }

    #[test]
    fn lists_packaged_handlers_without_classic_open_commands() {
        let fixture = TestClass {
            root: RegKey::predef(HKEY_CURRENT_USER),
            path: format!(
                r"Software\LinkSelector-Packages-Test-{}",
                std::process::id()
            ),
        };
        let (root, _) = fixture.root.create_subkey(&fixture.path).unwrap();
        let (packages, _) = root.create_subkey("Packages").unwrap();
        let (classes, _) = root.create_subkey("Classes").unwrap();
        let (associations, _) = packages
            .create_subkey(
                r"Example.Firefox_1.0.0.0_x64__8wekyb3d8bbwe\BrowserMain\Capabilities\URLAssociations",
            )
            .unwrap();
        associations.set_value("https", &"AppXBrowser123").unwrap();
        associations.set_value("http", &"AppXMissing123").unwrap();
        classes.create_subkey("AppXBrowser123").unwrap();
        let (duplicate, _) = packages
            .create_subkey(
                r"Example.Firefox_2.0.0.0_x64__8wekyb3d8bbwe\BrowserMain\Capabilities\URLAssociations",
            )
            .unwrap();
        duplicate.set_value("https", &"appxbrowser123").unwrap();

        let mut seen = HashSet::new();
        let mut choices = Vec::new();
        add_packaged_choices(&packages, &classes, "http", &mut seen, &mut choices);
        assert!(choices.is_empty());
        add_packaged_choices(&packages, &classes, "https", &mut seen, &mut choices);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].id.to_ascii_lowercase(), "msix:appxbrowser123");
        assert_eq!(
            choices[0].package_family.as_deref(),
            Some("Example.Firefox_8wekyb3d8bbwe")
        );
        assert!(!choices[0].name.is_empty());
    }

    #[test]
    fn selected_prog_id_receives_the_original_url() {
        let scratch = tempfile::tempdir().unwrap();
        let receiver = scratch.path().join("receive-url.ps1");
        fs::write(
            &receiver,
            r#"param([string]$Url)
Start-Sleep -Milliseconds 400
[System.IO.File]::WriteAllText((Join-Path $PSScriptRoot 'received.txt'), $Url)
"#,
        )
        .unwrap();

        let id = format!("LinkSelector.Test.{}", std::process::id());
        let class = TestClass {
            root: RegKey::predef(HKEY_CURRENT_USER),
            path: format!(r"Software\Classes\{id}"),
        };
        let (key, _) = class.root.create_subkey(&class.path).unwrap();
        key.set_value("URL Protocol", &"").unwrap();
        let (command, _) = key.create_subkey(r"shell\open\command").unwrap();
        let powershell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let command_line = format!(
            r#""{}" -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "{}" "%1""#,
            powershell.display(),
            receiver.display()
        );
        command.set_value("", &command_line).unwrap();

        // If a default browser were launched instead, this URL would stay on loopback.
        let address = "http://127.0.0.1:9/selected?x=1&y=2";
        let url = Url::parse(address).unwrap();
        let browser = BrowserChoice {
            id,
            name: "Test browser".into(),
            package_family: None,
        };
        let pending = std::sync::Mutex::new(crate::PendingLinks::default());
        pending.lock().unwrap().add([url.clone()]);
        let mut request = LaunchRequest::new(&pending, crate::MAIN_WINDOW, 1);
        let mut child = spawn_browser(&url, &browser, &mut request).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        let exit = loop {
            match child.try_wait() {
                Ok(Some(exit)) => break exit,
                Ok(None) if std::time::Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(50));
                }
                outcome => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("The test browser did not exit within the deadline: {outcome:?}");
                }
            }
        };
        assert!(exit.success(), "The test browser exited with {exit}");
        let received = fs::read_to_string(scratch.path().join("received.txt")).unwrap();
        assert_eq!(received, address);
    }

    #[test]
    #[ignore = "requires a signed Firefox MSIX registered for the test user"]
    fn selected_firefox_msix_receives_the_link() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            time::Instant,
        };

        let family = std::env::var("LINK_SELECTOR_TEST_MSIX").unwrap();
        for scheme in ["http", "https"] {
            let url = Url::parse(&format!("{scheme}://127.0.0.1/")).unwrap();
            assert!(browser_choices(&url, "")
                .iter()
                .any(|browser| browser.package_family.as_deref() == Some(&family)));
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let path = "/link-selector-msix?x=1&y=2";
        let url = Url::parse(&format!("http://{}{path}", listener.local_addr().unwrap())).unwrap();
        let pending = std::sync::Mutex::new(crate::PendingLinks::default());
        pending.lock().unwrap().add([url.clone()]);
        let mut request = LaunchRequest::new(&pending, crate::MAIN_WINDOW, 1);
        open_packaged_browser(&url, &family, None, &mut request).unwrap();

        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                Err(error) => panic!("Could not accept the browser request: {error}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut headers = BufReader::new(&stream).lines();
            let request = headers.next().and_then(Result::ok).unwrap_or_default();
            let firefox = headers
                .map_while(Result::ok)
                .take_while(|line| !line.is_empty())
                .any(|line| {
                    line.to_ascii_lowercase().starts_with("user-agent:")
                        && line.contains("Firefox/")
                });
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            if request.starts_with(&format!("GET {path} ")) && firefox {
                return;
            }
        }
        panic!("The selected Firefox MSIX did not request {path}");
    }
}
