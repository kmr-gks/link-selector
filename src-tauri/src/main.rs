#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod links;
mod picker_size;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

use links::{LaunchError, LaunchRequest, PendingLinks, PickerClose, MAIN_WINDOW};
use picker_size::{Dimensions, PickerSize};
use serde::Serialize;
use std::sync::Mutex;
use tauri::RunEvent;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, Url, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

#[derive(Serialize)]
struct BrowserChoice {
    id: String,
    name: String,
    #[cfg(target_os = "windows")]
    #[serde(skip)]
    package_family: Option<String>,
}

#[derive(Serialize)]
struct Picker {
    id: u64,
    url: String,
    browsers: Vec<BrowserChoice>,
    waiting: usize,
}

#[tauri::command]
fn picker_state(window: WebviewWindow, state: State<'_, Mutex<PendingLinks>>) -> Option<Picker> {
    let (link, waiting) = {
        let pending = state.lock().unwrap();
        let link = pending.get(window.label()).cloned()?;
        let waiting = if window.label() == MAIN_WINDOW {
            pending.deferred_count()
        } else {
            0
        };
        (link, waiting)
    };
    let app = window.app_handle();
    Some(Picker {
        id: link.id,
        url: link.url.to_string(),
        browsers: platform::browser_choices(&link.url, &app.config().identifier),
        waiting,
    })
}

#[tauri::command]
fn picker_rendered(window: WebviewWindow, state: State<'_, Mutex<PendingLinks>>, id: u64) {
    if window.label() == MAIN_WINDOW {
        state.lock().unwrap().confirm_main_render(id);
    }
}

#[tauri::command]
fn picker_waiting(
    window: WebviewWindow,
    state: State<'_, Mutex<PendingLinks>>,
) -> Option<(u64, usize)> {
    if window.label() != MAIN_WINDOW {
        return None;
    }
    let pending = state.lock().unwrap();
    let link = pending.get(MAIN_WINDOW)?;
    Some((link.id, pending.deferred_count()))
}

#[tauri::command]
fn picker_icons(
    window: WebviewWindow,
    pending: State<'_, Mutex<PendingLinks>>,
    id: u64,
) -> std::collections::HashMap<String, String> {
    let link = pending
        .lock()
        .unwrap()
        .get_with_id(window.label(), id)
        .cloned();
    let app = window.app_handle();
    link.map(|link| platform::browser_icons(&link.url, &app.config().identifier))
        .unwrap_or_default()
}

#[tauri::command]
async fn open_browser(
    window: WebviewWindow,
    state: State<'_, Mutex<PendingLinks>>,
    id: u64,
    browser_id: String,
) -> Result<(), String> {
    let link = state
        .lock()
        .unwrap()
        .get_with_id(window.label(), id)
        .cloned()
        .ok_or("このリンクは処理済みです。")?;
    let browser = platform::browser_choices(&link.url, &window.app_handle().config().identifier)
        .into_iter()
        .find(|browser| browser.id == browser_id)
        .ok_or("ブラウザが見つかりませんでした。別のブラウザを選んでください。")?;
    let worker_window = window.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let owner_error = || {
            format!(
                "{} を開けませんでした。別のブラウザを選んでください。",
                browser.name
            )
        };
        #[cfg(target_os = "windows")]
        let owner = if browser.package_family.is_some() {
            Some(worker_window.hwnd().map_err(|_| owner_error())?.0 as isize)
        } else {
            None
        };
        let pending = worker_window.state::<Mutex<PendingLinks>>();
        let dismiss = || {
            let window = worker_window.clone();
            let _ = worker_window.run_on_main_thread(move || finish_link(&window, id));
        };
        let mut request = LaunchRequest::new(&pending, worker_window.label(), id, &dismiss);
        #[cfg(target_os = "windows")]
        let outcome = if let Some(family) = browser.package_family.as_ref() {
            platform::open_packaged_browser(&link.url, family, owner, &mut request)
        } else {
            platform::open_in_browser(&link.url, &browser, &mut request)
        };
        #[cfg(target_os = "macos")]
        let outcome = platform::open_in_browser(&link.url, &browser, &mut request);
        match outcome {
            Ok(()) => {
                request.complete();
                Ok(())
            }
            Err(LaunchError::DismissedOrAlreadySubmitted) => {
                Err("このリンクは処理済みです。".to_owned())
            }
            Err(LaunchError::Platform(error)) => {
                eprintln!("Browser launch failed: {:?}", error.kind());
                Err(owner_error())
            }
        }
    })
    .await
    .map_err(|_| "ブラウザを起動できませんでした。".to_owned())??;

    let (completed, mut result) = tauri::async_runtime::channel(1);
    window
        .clone()
        .run_on_main_thread(move || {
            finish_link(&window, id);
            let _ = completed.try_send(());
        })
        .map_err(|_| "選択画面を閉じられませんでした。".to_owned())?;
    result
        .recv()
        .await
        .ok_or("選択画面を閉じられませんでした。")?;
    Ok(())
}

