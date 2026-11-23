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
let emitDeferred: (id: number, count: number) => void;
let picker: typeof link | null;
let failedLaunch: boolean;

function key(name: string, options: KeyboardEventInit = {}) {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: name, ...options }));
}

describe("browser picker", () => {
  beforeAll(async () => {
    document.body.innerHTML = '<main id="app"></main>';
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "main-picker-changed") {
        emitQueueChange = () => handler({ event, id: 1, payload: undefined });
      } else if (event === "picker-deferred") {
        emitDeferred = (id, count) => handler({ event, id: 2, payload: [id, count] as never });
      }
      return () => {};
    });
    await import("../src/main");
  });

  beforeEach(async () => {
    picker = { ...link };
    failedLaunch = false;
    request.mockReset();
    request.mockImplementation(async (command) => {
      if (command === "picker_state") return picker as never;
      if (command === "open_browser" && failedLaunch) throw "ブラウザを開けませんでした。";
      if (command === "cancel_link") {
        picker = null;
        emitQueueChange();
      }
      return undefined as never;
    });
    emitQueueChange();
    await vi.waitFor(() => expect(document.querySelector("h1")?.textContent).toBe("ブラウザを選択"));
  });

  it("announces recovered links and displays the next link in the main window", async () => {
    emitDeferred(7, 2);
    expect(document.querySelector(".waiting")?.textContent).toContain("2 件");
    expect(document.querySelector(".link-url")?.textContent).toBe(link.url);
    picker = { ...link, id: 8, url: "https://example.com/next", waiting: 1 };
    emitQueueChange();
    await vi.waitFor(() => {
      expect(document.querySelector(".link-url")?.textContent).toBe(picker?.url);
      expect(document.querySelector(".waiting")?.textContent).toContain("1 件");
    });
  });
  it("keeps a notification that arrives before the initial response renders", async () => {
    let resolvePicker!: (value: typeof link) => void;
    const response = new Promise<typeof link>((resolve) => { resolvePicker = resolve; });
    request.mockImplementation(async (command) => {
      if (command === "picker_state") return response as never;
      return undefined as never;
    });
    emitQueueChange();
    emitDeferred(9, 3);
    resolvePicker({ ...link, id: 9, url: "https://example.com/recovered", waiting: 0 });
    await vi.waitFor(() => {
      expect(document.querySelector(".link-url")?.textContent).toBe("https://example.com/recovered");
      expect(document.querySelector(".waiting")?.textContent).toContain("3 件");
    });
    emitDeferred(7, 4);
    expect(document.querySelector(".waiting")?.textContent).toContain("3 件");
  });

  it("preserves an event for an older link promoted after a newer one", async () => {
    picker = { ...link, id: 12, url: "https://example.com/newer", waiting: 0 };
    emitQueueChange();
    await vi.waitFor(() => expect(document.querySelector(".link-url")?.textContent).toBe(picker?.url));
    emitDeferred(12, 1);

    let resolvePicker!: (value: typeof link) => void;
    const response = new Promise<typeof link>((resolve) => { resolvePicker = resolve; });
    request.mockImplementation(async (command) => command === "picker_state" ? response as never : undefined as never);
    emitQueueChange();
    emitDeferred(11, 1);
    emitDeferred(12, 2);
    resolvePicker({ ...link, id: 11, url: "https://example.com/older", waiting: 0 });
    await vi.waitFor(() => expect(document.querySelector(".link-url")?.textContent).toBe("https://example.com/older"));
    expect(document.querySelector(".waiting")?.textContent).toContain("1 件");
  });

  it("can dismiss the picker while the browser launch is still awaiting a response", async () => {
    let completeLaunch!: () => void;
    const launch = new Promise<void>((resolve) => { completeLaunch = resolve; });
    request.mockImplementation(async (command) => {
      if (command === "picker_state") return picker as never;
      if (command === "open_browser") return launch as never;
      if (command === "cancel_link") {
        picker = null;
        emitQueueChange();
      }
      return undefined as never;
    });
    key("1");
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith("open_browser", { id: 7, browserId: "browser-one" }));
    key("2");
    expect(request.mock.calls.filter(([command]) => command === "open_browser")).toHaveLength(1);
    key("Escape");
    await vi.waitFor(() => expect(document.querySelector(".link-url")).toBeNull());
    expect(request).toHaveBeenCalledWith("cancel_link", { id: 7, browserId: undefined });
    completeLaunch();
  });

  it("blocks browser choices and repeated cancellation until the cancelled picker is replaced", async () => {
    let completeCancellation!: () => void;
    const cancellation = new Promise<void>((resolve) => { completeCancellation = resolve; });
    request.mockImplementation((command) => {
      if (command === "picker_state") return Promise.resolve(picker as never);
      if (command === "cancel_link") return cancellation as never;
      return Promise.resolve(undefined as never);
    });

    key("Escape");
    key("Escape");
    key("1");
    document.querySelector<HTMLButtonElement>(".browser")!.click();
    expect(request.mock.calls.filter(([command]) => command === "cancel_link")).toHaveLength(1);
    expect(request).not.toHaveBeenCalledWith("open_browser", expect.anything());

    completeCancellation();
    await cancellation;
    key("1");
    expect(request).not.toHaveBeenCalledWith("open_browser", expect.anything());

    picker = { ...link, id: 8, url: "https://example.com/promoted" };
    emitQueueChange();
    await vi.waitFor(() => expect(document.querySelector(".link-url")?.textContent).toBe(picker?.url));
    key("1");
    expect(request).toHaveBeenCalledWith("open_browser", { id: 8, browserId: "browser-one" });
  });

  it("allows selection again if cancelling the current link fails", async () => {
    request.mockRejectedValueOnce(new Error("キャンセルできませんでした。"));
    key("Escape");
    await vi.waitFor(() => expect(document.querySelector(".status")?.textContent).toContain("キャンセルできませんでした。"));
    key("1");
    expect(request).toHaveBeenCalledWith("open_browser", { id: 7, browserId: "browser-one" });
  });

  it("loads the promoted link after a temporary state error", async () => {
    let failNextRead = false;
    request.mockImplementation(async (command) => {
      if (command === "cancel_link") {
        picker = { ...link, id: 10, url: "https://example.com/promoted" };
        failNextRead = true;
        emitQueueChange();
      }
      if (command === "picker_state") {
        if (failNextRead) {
          failNextRead = false;
          throw new Error("read temporarily unavailable");
        }
        return picker as never;
      }
      return undefined as never;
    });
    key("Escape");
    await vi.waitFor(() => expect(document.querySelector(".link-url")?.textContent).toBe("https://example.com/promoted"));
    key("1");
    expect(request).toHaveBeenCalledWith("open_browser", { id: 10, browserId: "browser-one" });
  });

  it("offers a retry when the picker state remains unavailable", async () => {
    let unavailable = true;
    request.mockImplementation(async (command) => {
      if (command === "picker_state") {
        if (unavailable) throw new Error("state unavailable");
        return { ...link, id: 11 } as never;
      }
      return undefined as never;
    });
    emitQueueChange();
    await vi.waitFor(() => expect(document.querySelector('[role="alert"]')).not.toBeNull());
    key("1");
    expect(request).not.toHaveBeenCalledWith("open_browser", expect.anything());
    unavailable = false;
    document.querySelector<HTMLButtonElement>("button.primary")!.click();
    await vi.waitFor(() => expect(document.querySelector(".link-url")).not.toBeNull());
    key("1");
    expect(request).toHaveBeenCalledWith("open_browser", { id: 11, browserId: "browser-one" });
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

  it("requests more height when a later notice reduces the list viewport", () => {
    const scrollHeight = vi.spyOn(HTMLElement.prototype, "scrollHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.classList.contains("browsers") ? 423 : 663;
      });
    const clientHeight = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
    try {
      request.mockClear();
      emitDeferred(7, 1);
      emitDeferred(7, 1);
      expect(request).toHaveBeenCalledExactlyOnceWith("fit_picker", { contentHeight: 686, viewportHeight: window.innerHeight, id: 7, growOnly: true });
    } finally {
      scrollHeight.mockRestore();
      clientHeight.mockRestore();
    }
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
      await vi.waitFor(() => expect(request).toHaveBeenCalledWith("fit_picker", { contentHeight: 621, viewportHeight: window.innerHeight, id: 7 }));
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

describe("main picker initialization", () => {
  const queueChanged = "main-picker-changed";
  const pickerDeferred = "picker-deferred";
  const subscribe = vi.mocked(listen);
  const subscriptions = new Map<string, (payload: unknown) => void>();
  let failEvent: string;
  let failuresRemaining: number;

  beforeEach(() => {
    vi.resetModules();
    document.body.innerHTML = '<main id="app"></main>';
    subscriptions.clear();
    subscribe.mockReset();
    subscribe.mockImplementation(async (event, handler) => {
      if (event === failEvent && failuresRemaining > 0) {
        failuresRemaining--;
        throw new Error("event subscription unavailable");
      }
      subscriptions.set(event, (payload) => handler({ event, id: 1, payload: payload as never }));
      return () => {};
    });
    request.mockReset();
    request.mockImplementation(async (command) => {
      if (command === "picker_state") return { ...link, browsers: [], waiting: 2 } as never;
      return undefined as never;
    });
  });

  it.each([queueChanged, pickerDeferred])("retries a transient failure in %s", async (event) => {
    failEvent = event;
    failuresRemaining = 1;
    await import("../src/main");
    await vi.waitFor(() => expect(document.querySelector(".link-url")?.textContent).toBe(link.url));

    expect(subscribe.mock.calls.filter(([name]) => name === event)).toHaveLength(2);
    expect(subscribe.mock.calls.filter(([name]) => name !== event)).toHaveLength(1);
    expect(request).toHaveBeenCalledWith("picker_state");
  });

  it("keeps showing links when only the deferred notification fails", async () => {
    failEvent = pickerDeferred;
    failuresRemaining = Infinity;
    request.mockResolvedValueOnce(link as never);
    await import("../src/main");
    await vi.waitFor(() => expect(document.querySelector("button.browser")).not.toBeNull());
    request.mockResolvedValueOnce({ ...link, id: 8, url: "https://example.com/next" } as never);
    subscriptions.get(queueChanged)!(undefined);
    await vi.waitFor(() => expect(document.querySelector(".link-url")?.textContent).toBe("https://example.com/next"));
  });

  it("recovers manually after a persistent failure while retaining the working subscription", async () => {
    failEvent = queueChanged;
    failuresRemaining = Infinity;
    await import("../src/main");
    await vi.waitFor(() => {
      expect(document.querySelector('[role="alert"]')?.textContent).toBe("選択画面を初期化できませんでした。");
    });
    expect(request).not.toHaveBeenCalledWith("picker_state");

    failuresRemaining = 0;
    const retry = document.querySelector<HTMLButtonElement>("button.primary")!;
    expect(retry.textContent).toBe("再読み込み");
    retry.click();
    retry.click();
    await vi.waitFor(() => expect(document.querySelector(".link-url")?.textContent).toBe(link.url));
    expect(subscribe.mock.calls.filter(([name]) => name === queueChanged)).toHaveLength(3);
    expect(subscribe.mock.calls.filter(([name]) => name === pickerDeferred)).toHaveLength(1);
    expect(document.querySelector(".waiting")?.textContent).toContain("2 件");
    subscriptions.get(pickerDeferred)!([link.id, 3]);
    expect(document.querySelector(".waiting")?.textContent).toContain("3 件");
    request.mockClear();
    subscriptions.get(queueChanged)!(undefined);
    expect(request).toHaveBeenCalledExactlyOnceWith("picker_state");
  });
});
