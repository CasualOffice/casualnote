// The web app. The Rust engine holds the notes; this file projects them to the DOM and turns
// input into operations. Document text only ever reaches the DOM as text.
import init, { Engine } from "./pkg/casual_note_wasm.js";
import { ICONS, I18N } from "./data.js";
import { openStore } from "./store.js";

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const uid = () => [...crypto.getRandomValues(new Uint8Array(12))].map(b => b.toString(16).padStart(2, "0")).join("");
const SVG = "http://www.w3.org/2000/svg";
const LANGS = [["en", "English"], ["zh-Hans", "简体中文"], ["ja", "日本語"], ["es", "Español"], ["ar", "العربية"], ["hi", "हिन्दी"]];

function el(tag, props = {}, ...kids) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === "class") n.className = v;
    else if (k === "text") n.textContent = v;
    else if (k.startsWith("on")) n.addEventListener(k.slice(2), v);
    else if (v !== false && v != null) n.setAttribute(k, v === true ? "" : v);
  }
  for (const c of kids) if (c != null) n.append(c);
  return n;
}
function icon(name) {
  const s = document.createElementNS(SVG, "svg");
  s.setAttribute("class", "i"); s.setAttribute("viewBox", "0 0 256 256"); s.setAttribute("aria-hidden", "true");
  // Fixed icon data shipped with the app, never document content.
  const doc = new DOMParser().parseFromString(`<svg xmlns="${SVG}">${ICONS[name] || ""}</svg>`, "image/svg+xml");
  s.append(...[...doc.documentElement.childNodes].map(c => document.importNode(c, true)));
  return s;
}

/* ---------- Engine ---------- */
let engine, store, lang = "en";
const t = k => I18N[lang]?.[k] ?? k;
const state = { cur: null, tabs: [], open: {}, view: null, right: true };
const undo = [], redo = [];
let burst = null;

const pages = () => JSON.parse(engine.pages());
const live = () => pages().filter(p => !p.trashed);
const pageData = id => JSON.parse(engine.page(id));
const metaOf = id => pages().find(p => p.id === id);

function run(op) {
  const inverse = JSON.parse(engine.apply(JSON.stringify(op)));
  store.changed();
  return inverse;
}
/** Runs edits as one undo step. */
function edit(...ops) {
  const inverses = ops.map(run);
  undo.push(inverses); redo.length = 0; burst = null;
}
function typed(op, key) {
  const inverse = run(op);
  const now = Date.now();
  if (burst && burst.key === key && now - burst.at < 700) { burst.entry.push(inverse); burst.at = now; }
  else { const entry = [inverse]; undo.push(entry); redo.length = 0; burst = { key, at: now, entry }; }
}
function replay(from, to) {
  const entry = from.pop(); if (!entry) return;
  burst = null;
  to.push([...entry].reverse().map(run));
  renderAll();
}

/* ---------- Text helpers: browser strings are UTF-16, as the engine's positions are ---------- */
const plain = b => b.runs.map(r => r.text).join("");
const isLow = c => c >= 0xDC00 && c <= 0xDFFF;
function diff(a, b) {
  let s = 0; const max = Math.min(a.length, b.length);
  while (s < max && a.charCodeAt(s) === b.charCodeAt(s)) s++;
  if (s > 0 && s < Math.max(a.length, b.length) && (isLow(a.charCodeAt(s)) || isLow(b.charCodeAt(s)))) s--;
  let e = 0;
  while (e < max - s && a.charCodeAt(a.length - 1 - e) === b.charCodeAt(b.length - 1 - e)) e++;
  if (e > 0 && (isLow(a.charCodeAt(a.length - e)) || isLow(b.charCodeAt(b.length - e)))) e--;
  return { at: s, delete: a.length - s - e, insert: b.slice(s, b.length - e) };
}
function paintRuns(node, runs) {
  node.replaceChildren();
  for (const r of runs) {
    let n = document.createTextNode(r.text);
    const m = r.marks || [];
    if (m.includes("code")) n = el("code", {}, n);
    if (m.includes("highlight")) n = el("mark", {}, n);
    if (m.includes("italic")) n = el("i", {}, n);
    if (m.includes("bold")) n = el("b", {}, n);
    node.append(n);
  }
}
function caretOffset(node) {
  const s = getSelection(); if (!s.rangeCount || !node.contains(s.anchorNode)) return 0;
  const r = document.createRange(); r.selectNodeContents(node); r.setEnd(s.anchorNode, s.anchorOffset);
  return r.toString().length;
}
function setCaret(node, offset = Infinity) {
  node.focus();
  const sel = getSelection(), r = document.createRange();
  const w = document.createTreeWalker(node, NodeFilter.SHOW_TEXT); let left = offset, n, done = false;
  while ((n = w.nextNode())) { if (left <= n.length) { r.setStart(n, left); done = true; break; } left -= n.length; }
  if (!done) { r.selectNodeContents(node); r.collapse(false); } else r.collapse(true);
  sel.removeAllRanges(); sel.addRange(r);
}
function selectionOffsets(node) {
  const s = getSelection(); if (!s.rangeCount) return null;
  const range = s.getRangeAt(0);
  if (!node.contains(range.startContainer) || !node.contains(range.endContainer)) return null;
  const pre = document.createRange(); pre.selectNodeContents(node); pre.setEnd(range.startContainer, range.startOffset);
  const start = pre.toString().length;
  return { start, end: start + range.toString().length };
}
function sliceRuns(runs, from, to = Infinity) {
  const out = []; let at = 0;
  for (const r of runs) {
    const a = Math.max(from, at), b = Math.min(to, at + r.text.length);
    if (a < b) out.push({ text: r.text.slice(a - at, b - at), marks: r.marks || [] });
    at += r.text.length;
  }
  return out;
}

