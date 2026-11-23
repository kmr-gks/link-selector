import { invoke, transformCallback } from "@tauri-apps/api/core";
import type { EventCallback } from "@tauri-apps/api/event";

// These listeners live as long as the picker webview. Tauri's listen helper
// leaves its callback registered when the IPC request rejects.
export async function listenPickerEvent<T>(event: string, handler: EventCallback<T>): Promise<unknown> {
  const callback = transformCallback(handler);
  try {
    return await invoke("plugin:event|listen", { event, target: { kind: "Any" }, handler: callback });
  } catch (error) {
    const { __TAURI_INTERNALS__: internals } = window as Window & {
      __TAURI_INTERNALS__?: { unregisterCallback(id: number): void };
    };
    internals?.unregisterCallback(callback);
    throw error;
  }
}
