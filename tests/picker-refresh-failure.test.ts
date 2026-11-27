// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listenPickerEvent } from "../src/picker-events";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../src/picker-events", () => ({ listenPickerEvent: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: "picker-2" }) }));

it("preserves the rendered additional picker after a temporary refresh failure", async () => {
  document.body.innerHTML = '<main id="app"></main>';
  let changed!: () => void;
  let stateRequests = 0;
  let fits = 0;
  vi.mocked(listenPickerEvent).mockImplementation(async (event, callback) => {
    changed = () => callback({ id: 1, event, payload: null as never });
  });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "picker_state") {
      stateRequests++;
      if (stateRequests >= 2) throw new Error("temporary IPC failure");
      return { id: 2, url: "https://example.com", waiting: 0, browsers: [{ id: "a", name: "A" }] } as never;
    }
    if (command === "fit_picker" && ++fits === 1) throw new Error("temporary position failure");
    if (command === "picker_icons") return { a: "data:image/png;base64,AA==" } as never;
    return undefined as never;
  });
  await import("../src/main");
  await vi.waitFor(() => expect(document.querySelector("button.browser")).not.toBeNull());
  const browser = document.querySelector<HTMLButtonElement>("button.browser")!;
  browser.focus();
  const list = document.querySelector("ul")!;
  list.scrollTop = 21;
  changed();
  await vi.waitFor(() => expect(fits).toBe(2));
  expect(stateRequests).toBe(3);
  expect(document.querySelector("button.browser")).toBe(browser);
  expect(document.activeElement).toBe(browser);
  expect(list.scrollTop).toBe(21);
});