/* ---------- Blocks ---------- */
const KINDS = [
  ["paragraph", "text-t", "Text"], ["heading1", "text-h-one", "Heading 1"], ["heading2", "text-h-two", "Heading 2"], ["heading3", "text-h-three", "Heading 3"],
  ["bulleted", "list-bullets", "Bulleted list"], ["numbered", "list-numbers", "Numbered list"], ["todo", "check-square", "To-do"], ["quote", "quotes", "Quote"],
  ["callout", "lightbulb", "Callout"], ["code", "code", "Code"], ["divider", "minus", "Divider"],
];
const CLS = { paragraph: "p", heading1: "h1", heading2: "h2", heading3: "h3", bulleted: "ul", numbered: "ol", todo: "todo", quote: "quote", callout: "callout", code: "code", divider: "hr" };
const MD = { "#": "heading1", "##": "heading2", "###": "heading3", "-": "bulleted", "*": "bulleted", "1.": "numbered", "[]": "todo", ">": "quote", "```": "code" };
let composing = false, page = null;
const blockOf = id => page.blocks.find(b => b.id === id);
const textOf = id => $(`.b[data-id="${id}"] > .t`);

function reload() { page = pageData(state.cur); }
function syncText(b, node) {
  const now = node.textContent, old = plain(b);
  if (now === old) return false;
  const d = diff(old, now);
  typed({ op: "splice_text", page: page.meta.id, block: b.id, ...d }, b.id);
  const id = b.id; reload();
  Object.assign(b, blockOf(id) || {});
  return true;
}
function blockEl(b, n) {
  const pid = page.meta.id;
  const row = el("div", { class: `b ${CLS[b.kind]}${b.checked ? " done" : ""}`, "data-id": b.id });
  row.append(el("div", { class: "gutter" },
    el("button", { class: "add", "aria-label": "Add block", onclick: () => { const id = uid(); edit({ op: "insert_block", page: pid, after: b.id, block: { id, kind: "paragraph", checked: false, runs: [] } }); renderPage(); setCaret(textOf(id)); } }, icon("plus")),
    handle(b)));
  if (b.kind === "divider") { row.append(el("hr")); return row; }
  if (b.kind === "bulleted") row.append(el("span", { class: "mk", text: "•" }));
  if (b.kind === "numbered") row.append(el("span", { class: "mk", text: new Intl.NumberFormat(lang).format(n) + "." }));
  if (b.kind === "callout") row.append(icon("lightbulb"));
  if (b.kind === "todo") row.append(el("input", { type: "checkbox", "aria-label": t("To-do"), ...(b.checked ? { checked: true } : {}), onchange: e => { edit({ op: "set_block", page: pid, block: b.id, kind: b.kind, checked: e.target.checked }); renderPage(); } }));
  const node = el("div", { class: "t ph", contenteditable: "true", role: "textbox", dir: "auto", spellcheck: "true", "data-ph": b.kind === "paragraph" ? t("Type / for blocks, [[ to link a page").split(",")[0] : "" });
  paintRuns(node, b.runs);
  node.addEventListener("compositionstart", () => { composing = true; });
  node.addEventListener("compositionend", () => {
    composing = false;
    // Read now: a Korean input method can start the next syllable straight after this event.
    const cur = blockOf(b.id); if (cur) { syncText(cur, node); afterText(cur, node); }
  });
  node.addEventListener("input", () => {
    if (composing) return;
    const cur = blockOf(b.id); if (!cur) return;
    syncText(cur, node); afterText(cur, node);
  });
  node.addEventListener("paste", e => { e.preventDefault(); document.execCommand("insertText", false, e.clipboardData.getData("text/plain").replace(/\r?\n+/g, " ")); });
  node.addEventListener("keydown", e => blockKeys(e, b.id, node));
  node.addEventListener("blur", () => { if (!composing) { const cur = blockOf(b.id); if (cur) paintRuns(node, cur.runs); } });
  row.append(node);
  return row;
}
function afterText(b, node) {
  const text = plain(b);
  const md = MD[text.replace(/ /g, " ").match(/^(\S+) $/)?.[1]];
  if (md && b.kind === "paragraph") {
    edit({ op: "splice_text", page: page.meta.id, block: b.id, at: 0, delete: text.length, insert: "" }, { op: "set_block", page: page.meta.id, block: b.id, kind: md, checked: false });
    renderPage(); setCaret(textOf(b.id)); return;
  }
  const before = text.slice(0, caretOffset(node));
  const slash = before.match(/(?:^|\s)\/([\p{L}\p{N} ]*)$/u);
  if (slash) slashMenu(b, node, slash[1], slash[0].trimStart().length, before.length);
  else if (menuState?.keys) closeMenu();
  renderMeta();
}
function blockKeys(e, id, node) {
  if (e.isComposing || composing) return;
  const b = blockOf(id); if (!b) return;
  if (menuState?.keys) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); const n = menuState.items.length; if (n) { menuState.on = (menuState.on + (e.key === "ArrowDown" ? 1 : n - 1)) % n; paintMenu(); } return; }
    if (e.key === "Enter") { e.preventDefault(); const it = menuState.items[menuState.on]; closeMenu(); it?.run(); return; }
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenu(); return; }
  }
  const pid = page.meta.id, i = page.blocks.indexOf(b), sel = getSelection();
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    if (b.kind === "code") { document.execCommand("insertText", false, "\n"); return; }
    syncText(b, node);
    const cur = blockOf(id), text = plain(cur);
    if (!text && cur.kind !== "paragraph") { edit({ op: "set_block", page: pid, block: id, kind: "paragraph", checked: false }); renderPage(); setCaret(textOf(id)); return; }
    const offs = selectionOffsets(node) || { start: text.length, end: text.length };
    const nid = uid(), kind = ["bulleted", "numbered", "todo"].includes(cur.kind) ? cur.kind : "paragraph";
    const tail = sliceRuns(cur.runs, offs.end);
    edit(
      { op: "splice_text", page: pid, block: id, at: offs.start, delete: text.length - offs.start, insert: "" },
      { op: "insert_block", page: pid, after: id, block: { id: nid, kind, checked: false, runs: tail } });
    renderPage(); setCaret(textOf(nid), 0);
  } else if (e.key === "Backspace" && sel.isCollapsed && caretOffset(node) === 0) {
    syncText(b, node);
    const cur = blockOf(id);
    if (cur.kind !== "paragraph") { e.preventDefault(); edit({ op: "set_block", page: pid, block: id, kind: "paragraph", checked: false }); renderPage(); setCaret(textOf(id), 0); return; }
    if (i === 0) return;
    e.preventDefault();
    const prev = page.blocks[i - 1];
    if (prev.kind === "divider") { edit({ op: "delete_block", page: pid, block: prev.id }); renderPage(); setCaret(textOf(id), 0); return; }
    const at = plain(prev).length, text = plain(cur), ops = [{ op: "splice_text", page: pid, block: prev.id, at, delete: 0, insert: text }];
    let o = at;
    for (const r of cur.runs) { for (const m of r.marks || []) ops.push({ op: "set_mark", page: pid, block: prev.id, start: o, end: o + r.text.length, mark: m, on: true }); o += r.text.length; }
    ops.push({ op: "delete_block", page: pid, block: id });
    edit(...ops); renderPage(); setCaret(textOf(prev.id), at);
  } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !e.shiftKey) {
    const r = sel.rangeCount ? sel.getRangeAt(0).getClientRects()[0] : null, box = node.getBoundingClientRect();
    const up = e.key === "ArrowUp";
    if (!r || (up ? r.top - box.top < 12 : box.bottom - r.bottom < 12)) {
      const list = up ? page.blocks.slice(0, i).reverse() : page.blocks.slice(i + 1);
      const next = list.find(x => x.kind !== "divider");
      if (next) { e.preventDefault(); setCaret(textOf(next.id), up ? Infinity : 0); }
    }
  }
}
function handle(b) {
  const h = el("button", { class: "hd", "aria-label": "Move or change block" }, icon("dots-six-vertical"));
  h.addEventListener("pointerdown", e => {
    e.preventDefault(); h.setPointerCapture(e.pointerId);
    const sx = e.clientX, sy = e.clientY; let drag = false, target = null, after = false;
    const clear = () => $$(".drop-before,.drop-after").forEach(n => n.classList.remove("drop-before", "drop-after"));
    const mv = ev => {
      if (!drag && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 4) return;
      drag = true; clear();
      const over = document.elementFromPoint(ev.clientX, ev.clientY)?.closest(".b");
      if (!over) { target = null; return; }
      const r = over.getBoundingClientRect(); after = ev.clientY > r.top + r.height / 2; target = over.dataset.id;
      over.classList.add(after ? "drop-after" : "drop-before");
    };
    const up = () => {
      h.removeEventListener("pointermove", mv); h.removeEventListener("pointerup", up); clear();
      if (!drag) { blockMenu(b, h.getBoundingClientRect()); return; }
      if (target && target !== b.id) {
        const list = page.blocks.filter(x => x.id !== b.id), idx = list.findIndex(x => x.id === target);
        const prev = after ? list[idx] : list[idx - 1];
        edit({ op: "move_block", page: page.meta.id, block: b.id, after: prev ? prev.id : null }); renderPage();
      }
    };
    h.addEventListener("pointermove", mv); h.addEventListener("pointerup", up);
  });
  return h;
}
function blockMenu(b, rect) {
  const pid = page.meta.id, items = [];
  if (b.kind !== "divider") KINDS.slice(0, 10).filter(k => k[0] !== b.kind).forEach(([kind, gl, label]) => items.push({ gl, label: t(label), run: () => { edit({ op: "set_block", page: pid, block: b.id, kind, checked: false }); renderPage(); } }));
  items.push({ gl: "trash", label: "Delete", danger: true, run: () => { const ops = [{ op: "delete_block", page: pid, block: b.id }]; if (page.blocks.length === 1) ops.push({ op: "insert_block", page: pid, after: null, block: { id: uid(), kind: "paragraph", checked: false, runs: [] } }); edit(...ops); renderPage(); } });
  menu(rect, items);
}
function slashMenu(b, node, q, len, caret) {
  const pid = page.meta.id, ql = q.toLowerCase();
  const items = KINDS.filter(k => (k[2] + " " + t(k[2])).toLowerCase().includes(ql)).map(([kind, gl, label]) => ({ gl, label: t(label), run: () => {
    const ops = [{ op: "splice_text", page: pid, block: b.id, at: caret - len, delete: len, insert: "" }];
    if (kind === "divider") {
      const nid = uid();
      ops.push({ op: "insert_block", page: pid, after: b.id, block: { id: uid(), kind: "divider", checked: false, runs: [] } });
      edit(...ops); reload();
      const hr = page.blocks[page.blocks.findIndex(x => x.id === b.id) + 1];
      edit({ op: "insert_block", page: pid, after: hr.id, block: { id: nid, kind: "paragraph", checked: false, runs: [] } });
      renderPage(); setCaret(textOf(nid)); return;
    }
    ops.push({ op: "set_block", page: pid, block: b.id, kind, checked: false });
    edit(...ops); renderPage(); setCaret(textOf(b.id));
  } }));
  const r = getSelection().getRangeAt(0).getClientRects()[0] || node.getBoundingClientRect();
  menu(r, items, { keys: true, hdr: t("Blocks") });
}

