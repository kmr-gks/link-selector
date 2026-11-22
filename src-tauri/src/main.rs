#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod picker_size;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
mod queue;

use picker_size::{Dimensions, PickerSize};
use queue::PendingLinks;
use serde::Serialize;
use std::sync::Mutex;
use tauri::RunEvent;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, Url, WebviewWindow,
    WindowEvent,
};

#[derive(Serialize)]
struct BrowserChoice {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct Picker {
    id: u64,
    url: String,
    browsers: Vec<BrowserChoice>,
}

#[tauri::command]
fn picker_state(app: AppHandle, state: State<'_, Mutex<PendingLinks>>) -> Option<Picker> {
    let link = state.lock().unwrap().current().cloned()?;
    Some(Picker {
        id: link.id,
        url: link.url.to_string(),
        browsers: platform::browser_choices(&link.url, &app.config().identifier),
    })
}

#[tauri::command]
fn picker_icons(
    app: AppHandle,
    pending: State<'_, Mutex<PendingLinks>>,
    id: u64,
) -> std::collections::HashMap<String, String> {
    let link = pending.lock().unwrap().current_with_id(id).cloned();
    link.map(|link| platform::browser_icons(&link.url, &app.config().identifier))
        .unwrap_or_default()
}

#[tauri::command]
fn open_browser(
    app: AppHandle,
    state: State<'_, Mutex<PendingLinks>>,
    id: u64,
    browser_id: String,
) -> Result<(), String> {
    let link = state
        .lock()
        .unwrap()
        .current_with_id(id)
        .cloned()
        .ok_or("このリンクは処理済みです。")?;
    let path = platform::browser_choices(&link.url, &app.config().identifier)
        .into_iter()
        .find(|browser| browser.id == browser_id)
        .ok_or("ブラウザが見つかりませんでした。別のブラウザを選んでください。")?;
    platform::open_in_browser(&link.url, &path).map_err(|_| {
        format!(
            "{} を開けませんでした。別のブラウザを選んでください。",
            path.name
        )
    })?;
    finish_link(&app, id, false);
    Ok(())
}

#[tauri::command]
fn cancel_link(app: AppHandle, id: u64) {
    finish_link(&app, id, true);
}

#[tauri::command]
fn copy_link(state: State<'_, Mutex<PendingLinks>>, id: u64) -> Result<(), String> {
    let state = state.lock().unwrap();
    let link = state
        .current_with_id(id)
        .ok_or("このリンクは処理済みです。")?;
    platform::copy(link.url.as_str()).map_err(|_| "リンクをコピーできませんでした。".to_owned())
}

#[tauri::command]
fn open_settings() -> Result<(), String> {
    platform::open_settings().map_err(|_| "システム設定を開けませんでした。".to_owned())
}

#[tauri::command]
fn fit_picker(window: WebviewWindow, content_height: Option<u32>) -> tauri::Result<()> {
    if window.is_maximized()? || window.is_fullscreen()? {
        return Ok(());
    }
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let default_height = window
        .app_handle()
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == window.label())
        .map_or(0.0, |config| config.height);
    let inner = window.inner_size()?;
    let outer = window.outer_size()?;
    let frame_width = outer.width.saturating_sub(inner.width);
    let frame_height = outer.height.saturating_sub(inner.height);
    let area = monitor.work_area();
    let max_height = area.size.height.saturating_sub(frame_height);
    let scale = window.scale_factor()?;
    let state = window.state::<Mutex<PickerSize>>();
    let preferred = state.lock().unwrap().preferred;
    let max_width = area.size.width.saturating_sub(frame_width);
    let preferred_height = preferred.map_or_else(
        || f64::from(content_height.unwrap_or_default()).max(default_height),
        |size| size.height,
    );
    let width = preferred
        .map_or(inner.width, |size| (size.width * scale).round() as u32)
        .min(max_width);
    let height = ((preferred_height * scale).ceil() as u32).min(max_height);
    if width == 0 || height == 0 {
        return Ok(());
    }

