// @vitest-environment jsdom
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: "main" }) }));

const request = vi.mocked(invoke);
const link = {
  id: 7,
  url: "https://example.com/path?x=1&y=2",
  waiting: 0,
  browsers: [
    { id: "browser-one", name: "Browser One" },
    { id: "browser-two", name: "Browser Two" },
  ],
};
let emitQueueChange: () => void;
let emitDeferred: (count: number) => void;
let picker: typeof link | null;
let failedLaunch: boolean;

function key(name: string, options: KeyboardEventInit = {}) {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: name, ...options }));
}

beforeAll(async () => {
  document.body.innerHTML = '<main id="app"></main>';
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    if (event === "main-picker-changed") {
      emitQueueChange = () => handler({ event, id: 1, payload: undefined });
    } else if (event === "picker-deferred") {
      emitDeferred = (count) => handler({ event, id: 2, payload: count as never });
    }
    return () => {};
  });
  await import("../src/main");
});

beforeEach(async () => {
  picker = link;
  failedLaunch = false;
  request.mockReset();
  request.mockImplementation(async (command) => {
    if (command === "picker_state") return picker as never;
    if (command === "open_browser" && failedLaunch) throw "ブラウザを開けませんでした。";
    return undefined as never;
  });
  emitQueueChange();
  await vi.waitFor(() => expect(document.querySelector("h1")?.textContent).toBe("ブラウザを選択"));
});

describe("browser picker", () => {
  it("announces recovered links and displays the next link in the main window", async () => {
    emitDeferred(2);
    expect(document.querySelector(".waiting")?.textContent).toContain("2 件");
    expect(document.querySelector(".link-url")?.textContent).toBe(link.url);
    picker = { ...link, id: 8, url: "https://example.com/next", waiting: 1 };
    emitQueueChange();
    await vi.waitFor(() => {
      expect(document.querySelector(".link-url")?.textContent).toBe(picker?.url);
      expect(document.querySelector(".waiting")?.textContent).toContain("1 件");
    });
  });
  it("shows the link and opens the browser assigned to a number key", async () => {
    expect(document.querySelector(".subtitle")?.textContent).toBe(link.url);
    key("2");
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith("open_browser", { id: 7, browserId: "browser-two" }));
  });

  it("shows working browser buttons while the icons are still loading", async () => {
    let completeIcons!: (icons: Record<string, string>) => void;
    const pendingIcons = new Promise<Record<string, string>>((resolve) => {
      completeIcons = resolve;
    });
    request.mockImplementation(async (command) => {
      if (command === "picker_state") return picker as never;
      if (command === "picker_icons") return pendingIcons as never;
      return undefined as never;
    });
    emitQueueChange();
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith("picker_icons", { id: 7 }));
    expect(document.querySelector(".browser-icon")).toBeInstanceOf(HTMLSpanElement);
    key("1");
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith("open_browser", { id: 7, browserId: "browser-one" }));

    const source = "data:image/png;base64,iVBORw0KGgo=";
    completeIcons({ "browser-one": source });
    await vi.waitFor(() => expect(document.querySelector<HTMLImageElement>("img.browser-icon")?.src).toBe(source));
  });

  it("keeps the full URL in a keyboard-accessible region", async () => {
    const url = `https://example.com/${"a".repeat(3000)}?end=visible`;
    picker = { ...link, url };
    emitQueueChange();
    await vi.waitFor(() => {
      const region = document.querySelector<HTMLElement>('[aria-label="開くリンク"]');
      expect(region?.textContent).toBe(url);
      expect(region?.tabIndex).toBe(0);
    });
  });

  it("requests space for browser rows below the fold", async () => {
    const scrollHeight = vi.spyOn(HTMLElement.prototype, "scrollHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.classList.contains("browsers") ? 456 : 465;
      });
    const clientHeight = vi.spyOn(HTMLElement.prototype, "clientHeight", "get")
      .mockReturnValue(300);
    try {
      emitQueueChange();
      await vi.waitFor(() => expect(request).toHaveBeenCalledWith("fit_picker", { contentHeight: 621, id: 7 }));
    } finally {
      scrollHeight.mockRestore();
      clientHeight.mockRestore();
    }
  });

  it("ignores modifier shortcuts and cancels the active link on Escape", async () => {
    key("1", { metaKey: true });
    expect(request).not.toHaveBeenCalledWith("open_browser", expect.anything());
    key("Escape");
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith("cancel_link", { id: 7 }));
  });

  it("allows recovery after a browser launch fails", async () => {
    failedLaunch = true;
    key("1");
    await vi.waitFor(() => expect(document.querySelector(".status")?.textContent).toContain("開けません"));
    failedLaunch = false;
    key("2");
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith("open_browser", { id: 7, browserId: "browser-two" }));
  });

  it("allows copying a link even when no browsers are available", async () => {
    picker = { ...link, browsers: [] };
    emitQueueChange();
    await vi.waitFor(() => expect(document.querySelectorAll(".browser")).toHaveLength(0));
    document.querySelector<HTMLButtonElement>(".actions button")!.click();
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith("copy_link", { id: 7 }));
  });
});
