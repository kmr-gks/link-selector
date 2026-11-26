import { invoke } from "@tauri-apps/api/core";
import type { EventCallback } from "@tauri-apps/api/event";
import { listenPickerEvent } from "./picker-events";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./style.css";

interface Browser {
  id: string;
  name: string;
}

interface Picker {
  id: number;
  url: string;
  browsers: Browser[];
  waiting: number;
}

const isWindows = navigator.userAgent.includes("Windows");
const app = document.querySelector<HTMLElement>("#app")!;
const isMainWindow = getCurrentWindow().label === "main";
let current: Picker | null = null;
let requestNumber = 0;
let launchingLinkId: number | undefined;
let statusMessage = "";
let closingLinkId: number | undefined;
let deferredDuringRefresh: Map<number, number> | undefined;
let listenersReady = false;

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  content = "",
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  node.textContent = content;
  return node;
}

function button(label: string, className: string, action: () => void) {
  const node = element("button", className, label);
  node.type = "button";
  node.addEventListener("click", action);
  return node;
}

function showLoadError(message: string, retry: () => Promise<void>) {
  app.classList.remove("picker");
  const error = element("p", "instructions", message);
  error.setAttribute("role", "alert");
  const reload = button("再読み込み", "primary", () => {
    reload.disabled = true;
    void retry();
  });
  const close = button("閉じる", "secondary close-load-error", () => {
    void invoke("close_picker_error").catch(() => {});
  });
  const actions = element("div", "actions");
  actions.append(reload, close);
  app.replaceChildren(error, actions);
}

async function refresh() {
  const request = ++requestNumber;
  const notifications = new Map<number, number>();
  deferredDuringRefresh = notifications;
  try {
    const view = await invoke<Picker | null>("picker_state").catch((error) => {
      if (request !== requestNumber) throw error;
      return invoke<Picker | null>("picker_state");
    });
    if (request !== requestNumber) return;
    if (view) {
      view.waiting = Math.max(view.waiting, notifications.get(view.id) ?? 0);
    }
    if (view?.id !== closingLinkId) closingLinkId = undefined;
    if (view?.id !== current?.id) statusMessage = "";
    current = view;
    render();
  } catch {
    if (request !== requestNumber) return;
    current = null;
    statusMessage = "";
    showLoadError("選択画面を読み込めませんでした。", refresh);
  } finally {
    if (deferredDuringRefresh === notifications) deferredDuringRefresh = undefined;
  }
}

function browserChoicesDisabled(id: number) {
  return id === launchingLinkId || id === closingLinkId;
}

function updatePickerActions(id: number) {
  const disabled = browserChoicesDisabled(id);
  app.querySelectorAll<HTMLButtonElement>(".browser").forEach((button) => {
    button.disabled = disabled;
  });
  const copy = app.querySelector<HTMLButtonElement>(".copy-link");
  if (copy) copy.disabled = id === closingLinkId;
}

async function processLink(command: "open_browser" | "cancel_link", browser?: Browser) {
  if (!current || current.id === closingLinkId || (current.id === launchingLinkId && command === "open_browser")) return;
  const { id } = current;
  if (command === "open_browser") {
    launchingLinkId = id;
    setStatus("");
  } else {
    closingLinkId = id;
  }
  updatePickerActions(id);
  try {
    await invoke(command, { id, browserId: browser?.id });
    if (command === "open_browser" && current?.id === id) closingLinkId = id;
  } catch (error) {
    if (command === "cancel_link" && closingLinkId === id) closingLinkId = undefined;
    if (current?.id === id) statusMessage = String(error);
  } finally {
    if (command === "open_browser" && launchingLinkId === id) launchingLinkId = undefined;
    if (current?.id === id) {
      updatePickerActions(id);
      updatePickerMessage(".status", statusText());
    }
  }
}

async function copyLink(id: number) {
  if (current?.id !== id || id === closingLinkId) return;
  try {
    await invoke("copy_link", { id });
    if (current?.id === id) setStatus("リンクをコピーしました。");
  } catch (error) {
    if (current?.id === id) setStatus(String(error));
  }
}

