use crate::BrowserChoice;
use std::{collections::HashSet, io, process::Command};
use tauri::Url;
use windows_sys::Win32::{
    Foundation::LocalFree,
    System::Environment::ExpandEnvironmentStringsW,
    UI::{
        Shell::{CommandLineToArgvW, ShellExecuteW},
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

pub fn browser_choices(url: &Url, _: &str) -> Vec<BrowserChoice> {
    let mut seen = HashSet::new();
    let mut choices = Vec::new();

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
    choices.push(BrowserChoice { id, name });
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

pub fn open_in_browser(url: &Url, browser: &BrowserChoice) -> io::Result<()> {
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

    command.spawn().map(|_| ())
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
    fn selected_prog_id_receives_the_original_url() {
        let scratch = tempfile::tempdir().unwrap();
        let receiver = scratch.path().join("receive-url.ps1");
        fs::write(
            &receiver,
            r#"param([string]$Url)
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
        };
        open_in_browser(&url, &browser).unwrap();
        let received = scratch.path().join("received.txt");
        for _ in 0..100 {
            if fs::read_to_string(&received).is_ok_and(|value| value == address) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!(
            "The selected ProgID did not receive the URL: {:?}",
            fs::read_to_string(received)
        );
    }
}
