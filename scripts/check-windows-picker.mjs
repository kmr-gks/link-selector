import { spawn } from "node:child_process";
import { once } from "node:events";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const [app, port] = process.argv.slice(2);
const firstUrl = "https://example.invalid/installer-smoke?link=first";
const secondUrl = "https://example.invalid/installer-smoke?link=second";

function readPicker(target) {
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  let timeout;
  return new Promise((resolve, reject) => {
    timeout = setTimeout(() => reject(new Error("WebView2 evaluation timed out")), 2000);
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({
        id: 1,
        method: "Runtime.evaluate",
        params: {
          expression: `(() => {
            const link = document.querySelector('#app.picker .link-url');
            return link?.checkVisibility({ visibilityProperty: true, opacityProperty: true })
              ? link.textContent : null;
          })()`,
          returnByValue: true,
        },
      }));
    }, { once: true });
    socket.addEventListener("message", ({ data }) => {
      const message = JSON.parse(data);
      if (message.id !== 1) return;
      if (message.error) reject(new Error(message.error.message));
      else resolve(message.result?.result?.value);
    });
    socket.addEventListener("error", () => reject(new Error("Could not connect to WebView2")), { once: true });
    socket.addEventListener("close", () => reject(new Error("WebView2 target closed")), { once: true });
  }).finally(() => {
    clearTimeout(timeout);
    socket.close();
  });
}

async function waitForPickers(process, port, matches, description) {
  const deadline = Date.now() + 45_000;
  let lastResult;
  while (Date.now() < deadline) {
    if (process.exitCode !== null || process.signalCode !== null) {
      throw new Error(`The installed app exited before ${description}`);
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`, {
        signal: AbortSignal.timeout(2000),
      });
      const targets = (await response.json()).filter(({ type }) => type === "page");
      const pickers = await Promise.all(targets.map(async (target) => ({
        id: target.id,
        url: await readPicker(target).catch(() => undefined),
      })));
      const result = matches(pickers);
      if (result) return result;
      lastResult = JSON.stringify(pickers);
    } catch (error) {
      lastResult = error.cause ? `${error}: ${error.cause}` : String(error);
    }
    await delay(250);
  }
  throw new Error(`Timed out waiting for ${description}. Last observation: ${lastResult}`);
}

async function confirmVisibleWindows(pid, count) {
  const script = fileURLToPath(new URL("./check-windows-picker-windows.ps1", import.meta.url));
  const check = spawn("pwsh.exe", ["-NoProfile", "-File", script,
    "-AppProcessId", String(pid), "-ExpectedCount", String(count)], { stdio: "inherit" });
  const [code] = await once(check, "exit");
  if (code !== 0) throw new Error(`Could not verify ${count} visible native picker windows`);
}

// Enable the local inspector only in the installed process launched by CI.
const env = {
  ...process.env,
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-address=127.0.0.1 --remote-debugging-port=${port}`,
};
const original = spawn(app, [firstUrl], { env, stdio: "ignore" });
await once(original, "spawn");
let forwarded;
try {
  const first = await waitForPickers(original, port,
    (pickers) => pickers.find(({ url }) => url === firstUrl), "the first URL");
  await confirmVisibleWindows(original.pid, 1);
  forwarded = spawn(app, [secondUrl], { env, stdio: "ignore" });
  const exit = once(forwarded, "exit");
  const [code] = await Promise.race([
    exit,
    delay(15_000, [], { ref: false }),
  ]);
  if (code !== 0) throw new Error(`The forwarding process did not exit successfully (${code})`);
  await waitForPickers(original, port, (pickers) =>
    pickers.some(({ id, url }) => id === first.id && url === firstUrl) &&
    pickers.some(({ id, url }) => id !== first.id && url === secondUrl),
  "both URLs in separate picker windows");
  await confirmVisibleWindows(original.pid, 2);
  console.log("Both installer smoke-test URLs rendered in separate, visible native windows.");
} finally {
  for (const process of [forwarded, original]) {
    if (process && process.exitCode === null && process.signalCode === null) {
      const exit = once(process, "exit");
      process.kill();
      await Promise.race([exit, delay(5000, undefined, { ref: false }).then(() => {
        throw new Error("The installer smoke-test process did not stop");
      })]);
    }
  }
}
