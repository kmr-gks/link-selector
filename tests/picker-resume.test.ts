// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listenPickerEvent } from "../src/picker-events";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../src/picker-events", () => ({ listenPickerEvent: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: "picker-2" }) }));

it("retries failed effects in the additional picker on reopen and restore", async () => {
  document.body.innerHTML = '<main id="app"></main>';
  let reopen!: () => void;
  let failFit!: (error: Error) => void;
  const initialFit = new Promise<never>((_, reject) => { failFit = reject; });
  let stateRequests = 0;
  let fits = 0;
  let iconRequests = 0;
  const icon = "data:image/png;base64,AA==";

  vi.mocked(listenPickerEvent).mockImplementation(async (event, callback, label) => {
    expect(event).toBe("picker-changed");
    expect(label).toBe("picker-2");
    reopen = () => callback({ id: 1, event, payload: null as never });
  });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "picker_state") {
      stateRequests++;
      return {
        id: 2, url: "https://example.com/", waiting: 0,
        browsers: [{ id: "a", name: "A" }, { id: "b", name: "B" }],
      } as never;
    }
    if (command === "fit_picker") {
      fits++;
      if (fits === 1) return initialFit;
      if (fits === 2) throw new Error("Window is still maximized");
    }
    if (command === "picker_icons") {
      iconRequests++;
      return (iconRequests === 1 ? { a: icon } : { a: icon, b: icon }) as never;
    }
    return undefined as never;
  });

  await import("../src/main");
  await vi.waitFor(() => expect(document.querySelectorAll("img.browser-icon")).toHaveLength(1));
  const firstIcon = document.querySelector("img.browser-icon");
  const button = document.querySelector<HTMLButtonElement>("button.browser")!;
  button.focus();
  const list = document.querySelector("ul")!;
  list.scrollTop = 23;

  reopen();
  await vi.waitFor(() => expect(stateRequests).toBe(2));
  failFit(new Error("Temporary position failure"));
  await vi.waitFor(() => {
    expect(fits).toBe(2);
    expect(document.querySelectorAll("img.browser-icon")).toHaveLength(2);
  });

  // Native geometry tracking sends the same targeted refresh after restoration.
  reopen();
  await vi.waitFor(() => expect(fits).toBe(3));
  expect(iconRequests).toBe(2);
  expect(document.querySelector("img.browser-icon")).toBe(firstIcon);
  expect(document.activeElement).toBe(button);
  expect(list.scrollTop).toBe(23);
});