#[tauri::command]
fn cancel_link(window: WebviewWindow, id: u64) {
    finish_link(&window, id);
}

#[tauri::command]
fn close_picker_error(window: WebviewWindow) -> tauri::Result<()> {
    if window.label() == MAIN_WINDOW {
        window.hide()
    } else {
        window.close()
    }
}

#[tauri::command]
fn copy_link(
    window: WebviewWindow,
    state: State<'_, Mutex<PendingLinks>>,
    id: u64,
) -> Result<(), String> {
    let state = state.lock().unwrap();
    let link = state
        .get_with_id(window.label(), id)
        .ok_or("このリンクは処理済みです。")?;
    platform::copy(link.url.as_str()).map_err(|_| "リンクをコピーできませんでした。".to_owned())
}

#[tauri::command]
fn open_settings() -> Result<(), String> {
    platform::open_settings().map_err(|_| "システム設定を開けませんでした。".to_owned())
}

#[tauri::command]
fn fit_picker(
    window: WebviewWindow,
    content_height: Option<u32>,
    viewport_height: Option<u32>,
    id: Option<u64>,
    grow_only: Option<bool>,
) -> tauri::Result<()> {
    let grow_only = grow_only.unwrap_or(false);
    let pending = window.state::<Mutex<PendingLinks>>();
    let preferred = if let Some(id) = id {
        let pending = pending.lock().unwrap();
        if !pending.can_fit(window.label(), Some(id), grow_only) {
            return Ok(());
        }
        pending
            .get_with_id(window.label(), id)
            .and_then(|link| link.preferred_size)
    } else {
        if !pending
            .lock()
            .unwrap()
            .can_fit(window.label(), None, grow_only)
        {
            return Ok(());
        }
        window
            .state::<Mutex<PickerSize>>()
            .lock()
            .unwrap()
            .preferred
    };
    if window.is_maximized()? || window.is_fullscreen()? {
        if let Some(id) = id {
            window
                .state::<Mutex<PendingLinks>>()
                .lock()
                .unwrap()
                .track_restored_size(window.label(), id);
        }
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
        .find(|config| config.label == MAIN_WINDOW)
        .map_or(0.0, |config| config.height);
    let inner = window.inner_size()?;
    let outer = window.outer_size()?;
    let frame_width = outer.width.saturating_sub(inner.width);
    let frame_height = outer.height.saturating_sub(inner.height);
    let area = monitor.work_area();
    let max_height = area.size.height.saturating_sub(frame_height);
    let scale = window.scale_factor()?;
    let max_width = area.size.width.saturating_sub(frame_width);
    let height = if let Some(size) = preferred {
        (size.height * scale).ceil() as u32
    } else if let Some(content) = content_height {
        let measured = viewport_height
            .filter(|height| *height > 0)
            .map(|height| (f64::from(inner.height) / scale - f64::from(height)).max(0.0));
        let padding = id.map_or(0.0, |id| {
            window
                .state::<Mutex<PendingLinks>>()
                .lock()
                .unwrap()
                .fit_padding(window.label(), id, measured)
        });
        ((f64::from(content) + padding).max(default_height) * scale).ceil() as u32
    } else {
        (default_height * scale).ceil() as u32
    };
    let width = preferred
        .map_or(inner.width, |size| (size.width * scale).round() as u32)
        .min(max_width);
    let height = height.min(max_height);
    if width == 0 || height == 0 || (grow_only && height <= inner.height) {
        return Ok(());
    }

    let target = PhysicalSize::new(width, height);
    if let Some(id) = id {
        // The resize event for this first, automatic fit must not become a preference.
        window
            .state::<Mutex<PendingLinks>>()
            .lock()
            .unwrap()
            .set_presented(window.label(), id, dimensions(target, scale));
    }
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
    Ok(())
}

fn dimensions(size: PhysicalSize<u32>, scale: f64) -> Dimensions {
    let logical = size.to_logical::<f64>(scale);
    Dimensions {
        width: logical.width,
        height: logical.height,
    }
}

fn record_picker_resize(window: &tauri::Window, size: PhysicalSize<u32>) {
    if !window.is_visible().unwrap_or(false)
        || window.is_maximized().unwrap_or(true)
        || window.is_fullscreen().unwrap_or(true)
        || window.is_minimized().unwrap_or(true)
        || window.inner_size().ok() != Some(size)
    {
        return;
    }
    let Ok(scale) = window.scale_factor() else {
        return;
    };
    let size = dimensions(size, scale);
    if window
        .state::<Mutex<PendingLinks>>()
        .lock()
        .unwrap()
        .record_resize(window.label(), size)
    {
        window
            .state::<Mutex<PickerSize>>()
            .lock()
            .unwrap()
            .remember(size);
    }
}

fn save_picker_size(app: &AppHandle) {
    if let Err(error) = app.state::<Mutex<PickerSize>>().lock().unwrap().save() {
        eprintln!("Unable to save picker size: {error}");
    }
}

fn finish_link(window: &WebviewWindow, id: u64) {
    let main = window.label() == MAIN_WINDOW;
    let preferred = window
        .state::<Mutex<PickerSize>>()
        .lock()
        .unwrap()
        .preferred;
    let show_next = {
        let pending = window.state::<Mutex<PendingLinks>>();
        let mut pending = pending.lock().unwrap();
        if pending.finish(window.label(), id).is_none() {
            return;
        }
        main && pending.show_next_deferred(preferred)
    };
    save_picker_size(window.app_handle());
    if main {
        if show_next {
            show_main_picker(window, Some(id));
        } else {
            let _ = window.hide();
            let _ = window.emit("main-picker-changed", Some(id));
        }
    } else {
        let _ = window.destroy();
    }
}

fn show_main_picker(window: &WebviewWindow, finished_id: Option<u64>) {
    show_window(window);
    let _ = window.emit("main-picker-changed", finished_id);
}

fn recover_picker(app: &AppHandle, label: &str) {
    let pending = app.state::<Mutex<PendingLinks>>();
    let preferred = app.state::<Mutex<PickerSize>>().lock().unwrap().preferred;
    let (show_next, waiting) = {
        let mut pending = pending.lock().unwrap();
        let show_next = pending.defer(label, preferred);
        let waiting = pending
            .get(MAIN_WINDOW)
            .map(|link| (link.id, pending.deferred_count()));
        (show_next, waiting)
    };
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        if show_next {
            show_main_picker(&window, None);
        } else if let Some(waiting) = waiting {
            let _ = window.emit("picker-deferred", waiting);
        }
    }
}