/* ---------- Menus ---------- */
let menuState = null;
function closeMenu() { if (menuState) { menuState.node.remove(); menuState = null; } }
function menu(rect, items, opts = {}) {
  closeMenu();
  const node = el("div", { class: "pop", role: "menu" });
  menuState = { node, items, on: 0, keys: !!opts.keys, hdr: opts.hdr };
  paintMenu(); document.body.append(node);
  const w = node.offsetWidth, h = node.offsetHeight;
  node.style.left = Math.max(8, Math.min(rect.left, innerWidth - w - 8)) + "px";
  node.style.top = (rect.bottom + h + 8 > innerHeight ? Math.max(8, rect.top - h - 4) : rect.bottom + 4) + "px";
}
function paintMenu() {
  const { node, items, hdr, keys, on } = menuState; node.replaceChildren();
  if (hdr) node.append(el("div", { class: "hdr", text: hdr }));
  if (!items.length) node.append(el("div", { class: "hdr", text: t("No matches") }));
  items.forEach((it, i) => node.append(el("button", { class: "mi" + (keys && i === on ? " on" : "") + (it.danger ? " danger" : ""), role: "menuitem", onmousedown: e => e.preventDefault(), onclick: () => { closeMenu(); it.run(); } },
    it.gl ? el("span", { class: "gl" }, icon(it.gl)) : null, el("span", { text: it.label }))));
}
document.addEventListener("pointerdown", e => {
  if (menuState && !e.target.closest(".pop")) closeMenu();
  if (!e.target.closest(".fmt") && !e.target.closest(".t")) $("#fmt").hidden = true;
}, true);

