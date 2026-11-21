#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
mod queue;

use queue::PendingLinks;
use serde::Serialize;
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tauri::{AppHandle, Emitter, Manager, State, Url, WindowEvent};

#[derive(Serialize)]
struct BrowserChoice {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct Browser {
    #[serde(flatten)]
    choice: BrowserChoice,
    icon: Option<String>,
}

#[derive(Serialize)]
struct Picker {
    id: u64,
    url: String,
    browsers: Vec<Browser>,
}

#[tauri::command]
fn picker_state(app: AppHandle, state: State<'_, Mutex<PendingLinks>>) -> Option<Picker> {
    let link = state.lock().unwrap().current().cloned()?;
    Some(Picker {
        id: link.id,
        url: link.url.to_string(),
        browsers: platform::browsers(&link.url, &app.config().identifier),
    })
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

fn finish_link(app: &AppHandle, id: u64, cancelled: bool) {
    let state = app.state::<Mutex<PendingLinks>>();
    let mut pending = state.lock().unwrap();
    if !pending.finish(id) {
        return;
    }
    let more_links = pending.current().is_some();
    drop(pending);

    if let Some(window) = app.get_webview_window("main") {
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
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            receive_arguments(app, args.into_iter().skip(1));
        }))
        .setup(|app| {
            receive_arguments(
                app.handle(),
                std::env::args_os()
                    .skip(1)
                    .filter_map(|arg| arg.into_string().ok()),
            );
            Ok(())
        });

    builder
        .manage(Mutex::new(PendingLinks::default()))
        .invoke_handler(tauri::generate_handler![
            picker_state,
            open_browser,
            cancel_link,
            copy_link,
            open_settings,
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
        .run(|_app, event| match event {
            #[cfg(target_os = "macos")]
            RunEvent::Opened { urls } => {
                receive_links(_app, urls);
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => show_window(_app),
            _ => {}
        });
}