fn receive_links(app: &AppHandle, urls: Vec<Url>) -> bool {
    let preferred = app.state::<Mutex<PickerSize>>().lock().unwrap().preferred;
    let windows = app
        .state::<Mutex<PendingLinks>>()
        .lock()
        .unwrap()
        .add(urls, preferred);
    if windows.is_empty() {
        return false;
    }
    let mut additional = windows.into_iter().peekable();
    if additional.peek().is_some_and(|label| label == MAIN_WINDOW) {
        additional.next();
        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
            show_main_picker(&window, None);
        }
    }

    // WebView2 window construction must start outside Tauri's event callback.
    let app = app.clone();
    let additional: Vec<_> = additional.collect();
    if !additional.is_empty() {
        tauri::async_runtime::spawn_blocking(move || {
            for label in additional {
                if let Err(error) = create_picker(&app, &label) {
                    eprintln!("Unable to create picker window: {error}");
                    let app = app.clone();
                    let _ = app
                        .clone()
                        .run_on_main_thread(move || recover_picker(&app, &label));
                }
            }
        });
    }
    true
}

fn create_picker(app: &AppHandle, label: &str) -> tauri::Result<()> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == MAIN_WINDOW)
        .expect("the main picker window is configured")
        .clone();
    config.label = label.to_owned();
    config.visible = false;
    config.focus = false;
    let preferred = app
        .state::<Mutex<PendingLinks>>()
        .lock()
        .unwrap()
        .get(label)
        .and_then(|link| link.preferred_size);
    if let Some(size) = preferred {
        config.width = size.width;
        config.height = size.height;
    }
    let window = WebviewWindowBuilder::from_config(app, &config)?.build()?;
    let slot = app
        .state::<Mutex<PendingLinks>>()
        .lock()
        .unwrap()
        .get(label)
        .map_or(0, |link| link.slot);
    position_picker(app, &window, slot);
    show_window(&window);
    Ok(())
}