/* Selection toolbar */
document.addEventListener("selectionchange", () => {
  const s = getSelection(), f = $("#fmt");
  if (!s.rangeCount || s.isCollapsed) { f.hidden = true; return; }
  const a = s.anchorNode, node = (a.nodeType === 1 ? a : a.parentElement)?.closest(".t");
  if (!node || node.closest(".code")) { f.hidden = true; return; }
  const r = s.getRangeAt(0).getBoundingClientRect();
  f.hidden = false;
  f.style.left = Math.max(8, Math.min(r.left, innerWidth - f.offsetWidth - 8)) + "px";
  f.style.top = Math.max(8, r.top - f.offsetHeight - 6) + "px";
});
$("#fmt").addEventListener("mousedown", e => e.preventDefault());
$("#fmt").addEventListener("click", e => {
  const btn = e.target.closest("button"); if (!btn) return;
  const s = getSelection(), a = s.anchorNode, node = (a?.nodeType === 1 ? a : a?.parentElement)?.closest(".t"); if (!node) return;
  const offs = selectionOffsets(node); if (!offs || offs.start === offs.end) return;
  const id = node.closest(".b").dataset.id, b = blockOf(id), mark = btn.dataset.f;
  const inside = sliceRuns(b.runs, offs.start, offs.end);
  const on = !inside.every(r => (r.marks || []).includes(mark));
  edit({ op: "set_mark", page: page.meta.id, block: id, start: offs.start, end: offs.end, mark, on });
  reload(); paintRuns(node, blockOf(id).runs);
});

