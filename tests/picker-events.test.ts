// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import { listenPickerEvent } from "../src/picker-events";

it("releases callbacks for rejected IPC registrations and retains the successful listener", async () => {
  const callbacks = new Map<number, (...args: never[]) => void>();
  let next = 0;
  const register = vi.fn(async (command: string, args: { handler: number }) => {
    expect(command).toBe("plugin:event|listen");
    expect(callbacks.has(args.handler)).toBe(true);
    if (register.mock.calls.length <= 3) throw "unavailable";
    return 9;
  });
  vi.stubGlobal("__TAURI_INTERNALS__", {
    invoke: register,
    transformCallback(handler: (...args: never[]) => void, once?: boolean) {
      const id = ++next;
      callbacks.set(id, (...args) => {
        if (once) callbacks.delete(id);
        handler(...args);
      });
      return id;
    },
    unregisterCallback(id: number) { callbacks.delete(id); },
  });
  try {
    const listener = vi.fn();
    for (let attempt = 0; attempt < 3; attempt++) {
      await expect(listenPickerEvent("picker-deferred", listener)).rejects.toBe("unavailable");
      expect(callbacks.size).toBe(0);
    }
    await expect(listenPickerEvent("picker-deferred", listener)).resolves.toBe(9);
    expect(callbacks.size).toBe(1);
  } finally {
    vi.unstubAllGlobals();
  }
});
