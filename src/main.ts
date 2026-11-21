import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./style.css";

interface Browser {
  id: string;
  name: string;
  icon: string | null;
}

interface Picker {
  id: number;
  url: string;
  browsers: Browser[];
}

const isWindows = navigator.userAgent.includes("Windows");
const app = document.querySelector<HTMLElement>("#app")!;
let current: Picker | null = null;
let requestNumber = 0;
let busy = false;

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
    current = view;
    busy = false;
    render();
  } catch {
    if (request === requestNumber) setStatus("選択画面を読み込めませんでした。");
  }
}

async function processLink(command: "open_browser" | "cancel_link", browser?: Browser) {
  if (!current || busy) return;
  const { id } = current;
  busy = true;
  setStatus("");
  try {
    await invoke(command, { id, browserId: browser?.id });
  } catch (error) {
    if (current?.id === id) setStatus(String(error));
  } finally {
    if (current?.id === id) busy = false;
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

function openSettings() {
  void invoke("open_settings").catch((error) => setStatus(String(error)));
}

function heading(title: string, description: string) {
  const header = element("header", "heading");
  const mark = element("span", "mark", "↗");
  mark.setAttribute("aria-hidden", "true");
  const text = element("div", "heading-text");
  const subtitle = element("p", "subtitle", description);
  subtitle.title = description;
  text.append(element("h1", "", title), subtitle);
  header.append(mark, text);
  return header;
}

function render() {
  const status = element("p", "status");
  status.setAttribute("role", "status");
  if (!current) {
    app.replaceChildren(
      heading("Link Selector", "リンクを開くブラウザを、その都度選べます。"),
      element("p", "instructions", isWindows
        ? "設定の「アプリ」→「既定のアプリ」で Link Selector を選び、HTTP と HTTPS の関連付けを設定してください。"
        : "システム設定の「デスクトップとDock」で、デフォルトのWebブラウザに Link Selector を選んでください。"),
      button("システム設定を開く", "primary", openSettings),
      status,
    );
    return;
  }

  const { id, url, browsers } = current;
  const list = element("ul", "browsers");
  list.setAttribute("aria-label", "インストール済みブラウザ");
  for (const [index, browser] of browsers.entries()) {
    const row = element("li", "");
    const choice = button("", "browser", () => void processLink("open_browser", browser));
    const icon = browser.icon ? element("img", "browser-icon") : element("span", "browser-icon", "↗");
    if (icon instanceof HTMLImageElement) {
      icon.src = browser.icon!;
      icon.alt = "";
    } else {
      icon.setAttribute("aria-hidden", "true");
    }
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
  app.replaceChildren(
    heading("ブラウザを選択", url),
    browsers.length ? list : element("p", "instructions", "このリンクを開けるブラウザが見つかりませんでした。"),
    status,
    footer,
  );
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

render();
void listen("link-queue-changed", () => void refresh())
  .then(refresh)
  .catch(() => setStatus("選択画面を初期化できませんでした。"));