/* ---------- Page ---------- */
function renderBlocks() {
  const box = $(".blocks"); if (!box) return;
  let n = 0;
  box.replaceChildren(...page.blocks.map(b => { n = b.kind === "numbered" ? n + 1 : 0; return blockEl(b, n); }));
}
function renderPage() {
  reload();
  if ($(".blocks")?.dataset.page === state.cur) { renderBlocks(); renderMeta(); return; }
  const title = el("h1", { class: "title", contenteditable: "true", dir: "auto", "aria-label": "Page title", text: page.meta.title });
  title.addEventListener("input", () => { typed({ op: "set_title", id: page.meta.id, title: title.textContent }, "title:" + page.meta.id); reload(); renderSide(); renderTabs(); renderCrumbs(); });
  title.addEventListener("paste", e => { e.preventDefault(); document.execCommand("insertText", false, e.clipboardData.getData("text/plain").replace(/\r?\n+/g, " ")); });
  title.addEventListener("keydown", e => { if (e.key === "Enter") { e.preventDefault(); const f = page.blocks.find(b => b.kind !== "divider"); if (f) setCaret(textOf(f.id), 0); } });
  const tail = el("div", { class: "tail", onclick: () => {
    const last = page.blocks[page.blocks.length - 1];
    if (last && last.kind !== "divider" && !plain(last)) { setCaret(textOf(last.id)); return; }
    const id = uid(); edit({ op: "insert_block", page: page.meta.id, after: last ? last.id : null, block: { id, kind: "paragraph", checked: false, runs: [] } }); renderPage(); setCaret(textOf(id));
  } });
  const blocks = el("div", { class: "blocks", "data-page": state.cur });
  const doc = el("div", { class: "doc" }, title, blocks, tail);
  $("#content").replaceChildren(el("div", { class: "docscroll" }, doc));
  renderBlocks(); renderMeta();
}
function renderMeta() {
  if (!page) return;
  const box = $("#outline"); box.replaceChildren();
  const heads = page.blocks.filter(b => /^heading/.test(b.kind) && plain(b));
  if (!heads.length) box.append(el("p", { text: t("Headings appear here.") }));
  heads.forEach(b => { const btn = el("button", { class: "oln", text: plain(b), onclick: () => $(`.b[data-id="${b.id}"]`)?.scrollIntoView({ block: "start" }) }); btn.style.paddingInlineStart = (Math.max(0, +b.kind.slice(-1) - 2) ? 10 : 0) + "px"; box.append(btn); });
  const text = [page.meta.title, ...page.blocks.map(plain)].join(" ");
  const words = [...new Intl.Segmenter(lang, { granularity: "word" }).segment(text)].filter(x => x.isWordLike).length;
  $("#counts").textContent = new Intl.NumberFormat(lang).format(words);
}