    let target = PhysicalSize::new(width, height);
    if target != inner {
        let position = window.outer_position()?;
        let last_x = area.position.x + (max_width - width) as i32;
        let last_y = area.position.y + (max_height - height) as i32;
        let target_position = PhysicalPosition::new(
            position.x.clamp(area.position.x, last_x),
            position.y.clamp(area.position.y, last_y),
        );
        window.set_size(target)?;
        if window.outer_position()? != target_position {
            window.set_position(target_position)?;
        }
    }
    state.lock().unwrap().presented = content_height.map(|_| dimensions(target, scale));
    Ok(())
}

fn dimensions(size: PhysicalSize<u32>, scale: f64) -> Dimensions {
    let logical = size.to_logical::<f64>(scale);
    Dimensions {
        width: logical.width,
        height: logical.height,
    }
}

fn remember_picker_size(window: &WebviewWindow) {
    let state = window.state::<Mutex<PickerSize>>();
    if window.is_maximized().unwrap_or(true) || window.is_fullscreen().unwrap_or(true) {
        state.lock().unwrap().presented = None;
        return;
    }
    if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
        if let Err(error) = state.lock().unwrap().remember(dimensions(size, scale)) {
            eprintln!("Unable to save picker size: {error}");
        }
    }
}

fn finish_link(app: &AppHandle, id: u64, cancelled: bool) {
    let state = app.state::<Mutex<PendingLinks>>();
    let mut pending = state.lock().unwrap();
    if !pending.finish(id) {
        return;
    }
    let more_links = pending.current().is_some();
    drop(pending);

    if let Some(window) = app.get_webview_window("main") {
        remember_picker_size(&window);
        if !more_links {
            let _ = window.hide();
        } else if !cancelled {
            let _ = window.set_focus();
        }
    }
    let _ = app.emit("link-queue-changed", ());
}

fn receive_links(app: &AppHandle, urls: Vec<Url>) -> bool {
    let state = app.state::<Mutex<PendingLinks>>();
    let mut pending = state.lock().unwrap();
    let was_empty = pending.current().is_none();
    let added_links = pending.enqueue(urls);
    drop(pending);
    if added_links {
        if was_empty {
            let _ = app.emit("link-queue-changed", ());
        }
        show_window(app);
    }
    added_links
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if matches!(window.is_visible(), Ok(false)) {
            let state = window.state::<Mutex<PickerSize>>();
            let has_preferred_size = state.lock().unwrap().preferred.is_some();
            if has_preferred_size {
                let _ = fit_picker(window.clone(), None);
            }
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "windows")]
fn receive_arguments(app: &AppHandle, args: impl IntoIterator<Item = String>) {
    let urls = args
        .into_iter()
        .filter_map(|argument| Url::parse(&argument).ok())
        .collect();
    if !receive_links(app, urls) {
        show_window(app);
    }
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "windows")]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _| {
        receive_arguments(app, args.into_iter().skip(1));
    }));

    builder
        .manage(Mutex::new(PendingLinks::default()))
        .setup(|app| {
            let file = app
                .path()
                .app_config_dir()
                .ok()
                .map(|dir| dir.join("picker-size.json"));
            let size = PickerSize::load(file);
            let has_preferred_size = size.preferred.is_some();
            app.manage(Mutex::new(size));
            if has_preferred_size {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = fit_picker(window, None);
                }
            }
            #[cfg(target_os = "windows")]
            receive_arguments(
                app.handle(),
                std::env::args_os()
                    .skip(1)
                    .filter_map(|arg| arg.into_string().ok()),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            picker_state,
            picker_icons,
            open_browser,
            cancel_link,
            copy_link,
            open_settings,
            fit_picker,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let state = window.state::<Mutex<PendingLinks>>();
                let id = state.lock().unwrap().current().map(|link| link.id);
                if let Some(id) = id {
                    finish_link(window.app_handle(), id, true);
                } else {
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build Link Selector")
        .run(|app, event| match event {
            RunEvent::ExitRequested { .. } => {
                if let Some(window) = app.get_webview_window("main") {
                    remember_picker_size(&window);
                }
            }
            #[cfg(target_os = "macos")]
            RunEvent::Opened { urls } => {
                receive_links(app, urls);
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => show_window(app),
            _ => {}
        });
}
