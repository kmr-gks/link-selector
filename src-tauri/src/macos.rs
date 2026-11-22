use crate::BrowserChoice;
use base64::{engine::general_purpose::STANDARD, Engine};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypeString, NSWorkspace,
};
use objc2_foundation::{NSBundle, NSDictionary, NSString, NSURL};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::Path,
    process::Command,
};
use tauri::Url;

pub fn browser_choices(url: &Url, own_identifier: &str) -> Vec<BrowserChoice> {
    // Look for browsers even for local HTML, which is also associated with editors.
    let address = if url.scheme() == "file" {
        "https://example.invalid/"
    } else {
        url.as_str()
    };
    let Some(url) = NSURL::URLWithString(&NSString::from_str(address)) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut browsers: Vec<_> = NSWorkspace::sharedWorkspace()
        .URLsForApplicationsToOpenURL(&url)
        .to_vec()
        .into_iter()
        .filter_map(|app| {
            let path = app.path()?.to_string();
            let bundle = NSBundle::bundleWithURL(&app)?;
            let identifier = bundle.bundleIdentifier()?.to_string();
            if identifier == own_identifier || !seen.insert(identifier) {
                return None;
            }
            let name = ["CFBundleDisplayName", "CFBundleName"]
                .iter()
                .find_map(|key| {
                    bundle
                        .objectForInfoDictionaryKey(&NSString::from_str(key))?
                        .downcast::<NSString>()
                        .ok()
                        .map(|name| name.to_string())
                })
                .unwrap_or_else(|| {
                    Path::new(&path)
                        .file_stem()
                        .unwrap_or(OsStr::new(&path))
                        .to_string_lossy()
                        .into_owned()
                });
            Some(BrowserChoice { id: path, name })
        })
        .collect();
    browsers.sort_by_cached_key(|browser| browser.name.to_lowercase());
    browsers
}

thread_local! {
    static ICONS: RefCell<HashMap<String, Option<String>>> = RefCell::default();
}

pub fn browser_icons(url: &Url, own_identifier: &str) -> HashMap<String, String> {
    let workspace = NSWorkspace::sharedWorkspace();
    ICONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        browser_choices(url, own_identifier)
            .into_iter()
            .filter_map(|choice| {
                let icon = cache
                    .entry(choice.id.clone())
                    .or_insert_with(|| browser_icon(&workspace, &choice.id));
                icon.clone().map(|icon| (choice.id, icon))
            })
            .collect()
    })
}

fn browser_icon(workspace: &NSWorkspace, path: &str) -> Option<String> {
    autoreleasepool(|_| {
        let icon = workspace.iconForFile(&NSString::from_str(path));
        let tiff = icon.TIFFRepresentation()?;
        let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
        // An empty dictionary requests the PNG encoder's default properties.
        let png = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )
        }?;
        Some(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.to_vec())
        ))
    })
}

pub fn open_in_browser(url: &Url, browser: &BrowserChoice) -> Result<(), ()> {
    open(&["-a", &browser.id, url.as_str()])
}

pub fn open_settings() -> Result<(), ()> {
    open(&["x-apple.systempreferences:com.apple.Desktop-Settings.extension"])
}

fn open(args: &[&str]) -> Result<(), ()> {
    let status = Command::new("/usr/bin/open")
        .args(args)
        .output()
        .map_err(|_| ())?;
    status.status.success().then_some(()).ok_or(())
}

pub fn copy(url: &str) -> Result<(), ()> {
    let clipboard = NSPasteboard::generalPasteboard();
    clipboard.clearContents();
    // AppKit's declared NSString constant is valid for string data on the general pasteboard.
    let copied =
        unsafe { clipboard.setString_forType(&NSString::from_str(url), NSPasteboardTypeString) };
    copied.then_some(()).ok_or(())
}