/* ---------- Shell ---------- */
function newPage(parent) {
  const id = uid();
  edit({ op: "create_page", id, parent: parent || null, title: "" }, { op: "insert_block", page: id, after: null, block: { id: uid(), kind: "paragraph", checked: false, runs: [] } });
  if (parent) state.open[parent] = true;
  openPage(id); $("#content .title")?.focus();
}
function openPage(id) {
  state.view = null; state.cur = id; if (!state.tabs.includes(id)) state.tabs.push(id);
  let q = metaOf(id), all = pages(); while (q?.parent) { state.open[q.parent] = true; q = all.find(x => x.id === q.parent); }
  renderAll();
  if (matchMedia("(max-width: 760px)").matches) $("#side").classList.add("closed");
}
function renderSide() {
  const tree = $("#tree"); tree.replaceChildren();
  const all = pages(), active = all.filter(p => !p.trashed);
  const row = (p, depth, kidsToo) => {
    const kids = active.filter(x => x.parent === p.id);
    const chev = el("button", { class: "chev", "aria-label": state.open[p.id] ? "Collapse" : "Expand", onclick: () => { state.open[p.id] = !state.open[p.id]; renderSide(); } }, kidsToo && kids.length ? icon(state.open[p.id] ? "caret-down" : "caret-right") : null);
    const r = el("div", { class: "row" + (p.id === state.cur && !state.view ? " on" : "") }, chev,
      el("button", { class: "open", onclick: () => openPage(p.id) }, icon("file-text"), el("span", { text: p.title || t("Untitled") })),
      el("button", { class: "add", "aria-label": "Add subpage", onclick: () => newPage(p.id) }, icon("plus")));
    r.style.paddingInlineStart = depth * 14 + "px";
    tree.append(r);
    if (kidsToo && state.open[p.id]) kids.forEach(k => row(k, depth + 1, true));
  };
  const favs = active.filter(p => p.favorite);
  if (favs.length) { tree.append(el("div", { class: "sec", text: t("Favorites") })); favs.forEach(p => row(p, 0, false)); }
  tree.append(el("div", { class: "sec" }, el("span", { text: t("Pages") })));
  active.filter(p => !p.parent || !active.some(x => x.id === p.parent)).forEach(p => row(p, 0, true));
  const trash = all.filter(p => p.trashed);
  if (trash.length) { tree.append(el("div", { class: "sec", text: t("Trash") })); trash.forEach(p => tree.append(el("div", { class: "row" }, el("span", { class: "chev" }), el("button", { class: "open", title: "Restore", onclick: () => { edit({ op: "set_trashed", id: p.id, value: false }); openPage(p.id); } }, icon("arrow-counter-clockwise"), el("span", { text: p.title || t("Untitled") }))))); }
}
function renderTabs() {
  const box = $("#tabs"); box.replaceChildren();
  const active = live();
  state.tabs = state.tabs.filter(id => active.some(p => p.id === id));
  state.tabs.forEach(id => {
    const p = active.find(x => x.id === id), on = id === state.cur && !state.view;
    box.append(el("div", { class: "tab" + (on ? " on" : "") },
      el("button", { class: "tl", role: "tab", "aria-selected": String(on), text: p.title || t("Untitled"), onclick: () => openPage(id) }),
      state.tabs.length > 1 ? el("button", { class: "cl", "aria-label": "Close tab", onclick: () => { const i = state.tabs.indexOf(id); state.tabs.splice(i, 1); if (state.cur === id) state.cur = state.tabs[Math.max(0, i - 1)]; renderAll(); } }, icon("x")) : null));
  });
  box.append(el("button", { class: "tabadd", "aria-label": t("New page"), onclick: () => newPage(null) }, icon("plus")));
}
function renderCrumbs() {
  const box = $("#crumbs"); box.replaceChildren();
  ["#favBtn", "#moreBtn"].forEach(q => { $(q).hidden = !!state.view; });
  if (state.view) { box.append(el("b", { text: t("Settings") })); return; }
  const all = pages(), chain = []; let p = all.find(x => x.id === state.cur);
  while (p) { chain.unshift(p); p = all.find(x => x.id === p.parent); }
  chain.forEach((q, i) => {
    if (i) box.append(el("span", { text: "/" }));
    box.append(i === chain.length - 1 ? el("b", { text: q.title || t("Untitled") }) : el("button", { text: q.title || t("Untitled"), onclick: () => openPage(q.id) }));
  });
  const cur = chain[chain.length - 1];
  $("#favBtn").replaceChildren(icon(cur?.favorite ? "star-fill" : "star"));
  $("#favBtn").setAttribute("aria-pressed", String(!!cur?.favorite));
}
function renderSettings() {
  const sel = el("select", { class: "langsel", id: "langSel", "aria-label": t("Language"), onchange: e => { setLang(e.target.value); renderAll(); } }, ...LANGS.map(([c, n]) => el("option", { value: c, text: n, ...(c === lang ? { selected: true } : {}) })));
  $("#content").replaceChildren(el("div", { class: "docscroll" }, el("div", { class: "agent" }, el("h1", { class: "title", text: t("Settings") }), el("h2", { text: t("Language") }), sel)));
}
function renderAll() {
  if (!live().length) { const id = uid(); engine.apply(JSON.stringify({ op: "create_page", id, parent: null, title: "" })); engine.apply(JSON.stringify({ op: "insert_block", page: id, after: null, block: { id: uid(), kind: "paragraph", checked: false, runs: [] } })); store.changed(); state.cur = id; }
  if (!state.cur || !live().some(p => p.id === state.cur)) state.cur = state.tabs.find(id => live().some(p => p.id === id)) || live()[0].id;
  if (!state.tabs.includes(state.cur)) state.tabs.push(state.cur);
  renderSide(); renderTabs(); renderCrumbs();
  if (state.view === "settings") { $("#content").replaceChildren(); renderSettings(); }
  else { const had = $(".blocks"); if (had) had.dataset.page = ""; renderPage(); }
  $("#right").hidden = !state.right;
}
function setLang(code) {
  lang = code; try { localStorage.setItem("casualnote-lang", code); } catch { /* optional */ }
  document.documentElement.lang = code; document.documentElement.dir = code === "ar" ? "rtl" : "ltr";
  document.documentElement.style.setProperty("--untitled", JSON.stringify(t("Untitled")));
  $$("[data-t]").forEach(n => { n.textContent = t(n.dataset.t); });
  paintSaved();
}

