#!/usr/bin/env node
// Photographs the real app, with example content, for the website. node tools/site-shots.mjs site/assets
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, readFileSync, writeFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, extname, normalize } from "node:path";
const root = new URL("../webapp/", import.meta.url).pathname, out = process.argv[2];
const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm" };
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^\/+/, "") || "index.html", file = join(root, path);
  if (!file.startsWith(root) || !existsSync(file)) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { "content-type": MIME[extname(file)] || "application/octet-stream" }); res.end(readFileSync(file));
});
await new Promise(r => server.listen(0, "127.0.0.1", r));
const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const profile = mkdtempSync(join(tmpdir(), "cn-shots-"));
const proc = spawn(chrome, ["--headless=new", "--disable-gpu", `--user-data-dir=${profile}`, "--remote-debugging-port=0", "--window-size=1440,900", "--lang=en-US", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const wsBase = await new Promise((ok, no) => { let log = ""; proc.stderr.on("data", d => { log += d; const m = log.match(/ws:\/\/[^\s]+/); if (m) ok(m[0]); }); setTimeout(() => no(new Error("no Chrome")), 20000); });
const target = (await (await fetch(`http://127.0.0.1:${new URL(wsBase).port}/json`)).json()).find(t => t.type === "page");
const ws = new WebSocket(target.webSocketDebuggerUrl); await new Promise(r => { ws.onopen = r; });
let id = 0; const waiting = new Map();
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id) { waiting.get(m.id)?.(m.result ?? m); waiting.delete(m.id); } };
const send = (method, params = {}) => new Promise(r => { const i = ++id; waiting.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async expr => (await send("Runtime.evaluate", { expression: expr, returnByValue: true })).result?.value;
const wait = ms => new Promise(r => setTimeout(r, ms));
const shot = async name => { await wait(700); const { data } = await send("Page.captureScreenshot", { format: "png" }); writeFileSync(join(out, name + ".png"), Buffer.from(data, "base64")); };
const open = title => evaluate(`(() => { document.querySelectorAll("#tree .chev[aria-label='Expand']").forEach(c => c.click()); [...document.querySelectorAll("#tree .row .open")].find(b => b.textContent.includes(${JSON.stringify(title)}))?.click(); })()`);
await send("Page.enable"); await send("Runtime.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 2, mobile: false });
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "light" }] });
await send("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/index.html?demo=1` });
for (let i = 0; i < 80; i++) { await wait(250); if (await evaluate(`[...document.querySelectorAll("#tree .row")].some(r => r.textContent.includes("Beta launch plan"))`)) break; }
await wait(800);
await open("Beta launch plan"); await shot("app-page");
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }] }); await shot("app-page-dark");
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "light" }] });
await open("Why would someone switch?"); await shot("app-canvas");
await evaluate(`document.querySelector("#graphBtn")?.click()`); await wait(1500); await shot("app-graph");
ws.close(); proc.kill(); server.close();
try { rmSync(profile, { recursive: true, force: true }); } catch { /* best effort */ }
console.log("done");