function statusText() {
  if (!current || current.id !== launchingLinkId) return statusMessage;
  const progress = "ブラウザを起動しています。画面を閉じた後も起動が続く場合があります。";
  return statusMessage ? `${progress} ${statusMessage}` : progress;
}

function setStatus(message: string) {
  statusMessage = message;
  updatePickerMessage(".status", statusText());
}

function updatePickerMessage(selector: string, message: string) {
  const element = app.querySelector(selector);
  if (!element || element.textContent === message) return;
  element.textContent = message;
  if (current) {
    fitPicker(app.querySelector<HTMLUListElement>(".browsers") ?? undefined, true);
  }
}

function waitingMessage(waiting: number) {
  return waiting > 0 ? `追加の画面を開けなかったリンク ${waiting} 件を、この画面で順番に表示します。` : "";
}

function openSettings() {
  void invoke("open_settings").catch((error) => setStatus(String(error)));
}

function heading(title: string, description: string, isLink = false) {
  const header = element("header", "heading");
  const mark = element("span", "mark", "↗");
  mark.setAttribute("aria-hidden", "true");
  const text = element("div", "heading-text");
  const subtitle = element("p", "subtitle", description);
  if (isLink) {
    subtitle.classList.add("link-url");
    subtitle.tabIndex = 0;
    subtitle.setAttribute("role", "region");
    subtitle.setAttribute("aria-label", "開くリンク");
  }
  text.append(element("h1", "", title), subtitle);
  header.append(mark, text);
  return header;
}

function fitPicker(list?: HTMLUListElement, growOnly?: true) {
  // A scrollable list's hidden rows do not contribute to the main element's height.
  const contentHeight = list
    ? app.scrollHeight + list.scrollHeight - list.clientHeight
    : current ? app.scrollHeight : null;
  void invoke("fit_picker", {
    contentHeight,
    viewportHeight: window.innerHeight,
    id: current?.id,
    ...(growOnly && { growOnly }),
  }).catch(() => {
    // The layout still allows scrolling if the OS cannot resize the window.
  });
}

function render() {
  const wasPicker = app.classList.contains("picker");
  app.classList.toggle("picker", current !== null);
  const status = element("p", "status");
  status.setAttribute("role", "status");
  if (!current) {
    if (!isMainWindow) {
      status.textContent = "リンクを読み込めませんでした。";
      app.replaceChildren(status);
      return;
    }
    app.replaceChildren(
      heading("Link Selector", "リンクを開くブラウザを、その都度選べます。"),
      element("p", "instructions", isWindows
        ? "設定の「アプリ」→「既定のアプリ」で Link Selector を選び、HTTP と HTTPS の関連付けを設定してください。"
        : "システム設定の「デスクトップとDock」で、デフォルトのWebブラウザに Link Selector を選んでください。"),
      button("システム設定を開く", "primary", openSettings),
      status,
    );
    if (wasPicker) fitPicker();
    return;
  }

  const { id, url, browsers } = current;
  status.textContent = statusText();
  const list = element("ul", "browsers");
  list.setAttribute("aria-label", "インストール済みブラウザ");
  const icons = new Map<string, HTMLElement>();
  for (const [index, browser] of browsers.entries()) {
    const row = element("li", "");
    const choice = button("", "browser", () => void processLink("open_browser", browser));
    choice.disabled = browserChoicesDisabled(id);
    const icon = element("span", "browser-icon", "↗");
    icon.setAttribute("aria-hidden", "true");
    icons.set(browser.id, icon);
    choice.append(icon, element("span", "browser-name", browser.name));
    if (index < 9) {
      const shortcut = element("kbd", "", String(index + 1));
      shortcut.setAttribute("aria-hidden", "true");
      choice.setAttribute("aria-keyshortcuts", String(index + 1));
      choice.append(shortcut);
    }
    row.append(choice);
    list.append(row);
  }
  const footer = element("footer", "actions");
  const copy = button("リンクをコピー", "secondary copy-link", () => void copyLink(id));
  copy.disabled = id === closingLinkId;
  footer.append(
    copy,
    button("キャンセル", "secondary", () => void processLink("cancel_link")),
  );
  const waiting = element("p", "waiting", waitingMessage(current.waiting));
  waiting.setAttribute("role", "status");
  app.replaceChildren(
    heading("ブラウザを選択", url, true),
    browsers.length ? list : element("p", "instructions", "このリンクを開けるブラウザが見つかりませんでした。"),
    waiting,
    status,
    footer,
  );
  if (isMainWindow) acknowledgePicker(id);
  fitPicker(browsers.length ? list : undefined);
  if (!isWindows && icons.size > 0) {
    // Allow a paint with the browser names and buttons before decoding icons.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      if (current?.id === id) void loadIcons(id, icons);
    }));
  }
}