/* Search across every page, offline. The engine index comes with Phase 4. */
function openSearch() {
  closeSearch();
  const input = el("input", { id: "palette", type: "search", placeholder: t("Search"), "aria-label": t("Search") });
  const list = el("div", { class: "list" });
  let on = 0, hits = [];
  const paint = () => {
    const q = input.value.trim().toLocaleLowerCase(lang).normalize("NFC"); hits = [];
    for (const m of live()) {
      if (!q) { hits.push({ m }); continue; }
      if ((m.title || "").toLocaleLowerCase(lang).normalize("NFC").includes(q)) hits.push({ m });
      for (const b of pageData(m.id).blocks) { const x = plain(b); if (x.toLocaleLowerCase(lang).normalize("NFC").includes(q)) hits.push({ m, x, b: b.id }); }
    }
    hits = hits.slice(0, 40); on = Math.min(on, Math.max(0, hits.length - 1));
    list.replaceChildren(...hits.map((h, i) => el("button", { class: "mi" + (i === on ? " on" : ""), onclick: () => go(h) }, el("span", { class: "gl" }, icon("file-text")), el("span", {}, h.m.title || t("Untitled"), h.x ? el("small", { text: h.x.slice(0, 120) }) : null))));
    if (!hits.length) list.append(el("div", { class: "hdr", text: t("No matches") }));
  };
  const go = h => { closeSearch(); openPage(h.m.id); if (h.b) $(`.b[data-id="${h.b}"]`)?.scrollIntoView({ block: "center" }); };
  input.addEventListener("input", () => { on = 0; paint(); });
  input.addEventListener("keydown", e => {
    if (e.key === "ArrowDown") { e.preventDefault(); on = Math.min(hits.length - 1, on + 1); paint(); }
    if (e.key === "ArrowUp") { e.preventDefault(); on = Math.max(0, on - 1); paint(); }
    if (e.key === "Enter" && hits[on]) go(hits[on]);
  });
  document.body.append(el("div", { class: "modalbg", onpointerdown: e => { if (e.target.classList.contains("modalbg")) closeSearch(); } }, el("div", { class: "modal", role: "dialog", "aria-label": t("Search") }, input, list)));
  paint(); input.focus();
}
function closeSearch() { $(".modalbg")?.remove(); }

