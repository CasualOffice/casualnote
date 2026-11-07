#!/usr/bin/env node
// Runs webapp/tests/e2e.js in real headless Chrome, with no dependencies. Exits non-zero on any failure.
//   node tools/e2e.mjs [--shot path.png] [--lang ar]
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, readFileSync, writeFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, extname, normalize } from "node:path";

const root = new URL("../webapp/", import.meta.url).pathname;
const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm" };
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^\/+/, "") || "index.html";
  const file = join(root, path);
  if (!file.startsWith(root) || !existsSync(file)) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { "content-type": MIME[extname(file)] || "application/octet-stream" });
  res.end(readFileSync(file));
});
await new Promise(r => server.listen(0, "127.0.0.1", r));
const appPort = server.address().port;

const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const profile = mkdtempSync(join(tmpdir(), "cn-e2e-"));
const proc = spawn(chrome, ["--headless=new", "--disable-gpu", `--user-data-dir=${profile}`, "--remote-debugging-port=0", "--window-size=1440,900", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const wsBase = await new Promise((resolve, reject) => {
  let log = ""; proc.stderr.on("data", d => { log += d; const m = log.match(/ws:\/\/[^\s]+/); if (m) resolve(m[0]); });
  setTimeout(() => reject(new Error("Chrome did not start")), 20000);
});
const port = new URL(wsBase).port;
const target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === "page");
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => { ws.onopen = r; });
let id = 0; const waiting = new Map(), problems = [];
ws.onmessage = e => {
  const m = JSON.parse(e.data);
  if (m.id) { waiting.get(m.id)?.(m.result ?? m); waiting.delete(m.id); }
  if (m.method === "Runtime.exceptionThrown") problems.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") problems.push(m.params.args.map(a => a.value ?? a.description).join(" "));
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") problems.push(m.params.entry.text + " " + (m.params.entry.url || ""));
};
const send = (method, params = {}) => new Promise(r => { const i = ++id; waiting.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async expr => (await send("Runtime.evaluate", { expression: expr, returnByValue: true })).result?.value;
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
await send("Page.navigate", { url: `http://127.0.0.1:${appPort}/index.html?e2e=1` });

let result = null;
for (let i = 0; i < 120 && result === null; i++) { await new Promise(r => setTimeout(r, 250)); result = await evaluate(`document.body?.getAttribute("data-e2e")`) ?? null; }
const shot = process.argv.indexOf("--shot");
if (shot > 0) {
  await new Promise(r => setTimeout(r, 300));
  const { data } = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(process.argv[shot + 1], Buffer.from(data, "base64"));
}
ws.close(); proc.kill(); server.close();
try { rmSync(profile, { recursive: true, force: true }); } catch { /* best effort */ }

console.log(result ?? "NO RESULT: the app never finished its checks");
if (problems.length) console.log("\nBrowser errors:\n" + problems.join("\n"));
const failed = result === null || problems.length > 0 || /^(FAIL|THROW)/m.test(result);
console.log(failed ? "\nFAILED" : "\nPASSED");
process.exit(failed ? 1 : 0);