function acknowledgePicker(id: number, delay = 250) {
  if (current?.id !== id) return;
  void invoke("picker_rendered", { id }).catch(() => {
    window.setTimeout(() => acknowledgePicker(id, Math.min(delay * 2, 4000)), delay);
  });
}

async function loadIcons(id: number, placeholders: Map<string, HTMLElement>) {
  try {
    const icons = await invoke<Record<string, string>>("picker_icons", { id });
    for (const [browserId, source] of Object.entries(icons)) {
      const placeholder = placeholders.get(browserId);
      if (!placeholder?.isConnected) continue;
      const icon = element("img", "browser-icon");
      icon.src = source;
      icon.alt = "";
      placeholder.replaceWith(icon);
    }
  } catch {
    // Keep the placeholder if native icon loading fails.
  }
}

window.addEventListener("keydown", (event) => {
  if (event.repeat || event.isComposing || event.metaKey || event.ctrlKey || event.altKey) return;
  if (!current) {
    if (event.key === "Escape") app.querySelector<HTMLButtonElement>(".close-load-error")?.click();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    void processLink("cancel_link");
  } else if (/^[1-9]$/.test(event.key)) {
    const browser = current.browsers[Number(event.key) - 1];
    if (browser) {
      event.preventDefault();
      void processLink("open_browser", browser);
    }
  }
});

function subscribeOnce<T>(event: string, handler: EventCallback<T>, onSubscribed?: () => void) {
  let subscription: Promise<unknown> | undefined;
  return () => subscription ??= listenPickerEvent(event, handler)
    .catch(() => listenPickerEvent(event, handler))
    .catch((error) => {
      subscription = undefined;
      throw error;
    })
    .then(onSubscribed);
}

const subscribeToPickerChanges = subscribeOnce<number | null>("main-picker-changed", ({ payload: finishedId }) => {
  if (current && current.id === finishedId) {
    closingLinkId = finishedId;
    updatePickerActions(finishedId);
  }
  if (listenersReady) void refresh();
});
type Waiting = [id: number, count: number];

function updateWaiting([id, count]: Waiting) {
  if (deferredDuringRefresh) {
    deferredDuringRefresh.set(id, Math.max(count, deferredDuringRefresh.get(id) ?? 0));
  }
  if (current?.id !== id) return;
  current.waiting = Math.max(current.waiting, count);
  updatePickerMessage(".waiting", waitingMessage(current.waiting));
}

function reconcileWaiting(delay = 250) {
  void invoke<Waiting | null>("picker_waiting").then((waiting) => {
    if (waiting) updateWaiting(waiting);
  }).catch(() => {
    window.setTimeout(() => reconcileWaiting(Math.min(delay * 2, 4000)), delay);
  });
}

const subscribeToDeferred = subscribeOnce<Waiting>("picker-deferred", ({ payload }) => {
  updateWaiting(payload);
}, () => {
  // If the first snapshot already started, reconcile links queued before registration.
  if (listenersReady) reconcileWaiting();
});

function recoverDeferredSubscription(delay = 250) {
  void subscribeToDeferred().catch(() => {
    window.setTimeout(() => recoverDeferredSubscription(Math.min(delay * 2, 4000)), delay);
  });
}

async function initializePicker() {
  try {
    await subscribeToPickerChanges();
  } catch {
    showLoadError("選択画面を初期化できませんでした。", initializePicker);
    return;
  }
  listenersReady = true;
  await refresh();
}

if (isMainWindow) {
  render();
  recoverDeferredSubscription();
  void initializePicker();
} else {
  const status = element("p", "status");
  status.setAttribute("role", "status");
  app.append(status);
  void refresh();
}