let saveState = "saved";
function paintSaved() { $("#saved").textContent = saveState === "saved" ? t("Saved on this device") : saveState === "failed" ? "Not saved. Export your notes." : "…"; }

function wire() {
  $$("[data-i]").forEach(n => n.replaceWith(icon(n.dataset.i)));
  $("#searchBtn").addEventListener("click", openSearch);
  $("#newBtn").addEventListener("click", () => newPage(null));
  $("#settingsBtn").addEventListener("click", () => { state.view = "settings"; renderAll(); });
  $("#menuBtn").addEventListener("click", () => $("#side").classList.toggle("closed"));
  $("#rightBtn").addEventListener("click", () => { state.right = !state.right; $("#right").hidden = !state.right; });
  $("#favBtn").addEventListener("click", () => { const m = metaOf(state.cur); edit({ op: "set_favorite", id: m.id, value: !m.favorite }); renderSide(); renderCrumbs(); });
  $("#moreBtn").addEventListener("click", e => {
    const m = metaOf(state.cur);
    menu(e.currentTarget.getBoundingClientRect(), [{ gl: "trash", label: t("Trash"), danger: true, run: () => { edit({ op: "set_trashed", id: m.id, value: true }); state.cur = null; renderAll(); } }]);
  });
  document.addEventListener("keydown", e => {
    const mod = e.metaKey || e.ctrlKey, k = e.key.toLowerCase();
    if (mod && k === "k") { e.preventDefault(); openSearch(); }
    else if (mod && k === "z") { e.preventDefault(); e.shiftKey ? replay(redo, undo) : replay(undo, redo); }
    else if (e.key === "Escape") { closeMenu(); closeSearch(); }
  });
}

async function start() {
  await init();
  const peer = crypto.getRandomValues(new BigUint64Array(1))[0] >> 1n;
  engine = new Engine(peer);
  store = await openStore(() => engine.export(), s => { saveState = s; paintSaved(); }, bytes => {
    // Another tab of the same browser saved: merge it, and repaint unless someone is typing here.
    engine.import(bytes);
    if (!composing && !document.activeElement?.closest?.(".t,.title")) renderAll();
    else { renderSide(); }
  });
  const saved = await store.load();
  if (saved) engine.import(saved);
  wire();
  let code = null; try { code = localStorage.getItem("casualnote-lang"); } catch { /* optional */ }
  setLang(code || LANGS.find(l => (navigator.language || "en").toLowerCase().startsWith(l[0].slice(0, 2)))?.[0] || "en");
  if (matchMedia("(max-width: 1120px)").matches) state.right = false;
  if (matchMedia("(max-width: 760px)").matches) $("#side").classList.add("closed");
  renderAll();
  if (new URLSearchParams(location.search).has("e2e")) {
    const { runE2E } = await import("./tests/e2e.js");
    runE2E({ engine, store, state, undoStack: undo, replay, redoStack: redo, pageNow: () => page, openPage, setLang, renderAll, $, $$, setCaret, textOf });
  }
}
start().catch(err => { console.error(err); document.getElementById("error").textContent = String(err?.message || err); });
