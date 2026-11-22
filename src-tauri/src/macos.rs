use crate::{BrowserChoice, LaunchError, LaunchRequest};
use base64::{engine::general_purpose::STANDARD, Engine};
use objc2::{rc::autoreleasepool, AnyThread};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext,
    NSPasteboard, NSPasteboardTypeString, NSWorkspace,
};
use objc2_foundation::{NSBundle, NSDictionary, NSPoint, NSRect, NSSize, NSString, NSURL};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::Path,
    process::Command,
};
use tauri::Url;

pub fn browser_choices(url: &Url, own_identifier: &str) -> Vec<BrowserChoice> {
    autoreleasepool(|_| collect_browser_choices(url, own_identifier))
}

fn collect_browser_choices(url: &Url, own_identifier: &str) -> Vec<BrowserChoice> {
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
        // Browser icons are 28 CSS pixels wide. A 56-pixel bitmap stays sharp on Retina.
        const PIXELS: isize = 56;
        // Null planes ask AppKit to allocate and manage the bitmap storage.
        let bitmap = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(), std::ptr::null_mut(), PIXELS, PIXELS, 8, 4, true, false,
                NSDeviceRGBColorSpace, 0, 0,
            )
        }?;
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        icon.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(56.0, 56.0)));
        NSGraphicsContext::restoreGraphicsState_class();

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

pub fn open_in_browser(
    url: &Url,
    browser: &BrowserChoice,
    request: &mut LaunchRequest<'_>,
) -> Result<(), LaunchError> {
    use std::{io, process::Stdio};

    let mut command = Command::new("/usr/bin/open");
    command
        .args(["-a", &browser.id, url.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = request.submit(|| command.spawn())?;
    if child.wait()?.success() {
        Ok(())
    } else {
        Err(io::Error::other("macOS did not launch the selected browser").into())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_lookup_does_not_leak_cocoa_objects_on_a_worker() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::tests::browser_lookup_in_worker",
                "--ignored",
                "--nocapture",
            ])
            .env("OBJC_DEBUG_MISSING_POOLS", "YES")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}\n{stderr}");
        let thread = stdout
            .lines()
            .find_map(|line| line.strip_prefix("lookup thread: "))
            .unwrap();
        assert!(
            !stderr.contains(&format!("MISSING POOLS: ({thread})")),
            "{stderr}"
        );
    }

    #[test]
    #[ignore = "runs in a child process with Objective-C diagnostics enabled"]
    fn browser_lookup_in_worker() {
        extern "C" {
            fn pthread_self() -> *const std::ffi::c_void;
        }
        let choices = std::thread::spawn(|| {
            println!("lookup thread: {:p}", unsafe { pthread_self() });
            browser_choices(&Url::parse("https://example.invalid/").unwrap(), "")
        })
        .join()
        .unwrap();
        assert!(choices
            .iter()
            .all(|choice| !choice.id.is_empty() && !choice.name.is_empty()));
    }
}
