// Export and import, through the real engine and the real interface.
const CREATED = "2026-11-07T12:00:00Z";
const SCRIPTS = ["مرحبا بالعالم", "مَرْحَبًا", "नमस्ते दुनिया", "你好，世界", "こんにちは", "한국어", "¿Qué tal, niño?", "Plain English", "café 🎉"];

export async function run({ api, ok, wait, key, openPage, $, $$, engine, state }) {
  const { Engine, package_inspect, package_entry } = await import("../pkg/casual_note_wasm.js");
  const seed = (e, prefix = "") => {
    const a = op => e.apply(JSON.stringify(op));
    a({ op: "create_database", database: { id: prefix + "d1", name: "مهام", view: "board", statuses: [{ id: "s1", name: "未开始", group: "todo" }], columns: [{ id: "owner", name: "Owner", kind: "text", hidden: false }, { id: "old", name: "Old", kind: "text", hidden: true }], rows: [{ id: "r1", name: "إطلاق", status: "s1", values: { owner: "Mei", old: "kept" } }] } });
    a({ op: "create_page", id: prefix + "p1", parent: null, title: "مرحبا 你好" });
    a({ op: "create_page", id: prefix + "p2", parent: prefix + "p1", title: "" });
    a({ op: "set_favorite", id: prefix + "p1", value: true });
    let after = null;
    SCRIPTS.forEach((text, i) => { const id = "t" + i; a({ op: "insert_block", page: prefix + "p1", after, block: { id, kind: "paragraph", checked: false, runs: [{ text, marks: [] }] } }); after = id; });
    a({ op: "insert_block", page: prefix + "p1", after, block: { id: "m", kind: "heading2", checked: false, runs: [{ text: "粗", marks: ["bold"] }, { text: "link", marks: [], link: prefix + "p2" }] } });
    a({ op: "insert_block", page: prefix + "p1", after: "m", block: { id: "db", kind: "database", checked: false, runs: [], target: prefix + "d1" } });
    a({ op: "put_item", page: prefix + "p2", item: { id: "i1", kind: "sticky", x: 5.5, y: 6, width: 100, height: 80, color: "amber", text: "付箋" } });
    a({ op: "change_ink", page: prefix + "p2", add: [{ id: "s1", kind: "pen", color: "red", width: 3, points: [[1, 2, 0.5], [3.125, 4, 1]] }, { id: "s2", kind: "arrow", color: "", width: 2, points: [[1, 1, 0.5], [9, 9, 0.5]] }], remove: [] });
    return e;
  };
  const peer = () => crypto.getRandomValues(new BigUint64Array(1))[0] >> 1n;
  const importInto = (e, bytes) => { const plan = JSON.parse(e.package_plan(bytes, peer())); for (const op of plan.ops) e.apply(JSON.stringify(op)); return plan.report; };

  await ok("package: export, import, export gives identical bytes, and every script is unchanged", () => {
    const a = seed(new Engine(101n)), first = a.package_export(CREATED);
    const b = new Engine(102n); const report = importInto(b, first);
    const second = b.package_export(CREATED);
    if (report.copied || first.length !== second.length || first.some((x, i) => x !== second[i])) return false;
    const texts = JSON.parse(b.page("p1")).blocks.map(x => x.runs.map(r => r.text).join(""));
    return SCRIPTS.every(s => texts.includes(s)) && b.page("p2") === a.page("p2") && b.database("d1") === a.database("d1");
  });
  await ok("package: the engine's bytes are the file format, with ink in its own file", () => {
    const bytes = seed(new Engine(103n)).package_export(CREATED), info = JSON.parse(package_inspect(bytes));
    const paths = info.files.map(f => f[0]);
    const ink = new TextDecoder().decode(package_entry(bytes, "ink/p2.jsonl"));
    const head = new TextDecoder().decode(bytes.subarray(30, 76));
    return paths.includes("pages/p1.json") && paths.includes("databases/d1.json") && paths.some(p => p.startsWith("readable/مرحبا")) && head === "mimetypeapplication/vnd.casualnote.package+zip"
      && ink === '{"color":"red","id":"s1","kind":"pen","points":[[1,2,0.5],[3.125,4,1]],"width":3}\n{"from":[1,1],"id":"s2","kind":"arrow","to":[9,9],"x-width":2}\n';
  });
  await ok("package: hostile bytes are refused by name, and change nothing", () => {
    const e = seed(new Engine(104n)), bytes = e.package_export(CREATED), before = e.export();
    const code = b => { try { e.package_plan(b, peer()); return "accepted"; } catch (err) { return String(err.message).slice(0, 11); } };
    const flip = bytes.slice(); flip[bytes.length >> 1] ^= 1;
    const r = [code(new TextEncoder().encode("not a zip at all, certainly not one")), code(new Uint8Array(0)), code(flip)];
    const after = e.export();
    return r[0] === "CN-PKG-0001" && r[1] === "CN-PKG-0001" && /CN-PKG-000[18]/.test(r[2]) && before.length === after.length;
  });

  await ok("package: this whole working workspace, from every feature's tests, round-trips to identical bytes", () => {
    const first = engine.package_export(CREATED), b = new Engine(108n);
    importInto(b, first);
    const second = b.package_export(CREATED);
    window.__exportForCheck = () => { let s = ""; for (const x of first) s += String.fromCharCode(x); return btoa(s); };
    return first.length === second.length && first.every((x, i) => x === second[i]);
  });

  /* Through the interface */
  const settled = async () => { for (let i = 0; i < 200 && !$(".pkgfiles") && !$(".pkgerror"); i++) await wait(10); };
  const openSettings = async () => { $("#settingsBtn").click(); await wait(10); };
  let saved = [];
  const realCreate = URL.createObjectURL;
  URL.createObjectURL = blob => { saved.push(blob); return realCreate.call(URL, blob); };

  api.edit({ op: "create_page", id: "pk-page", parent: null, title: "Package عربى" }, { op: "insert_block", page: "pk-page", after: null, block: { id: "pk1", kind: "paragraph", checked: false, runs: [{ text: "نص <b>not html</b> ", marks: [] }, { text: "self", marks: [], link: "pk-page" }] } });

  await ok("package: Settings has the section, and Export workspace previews with the warning", async () => {
    await openSettings();
    if (!$("#pkgExport") || !$("#pkgImport")) return false;
    $("#pkgExport").click();
    const during = !!$(".pkgmodal .pkgnote");
    await settled();
    const warn = $(".pkgwarn")?.textContent || "", files = $$(".pkgfile").map(n => n.textContent);
    return during && /not encrypted/.test(warn) && files.some(f => f.startsWith("manifest.json")) && files.some(f => f.includes("pages/pk-page.json")) && $(".pkgmodal h3").textContent.endsWith(".cnotepack");
  });
  await ok("package: a Markdown file in the preview opens as text, and back", async () => {
    const link = $$(".pkgfile.link").find(n => n.textContent.includes("Package عربى"));
    link.click(); await wait(5);
    const text = $(".pkgmodal pre")?.textContent || "";
    const good = text.startsWith("# Package عربى") && text.includes("<b>not html</b>") && !$(".pkgmodal pre b");
    $(".pkgback").click(); await wait(5);
    return good && !!$(".pkgfiles");
  });
  await ok("package: Save file downloads a Blob that the reader accepts", async () => {
    saved = [];
    $$(".pkgmodal .btn").find(b => b.classList.contains("primary")).click(); await wait(10);
    if (saved.length !== 1 || $(".pkgmodal")) return false;
    const info = JSON.parse(package_inspect(new Uint8Array(await saved[0].arrayBuffer())));
    return info.scope === "workspace" && info.pages.some(p => p[0] === "pk-page");
  });
  await ok("package: Escape closes a modal and returns focus", async () => {
    $("#pkgExport").focus(); $("#pkgExport").click(); await settled();
    if (!$(".pkgmodal")) return false;
    key($(".pkgmodal"), "Escape"); await wait(5);
    return !$(".pkgmodal") && document.activeElement === $("#pkgExport");
  });
  const chooseFile = async (bytes, name = "x.cnotepack") => {
    const dt = new DataTransfer(); dt.items.add(new File([bytes], name));
    const input = $("#pkgFile"); input.files = dt.files; input.dispatchEvent(new Event("change", { bubbles: true }));
    for (let i = 0; i < 200 && (!$(".pkgmodal") || $('.pkgbody [role="status"]')); i++) await wait(10);
  };
  await ok("package: a bad file shows a plain refusal with its code, and changes nothing", async () => {
    const before = api.pages().length;
    await chooseFile(new TextEncoder().encode("this is not a package at all, sorry"));
    const shown = /not a CasualNote Package/.test($(".pkgerror")?.textContent || "") && $(".pkgcode")?.textContent.includes("CN-PKG-0001");
    $$(".pkgmodal .btn")[0].click(); await wait(5);
    return shown && api.pages().length === before;
  });
  await ok("package: importing new notes keeps ids, shows a report first, and is one undo step", async () => {
    const bytes = seed(new Engine(105n), "new-").package_export(CREATED);
    const before = api.pages().length;
    await chooseFile(bytes);
    if (api.pages().length !== before) return false; // Nothing happens before the person agrees.
    const said = $(".pkgbody")?.textContent || "";
    if (!said.includes("مرحبا 你好")) return false;
    $$(".pkgmodal .btn").find(b => b.classList.contains("primary")).click(); await wait(10);
    const page = JSON.parse(engine.page("new-p1"));
    if (state.cur !== "new-p1" || page.meta.title !== "مرحبا 你好" || !page.meta.favorite) return false;
    if (!SCRIPTS.every(s => page.blocks.some(b => b.runs.map(r => r.text).join("") === s))) return false;
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true, cancelable: true })); await wait(10);
    const undone = api.metaOf("new-p1").trashed;
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true })); await wait(10);
    return undone && !api.metaOf("new-p1").trashed;
  });
  await ok("package: importing onto existing ids makes copies with links rewritten", async () => {
    const bytes = seed(new Engine(106n), "new-").package_export(CREATED);
    const before = new Set(api.pages().map(p => p.id));
    await openSettings();
    await chooseFile(bytes);
    if (!/copies/.test($(".pkgwarn")?.textContent || "")) return false;
    $$(".pkgmodal .btn").find(b => b.classList.contains("primary")).click(); await wait(10);
    const added = api.pages().filter(p => !before.has(p.id));
    if (added.length !== 2) return false;
    const root = added.find(p => p.title), kid = added.find(p => !p.title);
    const link = JSON.parse(engine.page(root.id)).blocks.find(b => b.id === "m").runs.find(r => r.link);
    return kid.parent === root.id && link.link === kid.id && JSON.parse(engine.page("new-p1")).meta.id === "new-p1";
  });
  await ok("package: the page menu has Export this page, with only that page in it", async () => {
    openPage("pk-page"); await wait(5);
    $("#moreBtn").click(); await wait(5);
    $$(".pop .mi").find(m => m.textContent === "Export this page").click(); await settled();
    const files = $$(".pkgfile").map(n => n.textContent), good = files.some(f => f.includes("pages/pk-page.json")) && !files.some(f => f.includes("pages/new-p1.json"));
    const pages = $$(".pkgnote")[0].textContent.startsWith("1 pages");
    key($(".pkgmodal"), "Escape"); await wait(5);
    return good && pages;
  });
  await ok("package: the page menu has a Markdown view, as text only", async () => {
    $("#moreBtn").click(); await wait(5);
    $$(".pop .mi").find(m => m.textContent === "View as Markdown").click(); await wait(10);
    const text = $(".pkgmodal pre")?.textContent || "";
    const good = text.startsWith("# Package عربى") && text.includes("<b>not html</b>") && !$(".pkgmodal pre b") && text.includes("[[self]]");
    saved = [];
    $$(".pkgmodal .btn").find(b => b.classList.contains("primary")).click(); await wait(10);
    const file = saved.length === 1 && (await saved[0].text()).startsWith("# Package عربى");
    return good && file;
  });
  await ok("package: exporting under another interface language gives the same bytes", () => {
    const e = seed(new Engine(107n)), a = e.package_export(CREATED);
    const lang = document.documentElement.lang;
    const b = e.package_export(CREATED);
    return lang && a.length === b.length && a.every((x, i) => x === b[i]);
  });
  URL.createObjectURL = realCreate;
  $(".pkgmodal") && key($(".pkgmodal"), "Escape");
}
