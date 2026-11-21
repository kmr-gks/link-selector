mod macos;

use serde::Serialize;
use std::{collections::VecDeque, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, Url, WindowEvent};

#[derive(Default)]
struct PendingLinks {
    next_id: u64,
    links: VecDeque<Link>,
}

#[derive(Clone)]
struct Link {
    id: u64,
    url: Url,
}

#[derive(Serialize)]
struct Picker {
    id: u64,
    url: String,
    browsers: Vec<macos::Browser>,
}

#[tauri::command]
fn picker_state(app: AppHandle, state: State<'_, Mutex<PendingLinks>>) -> Option<Picker> {
    let link = state.lock().unwrap().links.front().cloned()?;
    Some(Picker {
        id: link.id,
        url: link.url.to_string(),
        browsers: macos::browsers(&link.url, &app.config().identifier),
    })
}

#[tauri::command]
fn open_browser(
    app: AppHandle,
    state: State<'_, Mutex<PendingLinks>>,
    id: u64,
    browser_path: String,
) -> Result<(), String> {
    let link = state
        .lock()
        .unwrap()
        .links
        .front()
        .filter(|link| link.id == id)
        .cloned()
        .ok_or("このリンクは処理済みです。")?;
    let path = macos::browser_paths(&link.url, &app.config().identifier)
        .into_iter()
        .find(|browser| browser.path == browser_path)
        .ok_or("ブラウザが見つかりませんでした。別のブラウザを選んでください。")?;
    macos::open_in_browser(&link.url, &path).map_err(|_| {
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
        .links
        .front()
        .filter(|link| link.id == id)
        .ok_or("このリンクは処理済みです。")?;
    macos::copy(link.url.as_str()).map_err(|_| "リンクをコピーできませんでした。".to_owned())
}

#[tauri::command]
fn open_settings() -> Result<(), String> {
    macos::open_settings().map_err(|_| "システム設定を開けませんでした。".to_owned())
}

fn finish_link(app: &AppHandle, id: u64, cancelled: bool) {
    let state = app.state::<Mutex<PendingLinks>>();
    let mut pending = state.lock().unwrap();
    if pending.links.front().is_none_or(|link| link.id != id) {
        return;
    }
    pending.links.pop_front();
    let more_links = !pending.links.is_empty();
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

fn receive_links(app: &AppHandle, urls: Vec<Url>) {
    let state = app.state::<Mutex<PendingLinks>>();
    let mut pending = state.lock().unwrap();
    let old_len = pending.links.len();
    for url in urls {
        if matches!(url.scheme(), "http" | "https") && url.has_host() {
            pending.next_id += 1;
            let id = pending.next_id;
            pending.links.push_back(Link { id, url });
        }
    }
    let added_links = pending.links.len() > old_len;
    drop(pending);
    if added_links {
        if old_len == 0 {
            let _ = app.emit("link-queue-changed", ());
        }
        show_window(app);
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    tauri::Builder::default()
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
                let id = state.lock().unwrap().links.front().map(|link| link.id);
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
            RunEvent::Opened { urls } => receive_links(app, urls),
            RunEvent::Reopen { .. } => show_window(app),
            _ => {}
        });
}
