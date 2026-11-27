// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listenPickerEvent } from "../src/picker-events";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../src/picker-events", () => ({ listenPickerEvent: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: "picker-2" }) }));

it("renders a new additional picker even when notifications are unavailable", async () => {
  document.body.innerHTML = '<main id="app"></main>';
  let changed!: () => void;
  vi.mocked(listenPickerEvent)
    .mockRejectedValueOnce(new Error("temporary subscription failure"))
    .mockRejectedValueOnce(new Error("temporary subscription failure"))
    .mockImplementation(async (event, callback) => {
      changed = () => callback({ id: 1, event, payload: null as never });
    });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "picker_state") {
      return { id: 2, url: "https://example.com", waiting: 0, browsers: [{ id: "a", name: "A" }] } as never;
    }
    if (command === "picker_icons") return { a: "data:image/png;base64,AA==" } as never;
    return undefined as never;
  });
  await import("../src/main");
  await vi.waitFor(() => expect(listenPickerEvent).toHaveBeenCalledTimes(2));
  const browser = document.querySelector("button.browser");
  expect(browser).not.toBeNull();
  expect(document.querySelector(".close-load-error")).toBeNull();
  await vi.waitFor(() => expect(listenPickerEvent).toHaveBeenCalledTimes(3));
  changed();
  await vi.waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) =>
    command === "picker_state")).toHaveLength(2));
  expect(document.querySelector("button.browser")).toBe(browser);
});
