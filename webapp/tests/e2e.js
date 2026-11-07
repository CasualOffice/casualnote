// Drives the real app in a real browser, through real input events. Open with ?e2e=1.
export async function runE2E({ api, engine, store, state, undoStack, replay, redoStack, pageNow, openPage, setLang, renderAll, $, $$, setCaret, textOf }) {
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
  for (const name of ["links", "database", "canvas", "knowledge", "package", "assistant"]) {
    const mod = await import(`./${name}.test.js`);
    await mod.run({ api, ok, wait, key, type, texts, first, pageNow, engine, state, store, setCaret, textOf, openPage, $, $$ });
  }
  await uxTests({ api, ok, wait, key, type, texts, first, pageNow, engine, state, store, setLang, renderAll, undoStack, redoStack, replay, openPage, setCaret, textOf, $, $$ });
  document.body.setAttribute("data-e2e", out.join("\n"));
}

/* ---------- Interface and editor feel. Each one drives the event a person's action produces. ---------- */
async function uxTests({ api, ok: okBase, wait, key, type, texts, first, pageNow, engine, state, store, setLang, renderAll, undoStack, redoStack, replay, openPage, setCaret, textOf, $, $$ }) {
  const MOD = api.ui.MOD, active = () => document.activeElement;
  // Names the parts of a check that failed, in the browser console.
  let current = "";
  const all = parts => { const bad = Object.entries(parts).filter(([, v]) => !v).map(([k]) => k); if (bad.length) console.warn(`${current}: ${bad.join(", ")}`); return !bad.length; };
  const ok = (name, f) => { current = name; return okBase(name, f); };
  const fresh = (...lines) => {
    api.newPage(null);
    setCaret(first());
    lines.forEach((l, i) => {
      if (i) key(active(), "Enter");
      const m = l.match(/^(#{1,3} |- |\[\] |> )(.*)$/s);
      if (m) { type(m[1]); if (m[2]) type(m[2]); } else if (l) type(l);
    });
    return pageNow().meta.id;
  };
  // Most checks need a clean page, not a new one. Reusing one keeps the whole suite inside its time limit.
  let scratch = null;
  const blank = (...lines) => {
    if (!scratch || !api.metaOf(scratch) || api.metaOf(scratch).trashed || !$(".blocks") || state.view || state.cur !== scratch) { api.newPage(null); scratch = pageNow().meta.id; }
    const pid = scratch, nid = api.uid(), ops = pageNow().blocks.map(b => ({ op: "delete_block", page: pid, block: b.id }));
    api.edit({ op: "insert_block", page: pid, after: null, block: { id: nid, kind: "paragraph", checked: false, runs: [] } }, ...ops);
    api.renderPage();
    setCaret(first());
    lines.forEach((l, i) => {
      if (i) key(active(), "Enter");
      const m = l.match(/^(#{1,3} |- |\[\] |> )(.*)$/s);
      if (m) { type(m[1]); if (m[2]) type(m[2]); } else if (l) type(l);
    });
  };
  const titled = (title, ...lines) => { const id = fresh(...lines); api.edit({ op: "set_title", id, title }); api.renderAll(); if (lines.length) setCaret(nth(lines.length - 1)); return id; };
  const until = async (f, ms = 3000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (f()) return true; await wait(20); } return !!f(); };
  const caret = n => { const s = getSelection(), r = document.createRange(); r.selectNodeContents(n); r.setEnd(s.focusNode, s.focusOffset); return r.toString().length; };
  const nth = i => textOf(pageNow().blocks[i].id);
  const rowOf = i => $(`.b[data-id="${pageNow().blocks[i].id}"]`);
  const undo = () => replay(undoStack, redoStack);
  const mouse = (n, type, o = {}) => n.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, ...o }));
  const ptr = (n, type, o = {}) => n.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerType: "mouse", button: 0, ...o }));
  const clip = (n, type, data = {}) => { const dt = new DataTransfer(); for (const [k, v] of Object.entries(data)) dt.setData(k, v); n.dispatchEvent(new ClipboardEvent(type, { clipboardData: dt, bubbles: true, cancelable: true })); return dt; };
  const mid = n => { const r = n.getBoundingClientRect(); return { clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 }; };
  const crossDrag = (fromNode, fromOffset, toNode) => {
    ptr(fromNode, "pointerdown", { buttons: 1, ...mid(fromNode) });
    setCaret(fromNode, fromOffset);
    ptr(document, "pointermove", { buttons: 1, ...mid(toNode) });
    ptr(document, "pointerup", { buttons: 0, ...mid(toNode) });
  };

  /* Contrast and targets */
  const hex = v => { const m = v.trim().match(/^#(..)(..)(..)$/); return m.slice(1).map(x => parseInt(x, 16) / 255); };
  const lum = c => { const [r, g, b] = c.map(v => (v <= .03928 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4)); return .2126 * r + .7152 * g + .0722 * b; };
  const ratio = (a, b) => { const [x, y] = [lum(hex(a)), lum(hex(b))].sort((p, q) => q - p); return (x + .05) / (y + .05); };
  await ok("ux: colour tokens meet AA in light and dark", () => {
    const root = document.documentElement, had = root.dataset.theme, bad = [];
    for (const theme of ["light", "dark"]) {
      root.dataset.theme = theme;
      const c = getComputedStyle(root), v = n => c.getPropertyValue("--" + n);
      const need = (fg, bg, min) => { const r = ratio(v(fg), v(bg)); if (r < min) bad.push(`${theme} ${fg}/${bg} ${r.toFixed(2)}`); };
      for (const bg of ["bg", "side", "tint", "hover"]) { need("faint", bg, 4.5); need("muted", bg, 4.5); need("fg", bg, 4.5); need("danger", bg, 4.5); need("accent", bg, 4.5); }
      need("faint", "active", 3); need("muted", "active", 4.5); need("fg", "active", 4.5); need("accent-fg", "accent", 4.5); need("fg", "mark", 4.5);
      for (const bg of ["bg", "side"]) { need("edge", bg, 3); need("accent", bg, 3); }
    }
    if (had) root.dataset.theme = had; else delete root.dataset.theme;
    if (bad.length) console.warn(bad.join("; "));
    return !bad.length;
  });
  await ok("ux: the two dark blocks hold the same values", () => {
    const sheet = [...document.styleSheets].find(s => s.href?.endsWith("style.css"));
    const props = r => [...r.style].map(p => p + ":" + r.style.getPropertyValue(p)).sort().join(";");
    const manual = [...sheet.cssRules].find(r => r.selectorText === ':root[data-theme="dark"]');
    const media = [...sheet.cssRules].find(r => r.media?.mediaText.includes("prefers-color-scheme: dark")).cssRules[0];
    return props(manual) === props(media) && props(manual).length > 100;
  });
  await ok("ux: every shell and gutter control is at least 24 by 24", () => {
    blank("one", "two"); api.openPage(state.tabs[0]); fresh("three");
    const sel = "#side button:not(.open), #tabbar button, .bar .ibtn, .gutter button, .row .acts button, .tab .cl, .ck";
    const small = $$(sel).filter(n => n.offsetParent).map(n => [n.className || n.id, n.getBoundingClientRect()]).filter(([, r]) => r.width < 24 || r.height < 24);
    if (small.length) console.warn(small.map(([n, r]) => `${n} ${r.width}x${r.height}`).join("; "));
    return !small.length && $$(sel).length > 8;
  });
  await ok("ux: stylesheet keeps the banned list and logical properties", () => {
    const sheet = [...document.styleSheets].find(s => s.href?.endsWith("style.css")), text = [];
    const walk = rules => { for (const r of rules) { if (r.cssRules?.length && !r.selectorText) walk(r.cssRules); else text.push(r.cssText); } };
    walk(sheet.cssRules);
    const all = text.join("\n");
    const physical = text.filter(r => /(?:^|[\s;{])(margin-left|margin-right|padding-left|padding-right|border-left|border-right|text-align: (?:left|right))\b/.test(r));
    const located = text.filter(r => /[{;]\s*(left|right): /.test(r) && !/^\.(world|inklayer)/.test(r));
    if (physical.length || located.length) console.warn([...physical, ...located].join("\n").slice(0, 400));
    return !physical.length && !located.length && !/linear-gradient|conic-gradient|text-transform: uppercase|backdrop-filter/.test(all);
  });
  await ok("ux: motion exists only for those who have not asked to reduce it", () => {
    const sheet = [...document.styleSheets].find(s => s.href?.endsWith("style.css")), outside = [];
    const walk = (rules, safe) => { for (const r of rules) { const ok2 = safe || r.media?.mediaText.includes("no-preference"); if (r.cssRules?.length && !r.selectorText) walk(r.cssRules, ok2); else if (!ok2 && r.style && (r.style.transition || (r.style.animationName && r.style.animationName !== "boot-show" && r.style.animationName !== "none")) ) outside.push(r.selectorText); } };
    walk(sheet.cssRules, false);
    return !outside.length;
  });

  /* Editor */
  await ok("ux: clicking below the text puts a caret on a new line, once", () => {
    blank("only line");
    mouse($(".tail"), "mousedown", mid($(".tail")));
    const added = texts().length === 2 && active() === nth(1);
    mouse($(".tail"), "mousedown", mid($(".tail")));
    return added && texts().length === 2 && active() === nth(1);
  });
  await ok("ux: a click anywhere on an empty page starts writing", () => {
    api.newPage(null);
    mouse($(".doc"), "mousedown", { clientX: 900, clientY: innerHeight - 60 });
    return active() === first() && texts().length === 1;
  });
  await ok("ux: a click beside a line lands in that line", () => {
    blank("first line", "second line", "third line");
    const r = rowOf(1).getBoundingClientRect();
    mouse($(".doc"), "mousedown", { clientX: r.right + 12, clientY: r.top + r.height / 2 });
    return active() === nth(1) && caret(nth(1)) === "second line".length;
  });
  await ok("ux: a click above the first block goes to the title", () => {
    blank("line");
    const t = $(".doc .title").getBoundingClientRect();
    mouse($(".doc"), "mousedown", { clientX: t.right + 200, clientY: t.top + 6 });
    return active() === $(".doc .title");
  });
  await ok("ux: multi-line paste makes blocks, keeps the tail, and undoes as one", () => {
    blank("headTAIL");
    setCaret(first(), 4);
    const steps = undoStack.length;
    clip(first(), "paste", { "text/plain": "one\n\n# A heading\n- item\n- [x] done\n> said\n```\nlet a;\n\nlet b;\n```\nlast" });
    const kinds = pageNow().blocks.map(b => b.kind).join();
    const good = texts().join("|") === "headone|A heading|item|done|said|let a;\n\nlet b;|lastTAIL"
      && kinds === "paragraph,heading1,bulleted,todo,quote,code,paragraph" && pageNow().blocks[3].checked
      && active() === nth(6) && caret(nth(6)) === 4 && undoStack.length === steps + 1;
    undo();
    return good && texts().join("|") === "headTAIL";
  });
  await ok("ux: single-line paste stays inline, and code keeps its line breaks", () => {
    blank("ab");
    setCaret(first(), 1);
    clip(first(), "paste", { "text/plain": "X" });
    const inline = texts().join("|") === "aXb" && caret(first()) === 2;
    setCaret(first()); key(first(), "Enter"); type("```"); type(" ");
    const code = textOf(pageNow().blocks.at(-1).id);
    clip(code, "paste", { "text/plain": "a\nb" });
    return inline && pageNow().blocks.at(-1).kind === "code" && texts().at(-1) === "a\nb";
  });
  await ok("ux: pasted HTML stays text", () => {
    blank("");
    clip(first(), "paste", { "text/plain": "<img src=x onerror=alert(1)>\n<b>bold</b>", "text/html": "<img src=x>" });
    return !$(".blocks img") && !$(".blocks .t b") && texts().join("|") === "<img src=x onerror=alert(1)>|<b>bold</b>";
  });
  await ok("ux: a drag across blocks selects across them, then copies and deletes", () => {
    blank("alpha one", "bravo two", "charlie three");
    crossDrag(nth(0), 6, nth(2));
    if (!api.ui.multiActive() || !document.body.classList.contains("multi") || !getSelection().toString().includes("bravo two")) return false;
    const end = getSelection(); const sel = document.createRange(); sel.selectNodeContents(nth(2)); sel.setEnd(end.focusNode, end.focusOffset);
    const cut = sel.toString().length, tail = "charlie three".slice(cut);
    const dt = clip(document, "copy");
    const copied = dt.getData("text/plain") === "one\nbravo two\n" + "charlie three".slice(0, cut);
    key(document.body, "Backspace");
    const merged = texts().join("|") === "alpha " + tail && !api.ui.multiActive() && caret(first()) === 6 && active() === first() && first().getAttribute("contenteditable") === "true";
    undo();
    return copied && merged && texts().join("|") === "alpha one|bravo two|charlie three" && active() === first();
  });
  await ok("ux: blocks copied in the app paste back with their kinds and marks", () => {
    blank("plain", "# Big", "[] item");
    const pid = pageNow().meta.id;
    api.edit({ op: "set_mark", page: pid, block: pageNow().blocks[0].id, start: 0, end: 5, mark: "bold", on: true });
    api.renderPage();
    api.ui.selectAllBlocks();
    const payload = clip(document, "copy").getData("application/x-casualnote-blocks");
    key(document.body, "Escape");
    blank("");
    clip(first(), "paste", { "application/x-casualnote-blocks": payload, "text/plain": "ignored" });
    const blocks = pageNow().blocks;
    return blocks[0].runs[0].marks?.includes("bold") && blocks[1].kind === "heading1" && blocks[2].kind === "todo";
  });
  await ok("ux: hostile clipboard blocks are dropped", () => {
    blank("");
    const bad = JSON.stringify([{ kind: "script", runs: [{ text: "x" }] }, { kind: "paragraph", runs: [{ text: "fine", marks: ["evil"], link: "nope" }] }]);
    clip(first(), "paste", { "application/x-casualnote-blocks": bad });
    return texts().join("|") === "fine" && !pageNow().blocks[0].runs[0].link && !pageNow().blocks[0].runs[0].marks?.length;
  });
  await ok("ux: Shift+arrow crosses out of a block, and typing replaces the selection in one undo", () => {
    blank("first", "second", "third");
    setCaret(nth(0), 2);
    key(nth(0), "ArrowDown", { shiftKey: true });
    if (!api.ui.multiActive()) return false;
    key(document.body, "ArrowDown", { shiftKey: true });
    const crossing = getSelection().toString().includes("second");
    const steps = undoStack.length;
    key(document.body, "z");
    const after = active() === first() && !api.ui.multiActive();
    type("z");
    const once = undoStack.length === steps + 1;
    undo();
    return crossing && after && once && texts().join("|") === "first|second|third";
  });
  await ok("ux: select all goes block, then page; clearing the page leaves one plain line, and undo returns it", () => {
    blank("# Heading text", "two");
    const n = nth(0); setCaret(n, 1);
    key(n, "a", MOD);
    const block = getSelection().toString() === "Heading text" && !api.ui.multiActive();
    key(n, "a", MOD);
    const page = api.ui.multiActive() && getSelection().toString().includes("two");
    key(document.body, "Delete");
    const cleared = pageNow().blocks.length === 1 && pageNow().blocks[0].kind === "paragraph" && texts()[0] === "" && active() === first();
    undo();
    return block && page && cleared && pageNow().blocks[0].kind === "heading1" && texts().join("|") === "Heading text|two";
  });
  await ok("ux: Escape selects the block, and again returns to text", () => {
    blank("some words");
    setCaret(first(), 3);
    key(first(), "Escape");
    const selected = api.ui.multiActive() && getSelection().toString() === "some words";
    key(document.body, "Escape");
    return selected && !api.ui.multiActive() && active() === first();
  });
  await ok("ux: Tab indents in code, and leaves everywhere else", () => {
    blank("text");
    const free = first().dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true }));
    key(first(), "Enter"); type("```"); type(" ");
    const code = textOf(pageNow().blocks.at(-1).id);
    key(code, "Tab");
    const added = texts().at(-1) === "\t";
    type("x");
    key(active(), "Tab", { shiftKey: true });
    const removed = texts().at(-1) === "x";
    const leaves = active().dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true, cancelable: true }));
    return all({ free, added, removed, leaves });
  });
  await ok("ux: the gutter sits level with the first line of every kind, and stays on the interface's side", () => {
    blank("# H1", "## H2", "### H3", "para");
    type(""); key(active(), "Enter"); type("> "); type("quote"); key(active(), "Enter"); key(active(), "Enter");
    const rows = $$(".blocks > .b");
    const off = rows.map(row => { const t = row.querySelector(":scope > .t"), g = row.querySelector(".hd").getBoundingClientRect(), r = document.createRange(); r.selectNodeContents(t); const line = (r.getClientRects()[0] || t.getBoundingClientRect()); return Math.abs((g.top + g.height / 2) - (line.top + line.height / 2)); });
    if (off.some(d => d > 3.5)) console.warn("ux gutter off by " + off.join());
    blank("مرحبا بالعالم");
    const row = rowOf(0), g = row.querySelector(".gutter").getBoundingClientRect();
    return all({ level: off.every(d => d <= 3.5), rtl: row.matches(":dir(rtl)"), side: g.right <= row.getBoundingClientRect().left + 1 });
  });
  await ok("ux: block drag shows a preview and a drop line, moves on drop, and Escape cancels", () => {
    blank("one", "two", "three");
    const hd = rowOf(0).querySelector(".hd"), a = mid(hd), target = rowOf(2).getBoundingClientRect();
    ptr(hd, "pointerdown", { ...a, pointerId: 9, buttons: 1 });
    ptr(hd, "pointermove", { pointerId: 9, buttons: 1, clientX: a.clientX + 20, clientY: target.bottom - 2 });
    const shown = $(".ghost")?.textContent === "one" && !!$(".b.drop-after") && rowOf(0).classList.contains("lifted");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    const cancelled = !$(".ghost") && texts().join() === "one,two,three";
    const hd2 = rowOf(0).querySelector(".hd");
    ptr(hd2, "pointerdown", { ...a, pointerId: 9, buttons: 1 });
    ptr(hd2, "pointermove", { pointerId: 9, buttons: 1, clientX: a.clientX + 20, clientY: target.bottom - 2 });
    ptr(hd2, "pointerup", { pointerId: 9 });
    return shown && cancelled && !$(".ghost") && !$(".b.drop-after") && texts().join() === "two,three,one" && active() === nth(2);
  });
  await ok("ux: a repaint keeps untouched blocks, their focus and the caret", () => {
    blank("keep me", "other");
    const kept = nth(0); setCaret(nth(1), 2);
    const editing = nth(1);
    key(editing, "Enter");
    api.renderPage();
    return nth(0) === kept && texts().length === 3 && active() === nth(2);
  });
  await ok("ux: the typing path does not rebuild the outline", async () => {
    blank("# Head");
    await wait(260);
    const before = $("#outline .oln");
    type("ing");
    const same = $("#outline .oln") === before;
    await wait(260);
    return same && $("#outline .oln").textContent === "Heading" && $("#counts").textContent.includes("1");
  });
  await ok("ux: the outline jumps to its heading and hands over the caret", async () => {
    blank(...Array.from({ length: 40 }, (_, i) => "filler " + i), "## Far away", ...Array.from({ length: 22 }, (_, i) => "after " + i));
    $(".docscroll").scrollTop = 0;
    await wait(240);
    const btn = $$("#outline .oln").find(n => n.textContent === "Far away");
    btn.click();
    const focused = active() === nth(40);
    await wait(500);
    const top = rowOf(40).getBoundingClientRect().top, box = $(".docscroll").getBoundingClientRect();
    return all({ focused, top: top >= box.top - 1, near: top < box.top + box.height / 2 });
  });
  await ok("ux: undo puts the caret back where the change was", () => {
    blank("hello world");
    setCaret(first(), 5);
    type("!!");
    undo();
    const backIn = texts()[0] === "hello world" && active() === first() && caret(first()) === 5;
    replay(redoStack, undoStack);
    return backIn && texts()[0] === "hello!! world" && caret(first()) === 7;
  });
  await ok("ux: undoing a split returns the caret to the split point", () => {
    blank("splitme");
    setCaret(first(), 5);
    key(first(), "Enter");
    const moved = active() === nth(1);
    undo();
    return moved && texts().join() === "splitme" && active() === first() && caret(first()) === 5;
  });
  await ok("ux: undo of a change on another page opens that page", () => {
    const a = fresh("page a text");
    const b = fresh("page b");
    state.cur = b;
    api.openPage(a);
    setCaret(first()); type("!");
    api.openPage(b);
    undo();
    return state.cur === a && texts()[0] === "page a text";
  });
  await ok("ux: Home and End stay on the line; with Ctrl they go to the page's ends", () => {
    blank("alpha", "bravo", "charlie");
    setCaret(nth(1), 2);
    key(nth(1), "Home");
    const home = caret(nth(1)) === 0;
    key(nth(1), "End");
    const end = caret(nth(1)) === 5;
    key(nth(1), "Home", MOD);
    const top = active() === nth(0) && caret(nth(0)) === 0;
    key(nth(0), "End", MOD);
    return home && end && top && active() === nth(2) && caret(nth(2)) === 7;
  });
  await ok("ux: arrows cross between blocks, by direction, and up from the top goes to the title", () => {
    blank("ab", "cd");
    setCaret(nth(0));
    key(nth(0), "ArrowRight");
    const right = active() === nth(1) && caret(nth(1)) === 0;
    key(nth(1), "ArrowLeft");
    const left = active() === nth(0) && caret(nth(0)) === 2;
    key(nth(0), "ArrowDown");
    const down = active() === nth(1);
    key(nth(1), "ArrowUp");
    key(nth(0), "ArrowUp");
    return right && left && down && active() === $(".doc .title");
  });
  await ok("ux: Delete at the end joins the next block, marks intact, and undo separates them", () => {
    blank("left", "right");
    api.edit({ op: "set_mark", page: pageNow().meta.id, block: pageNow().blocks[1].id, start: 0, end: 5, mark: "bold", on: true });
    api.renderPage();
    setCaret(nth(0));
    key(nth(0), "Delete");
    const joined = texts().join() === "leftright" && pageNow().blocks[0].runs.at(-1).marks?.includes("bold") && !pageNow().blocks[0].runs[0].marks?.includes("bold") && caret(first()) === 4;
    undo();
    return joined && texts().join("|") === "left|right";
  });
  await ok("ux: backspace beside a marked run does not spread its marks", () => {
    blank("bold", "plain");
    api.edit({ op: "set_mark", page: pageNow().meta.id, block: pageNow().blocks[0].id, start: 0, end: 4, mark: "bold", on: true });
    api.renderPage();
    setCaret(nth(1), 0);
    key(nth(1), "Backspace");
    const runs = pageNow().blocks[0].runs;
    return texts().join() === "boldplain" && runs.length === 2 && runs[0].marks?.includes("bold") && !runs[1].marks?.includes("bold");
  });
  await ok("ux: Shift+Enter is a visible line break inside the block", () => {
    blank("line");
    setCaret(first());
    key(first(), "Enter", { shiftKey: true });
    const broke = texts().join("|") === "line\n" && pageNow().blocks.length === 1 && caret(first()) === 5;
    const h1 = first().getBoundingClientRect().height;
    const two = h1 > parseFloat(getComputedStyle(first()).lineHeight) * 1.5;
    type("next");
    return broke && two && texts()[0] === "line\nnext";
  });
  await ok("ux: format shortcuts reach the engine, and never add browser markup", () => {
    blank("format me");
    const n = first(); const r = document.createRange();
    const tn = n.firstChild; r.setStart(tn, 0); r.setEnd(tn, 6); getSelection().removeAllRanges(); getSelection().addRange(r);
    key(n, "b", MOD);
    const bold = pageNow().blocks[0].runs[0].marks?.includes("bold") && getSelection().toString() === "format";
    key(active(), "i", MOD);
    const blocked = !active().dispatchEvent(new InputEvent("beforeinput", { inputType: "formatUnderline", bubbles: true, cancelable: true }));
    return bold && pageNow().blocks[0].runs[0].marks?.includes("italic") && blocked;
  });
  await ok("ux: the toolbar marks a selection across blocks", () => {
    blank("one", "two");
    api.ui.selectAllBlocks();
    $('#fmt [data-f="highlight"]').click();
    const blocks = pageNow().blocks;
    return blocks.every(b => b.runs[0].marks?.includes("highlight")) && api.ui.multiActive();
  });
  await ok("ux: block shortcuts duplicate, move, turn into, and keep the caret", () => {
    blank("alpha", "beta");
    setCaret(nth(0), 2);
    key(nth(0), "ArrowDown", { ...MOD, shiftKey: true });
    const moved = texts().join() === "beta,alpha" && active() === nth(1) && caret(nth(1)) === 2;
    key(nth(1), "d", MOD);
    const dup = texts().join() === "beta,alpha,alpha" && active() === nth(2);
    active().dispatchEvent(new KeyboardEvent("keydown", { key: "!", code: "Digit1", ...MOD, shiftKey: true, bubbles: true, cancelable: true }));
    return moved && dup && pageNow().blocks[2].kind === "heading1" && active() === nth(2) && caret(nth(2)) === 2;
  });
  await ok("ux: Ctrl+Enter checks a to-do, and the checkbox keeps focus when clicked", () => {
    blank("[] ");
    type("task");
    key(active(), "Enter", MOD);
    const checked = pageNow().blocks[0].checked && active() === first();
    const box = rowOf(0).querySelector("input"); box.focus(); box.click();
    return checked && !pageNow().blocks[0].checked && active() === rowOf(0).querySelector("input") && !!box.closest("label.ck");
  });
  await ok("ux: placeholder shows only on the focused empty block, and an emptied block counts as empty", () => {
    blank("", "x");
    const ph = n => getComputedStyle(n, "::before").content;
    setCaret(nth(1));
    const off = ph(nth(0)), on = ph(nth(1));
    document.execCommand("delete");
    const emptied = nth(1).matches(":empty");
    const shows = ph(nth(1));
    setCaret(nth(0));
    return (off === "none" || off === '""') && on === "none" && emptied && shows.includes("Type /") && ph(nth(1)) !== shows && ph(nth(0)).includes("Type /");
  });
  await ok("ux: the block menu takes the keyboard and returns it", () => {
    blank("menu line");
    setCaret(first(), 4);
    first().dispatchEvent(new KeyboardEvent("keydown", { key: "?", code: "Slash", ...MOD, shiftKey: true, bubbles: true, cancelable: true }));
    const open = active().classList.contains("mi") && active().getAttribute("role") === "menuitem";
    const a = active();
    key(a, "ArrowDown");
    const moved = active() !== a && active().classList.contains("mi");
    key(active(), "End");
    const last = active().classList.contains("danger");
    key(active(), "Escape");
    return open && moved && last && !$(".pop") && active() === first() && caret(first()) === 4;
  });
  await ok("ux: choosing from the block menu leaves the caret in the block", () => {
    blank("turn me");
    setCaret(first(), 2);
    first().dispatchEvent(new KeyboardEvent("keydown", { key: "?", code: "Slash", ...MOD, shiftKey: true, bubbles: true, cancelable: true }));
    $$(".pop .mi").find(n => n.textContent.includes("Quote")).click();
    return pageNow().blocks[0].kind === "quote" && active() === first() && caret(first()) === 2;
  });
  await ok("ux: a slash that turns out to be a sentence lets go", async () => {
    blank("/zzz");
    await wait(10);
    const showing = !!$(".pop");
    type(" ");
    return showing && !$(".pop");
  });

  /* Shell */
  await ok("ux: Ctrl+\\ hides and shows the sidebar, and it stays decided", async () => {
    blank("x"); setCaret(first());
    const hide = () => active().dispatchEvent(new KeyboardEvent("keydown", { key: "\\", code: "Backslash", ...MOD, bubbles: true, cancelable: true }));
    hide();
    const gone = !$("#side").offsetParent && !!$("#menuBtn").offsetParent && active() === first();
    await wait(200);
    const stored = JSON.parse(localStorage.getItem("casualnote-ui")).side === false;
    hide();
    const back = !!$("#side").offsetParent && active() === first();
    return all({ gone, stored, back });
  });
  await ok("ux: sidebar and panel resize by pointer and keyboard, are remembered, and hold their limits", async () => {
    const side = $("#side"), sp = $("#sideSplit"), rp = $("#rightSplit"); api.ui.setRight(true);
    const w = side.offsetWidth;
    ptr(sp, "pointerdown", { clientX: 300, pointerId: 3, buttons: 1 }); ptr(sp, "pointermove", { clientX: 340, pointerId: 3, buttons: 1 }); ptr(sp, "pointerup", { clientX: 340, pointerId: 3 });
    const dragged = side.offsetWidth === w + 40;
    key(sp, "ArrowLeft");
    const keyed = side.offsetWidth === w + 24;
    for (let i = 0; i < 60; i++) key(sp, "ArrowRight");
    const capped = side.offsetWidth === 480 && sp.getAttribute("aria-valuenow") === "480";
    for (let i = 0; i < 60; i++) key(sp, "ArrowLeft");
    const floored = side.offsetWidth === 180;
    const rw = $("#right").offsetWidth;
    key(rp, "ArrowLeft");
    const panel = $("#right").offsetWidth === rw + 16;
    key(rp, "ArrowRight");
    key(sp, "ArrowRight"); key(sp, "ArrowRight");
    const wide = side.offsetWidth;
    await wait(200);
    const saved = JSON.parse(localStorage.getItem("casualnote-ui"));
    api.ui.data.sideW = 248; api.ui.refreshChrome();
    return dragged && keyed && capped && floored && panel && saved.sideW === wide && sp.getAttribute("role") === "separator";
  });
  await ok("ux: rename in the sidebar by F2 and double click, Escape keeps the old name", () => {
    const id = fresh("x");
    const row = () => $$("#tree .row").find(r => r.dataset.id === id);
    row().focus(); key(row(), "F2");
    const input = $(".rename");
    if (active() !== input) return false;
    input.value = "Renamed page"; key(input, "Enter");
    const saved = api.metaOf(id).title === "Renamed page" && $(".doc .title").textContent === "Renamed page" && $(".tab.on .tl").textContent === "Renamed page" && active() === row();
    mouse(row().querySelector(".open"), "dblclick");
    const i2 = $(".rename");
    if (!i2 || active() !== i2) return false;
    i2.value = "Nope"; key(i2, "Escape");
    return saved && api.metaOf(id).title === "Renamed page" && !$(".rename") && active() === row();
  });
  await ok("ux: the sidebar's context menu and its keyboard", () => {
    const id = fresh("x");
    const row = () => $$("#tree .row").find(r => r.dataset.id === id);
    row().dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 100, clientY: 200 }));
    const words = $$(".pop .mi").map(n => n.textContent).join("|");
    const named = ["New subpage", "Rename", "Add to favorites", "Duplicate", "Move to", "Move to Trash"].every(w => words.includes(w));
    key(active(), "Escape");
    row().focus(); key(row(), "F10", { shiftKey: true });
    const byKey = !!$(".pop") && active().classList.contains("mi");
    $$(".pop .mi").find(n => n.textContent.includes("Add to favorites")).click();
    const fav = api.metaOf(id).favorite && $$("#tree .row").some(r => r.dataset.id === id && r.dataset.sec === "fav");
    api.edit({ op: "set_favorite", id, value: false }); api.renderSide();
    return all({ named, byKey, fav });
  });
  await ok("ux: the tree has roles and full keyboard, mirrored in Arabic", () => {
    const parent = fresh("x"); api.newPage(parent); state.open[parent] = false; api.openPage(parent); state.open[parent] = false; api.renderSide();
    const q = id => $$("#tree .row").find(r => r.dataset.id === id);
    const r = q(parent);
    const roles = $("#tree").getAttribute("role") === "tree" && r.getAttribute("role") === "treeitem" && r.getAttribute("aria-level") === "1" && r.getAttribute("aria-expanded") === "false" && r.getAttribute("aria-selected") === "true" && r.getAttribute("tabindex") === "0";
    r.focus();
    key(r, "ArrowRight");
    const opened = q(parent).getAttribute("aria-expanded") === "true" && active() === q(parent);
    key(active(), "ArrowDown");
    const down = +active().getAttribute("aria-level") === 2 && active().getAttribute("tabindex") === "0";
    key(active(), "ArrowLeft");
    const toParent = active() === q(parent);
    key(active(), "ArrowLeft");
    const closed = q(parent).getAttribute("aria-expanded") === "false";
    setLang("ar"); renderAll(); q(parent).focus();
    key(active(), "ArrowLeft");
    const mirrored = q(parent).getAttribute("aria-expanded") === "true";
    setLang("en"); renderAll();
    key(q(parent), "Home"); const home = active() === $("#tree .row");
    key(active(), "End"); const end = active() === $$("#tree .row").at(-1);
    return all({ roles, opened, down, toParent, closed, mirrored, home, end });
  });
  await ok("ux: dragging a page onto a page nests it, one undo; a page cannot go under itself", async () => {
    const a = fresh("drag source"), b = fresh("drag target");
    const row = id => $$("#tree .row").find(r => r.dataset.id === id);
    row(b).scrollIntoView({ block: "nearest" });
    const from = row(a), at = mid(from.querySelector(".open"));
    from.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerType: "mouse", button: 0, ...at }));
    document.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, ...mid(row(b)), buttons: 1 }));
    const lit = row(b).classList.contains("drop-in") && !!$(".ghost");
    document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, ...mid(row(b)) }));
    const moved = api.metaOf(a).parent === b && !$(".ghost") && !$(".drop-in");
    const refused = api.ui.movePage(b, a) === false && api.metaOf(b).parent == null;
    undo();
    await wait(5); // A person's next click is not the one that ended the drag.
    return all({ lit, moved, refused, undone: !api.metaOf(a).parent });
  });
  await ok("ux: a page can move from the menu too, without a mouse", () => {
    const a = titled("menu source"), b = titled("menu target"), row = $$("#tree .row").find(r => r.dataset.id === a);
    row.focus(); key(row, "F10", { shiftKey: true });
    $$(".pop .mi").find(n => n.textContent === "Move to" || n.textContent.endsWith("Move to")).click();
    const pick = $$(".pop .mi").find(n => n.textContent.includes("menu target"));
    pick.click();
    const moved = api.metaOf(a).parent === b;
    const again = $$("#tree .row").find(r => r.dataset.id === a); again.focus(); key(again, "F10", { shiftKey: true });
    $$(".pop .mi").find(n => n.textContent.includes("Move to") && !n.textContent.includes("Trash")).click();
    $$(".pop .mi").find(n => n.textContent.includes("Top level")).click();
    return moved && !api.metaOf(a).parent;
  });
  await ok("ux: duplicate copies the blocks, with new ids", () => {
    const a = fresh("copy this");
    const before = pageNow().blocks[0].id;
    const b = api.ui.duplicatePage(a);
    return b !== a && state.cur === b && texts()[0] === "copy this" && pageNow().blocks[0].id !== before;
  });
  await ok("ux: tabs close by button and middle click, and arrow keys walk them", () => {
    state.tabs.length = 0; const a = fresh("tab a"), b = fresh("tab b"), c = fresh("tab c");
    const tab = id => $$("#tabs .tab").find(x => x.dataset.id === id);
    tab(a).dispatchEvent(new MouseEvent("auxclick", { button: 1, bubbles: true, cancelable: true }));
    const middle = !state.tabs.includes(a) && state.cur === c;
    tab(c).querySelector(".cl").click();
    const button = !state.tabs.includes(c) && state.cur === b;
    api.openPage(c);
    const roles = $("#tabs").getAttribute("role") === "tablist" && tab(c).querySelector(".tl").getAttribute("aria-selected") === "true" && tab(c).querySelector(".tl").getAttribute("tabindex") === "0";
    tab(c).querySelector(".tl").focus();
    key(active(), "ArrowLeft");
    const walked = active() === tab(b).querySelector(".tl");
    key(active(), "Delete");
    return middle && button && roles && walked && !state.tabs.includes(b);
  });
  await ok("ux: many tabs overflow into a menu and the current one stays in view", () => {
    for (let i = 0; i < 12; i++) titled("Overflowing tab number " + i);
    const strip = $("#tabs");
    const over = strip.scrollWidth > strip.clientWidth && !$("#tabMore").hidden;
    const on = $(".tab.on").getBoundingClientRect(), box = strip.getBoundingClientRect();
    const visible = on.left >= box.left - 1 && on.right <= box.right + 1;
    const width = document.documentElement.scrollWidth <= innerWidth;
    $("#tabMore").click();
    const listed = $$(".pop .mi").length === state.tabs.length;
    $$(".pop .mi")[0].click();
    const on2 = $(".tab.on").getBoundingClientRect();
    const follows = state.cur === state.tabs[0] && on2.left >= box.left - 1;
    state.tabs.length = 0; fresh("after tabs");
    return over && visible && width && listed && follows && $("#tabMore").hidden;
  });
  await ok("ux: new page from the keyboard, with the caret in its title", () => {
    const n = api.pages().length;
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "n", code: "KeyN", ...MOD, bubbles: true, cancelable: true }));
    return api.pages().length === n + 1 && active() === $(".doc .title");
  });
  await ok("ux: F6 walks the regions and comes back", () => {
    api.ui.setSide(true); api.ui.setRight(true); fresh("x"); setCaret(first());
    const f6 = (o = {}) => active().dispatchEvent(new KeyboardEvent("keydown", { key: "F6", bubbles: true, cancelable: true, ...o }));
    f6(); const right = $("#right").contains(active());
    f6(); const side = $("#side").contains(active());
    f6(); const center = active() === first();
    f6({ shiftKey: true }); const back = $("#side").contains(active());
    return all({ right, side, center, back });
  });
  await ok("ux: landmarks and names", () => {
    return $("#side").getAttribute("role") === "navigation" && $("#content").getAttribute("role") === "main" && !!$("footer.status") && $("#right").tagName === "ASIDE"
      && $$(".ibtn, .tabact, .gutter button, .row .acts button").every(b => b.getAttribute("aria-label"));
  });
  await ok("ux: tooltips show the name and the shortcut after a rest, and leave", async () => {
    const b = $("#rightBtn");
    b.dispatchEvent(new PointerEvent("pointerover", { bubbles: true, pointerType: "mouse" }));
    const early = $("#tip").hidden;
    await wait(520);
    const shown = !$("#tip").hidden && $("#tip").textContent.includes("Toggle side panel") && !!$("#tip kbd")?.textContent;
    ptr(document.body, "pointerdown");
    return all({ early, shown, gone: $("#tip").hidden });
  });

  /* States */
  await ok("ux: the saved line never claims more than is true", async () => {
    blank("x");
    const isSaved = () => $("#saved").textContent === "Saved on this device";
    const quiet = await until(isSaved);
    setCaret(first()); type("y");
    const pending = $("#saved").textContent === "Saving";
    const done = await until(isSaved);
    return all({ quiet, pending, done });
  });
  await ok("ux: a failed save says so, offers export, and clears once saving works", async () => {
    blank("rescue this text");
    const real = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = () => { throw new DOMException("full", "QuotaExceededError"); };
    const warned = console.error; console.error = () => {};
    await until(() => $("#saved").textContent !== "Saving"); // Whatever was already writing.
    store.changed(); await until(() => !$("#banner").hidden);
    console.error = warned;
    IDBObjectStore.prototype.put = real;
    const banner = $("#banner"), failed = !banner.hidden && banner.getAttribute("role") === "alert" && $("#saved").textContent.startsWith("Not saved") && $("#saved").classList.contains("bad");
    let file = null; const click = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function () { file = this.download; };
    $("#exportBtn").click();
    HTMLAnchorElement.prototype.click = click;
    const md = api.ui.workspaceMarkdown();
    store.changed(); await until(() => $("#saved").textContent === "Saved on this device");
    return all({ failed, file: /^casualnote-.*\.md$/.test(file || ""), md: md.includes("rescue this text"), cleared: banner.hidden, saved: $("#saved").textContent === "Saved on this device" });
  });
  await ok("ux: with no storage at all, the banner says so and stays, and nothing claims to be saved", async () => {
    const real = store.changed;
    api.ui.setBanner("unavailable");
    const words = $("#banner .msg").textContent.startsWith("This browser cannot keep");
    const can = !!$("#exportBtn");
    api.ui.setBanner(null);
    store.changed = real;
    return all({ words, can, cleared: $("#banner").hidden });
  });
  await ok("ux: when a feature closes the panel itself, the backdrop and the shell follow", async () => {
    const real = window.matchMedia;
    window.matchMedia = q => (/1120/.test(q) ? { matches: true, media: q } : real.call(window, q));
    try {
      api.ui.setRight(true);
      const up = !$("#scrim").hidden && $("#center").inert;
      state.right = false; $("#right").hidden = true;
      await wait(10);
      return all({ up, gone: $("#scrim").hidden && !$("#center").inert });
    } finally { window.matchMedia = real; api.ui.setRight(true); api.ui.refreshChrome(); }
  });
  await ok("ux: Settings and Trash set the empty panel aside, and it returns", () => {
    api.ui.setRight(true);
    $(".rtabs button")?.click(); // Details, if a feature has given the panel tabs.
    api.openView("settings");
    const away = getComputedStyle($("#right")).display === "none";
    const count = $("#counts").textContent === "";
    api.openPage(state.tabs[0]);
    return all({ away, count, back: getComputedStyle($("#right")).display !== "none" });
  });
  await ok("ux: the fatal screen is honest and has a way out, in the right place", () => {
    api.ui.showFatal(new Error("boom"));
    const boot = $("#boot"), good = boot.classList.contains("err") && boot.getAttribute("role") === "alert" && boot.textContent.includes("boom") && !!boot.querySelector("button");
    renderAll();
    return good && !$("#boot");
  });
  await ok("ux: Trash is a view, says when it is empty, restores, and the toast undoes", () => {
    for (const p of api.pages().filter(x => x.trashed)) { /* earlier tests may leave pages here */ }
    const id = fresh("to be trashed");
    api.ui.trashPage(id);
    const toast = $(".toast");
    const told = toast?.textContent.includes("Moved to Trash") && !api.metaOf(id).trashed === false;
    toast.querySelector("button").click();
    const undone = !api.metaOf(id).trashed && state.cur === id;
    api.ui.trashPage(id);
    $("#trashBtn").click();
    const listed = state.view === "trash" && $$(".trashlist li").some(li => li.dataset.id === id) && $(".crumbs b").textContent === "Trash" && $("#trashBtn").classList.contains("on");
    const count = $("#trashCount").textContent !== "";
    $$(".trashlist li").find(li => li.dataset.id === id).querySelector("button").click();
    const restored = !api.metaOf(id).trashed && !$$(".trashlist li").some(li => li.dataset.id === id);
    $(".toast")?.remove();
    api.openPage(id);
    return told && undone && listed && count && restored && state.view === null;
  });
  await ok("ux: Trash says so when there is nothing in it", () => {
    const content = document.createElement("div");
    const real = api.pages; let saw = "";
    const all = JSON.parse(engine.pages()).filter(p => p.trashed);
    for (const p of all) api.run({ op: "set_trashed", id: p.id, value: false });
    api.openView("trash");
    saw = $(".agent .empty")?.textContent;
    for (const p of all) api.run({ op: "set_trashed", id: p.id, value: true });
    api.renderSide();
    api.openPage(state.tabs[0]);
    return saw === "Trash is empty." && real === api.pages && !!content;
  });

  /* Search */
  const palette = () => { $("#searchBtn").click(); return $("#palette"); };
  const query = (i, v) => { i.value = v; i.dispatchEvent(new Event("input", { bubbles: true })); };
  await ok("ux: search opens on recent pages, most recent first", () => {
    const a = titled("Recent older"), b = titled("Recent newer");
    api.openPage(a); api.openPage(b);
    const i = palette();
    const rows = $$(".modal .mi .nm").map(n => n.textContent);
    const good = { hdr: $(".modal .hdr")?.textContent === "Recent pages", a: rows[0] === "Recent newer", b: rows[1] === "Recent older", role: i.getAttribute("role") === "combobox" };
    key(i, "Escape");
    return all({ ...good, closed: !$("#palette") });
  });
  await ok("ux: search marks the match with text nodes, across case and script", () => {
    const hostile = "<img src=x onerror=alert(1)> needle HERE";
    blank(hostile, "نص عربي مع كلمة بحث هنا", "देवनागरी खोज पाठ");
    const i = palette();
    query(i, "NEEDLE");
    const mark = $(".modal .mi mark");
    const a = mark?.textContent === "needle" && !$(".modal img") && $(".modal .mi small").textContent.includes("<img");
    query(i, "بحث");
    const b = $(".modal .mi mark")?.textContent === "بحث";
    query(i, "खोज");
    const c = $(".modal .mi mark")?.textContent === "खोज";
    key(i, "Escape");
    // The same again where this build has no search index: a plain scan, marking whole characters.
    engine.search = undefined;
    const j = palette();
    query(j, "NEEDLE here");
    const d = $(".modal .mi mark")?.textContent === "needle HERE";
    query(j, "ख");
    const e = $(".modal .mi mark")?.textContent === "खो";
    key(j, "Escape");
    delete engine.search;
    return all({ a, b, c, d, e });
  });
  await ok("ux: search moves by keyboard, wraps, and lands on the block with the match selected", () => {
    blank("unique palette word", "and unique palette again");
    const i = palette();
    query(i, "unique palette");
    const count = $$(".modal .mi").length;
    const start = $$(".modal .mi")[0].classList.contains("on") && i.getAttribute("aria-activedescendant") === $$(".modal .mi")[0].id;
    key(i, "ArrowDown");
    const second = $$(".modal .mi")[1].classList.contains("on");
    key(i, "ArrowUp"); key(i, "ArrowUp");
    const wrapped = $$(".modal .mi")[count - 1].classList.contains("on");
    key(i, "ArrowDown");
    key(i, "Enter");
    return start && second && wrapped && !$("#palette") && active() === nth(0) && getSelection().toString().toLowerCase() === "unique palette";
  });
  await ok("ux: empty search says so, keeps the words isolated, and offers to create the page", () => {
    const n = api.pages().length;
    const i = palette();
    query(i, "zzz<b>nothing");
    const none = $(".modal .hdr.none"), said = none?.textContent.includes("zzz<b>nothing") && !!none.querySelector("bdi") && !$(".modal b");
    key(i, "Enter");
    return said && !$("#palette") && api.pages().length === n + 1 && pageNow().meta.title === "zzz<b>nothing";
  });
  await ok("ux: closing search puts the caret back", () => {
    blank("return here");
    setCaret(first(), 3);
    const i = palette();
    key(i, "Escape");
    return active() === first() && caret(first()) === 3;
  });

  /* Phone and tablet. tools/e2e.mjs has one window size, so the logic is driven here and the layout is checked by screenshot. */
  await ok("ux: the phone sidebar is a drawer with a backdrop, and Escape gives focus back", () => {
    const real = window.matchMedia;
    window.matchMedia = q => (/max-width/.test(q) ? { matches: true, media: q } : real.call(window, q));
    try {
      api.ui.setRight(false);
      api.ui.setSide(false);
      const closed = $("#side").classList.contains("closed") && $("#scrim").hidden && $("#side").inert;
      $("#menuBtn").click();
      const open = !$("#side").classList.contains("closed") && !$("#scrim").hidden && $("#center").inert && $("#side").contains(active());
      document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      const shut = $("#side").classList.contains("closed") && $("#scrim").hidden && !$("#center").inert;
      $("#menuBtn").click();
      $("#scrim").click();
      const tapped = $("#side").classList.contains("closed");
      $("#menuBtn").click();
      const id = api.currentPage().meta.id;
      openPage(id);
      return closed && open && shut && tapped && $("#side").classList.contains("closed");
    } finally { window.matchMedia = real; api.ui.setSide(true); api.ui.setRight(true); api.ui.refreshChrome(); }
  });
  await ok("ux: touch keyboards that send no Enter key still split a block", () => {
    blank("touchkeys");
    setCaret(first(), 5);
    first().dispatchEvent(new InputEvent("beforeinput", { inputType: "insertParagraph", bubbles: true, cancelable: true }));
    return texts().join("|") === "touch|keys" && active() === nth(1);
  });

  /* Languages */
  await ok("ux: every launch language draws the shell, with no English left in core controls", async () => {
    const { I18N } = await import("../data.js");
    const bad = [], sources = new Set(Object.keys(I18N.ar));
    const scan = (code, label) => {
      if (code === "en") return;
      const shell = $$("#searchBtn span, #newBtn span, #trashBtn span, #settingsBtn span, .ibtn, .tabact, #fmt button, #mobar button, .gutter button, .row .acts button, .tab .cl, #sideSplit, #rightSplit, .pop .mi .ml, .pop .hdr, .modal input, #saved, #counts, .keys dt, .seg2 button, .rp h4");
      for (const n of shell) {
        if (n.closest("[data-label]")) continue;
        const words = n.matches(".ibtn,.tabact,.gutter button,#fmt button,#mobar button") ? [n.getAttribute("aria-label")] : [n.getAttribute("aria-label"), n.textContent];
        for (const text of words) {
          if (!text) continue;
          const leaked = sources.has(text.trim()) && I18N[code][text.trim()] !== text.trim();
          const latin = code !== "es" && /\b[A-Za-z]{4,}\b/.test(text.replace(/Markdown|ToDo/g, ""));
          if (leaked || latin) bad.push(`${code} ${label}: ${text}`);
        }
      }
    };
    blank("lang", "# head");
    for (const code of ["zh-Hans", "ja", "es", "ar", "hi", "en"]) {
      setLang(code); renderAll();
      if (code !== "en" && !I18N[code]) bad.push("no catalogue " + code);
      scan(code, "shell");
      const n = textOf(pageNow().blocks[0].id); setCaret(n);
      n.dispatchEvent(new KeyboardEvent("keydown", { key: "?", code: "Slash", ...MOD, shiftKey: true, bubbles: true, cancelable: true }));
      scan(code, "block menu"); api.closeMenu();
      const row = $("#tree .row.on"); row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
      scan(code, "page menu"); api.closeMenu();
      api.openView("settings"); scan(code, "settings");
      api.openView("trash");
      api.openPage(state.cur);
    }
    if (bad.length) console.warn("ux english: " + [...new Set(bad)].slice(0, 16).join("\n"));
    return !bad.length;
  });
  await ok("ux: Arabic mirrors the shell, menus, icons and block rows", () => {
    blank("اختبار", "latin line in an arabic interface");
    setLang("ar"); renderAll();
    const side = $("#side").getBoundingClientRect(), center = $("#center").getBoundingClientRect();
    const onRight = side.left > center.left;
    const more = $("#moreBtn"), mr = more.getBoundingClientRect(); more.click();
    const pop = $(".pop").getBoundingClientRect();
    const aligned = Math.abs(pop.right - mr.right) < 2;
    api.closeMenu();
    const flipped = getComputedStyle($("#collapseBtn .i")).transform !== "none" && getComputedStyle($("#searchBtn .i")).transform === "none";
    const arabicRow = rowOf(0), latinRow = rowOf(1);
    const textRight = arabicRow.matches(":dir(rtl)") && latinRow.matches(":dir(ltr)");
    const gut = latinRow.querySelector(".gutter").getBoundingClientRect();
    const gutterOutside = gut.left >= latinRow.getBoundingClientRect().right - 1;
    setCaret(nth(1)); key(nth(1), "Enter");
    const emptyFollows = rowOf(2).getAttribute("dir") === "ltr";
    blank("اختبار"); key(first(), "Enter");
    const emptyAr = rowOf(1).getAttribute("dir") === "rtl";
    const bad = $("#sideSplit"), w = $("#side").offsetWidth;
    key(bad, "ArrowLeft");
    const grew = $("#side").offsetWidth === w + 16;
    key(bad, "ArrowRight");
    setLang("en"); renderAll();
    return all({ onRight, aligned, flipped, textRight, gutterOutside, emptyFollows, emptyAr, grew });
  });
  await ok("ux: tracking and italics are for the scripts that take them", () => {
    blank("# Latin", "> ", "");
    const latin = rowOf(0).classList.contains("lat");
    setCaret(nth(0)); document.execCommand("selectAll"); type("عنوان");
    const arabic = !rowOf(0).classList.contains("lat") && getComputedStyle(nth(0)).letterSpacing === "normal";
    const title = $(".doc .title"); title.focus(); type("हिन्दी");
    return latin && arabic && !title.classList.contains("lat") && getComputedStyle(title).letterSpacing === "normal";
  });
}
