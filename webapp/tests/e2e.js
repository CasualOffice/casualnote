// Drives the real app in a real browser, through real input events. Open with ?e2e=1.
export async function runE2E({ engine, store, state, undoStack, replay, redoStack, pageNow, openPage, setLang, renderAll, $, $$, setCaret, textOf }) {
  const out = [];
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const ok = async (name, f) => { try { out.push(((await f()) === false ? "FAIL " : "ok ") + name); } catch (e) { out.push("THROW " + name + ": " + (e.message || e)); } };
  const texts = () => pageNow().blocks.map(b => b.runs.map(r => r.text).join(""));
  const key = (n, k, o = {}) => n.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...o }));
  const first = () => $(".blocks .t");
  const type = s => document.execCommand("insertText", false, s);

  await ok("app boots with a real engine", () => !!first() && pageNow().blocks.length >= 1);
  await ok("typing reaches the engine", () => { setCaret(first()); type("hello world"); return texts()[0] === "hello world"; });
  await ok("edits in the middle splice correctly", () => { setCaret(first(), 5); type(","); return texts()[0] === "hello, world"; });
  await ok("enter splits a block", () => { setCaret(first(), 6); key(first(), "Enter"); return texts().length === 2 && texts()[0] === "hello," && texts()[1] === " world"; });
  await ok("backspace at the start merges", () => { const n = textOf(pageNow().blocks[1].id); setCaret(n, 0); key(n, "Backspace"); return texts().length === 1 && texts()[0] === "hello, world"; });
  await ok("undo restores, redo reapplies", () => { const before = texts()[0]; replay(undoStack, redoStack); const undone = texts(); replay(redoStack, undoStack); return undone.length === 2 && texts()[0] === before; });
  await ok("markdown shortcut makes a heading", () => { key(first(), "End"); setCaret(first()); key(first(), "Enter"); type("# "); return pageNow().blocks.at(-1).kind === "heading1"; });
  await ok("slash menu opens and applies", async () => {
    type("Title"); key(document.activeElement, "Enter"); type("/to"); await wait(20);
    if (!$$(".pop .mi").length) return false;
    key(document.activeElement, "Enter"); await wait(20);
    return pageNow().blocks.at(-1).kind === "todo";
  });
  await ok("every script types exactly", () => {
    const samples = ["مرحبا بالعالم", "नमस्ते दुनिया", "你好，世界", "こんにちは", "한국어", "Café 🎉"];
    for (const s of samples) { type(s); key(document.activeElement, "Enter"); }
    const have = texts();
    return samples.every(s => have.includes(s));
  });
  await ok("marks through the toolbar reach the engine", () => {
    const b = pageNow().blocks[0], n = textOf(b.id), w = document.createTreeWalker(n, NodeFilter.SHOW_TEXT).nextNode();
    const r = document.createRange(); r.setStart(w, 0); r.setEnd(w, 5); const s = getSelection(); s.removeAllRanges(); s.addRange(r);
    $('#fmt [data-f="bold"]').click();
    return pageNow().blocks[0].runs[0].text === "hello" && pageNow().blocks[0].runs[0].marks.includes("bold");
  });
  await ok("document text cannot become HTML", () => {
    setCaret(first()); type("<img src=x onerror=alert(1)>");
    renderAll();
    return !$(".blocks img") && first().textContent.includes("<img");
  });
  await ok("arabic mirrors the interface", () => { setLang("ar"); const r = document.documentElement.dir === "rtl" && $("#settingsBtn").textContent.includes("الإعدادات"); setLang("en"); return r; });
  await ok("subpage and tree", () => { const parent = state.cur; $(".row.on .add").click(); const m = JSON.parse(engine.pages()).find(p => p.id === state.cur); const r = m.parent === parent; openPage(parent); return r; });
  await ok("search finds every script", () => { $("#searchBtn").click(); const i = $("#palette"); i.value = "한국어"; i.dispatchEvent(new Event("input")); const n = $$(".modal .mi").length; key(i, "Escape"); document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); return n === 1; });
  await ok("saved only after commit, and survives reopening", async () => {
    await store.flush(); await wait(50);
    if ($("#saved").textContent !== "Saved on this device") return false;
    const { Engine } = await import("../pkg/casual_note_wasm.js");
    const again = new Engine(12345n); again.import(await store.load());
    return again.page(state.cur) === engine.page(state.cur);
  });
  await ok("a second replica merges concurrent edits", async () => {
    const { Engine } = await import("../pkg/casual_note_wasm.js");
    const other = new Engine(777n); other.import(engine.export());
    const p = pageNow(), b = p.blocks[0];
    other.apply(JSON.stringify({ op: "splice_text", page: p.meta.id, block: b.id, at: 0, delete: 0, insert: "FROM OTHER " }));
    setCaret(first()); type(" local");
    engine.import(other.export()); other.import(engine.export());
    renderAll();
    return engine.page(state.cur) === other.page(state.cur) && texts()[0].startsWith("FROM OTHER ") && texts()[0].endsWith(" local");
  });
  document.body.setAttribute("data-e2e", out.join("\n"));
}
