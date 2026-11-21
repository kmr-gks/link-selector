use crate::{Browser, BrowserChoice};
use std::collections::HashSet;

use tauri::Url;
use windows_sys::Win32::UI::{
    Shell::{
        ShellExecuteExW, ShellExecuteW, SEE_MASK_CLASSNAME, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC,
        SHELLEXECUTEINFOW,
    },
    WindowsAndMessaging::SW_SHOWNORMAL,
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
    let classes = RegKey::predef(HKEY_CLASSES_ROOT);
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
                            &classes,
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
                        &classes,
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
    classes: &RegKey,
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
        || classes
            .open_subkey(format!(r"{id}\shell\open\command"))
            .is_err()
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

pub fn browsers(url: &Url, own_identifier: &str) -> Vec<Browser> {
    browser_choices(url, own_identifier)
        .into_iter()
        .map(|choice| Browser { choice, icon: None })
        .collect()
}

pub fn open_in_browser(url: &Url, browser: &BrowserChoice) -> Result<(), ()> {
    let verb = wide("open");
    let address = wide(url.as_str());
    let class = wide(&browser.id);
    let mut options = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_CLASSNAME | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: address.as_ptr(),
        lpClass: class.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // Supplying the selected ProgID lets the shell apply that browser's registered command.
    (unsafe { ShellExecuteExW(&mut options) } != 0)
        .then_some(())
        .ok_or(())
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
    clipboard_win::set_clipboard(clipboard_win::formats::Unicode, url).map_err(|_| ())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
