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
const windowLabel = getCurrentWindow().label;
const isMainWindow = windowLabel === "main";
let current: Picker | null = null;
let requestNumber = 0;
let launchingLinkId: number | undefined;
let statusMessage = "";
let closingLinkId: number | undefined;
let deferredDuringRefresh: Map<number, number> | undefined;
let listenersReady = false;
let initializing = false;
let initializationTimeout: number | undefined;
let pickerEffects: {
  fit: ReturnType<typeof retryable>;
  grow: ReturnType<typeof retryable>;
  icons?: ReturnType<typeof retryable>;
} | undefined;

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
  pickerEffects = undefined;
  app.classList.remove("picker");
  const error = element("p", "instructions", message);
  error.setAttribute("role", "alert");
  const reload = button("再読み込み", "primary", () => {
    reload.disabled = true;
    void retry().finally(() => {
      reload.disabled = false;
    });
  });
  const close = button("閉じる", "secondary close-load-error", () => {
    void invoke("close_picker_error").catch(() => {});
  });
  const actions = element("div", "actions");
  actions.append(reload, close);
  app.replaceChildren(error, actions);
}

function retryPickerEffects() {
  pickerEffects?.fit.retry();
  pickerEffects?.grow.retry();
  pickerEffects?.icons?.retry();
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
    const previous = current;
    if (view && previous && previous.id === view.id && previous.url === view.url &&
      previous.browsers.length === view.browsers.length &&
      view.browsers.every((browser, index) => browser.id === previous.browsers[index].id &&
        browser.name === previous.browsers[index].name)) {
      updateWaiting([view.id, view.waiting]);
      retryPickerEffects();
      return;
    }
    if (view?.id !== closingLinkId) closingLinkId = undefined;
    if (view?.id !== current?.id) statusMessage = "";
    current = view;
    render();
  } catch {
    if (request !== requestNumber) return;
    if (current && current.id !== closingLinkId) {
      retryPickerEffects();
      return;
    }
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
  pickerEffects?.grow.run();
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

function retryable(action: () => Promise<unknown>) {
  let attempt = 0;
  let failed = false;
  let running = false;
  let retryRequested = false;
  function run() {
    const request = ++attempt;
    failed = false;
    running = true;
    retryRequested = false;
    void action().then(() => {
      if (request === attempt) running = false;
    }, () => {
      if (request !== attempt) return;
      running = false;
      failed = true;
      if (retryRequested) run();
    });
  }
  return {
    run,
    retry: () => {
      if (failed) run();
      else if (running) retryRequested = true;
    },
  };
}

function fitPicker(list?: HTMLUListElement, growOnly?: true, id = current?.id) {
  if (id !== current?.id) return Promise.resolve();
  // A scrollable list's hidden rows do not contribute to the main element's height.
  const contentHeight = list
    ? app.scrollHeight + list.scrollHeight - list.clientHeight
    : current ? app.scrollHeight : null;
  return invoke("fit_picker", {
    contentHeight,
    viewportHeight: window.innerHeight,
    id,
    ...(growOnly && { growOnly }),
  });
}

function render() {
  pickerEffects = undefined;
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
    if (wasPicker) void fitPicker().catch(() => {});
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
  const browserList = browsers.length ? list : undefined;
  const effects = {
    fit: retryable(() => fitPicker(browserList, undefined, id)),
    grow: retryable(() => fitPicker(browserList, true, id)),
    icons: !isWindows && icons.size ? retryable(() => loadIcons(id, icons)) : undefined,
  };
  pickerEffects = effects;
  effects.fit.run();
  if (effects.icons) {
    // Allow a paint with the browser names and buttons before decoding icons.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      if (pickerEffects === effects) effects.icons?.run();
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
  if (current?.id !== id) return;
  const icons = await invoke<Record<string, string>>("picker_icons", { id });
  for (const [browserId, source] of Object.entries(icons)) {
    const placeholder = placeholders.get(browserId);
    if (!placeholder?.isConnected) continue;
    const icon = element("img", "browser-icon");
    icon.src = source;
    icon.alt = "";
    placeholder.replaceWith(icon);
    placeholders.delete(browserId);
  }
  if (current?.id === id && placeholders.size) throw new Error("Some browser icons are unavailable");
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

function subscribeOnce<T>(event: string, handler: EventCallback<T>, onSubscribed?: () => void, label?: string) {
  let subscription: Promise<unknown> | undefined;
  const subscribe = () => listenPickerEvent(event, handler, label);
  return () => subscription ??= subscribe()
    .catch(subscribe)
    .catch((error) => {
      subscription = undefined;
      throw error;
    })
    .then(onSubscribed);
}

const subscribeToPickerChanges = subscribeOnce<number | null>("picker-changed", ({ payload: finishedId }) => {
  if (current && current.id === finishedId) {
    closingLinkId = finishedId;
    updatePickerActions(finishedId);
  }
  if (listenersReady) void refresh();
}, undefined, windowLabel);
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

async function initializePicker(delay = 250) {
  if (listenersReady || initializing) return;
  window.clearTimeout(initializationTimeout);
  initializing = true;
  try {
    await subscribeToPickerChanges();
  } catch {
    if (isMainWindow && !app.querySelector(".close-load-error")) {
      showLoadError("選択画面を初期化できませんでした。", initializePicker);
    }
    initializationTimeout = window.setTimeout(() => {
      void initializePicker(Math.min(delay * 2, 4000));
    }, delay);
    return;
  } finally {
    initializing = false;
  }
  listenersReady = true;
  if (isMainWindow) await refresh();
}

if (isMainWindow) {
  render();
  recoverDeferredSubscription();
} else {
  const status = element("p", "status");
  status.setAttribute("role", "status");
  app.append(status);
  void refresh();
}

window.addEventListener("focus", () => {
  if (!listenersReady) void initializePicker();
});
void initializePicker();