fn position_picker(app: &AppHandle, window: &WebviewWindow, slot: usize) {
    let Some(anchor) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let (Ok(position), Ok(Some(monitor)), Ok(size)) = (
        anchor.outer_position(),
        anchor.current_monitor(),
        window.outer_size(),
    ) else {
        return;
    };
    let area = monitor.work_area();
    let distance = (24.0 * (slot % 8) as f64 * monitor.scale_factor()).round() as i32;
    let offset = |position: i32, start: i32, room: u32| {
        let end = start + room as i32;
        let position = position.clamp(start, end);
        if end - position >= distance {
            position + distance
        } else {
            (position - distance).max(start)
        }
    };
    let _ = window.set_position(PhysicalPosition::new(
        offset(
            position.x,
            area.position.x,
            area.size.width.saturating_sub(size.width),
        ),
        offset(
            position.y,
            area.position.y,
            area.size.height.saturating_sub(size.height),
        ),
    ));
}

fn show_window(window: &WebviewWindow) {
    let pending = window.state::<Mutex<PendingLinks>>();
    let link = pending
        .lock()
        .unwrap()
        .get(window.label())
        .map(|link| (link.id, link.preferred_size));
    let id = link.map(|(id, _)| id);
    if window.label() == MAIN_WINDOW && matches!(window.is_visible(), Ok(false)) {
        let preferred = link.map_or_else(
            || {
                window
                    .state::<Mutex<PickerSize>>()
                    .lock()
                    .unwrap()
                    .preferred
            },
            |(_, size)| size,
        );
        if preferred.is_some() {
            let _ = fit_picker(window.clone(), None, None, id, None);
        }
    }
    let _ = window.unminimize();
    if let Some(id) = id {
        if window.is_maximized().unwrap_or(true)
            || window.is_fullscreen().unwrap_or(true)
            || window.is_minimized().unwrap_or(true)
        {
            pending
                .lock()
                .unwrap()
                .track_restored_size(window.label(), id);
        } else if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
            pending
                .lock()
                .unwrap()
                .track_loading_size(window.label(), id, dimensions(size, scale));
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn reopen(app: &AppHandle) {
    let active = app
        .state::<Mutex<PendingLinks>>()
        .lock()
        .unwrap()
        .windows()
        .filter_map(|(label, link)| {
            app.get_webview_window(label)
                .map(|window| (link.id, window))
        })
        .max_by_key(|(id, _)| *id)
        .map(|(_, window)| window)
        .or_else(|| app.get_webview_window(MAIN_WINDOW));
    if let Some(window) = active {
        if window.label() == MAIN_WINDOW {
            show_main_picker(&window, None);
        } else {
            show_window(&window);
        }
    }
}

#[cfg(target_os = "windows")]
fn receive_arguments(app: &AppHandle, args: impl IntoIterator<Item = String>) {
    let urls = args
        .into_iter()
        .filter_map(|argument| Url::parse(&argument).ok())
        .collect();
    let startup = app.state::<Mutex<links::StartupLinks>>();
    let Some(urls) = startup.lock().unwrap().route(urls) else {
        return;
    };
    if !receive_links(app, urls) {
        reopen(app);
    }
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "windows")]
    let builder = builder
        .manage(Mutex::new(links::StartupLinks::default()))
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            receive_arguments(app, args.into_iter().skip(1));
        }));

    #[cfg(target_os = "macos")]
    let mut startup_links = links::StartupLinks::default();

    builder
        .manage(Mutex::new(PendingLinks::default()))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let file = app
                .path()
                .app_config_dir()
                .ok()
                .map(|dir| dir.join("picker-size.json"));
            let size = PickerSize::load(file);
            let has_preferred_size = size.preferred.is_some();
            app.manage(Mutex::new(size));
            if has_preferred_size {
                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                    let _ = fit_picker(window, None, None, None, None);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            picker_state,
            picker_rendered,
            picker_icons,
            picker_waiting,
            open_browser,
            cancel_link,
            close_picker_error,
            copy_link,
            open_settings,
            fit_picker,
        ])
        .on_window_event(|window, event| match event {
            WindowEvent::Resized(size) => {
                if window.try_state::<Mutex<PickerSize>>().is_some() {
                    record_picker_resize(window, *size);
                }
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                new_inner_size,
                ..
            } => {
                let is_normal = !window.is_maximized().unwrap_or(true)
                    && !window.is_fullscreen().unwrap_or(true)
                    && !window.is_minimized().unwrap_or(true);
                window
                    .state::<Mutex<PendingLinks>>()
                    .lock()
                    .unwrap()
                    .record_scale_change(
                        window.label(),
                        dimensions(*new_inner_size, *scale_factor),
                        is_normal,
                    );
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                    let state = window.state::<Mutex<PendingLinks>>();
                    let action = state.lock().unwrap().close_request(window.label());
                    match action {
                        PickerClose::Finish(id) => finish_link(&webview, id),
                        PickerClose::Hide => {
                            // Closing an unrendered picker hides it without discarding its link.
                            let _ = webview.hide();
                        }
                        PickerClose::Destroy => {
                            let _ = webview.destroy();
                        }
                    }
                }
            }
            WindowEvent::Destroyed if window.label() != MAIN_WINDOW => {
                window
                    .state::<Mutex<PendingLinks>>()
                    .lock()
                    .unwrap()
                    .remove(window.label());
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("failed to build Link Selector")
        .run(move |app, event| match event {
            RunEvent::ExitRequested { .. } => save_picker_size(app),
            #[cfg(target_os = "macos")]
            RunEvent::Opened { urls } => {
                if let Some(urls) = startup_links.route(urls) {
                    receive_links(app, urls);
                }
            }
            #[cfg(target_os = "windows")]
            RunEvent::Ready => {
                let initial = std::env::args_os()
                    .skip(1)
                    .filter_map(|arg| arg.into_string().ok())
                    .filter_map(|arg| Url::parse(&arg).ok());
                let startup = app.state::<Mutex<links::StartupLinks>>();
                let urls = startup.lock().unwrap().ready(initial);
                receive_links(app, urls);
            }
            #[cfg(target_os = "macos")]
            RunEvent::Ready => {
                receive_links(app, startup_links.ready([]));
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => reopen(app),
            _ => {}
        });
}
