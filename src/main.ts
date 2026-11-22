import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
let busy = false;
let deferred: [id: number, count: number] | undefined;

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

async function refresh() {
  const request = ++requestNumber;
  try {
    const view = await invoke<Picker | null>("picker_state");
    if (request !== requestNumber) return;
    if (view && deferred?.[0] === view.id) {
      view.waiting = Math.max(view.waiting, deferred[1]);
    }
    current = view;
    busy = false;
    render();
  } catch {
    if (request === requestNumber) setStatus("選択画面を読み込めませんでした。");
  }
}

async function processLink(command: "open_browser" | "cancel_link", browser?: Browser) {
  if (!current || (busy && command === "open_browser")) return;
  const { id } = current;
  if (command === "open_browser") {
    busy = true;
    setStatus("ブラウザを起動しています。画面を閉じた後も起動が続く場合があります。");
  }
  try {
    await invoke(command, { id, browserId: browser?.id });
  } catch (error) {
    if (current?.id === id) setStatus(String(error));
  } finally {
    if (command === "open_browser" && current?.id === id) busy = false;
  }
}

async function copyLink(id: number) {
  try {
    await invoke("copy_link", { id });
    if (current?.id === id) setStatus("リンクをコピーしました。");
  } catch (error) {
    if (current?.id === id) setStatus(String(error));
  }
}

function setStatus(message: string) {
  const status = document.querySelector<HTMLElement>(".status");
  if (status) status.textContent = message;
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

function fitPicker(list?: HTMLUListElement) {
  // A scrollable list's hidden rows do not contribute to the main element's height.
  const contentHeight = list
    ? app.scrollHeight + list.scrollHeight - list.clientHeight
    : current ? app.scrollHeight : null;
  void invoke("fit_picker", { contentHeight, id: current?.id }).catch(() => {
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
  const list = element("ul", "browsers");
  list.setAttribute("aria-label", "インストール済みブラウザ");
  const icons = new Map<string, HTMLElement>();
  for (const [index, browser] of browsers.entries()) {
    const row = element("li", "");
    const choice = button("", "browser", () => void processLink("open_browser", browser));
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
  footer.append(
    button("リンクをコピー", "secondary", () => void copyLink(id)),
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
  fitPicker(browsers.length ? list : undefined);
  if (!isWindows && icons.size > 0) {
    // Allow a paint with the browser names and buttons before decoding icons.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      if (current?.id === id) void loadIcons(id, icons);
    }));
  }
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
  if (!current || event.repeat || event.isComposing || event.metaKey || event.ctrlKey || event.altKey) return;
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

if (isMainWindow) {
  render();
} else {
  const status = element("p", "status");
  status.setAttribute("role", "status");
  app.append(status);
}
const ready = isMainWindow
  ? Promise.all([
    listen("main-picker-changed", () => void refresh()),
    listen<[number, number]>("picker-deferred", ({ payload }) => {
      const [id, count] = payload;
      if (!deferred || id > deferred[0] || (id === deferred[0] && count > deferred[1])) {
        deferred = payload;
      }
      if (current?.id !== id) return;
      current.waiting = Math.max(current.waiting, count);
      const waiting = app.querySelector(".waiting");
      if (waiting) waiting.textContent = waitingMessage(current.waiting);
    }),
  ])
  : Promise.resolve();
void ready.then(refresh).catch(() => setStatus("選択画面を初期化できませんでした。"));
