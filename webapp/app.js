// The web app. The Rust engine holds the notes; this file projects them to the DOM and turns
// input into operations. Document text only ever reaches the DOM as text.
import init, { Engine } from "./pkg/casual_note_wasm.js";
import { ICONS, I18N } from "./data.js";
import { openStore } from "./store.js";
import { loadFeatures } from "./features/index.js";

const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const uid = () => [...crypto.getRandomValues(new Uint8Array(12))].map(b => b.toString(16).padStart(2, "0")).join("");
const SVG = "http://www.w3.org/2000/svg";
const LANGS = [["en", "English"], ["zh-Hans", "简体中文"], ["ja", "日本語"], ["es", "Español"], ["ar", "العربية"], ["hi", "हिन्दी"]];
const MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
// Phosphor Icons 2.1.1, MIT licence, see NOTICE. Interface verbs the shipped set lacks.
const MORE_ICONS = {
  "arrow-counter-clockwise": '<path d="M224,128a96,96,0,0,1-94.71,96H128A95.38,95.38,0,0,1,62.1,197.8a8,8,0,0,1,11-11.63A80,80,0,1,0,71.43,71.39a3.07,3.07,0,0,1-.26.25L44.59,96H72a8,8,0,0,1,0,16H24a8,8,0,0,1-8-8V56a8,8,0,0,1,16,0V85.8L60.25,60A96,96,0,0,1,224,128Z"/>',
  "arrow-clockwise": '<path d="M240,56v48a8,8,0,0,1-8,8H184a8,8,0,0,1,0-16H211.4L184.81,71.64l-.25-.24a80,80,0,1,0-1.67,114.78,8,8,0,0,1,11,11.63A95.44,95.44,0,0,1,128,224h-1.32A96,96,0,1,1,195.75,60L224,85.8V56a8,8,0,1,1,16,0Z"/>',
  "warning": '<path d="M236.8,188.09,149.35,36.22h0a24.76,24.76,0,0,0-42.7,0L19.2,188.09a23.51,23.51,0,0,0,0,23.72A24.35,24.35,0,0,0,40.55,224h174.9a24.35,24.35,0,0,0,21.33-12.19A23.51,23.51,0,0,0,236.8,188.09ZM222.93,203.8a8.5,8.5,0,0,1-7.48,4.2H40.55a8.5,8.5,0,0,1-7.48-4.2,7.59,7.59,0,0,1,0-7.72L120.52,44.21a8.75,8.75,0,0,1,15,0l87.45,151.87A7.59,7.59,0,0,1,222.93,203.8ZM120,144V104a8,8,0,0,1,16,0v40a8,8,0,0,1-16,0Zm20,36a12,12,0,1,1-12-12A12,12,0,0,1,140,180Z"/>',
  "pencil-simple": '<path d="M227.31,73.37,182.63,28.68a16,16,0,0,0-22.63,0L36.69,152A15.86,15.86,0,0,0,32,163.31V208a16,16,0,0,0,16,16H92.69A15.86,15.86,0,0,0,104,219.31L227.31,96a16,16,0,0,0,0-22.63ZM92.69,208H48V163.31l88-88L180.69,120ZM192,108.68,147.31,64l24-24L216,84.68Z"/>',
};

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
const iconCache = {};
function icon(name) {
  const s = document.createElementNS(SVG, "svg");
  s.setAttribute("class", "i"); s.setAttribute("viewBox", "0 0 256 256"); s.setAttribute("aria-hidden", "true"); s.setAttribute("data-g", name);
  // Fixed icon data shipped with the app, never document content. Parsed once: a long tree draws hundreds.
  const parsed = iconCache[name] ??= new DOMParser().parseFromString(`<svg xmlns="${SVG}">${ICONS[name] || MORE_ICONS[name] || ""}</svg>`, "image/svg+xml").documentElement;
  for (const c of parsed.childNodes) s.append(document.importNode(c, true));
  return s;
}

/* ---------- Engine ---------- */
let engine, store, lang = "en";
const t = k => I18N[lang]?.[k] ?? k;
const tf = (k, vars) => t(k).replace(/\{(\w+)\}/g, (_, v) => vars[v] ?? "");
const iso = s => "⁨" + s + "⁩"; // Isolates user text inside a plain-text sink: aria-label, document.title.
const state = { cur: null, tabs: [], open: {}, view: null, right: true };
const undo = [], redo = [];
let burst = null;

// What features add. See webapp/features/index.js.
const ext = { views: {}, blocks: {}, slash: [], pageMenu: [], panels: [], settings: [], textHooks: [], nav: [], onChange: [] };

const pages = () => JSON.parse(engine.pages());
const live = () => pages().filter(p => !p.trashed);
const pageData = id => JSON.parse(engine.page(id));
const metaOf = id => pages().find(p => p.id === id);

function run(op) {
  const inverse = JSON.parse(engine.apply(JSON.stringify(op)));
  store.changed();
  for (const f of ext.onChange) f(op);
  return inverse;
}
const stamp = (entry, before) => { entry.before = before; entry.page = state.view ? null : state.cur; return entry; };
/** Runs edits as one undo step. */
function edit(...ops) {
  const before = captureCaret();
  const inverses = ops.map(run);
  undo.push(stamp(inverses, before)); redo.length = 0; burst = null;
}
function typed(op, key, before) {
  const inverse = run(op);
  const now = Date.now();
  // One undo step is one run of typing: the same place, without a pause. Moving the caret starts a new one.
  const joins = burst && burst.key === key && now - burst.at < 700 && (op.op !== "splice_text" || burst.pos === undefined || op.at === burst.pos || op.at + op.delete === burst.pos);
  if (joins) burst.entry.push(inverse);
  else { const entry = stamp([inverse], before || null); undo.push(entry); redo.length = 0; burst = { key, entry }; }
  burst.at = now;
  burst.pos = op.op === "splice_text" ? op.at + op.insert.length : undefined;
}
function replay(from, to) {
  const entry = from.pop(); if (!entry) return;
  burst = null;
  if (entry.page && entry.page !== state.cur && metaOf(entry.page)) { state.view = null; state.cur = entry.page; if (!state.tabs.includes(entry.page)) state.tabs.push(entry.page); }
  const here = captureCaret();
  to.push(stamp([...entry].reverse().map(run), here));
  renderAll();
  if (!restoreCaret(entry.before)) restoreCaret(here);
}

/* ---------- Per-browser interface memory. Never in a package, never in the engine. ---------- */
const UI_KEY = "casualnote-ui";
const ui = { sideW: 248, rightW: 300, side: true, recent: [] };
const SIDE = [180, 480], RIGHT = [220, 560];
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
let uiSaved = null;
function loadUI() {
  try { uiSaved = JSON.parse(localStorage.getItem(UI_KEY) || "null"); } catch { uiSaved = null; }
  if (!uiSaved) return;
  if (Number.isFinite(uiSaved.sideW)) ui.sideW = clamp(uiSaved.sideW, ...SIDE);
  if (Number.isFinite(uiSaved.rightW)) ui.rightW = clamp(uiSaved.rightW, ...RIGHT);
  if (typeof uiSaved.side === "boolean") ui.side = uiSaved.side;
  if (Array.isArray(uiSaved.recent)) ui.recent = uiSaved.recent.filter(x => typeof x === "string").slice(0, 12);
}
let uiTimer = null;
function saveUI() {
  clearTimeout(uiTimer);
  uiTimer = setTimeout(() => { try { localStorage.setItem(UI_KEY, JSON.stringify({ ...ui, right: state.right, tabs: state.tabs, cur: state.cur, open: state.open })); } catch { /* optional */ } }, 150);
}
const isPhone = () => matchMedia("(max-width: 760px)").matches;
const isOverlayPanel = () => matchMedia("(max-width: 1120px)").matches;
const isCoarse = () => matchMedia("(pointer: coarse)").matches;
const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

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
    if (r.link) n = el("a", { class: "ref", "data-link": r.link, href: "#" }, n);
    node.append(n);
  }
  // A line break at the very end of a block needs something after it to show the new line.
  if (runs.at(-1)?.text.endsWith("\n")) node.append(el("br"));
}
function caretOffset(node) {
  const s = getSelection(); if (!s.rangeCount || !node.contains(s.anchorNode)) return 0;
  const r = document.createRange(); r.selectNodeContents(node); r.setEnd(s.anchorNode, s.anchorOffset);
  return r.toString().length;
}
function pointIn(node, offset) {
  const w = document.createTreeWalker(node, NodeFilter.SHOW_TEXT); let left = offset, n;
  while ((n = w.nextNode())) { if (left <= n.length) return [n, left]; left -= n.length; }
  return [node, node.childNodes.length - (node.lastChild?.nodeName === "BR" ? 1 : 0)];
}
function setCaret(node, offset = Infinity) {
  if (!node) return;
  node.focus();
  const [n, o] = pointIn(node, offset);
  getSelection().collapse(n, o);
}
function setRange(node, start, end = start) {
  node.focus();
  const [a, ao] = pointIn(node, start), [b, bo] = pointIn(node, end);
  getSelection().setBaseAndExtent(a, ao, b, bo);
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
    if (a < b) out.push({ text: r.text.slice(a - at, b - at), marks: r.marks || [], ...(r.link ? { link: r.link } : {}) });
    at += r.text.length;
  }
  return out;
}
function caretAtPoint(x, y) {
  if (document.caretPositionFromPoint) { const p = document.caretPositionFromPoint(x, y); return p && { node: p.offsetNode, offset: p.offset }; }
  const r = document.caretRangeFromPoint?.(x, y);
  return r && { node: r.startContainer, offset: r.startOffset };
}
/** Caret at a screen point, held inside this text: for clicks in the margin and arrow keys between blocks. */
function placeAt(node, x, y) {
  node.focus();
  const box = node.getBoundingClientRect();
  const p = caretAtPoint(clamp(x, box.left + 1, box.right - 1), clamp(y, box.top + 1, box.bottom - 1));
  if (p && node.contains(p.node)) getSelection().collapse(p.node, p.offset); else setCaret(node);
}

/* ---------- Where the caret is, by model position, so that it survives a repaint ---------- */
function captureCaret() {
  const a = document.activeElement;
  if (!a?.classList || !page) return null;
  if (a.classList.contains("title") && a.closest(".doc")) { const o = selectionOffsets(a); return o && { page: state.cur, id: "title", ...o }; }
  if (a.classList.contains("t")) { const o = selectionOffsets(a), row = a.closest(".b"); return o && row && { page: state.cur, id: row.dataset.id, ...o }; }
  if (a.matches?.('.b input[type="checkbox"]')) return { page: state.cur, id: a.closest(".b").dataset.id, box: true };
  return null;
}
function restoreCaret(c) {
  if (!c || c.page !== state.cur || state.view) return false;
  if (c.box) { const b = $(`.b[data-id="${CSS.escape(c.id)}"] input[type="checkbox"]`); b?.focus(); return !!b; }
  const node = c.id === "title" ? $(".doc .title") : textOf(c.id);
  if (!node) return false;
  setRange(node, c.start, c.end);
  return true;
}
const lastCaret = {}, pageScroll = {};
/** Puts the person back in the document after an action elsewhere. Touch screens keep the keyboard down. */
function focusEditor() {
  if (state.view || isCoarse()) return;
  if (restoreCaret(lastCaret[state.cur])) return;
  const title = $(".doc .title"); if (!title) return;
  if (!title.textContent) { title.focus(); return; }
  const first = page.blocks.find(b => textOf(b.id));
  if (first) setCaret(textOf(first.id));
}

/* ---------- Blocks ---------- */
const KINDS = [
  ["paragraph", "text-t", "Text"], ["heading1", "text-h-one", "Heading 1"], ["heading2", "text-h-two", "Heading 2"], ["heading3", "text-h-three", "Heading 3"],
  ["bulleted", "list-bullets", "Bulleted list"], ["numbered", "list-numbers", "Numbered list"], ["todo", "check-square", "To-do"], ["quote", "quotes", "Quote"],
  ["callout", "lightbulb", "Callout"], ["code", "code", "Code"], ["divider", "minus", "Divider"],
];
const CLS = { paragraph: "p", heading1: "h1", heading2: "h2", heading3: "h3", bulleted: "ul", numbered: "ol", todo: "todo", quote: "quote", callout: "callout", code: "code", divider: "hr" };
const MD = { "#": "heading1", "##": "heading2", "###": "heading3", "-": "bulleted", "*": "bulleted", "1.": "numbered", "[]": "todo", ">": "quote", "```": "code" };
const MARKS = ["bold", "italic", "code", "highlight"];
const PLACEHOLDER = { paragraph: "Type / for blocks, [[ to link a page", heading1: "Heading 1", heading2: "Heading 2", heading3: "Heading 3", quote: "Quote", todo: "To-do", bulleted: "Bulleted list", numbered: "Numbered list", callout: "Callout" };
const LATIN = /^[\P{L}\p{Script=Latin}\p{Script=Greek}\p{Script=Cyrillic}]*$/u;
let composing = false, page = null;
const blockOf = id => page.blocks.find(b => b.id === id);
const textOf = id => $(`.b[data-id="${CSS.escape(id)}"] > .t`);
const hasText = b => b.kind !== "divider" && !ext.blocks[b.kind];
const newParagraph = id => ({ id, kind: "paragraph", checked: false, runs: [] });

function reload() { page = pageData(state.cur); }
function syncText(b, node) {
  const now = node.textContent, old = plain(b);
  if (now === old) return false;
  const d = diff(old, now);
  typed({ op: "splice_text", page: page.meta.id, block: b.id, ...d }, b.id, { page: state.cur, id: b.id, start: d.at, end: d.at + d.delete });
  const id = b.id; reload();
  Object.assign(b, blockOf(id) || {});
  return true;
}
/** The ops that write these runs into a block at a position, marks and links included. */
function runsOps(pid, id, at, runs, before = []) {
  const text = runs.map(r => r.text).join(""); if (!text) return [];
  const ops = [{ op: "splice_text", page: pid, block: id, at, delete: 0, insert: text }];
  const prev = sliceRuns(before, at - 1, at)[0];
  let o = at;
  for (const r of runs) {
    const end = o + r.text.length;
    for (const m of r.marks || []) ops.push({ op: "set_mark", page: pid, block: id, start: o, end, mark: m, on: true });
    for (const m of prev?.marks || []) if (!(r.marks || []).includes(m)) ops.push({ op: "set_mark", page: pid, block: id, start: o, end, mark: m, on: false });
    if (r.link) ops.push({ op: "set_link", page: pid, block: id, start: o, end, target: r.link });
    else if (prev?.link) ops.push({ op: "set_link", page: pid, block: id, start: o, end, target: null });
    o = end;
  }
  return ops;
}
/** Direction of a row. Per docs/50, an empty block follows the block above until a strong character arrives. */
function paintDir(row, b) {
  if (b.kind === "code" || !hasText(b)) return;
  if (plain(b)) { if (row.getAttribute("dir") !== "auto") row.setAttribute("dir", "auto"); return; }
  const prev = row.previousElementSibling;
  row.setAttribute("dir", prev ? getComputedStyle(prev).direction : document.documentElement.dir);
}
function paintScript(row, text) { if (text.length < 4000) row.classList.toggle("lat", LATIN.test(text)); }
function addBlockAfter(id) {
  const nid = uid();
  edit({ op: "insert_block", page: page.meta.id, after: id, block: newParagraph(nid) });
  renderPage(); setCaret(textOf(nid));
}
function blockEl(b, n) {
  const pid = page.meta.id;
  const row = el("div", { class: `b ${CLS[b.kind] || "x-" + b.kind}${b.checked ? " done" : ""}`, "data-id": b.id });
  row.append(el("div", { class: "gutter", dir: document.documentElement.dir },
    el("button", { class: "add", tabindex: "-1", "aria-label": t("Add block"), "data-tip": t("Add block"), onclick: () => addBlockAfter(b.id) }, icon("plus")),
    handle(b)));
  if (b.kind === "divider") { row.append(el("hr")); return row; }
  if (ext.blocks[b.kind]) { row.classList.add("custom"); row.append(ext.blocks[b.kind](b)); return row; }
  row.setAttribute("dir", b.kind === "code" ? "ltr" : "auto");
  if (b.kind === "bulleted") row.append(el("span", { class: "mk", text: "•" }));
  if (b.kind === "numbered") row.append(el("span", { class: "mk", text: new Intl.NumberFormat(lang).format(n) + "." }));
  if (b.kind === "callout") row.append(icon("lightbulb"));
  if (b.kind === "todo") row.append(el("label", { class: "ck" }, el("input", { type: "checkbox", "aria-label": t("To-do"), ...(b.checked ? { checked: true } : {}), onchange: e => { edit({ op: "set_block", page: pid, block: b.id, kind: b.kind, checked: e.target.checked }); renderPage(); $(`.b[data-id="${CSS.escape(b.id)}"] input[type="checkbox"]`)?.focus(); } })));
  const label = t(KINDS.find(k => k[0] === b.kind)?.[2] || "Text");
  const node = el("div", { class: "t ph", contenteditable: "true", role: "textbox", "aria-multiline": "true", "aria-label": label, spellcheck: "true", "data-ph": PLACEHOLDER[b.kind] ? t(PLACEHOLDER[b.kind]) : "" });
  paintRuns(node, b.runs);
  node.addEventListener("compositionstart", () => { composing = true; $("#fmt").hidden = true; });
  node.addEventListener("compositionend", () => {
    composing = false;
    // Read now: a Korean input method can start the next syllable straight after this event.
    const cur = blockOf(b.id); if (cur) { syncText(cur, node); afterText(cur, node); }
  });
  node.addEventListener("input", () => {
    if (composing) return;
    const cur = blockOf(b.id); if (!cur) return;
    // Browsers leave a stray <br> behind in an emptied host, and then :empty stops matching.
    if (!node.textContent && node.firstChild) node.replaceChildren();
    syncText(cur, node); afterText(cur, node);
  });
  node.addEventListener("beforeinput", e => {
    if (composing || e.isComposing) return;
    const type = e.inputType;
    if (type === "historyUndo") { e.preventDefault(); replay(undo, redo); }
    else if (type === "historyRedo") { e.preventDefault(); replay(redo, undo); }
    else if (type === "formatBold") { e.preventDefault(); toggleMark("bold"); }
    else if (type === "formatItalic") { e.preventDefault(); toggleMark("italic"); }
    else if (type.startsWith("format")) e.preventDefault();
    else if (type === "insertParagraph") { e.preventDefault(); enterKey(b.id, node); } // Touch keyboards send this and no Enter key.
    else if (type === "insertLineBreak") { e.preventDefault(); softBreak(b.id, node); }
  });
  node.addEventListener("paste", e => { e.preventDefault(); pasteInto(e.clipboardData, b.id, node); });
  node.addEventListener("keydown", e => blockKeys(e, b.id, node));
  node.addEventListener("blur", () => {
    if (composing || multi) return;
    const cur = blockOf(b.id); if (!cur) return;
    const o = selectionOffsets(node);
    if (o) lastCaret[state.cur] = { page: state.cur, id: b.id, ...o };
    paintRuns(node, cur.runs);
    if (o && document.activeElement === node) setRange(node, o.start, o.end);
  });
  row.append(node);
  paintScript(row, plain(b));
  return row;
}
function afterText(b, node) {
  const row = node.parentElement, text = plain(b);
  if (!text || row.getAttribute("dir") !== "auto") paintDir(row, b);
  paintScript(row, text);
  if (ext.textHooks.some(h => h(b, node, text.slice(0, caretOffset(node))))) return;
  const md = MD[text.replace(/ /g, " ").match(/^(\S+) $/)?.[1]];
  if (md && b.kind === "paragraph") {
    edit({ op: "splice_text", page: page.meta.id, block: b.id, at: 0, delete: text.length, insert: "" }, { op: "set_block", page: page.meta.id, block: b.id, kind: md, checked: false });
    renderPage(); setCaret(textOf(b.id)); return;
  }
  const before = text.slice(0, caretOffset(node));
  const slash = before.match(/(?:^|\s)[/／、]([\p{L}\p{N} ]*)$/u);
  if (slash) slashMenu(b, node, slash[1], slash[0].trimStart().length, before.length);
  else if (menuState?.keys) closeMenu();
  scheduleMeta();
}
/* The typing path stays light: the outline, the word count and the panels catch up when typing rests. */
let metaTimer = null;
function scheduleMeta() { clearTimeout(metaTimer); metaTimer = setTimeout(() => { metaTimer = null; if (!state.view) renderMeta(); }, 220); }

const modOf = e => (MAC ? e.metaKey : e.ctrlKey) && !e.altKey;
/** Latin keys by what they type, other layouts by physical key, per docs/47. */
const keyName = e => (/^[\x20-\x7e]$/.test(e.key) ? e.key.toLowerCase() : (e.code || "").replace(/^Key|^Digit/, "").toLowerCase());
const TURN = { Digit0: "paragraph", Digit1: "heading1", Digit2: "heading2", Digit3: "heading3", Digit7: "numbered", Digit8: "bulleted", Digit9: "todo" };
const blockDir = node => getComputedStyle(node).direction;
let stickyX = null;

function enterKey(id, node) {
  const b = blockOf(id); if (!b) return;
  const pid = page.meta.id;
  if (b.kind === "code") { softBreak(id, node); return; }
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
}
/** A line break inside a block: code, and Shift+Enter anywhere. */
function softBreak(id, node) {
  const b = blockOf(id); if (!b) return;
  syncText(b, node);
  const cur = blockOf(id), o = selectionOffsets(node) || { start: plain(cur).length, end: plain(cur).length };
  typed({ op: "splice_text", page: page.meta.id, block: id, at: o.start, delete: o.end - o.start, insert: "\n" }, id, { page: state.cur, id, ...o });
  reload();
  paintRuns(node, blockOf(id).runs);
  setCaret(node, o.start + 1);
  scheduleMeta();
}
function mergeBack(id, node) {
  const pid = page.meta.id, cur = blockOf(id), i = page.blocks.indexOf(cur), prev = page.blocks[i - 1];
  if (!prev) return false;
  if (prev.kind === "divider") { edit({ op: "delete_block", page: pid, block: prev.id }); renderPage(); setCaret(textOf(id), 0); return true; }
  if (!hasText(prev)) { // A database or embed above: an empty line goes, anything else stays put.
    if (!plain(cur) && page.blocks.length > 1) { const near = page.blocks.slice(0, i).reverse().find(hasText) || page.blocks.slice(i + 1).find(hasText); edit({ op: "delete_block", page: pid, block: id }); renderPage(); if (near) setCaret(textOf(near.id)); }
    return true;
  }
  const at = plain(prev).length;
  edit(...runsOps(pid, prev.id, at, cur.runs, prev.runs), { op: "delete_block", page: pid, block: id });
  renderPage(); setCaret(textOf(prev.id), at);
  return true;
}
function mergeForward(id) {
  const pid = page.meta.id, cur = blockOf(id), i = page.blocks.indexOf(cur), next = page.blocks[i + 1];
  if (!next) return;
  const at = plain(cur).length;
  if (hasText(next)) edit(...runsOps(pid, id, at, next.runs, cur.runs), { op: "delete_block", page: pid, block: next.id });
  else if (next.kind === "divider") edit({ op: "delete_block", page: pid, block: next.id });
  else return;
  renderPage(); setCaret(textOf(id), at);
}
/** True when the moving end of the selection is on the block's first or last visual line. */
function atEdgeLine(node, up) {
  const s = getSelection(); if (!s.focusNode) return true;
  const r = document.createRange(); r.setStart(s.focusNode, s.focusOffset); r.collapse(true);
  const rect = r.getClientRects()[0], box = node.getBoundingClientRect();
  if (!rect) return !node.textContent || (up ? caretOffset(node) === 0 : caretOffset(node) >= node.textContent.length);
  return up ? rect.top - box.top < rect.height * .6 : box.bottom - rect.bottom < rect.height * .6;
}
function focusOffset(node) {
  const s = getSelection(); if (!s.focusNode || !node.contains(s.focusNode)) return 0;
  const r = document.createRange(); r.selectNodeContents(node); r.setEnd(s.focusNode, s.focusOffset);
  return r.toString().length;
}
function blockKeys(e, id, node) {
  if (e.isComposing || composing) return;
  const b = blockOf(id); if (!b) return;
  if (menuState?.keys) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); const n = menuState.items.length; if (n) { menuState.on = (menuState.on + (e.key === "ArrowDown" ? 1 : n - 1)) % n; paintMenu(); } return; }
    if (e.key === "Enter" && menuState.items.length) { e.preventDefault(); const it = menuState.items[menuState.on]; closeMenu(); it?.run(); return; }
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenu(); return; }
    if (e.key === "Tab") { e.preventDefault(); const it = menuState.items[menuState.on]; closeMenu(); it?.run(); return; }
  }
  const pid = page.meta.id, i = page.blocks.indexOf(b), sel = getSelection(), mod = modOf(e), k = keyName(e);
  if (e.key !== "ArrowUp" && e.key !== "ArrowDown") stickyX = null;

  if (mod && !e.shiftKey && k === "a") {
    e.preventDefault();
    const o = selectionOffsets(node), len = node.textContent.length;
    if (o && o.start === 0 && o.end === len) selectAllBlocks(); else setRange(node, 0, len);
    return;
  }
  if (mod && !e.shiftKey && k === "b") { e.preventDefault(); toggleMark("bold"); return; }
  if (mod && !e.shiftKey && k === "i") { e.preventDefault(); toggleMark("italic"); return; }
  if (mod && !e.shiftKey && k === "e") { e.preventDefault(); toggleMark("code"); return; }
  if (mod && !e.shiftKey && k === "u") { e.preventDefault(); return; } // No underline in this model; do not let the browser add markup.
  if (mod && e.shiftKey && (k === "h" || k === "m")) { e.preventDefault(); toggleMark("highlight"); return; }
  if (mod && !e.shiftKey && k === "d") { e.preventDefault(); duplicateBlock(id); return; }
  if (mod && e.shiftKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) { e.preventDefault(); moveBlockBy(id, e.key === "ArrowUp" ? -1 : 1); return; }
  if (mod && e.shiftKey && e.code === "Slash") { e.preventDefault(); e.stopPropagation(); blockMenu(b, node.getBoundingClientRect()); return; }
  if (mod && e.shiftKey && TURN[e.code]) { e.preventDefault(); turnInto(id, TURN[e.code]); return; }
  if (mod && !e.shiftKey && e.key === "Enter") { e.preventDefault(); if (b.kind === "todo") { syncText(b, node); const o = selectionOffsets(node); edit({ op: "set_block", page: pid, block: id, kind: "todo", checked: !b.checked }); renderPage(); if (o) setRange(textOf(id), o.start, o.end); } return; }
  if (mod && e.key === "Home") { e.preventDefault(); const f = page.blocks.find(hasText); if (f) { setCaret(textOf(f.id), 0); textOf(f.id).scrollIntoView({ block: "nearest" }); } return; }
  if (mod && e.key === "End") { e.preventDefault(); const f = [...page.blocks].reverse().find(hasText); if (f) { setCaret(textOf(f.id)); textOf(f.id).scrollIntoView({ block: "nearest" }); } return; }
  if (e.key === "Home" || e.key === "End") {
    e.preventDefault();
    sel.modify(e.shiftKey ? "extend" : "move", e.key === "Home" ? "backward" : "forward", "lineboundary");
    return;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    if (menuState || !$("#side").classList.contains("closed") && isPhone()) return;
    selectBlockRange(id);
    return;
  }
  if (e.key === "Tab" && b.kind === "code") {
    const o = selectionOffsets(node); if (!o) return;
    syncText(b, node);
    const text = plain(blockOf(id));
    const lineStart = text.lastIndexOf("\n", o.start - 1) + 1;
    if (!e.shiftKey) { e.preventDefault(); typed({ op: "splice_text", page: pid, block: id, at: o.start, delete: o.end - o.start, insert: "\t" }, id, { page: state.cur, id, ...o }); reload(); paintRuns(node, blockOf(id).runs); setCaret(node, o.start + 1); return; }
    const lead = text.slice(lineStart).match(/^(\t| {1,2})/)?.[0];
    if (lead) { e.preventDefault(); typed({ op: "splice_text", page: pid, block: id, at: lineStart, delete: lead.length, insert: "" }, id, { page: state.cur, id, ...o }); reload(); paintRuns(node, blockOf(id).runs); setCaret(node, Math.max(lineStart, o.start - lead.length)); }
    return; // Otherwise Shift+Tab leaves the block: the keyboard is never trapped.
  }
  if (e.key === "Enter" && e.shiftKey) { e.preventDefault(); softBreak(id, node); return; }
  if (e.key === "Enter") { e.preventDefault(); enterKey(id, node); return; }
  if (e.key === "Backspace" && sel.isCollapsed && caretOffset(node) === 0 && !mod) {
    syncText(b, node);
    const cur = blockOf(id);
    if (cur.kind !== "paragraph") { e.preventDefault(); edit({ op: "set_block", page: pid, block: id, kind: "paragraph", checked: false }); renderPage(); setCaret(textOf(id), 0); return; }
    if (i === 0) return;
    e.preventDefault();
    mergeBack(id, node);
    return;
  }
  if (e.key === "Delete" && sel.isCollapsed && caretOffset(node) >= node.textContent.length) {
    e.preventDefault();
    syncText(b, node);
    mergeForward(id);
    return;
  }
  if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && sel.isCollapsed && !e.shiftKey && !mod) {
    const back = (e.key === "ArrowLeft") === (blockDir(node) === "ltr"), off = caretOffset(node);
    if (back ? off === 0 : off >= node.textContent.length) {
      const list = back ? page.blocks.slice(0, i).reverse() : page.blocks.slice(i + 1), next = list.find(hasText);
      if (next) { e.preventDefault(); setCaret(textOf(next.id), back ? Infinity : 0); }
      else if (back) { const title = $(".doc .title"); if (title) { e.preventDefault(); setCaret(title); } }
    }
    return;
  }
  if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !mod) {
    const up = e.key === "ArrowUp";
    if (!atEdgeLine(node, up)) { stickyX = null; if (!e.shiftKey) return; return; }
    if (e.shiftKey) {
      e.preventDefault();
      const a = [sel.anchorNode, sel.anchorOffset], f = [sel.focusNode, sel.focusOffset];
      enterMulti(); sel.setBaseAndExtent(...a, ...f);
      sel.modify("extend", up ? "backward" : "forward", "line");
      return;
    }
    const r = sel.rangeCount ? sel.getRangeAt(0).getClientRects()[0] : null;
    if (stickyX === null) stickyX = r ? r.left : node.getBoundingClientRect().left;
    const list = up ? page.blocks.slice(0, i).reverse() : page.blocks.slice(i + 1), next = list.find(hasText);
    if (next) {
      e.preventDefault();
      const n = textOf(next.id); n.scrollIntoView({ block: "nearest" });
      const box = n.getBoundingClientRect();
      placeAt(n, stickyX, up ? box.bottom - 4 : box.top + 4);
    } else if (up) { const title = $(".doc .title"); if (title) { e.preventDefault(); setCaret(title); } }
    return;
  }
  if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && e.shiftKey && !mod) {
    const forward = (e.key === "ArrowRight") === (blockDir(node) === "ltr"), off = focusOffset(node);
    if (forward ? off >= node.textContent.length : off === 0) {
      const a = [sel.anchorNode, sel.anchorOffset], f = [sel.focusNode, sel.focusOffset];
      e.preventDefault(); enterMulti(); sel.setBaseAndExtent(...a, ...f);
      sel.modify("extend", forward ? "forward" : "backward", "character");
    }
  }
}
function turnInto(id, kind, checked = false) {
  const b = blockOf(id), o = textOf(id) ? selectionOffsets(textOf(id)) : null;
  edit({ op: "set_block", page: page.meta.id, block: id, kind, checked });
  renderPage();
  if (textOf(id)) { if (o) setRange(textOf(id), o.start, o.end); else setCaret(textOf(id)); }
  return b;
}
function duplicateBlock(id) {
  const b = blockOf(id), nid = uid(), n = textOf(id), o = n ? selectionOffsets(n) : null;
  if (n) syncText(b, n);
  const cur = blockOf(id);
  edit({ op: "insert_block", page: page.meta.id, after: id, block: { id: nid, kind: cur.kind, checked: cur.checked, runs: cur.runs, ...(cur.target ? { target: cur.target } : {}) } });
  renderPage();
  if (textOf(nid)) { if (o) setRange(textOf(nid), o.start, o.end); else setCaret(textOf(nid)); }
}
function moveBlockBy(id, delta) {
  const i = page.blocks.findIndex(b => b.id === id), j = i + delta;
  if (i < 0 || j < 0 || j >= page.blocks.length) return;
  const n = textOf(id), o = n ? selectionOffsets(n) : null;
  const rest = page.blocks.filter(b => b.id !== id), after = rest[j - 1];
  edit({ op: "move_block", page: page.meta.id, block: id, after: after ? after.id : null });
  renderPage();
  if (textOf(id)) { if (o) setRange(textOf(id), o.start, o.end); textOf(id).scrollIntoView({ block: "nearest" }); }
}
function deleteBlock(id) {
  const pid = page.meta.id, i = page.blocks.findIndex(x => x.id === id);
  const near = page.blocks.slice(i + 1).find(hasText) || page.blocks.slice(0, i).reverse().find(hasText);
  const nid = uid(), ops = [{ op: "delete_block", page: pid, block: id }];
  if (page.blocks.length === 1) ops.push({ op: "insert_block", page: pid, after: null, block: newParagraph(nid) });
  edit(...ops); renderPage();
  const target = page.blocks.length === 1 && blockOf(nid) ? nid : near?.id;
  if (target && textOf(target)) setCaret(textOf(target), target === nid ? 0 : Infinity);
}
function toggleMark(mark) {
  const s = getSelection();
  if (multi) {
    const rg = multiRange(); if (!rg) return;
    const parts = rg.blocks.filter(hasText).map(b => {
      const start = b === rg.blocks[0] ? rg.from.off : 0, end = b === rg.blocks.at(-1) ? rg.to.off : plain(b).length;
      return { b, start, end };
    }).filter(p => p.end > p.start);
    if (!parts.length) return;
    const on = !parts.every(p => sliceRuns(p.b.runs, p.start, p.end).every(r => (r.marks || []).includes(mark)));
    const a = [s.anchorNode && rangePos(s.anchorNode, s.anchorOffset), rangePos(s.focusNode, s.focusOffset)];
    edit(...parts.map(p => ({ op: "set_mark", page: page.meta.id, block: p.b.id, start: p.start, end: p.end, mark, on })));
    exitMulti(); renderPage();
    selectModelRange(a[0], a[1]);
    return;
  }
  const a = s.anchorNode, node = (a?.nodeType === 1 ? a : a?.parentElement)?.closest(".t"); if (!node) return;
  const offs = selectionOffsets(node); if (!offs || offs.start === offs.end) return;
  const id = node.closest(".b").dataset.id, b = blockOf(id);
  const inside = sliceRuns(b.runs, offs.start, offs.end);
  const on = !inside.every(r => (r.marks || []).includes(mark));
  edit({ op: "set_mark", page: page.meta.id, block: id, start: offs.start, end: offs.end, mark, on });
  reload(); paintRuns(node, blockOf(id).runs); setRange(node, offs.start, offs.end);
}

/* ---------- Selection across blocks ----------
   Browsers stop a native selection at the edge of an editing host. So while a selection crosses
   blocks, every host in the page is switched off and the browser selects plain text: drag, Shift+arrows,
   highlight and screen readers all stay native. It ends on the next click, key or edit. */
let multi = false, dragSel = null;
const hosts = () => $$(".blocks .t, .doc .title");
function enterMulti() {
  if (multi) return;
  closeMenu();
  multi = true;
  const c = captureCaret(); if (c && c.id !== "title") lastCaret[state.cur] = c;
  for (const n of hosts()) n.setAttribute("contenteditable", "false");
  document.body.classList.add("multi");
}
function exitMulti() {
  if (!multi) return;
  multi = false;
  for (const n of hosts()) n.setAttribute("contenteditable", "true");
  document.body.classList.remove("multi");
  $$(".b.insel").forEach(n => n.classList.remove("insel"));
}
function rangePos(node, offset) {
  const box = $(".blocks"); if (!box || !node) return null;
  const elem = node.nodeType === 1 ? node : node.parentElement;
  const row = elem?.closest?.(".b");
  if (row && box.contains(row)) {
    const tn = row.querySelector(":scope > .t");
    if (tn && tn.contains(node)) { const r = document.createRange(); r.selectNodeContents(tn); r.setEnd(node, offset); return { id: row.dataset.id, off: r.toString().length }; }
    // In a marker, a gutter or a block with no text.
    return { id: row.dataset.id, off: 0 };
  }
  const rows = box.children;
  if (!rows.length) return null;
  const end = kid => { const b = blockOf(kid.dataset.id); return { id: kid.dataset.id, off: b ? plain(b).length : 0 }; };
  if (node === box) return offset < rows.length ? { id: rows[offset].dataset.id, off: 0 } : end(rows[rows.length - 1]);
  return box.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_PRECEDING ? { id: rows[0].dataset.id, off: 0 } : end(rows[rows.length - 1]);
}
/** The selection as model positions, first to last. */
function multiRange() {
  const s = getSelection(); if (!s.rangeCount || !page) return null;
  let a = rangePos(s.anchorNode, s.anchorOffset), b = rangePos(s.focusNode, s.focusOffset);
  if (!a || !b) return null;
  let i = page.blocks.findIndex(x => x.id === a.id), j = page.blocks.findIndex(x => x.id === b.id);
  if (i < 0 || j < 0) return null;
  if (i > j || (i === j && a.off > b.off)) { [a, b] = [b, a]; [i, j] = [j, i]; }
  return { from: a, to: b, blocks: page.blocks.slice(i, j + 1) };
}
function selectModelRange(a, b) {
  if (!a || !b) return;
  const pa = textOf(a.id), pb = textOf(b.id); if (!pa || !pb) return;
  if (a.id === b.id) { setRange(pa, a.off, b.off); return; }
  const [an, ao] = pointIn(pa, a.off), [bn, bo] = pointIn(pb, b.off);
  enterMulti(); getSelection().setBaseAndExtent(an, ao, bn, bo);
}
function selectAllBlocks() {
  const box = $(".blocks"); if (!box) return;
  enterMulti();
  getSelection().setBaseAndExtent(box, 0, box, box.childNodes.length);
  paintInsel();
}
/** Escape in a block selects it whole. The second Escape, or any arrow, goes back to text. */
function selectBlockRange(id) {
  const n = textOf(id); if (!n) return;
  enterMulti();
  getSelection().selectAllChildren(n);
  paintInsel();
}
function deleteMulti() {
  const rg = multiRange();
  exitMulti();
  if (!rg) return null;
  const pid = page.meta.id, list = rg.blocks, tx = list.filter(hasText), first = tx[0], last = tx.at(-1), ops = [];
  let caret;
  if (first) {
    const from = first === list[0] ? rg.from.off : 0, to = last === list.at(-1) ? rg.to.off : plain(last).length;
    if (first === last) { if (to > from) ops.push({ op: "splice_text", page: pid, block: first.id, at: from, delete: to - from, insert: "" }); }
    else {
      const len = plain(first).length;
      if (len > from) ops.push({ op: "splice_text", page: pid, block: first.id, at: from, delete: len - from, insert: "" });
      ops.push(...runsOps(pid, first.id, from, sliceRuns(last.runs, to), sliceRuns(first.runs, 0, from)));
    }
    for (const b of list) if (b !== first) ops.push({ op: "delete_block", page: pid, block: b.id });
    const all = list.length === page.blocks.length && from === 0 && to === plain(last).length;
    if (all && first.kind !== "paragraph") ops.push({ op: "set_block", page: pid, block: first.id, kind: "paragraph", checked: false });
    caret = { id: first.id, off: from };
  } else {
    const at = page.blocks.indexOf(list[0]), before = page.blocks[at - 1], nid = uid();
    for (const b of list) ops.push({ op: "delete_block", page: pid, block: b.id });
    const near = page.blocks.slice(0, at).reverse().find(hasText) || page.blocks.slice(at + list.length).find(hasText);
    if (near) caret = { id: near.id, off: plain(near).length };
    else { ops.push({ op: "insert_block", page: pid, after: before ? before.id : null, block: newParagraph(nid) }); caret = { id: nid, off: 0 }; }
  }
  if (ops.length) {
    edit(...ops);
    undo.at(-1).before = { page: state.cur, id: rg.from.id, start: rg.from.off, end: rg.from.off };
  }
  renderPage();
  const n = textOf(caret.id); if (n) setCaret(n, caret.off);
  return caret;
}
function rangeBlocks(rg) {
  return rg.blocks.filter(b => hasText(b) || b.kind === "divider").map(b => {
    const start = b === rg.blocks[0] ? rg.from.off : 0, end = b === rg.blocks.at(-1) ? rg.to.off : plain(b).length;
    return { kind: b.kind, checked: b.checked, runs: sliceRuns(b.runs, start, end) };
  });
}
const CLIP = "application/x-casualnote-blocks";
function writeClipboard(data, rg) {
  const blocks = rangeBlocks(rg);
  data.setData("text/plain", blocks.filter(b => b.kind !== "divider").map(b => b.runs.map(r => r.text).join("")).join("\n"));
  data.setData(CLIP, JSON.stringify(blocks));
}
function paintInsel() {
  const rg = multi && multiRange(), inside = new Set(rg ? rg.blocks.map(b => b.id) : []);
  for (const row of $$(".blocks > .b")) row.classList.toggle("insel", inside.has(row.dataset.id) && inside.size > 0);
}
function multiKeys(e) {
  if (!multi) return;
  if (e.isComposing) return;
  const s = getSelection(), mod = modOf(e), k = keyName(e);
  const toEdge = atEnd => {
    const rg = multiRange(); exitMulti();
    if (!rg) return;
    const p = atEnd ? rg.to : rg.from, b = blockOf(p.id);
    if (hasText(b)) setCaret(textOf(p.id), p.off); else focusEditor();
  };
  if (mod && k === "a") { e.preventDefault(); selectAllBlocks(); return; }
  if (mod && k === "c") return;
  if (mod && k === "x") { e.preventDefault(); e.stopPropagation(); document.execCommand("copy"); deleteMulti(); return; }
  if (mod && k === "v") { return; }
  if (mod && k === "b") { e.preventDefault(); toggleMark("bold"); return; }
  if (mod && k === "i") { e.preventDefault(); toggleMark("italic"); return; }
  if (mod && k === "e") { e.preventDefault(); toggleMark("code"); return; }
  if (mod && e.shiftKey && (k === "h" || k === "m")) { e.preventDefault(); toggleMark("highlight"); return; }
  if (mod && (k === "z" || k === "y")) { exitMulti(); return; }
  if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); toEdge(true); return; }
  if (e.key === "Backspace" || e.key === "Delete") { e.preventDefault(); deleteMulti(); return; }
  if (e.key === "Enter") { e.preventDefault(); const c = deleteMulti(); const n = c && textOf(c.id); if (n) enterKey(c.id, n); return; }
  if (e.key === "Tab") { exitMulti(); return; }
  if (e.key.startsWith("Arrow") || e.key === "Home" || e.key === "End") {
    e.preventDefault();
    const rtl = document.documentElement.dir === "rtl";
    const fwd = e.key === "ArrowDown" || e.key === "End" || (e.key === "ArrowRight" && !rtl) || (e.key === "ArrowLeft" && rtl);
    if (!e.shiftKey) { toEdge(fwd); return; }
    s.modify("extend", fwd ? "forward" : "backward", e.key === "ArrowUp" || e.key === "ArrowDown" ? "line" : e.key === "Home" || e.key === "End" ? "lineboundary" : "character");
    return;
  }
  if (!mod && !e.altKey && !e.metaKey && (e.key.length === 1 || e.key === "Process")) { // The key then types into the caret left behind, and both undo as one.
    const c = deleteMulti();
    if (c && undo.length) burst = { key: c.id, at: Date.now(), entry: undo.at(-1) };
  }
}
function rowAtY(y) {
  const box = $(".blocks"); if (!box) return null;
  const rows = [...box.children]; if (!rows.length) return null;
  return rows.find(r => y <= r.getBoundingClientRect().bottom) || rows.at(-1);
}
function dragPoint(row, x, y) {
  const tn = row.querySelector(":scope > .t");
  if (!tn) return { node: row, offset: 0 };
  const b = tn.getBoundingClientRect();
  return caretAtPoint(clamp(x, b.left + 1, b.right - 1), clamp(y, b.top + 1, b.bottom - 1)) || { node: tn, offset: 0 };
}
function autoScroll(y) {
  const sc = $(".docscroll"); if (!sc) return;
  const r = sc.getBoundingClientRect();
  if (y < r.top + 40) sc.scrollBy(0, -16); else if (y > r.bottom - 40) sc.scrollBy(0, 16);
}
function wireSelection() {
  // Browsers drop the selection before blur reports it. So the place is noted on what moves focus.
  const remember = () => { const c = captureCaret(); if (c) lastCaret[state.cur] = c; };
  document.addEventListener("keydown", e => { if (e.key === "F6" || e.key === "Tab" || e.key === "Escape" || e.metaKey || e.ctrlKey) remember(); }, true);
  document.addEventListener("pointerdown", e => {
    remember();
    if (multi && e.button !== 2) exitMulti();
    if (e.button === 0 && e.pointerType === "mouse") {
      const tn = e.target.closest?.(".blocks .t");
      dragSel = tn ? { row: tn.closest(".b"), a: null } : null;
    }
  }, true);
  document.addEventListener("pointermove", e => {
    if (!dragSel) return;
    if (!(e.buttons & 1)) { dragSel = null; return; }
    const row = rowAtY(e.clientY); if (!row) return;
    if (!multi && row === dragSel.row && !dragSel.manual) return;
    const s = getSelection();
    if (!dragSel.a) dragSel.a = [s.anchorNode, s.anchorOffset];
    if (!multi && row !== dragSel.row) enterMulti();
    const p = dragPoint(row, e.clientX, e.clientY);
    if (p && dragSel.a[0]?.isConnected) { s.setBaseAndExtent(dragSel.a[0], dragSel.a[1], p.node, p.offset); paintInsel(); }
    autoScroll(e.clientY);
  });
  document.addEventListener("pointerup", () => {
    if (!dragSel) return;
    const was = dragSel; dragSel = null;
    if (!multi) return;
    const rg = multiRange();
    if (rg && rg.blocks.length === 1 && hasText(rg.blocks[0])) { // Came back to where it started.
      const id = rg.blocks[0].id, { from, to } = rg;
      exitMulti(); setRange(textOf(id), from.off, to.off);
    }
    return was;
  });
  document.addEventListener("keydown", multiKeys, true);
  document.addEventListener("selectstart", e => { if (dragSel?.manual) e.preventDefault(); });
  document.addEventListener("copy", e => { if (!multi) return; const rg = multiRange(); if (rg) { e.preventDefault(); writeClipboard(e.clipboardData, rg); } });
  document.addEventListener("cut", e => { if (!multi) return; const rg = multiRange(); if (rg) { e.preventDefault(); writeClipboard(e.clipboardData, rg); deleteMulti(); } });
  document.addEventListener("paste", e => {
    if (!multi) return;
    e.preventDefault();
    const data = e.clipboardData, c = deleteMulti(), n = c && textOf(c.id);
    if (n) pasteInto(data, c.id, n);
  });
}

/* ---------- Paste ---------- */
const FENCE = /^```/;
function parseLines(text) {
  const out = []; let code = null;
  for (const raw of text.replace(/\r\n?/g, "\n").split("\n")) {
    if (FENCE.test(raw.trim())) { if (code) { out.push({ kind: "code", checked: false, runs: [{ text: code.join("\n"), marks: [] }] }); code = null; } else code = []; continue; }
    if (code) { code.push(raw); continue; }
    if (!raw.trim()) continue;
    const m = raw.match(/^\s*(#{1,3} |[-*•] \[[ xX]\] |\[\] |[-*•] |\d+[.)] |> )/);
    let kind = "paragraph", checked = false, body = raw.trim();
    if (m) {
      const p = m[1].trim();
      body = raw.slice(m[0].length);
      if (p[0] === "#") kind = "heading" + p.length;
      else if (p.includes("[")) { kind = "todo"; checked = /x/i.test(p); }
      else if (/^\d/.test(p)) kind = "numbered";
      else if (p === ">") kind = "quote";
      else kind = "bulleted";
    }
    out.push({ kind, checked, runs: body ? [{ text: body, marks: [] }] : [] });
  }
  if (code) out.push({ kind: "code", checked: false, runs: [{ text: code.join("\n"), marks: [] }] });
  return out;
}
/** Clipboard data is untrusted: only known kinds, known marks, and links to pages that exist. */
function readClip(raw) {
  try {
    const all = pages(), kinds = KINDS.map(k => k[0]);
    return JSON.parse(raw).filter(b => kinds.includes(b.kind)).slice(0, 5000).map(b => ({
      kind: b.kind, checked: !!b.checked,
      runs: (Array.isArray(b.runs) ? b.runs : []).filter(r => typeof r?.text === "string" && r.text).map(r => ({
        text: r.text,
        marks: (Array.isArray(r.marks) ? r.marks : []).filter(m => MARKS.includes(m)),
        ...(typeof r.link === "string" && all.some(p => p.id === r.link) ? { link: r.link } : {}),
      })),
    }));
  } catch { return null; }
}
function pasteInto(data, id, node) {
  const b = blockOf(id); if (!b) return;
  const internal = data.getData(CLIP), text = data.getData("text/plain");
  let parsed = internal ? readClip(internal) : null;
  if (!parsed) {
    if (!text) return;
    if (b.kind === "code") { insertInline(id, node, text.replace(/\r\n?/g, "\n")); return; }
    if (!/\n/.test(text.replace(/\r\n?/g, "\n").trim())) { insertInline(id, node, text.replace(/[\r\n]+/g, " ")); return; }
    parsed = parseLines(text);
  }
  if (!parsed.length) return;
  syncText(b, node);
  const cur = blockOf(id), pid = page.meta.id, whole = plain(cur);
  const o = selectionOffsets(node) || { start: whole.length, end: whole.length };
  const tail = sliceRuns(cur.runs, o.end), head = sliceRuns(cur.runs, 0, o.start), ops = [];
  if (whole.length > o.start) ops.push({ op: "splice_text", page: pid, block: id, at: o.start, delete: whole.length - o.start, insert: "" });
  let first = parsed[0], rest = parsed.slice(1);
  if (first.kind === "divider") { rest = parsed; first = null; }
  if (first) {
    if (!whole && cur.kind === "paragraph" && first.kind !== "paragraph") ops.push({ op: "set_block", page: pid, block: id, kind: first.kind, checked: first.checked });
    ops.push(...runsOps(pid, id, o.start, first.runs, head));
  }
  let lastId = id, lastLen = o.start + (first ? first.runs.map(r => r.text).join("").length : 0), lastRuns = first ? [...head, ...first.runs] : head;
  for (const p of rest) {
    const nid = uid();
    ops.push({ op: "insert_block", page: pid, after: lastId, block: { id: nid, kind: p.kind, checked: p.checked, runs: p.runs } });
    lastId = nid; lastRuns = p.runs; lastLen = p.runs.map(r => r.text).join("").length;
    if (p.kind === "divider") { const after = uid(); ops.push({ op: "insert_block", page: pid, after: nid, block: newParagraph(after) }); lastId = after; lastRuns = []; lastLen = 0; }
  }
  ops.push(...runsOps(pid, lastId, lastLen, tail, lastRuns));
  edit(...ops);
  renderPage();
  const n = textOf(lastId); if (n) { setCaret(n, lastLen); n.scrollIntoView({ block: "nearest" }); }
}
function insertInline(id, node, text) {
  if (!text.includes("\n")) { document.execCommand("insertText", false, text); return; }
  // Line breaks through insertText differ by browser. The engine says what happened, then the screen follows.
  const b = blockOf(id); syncText(b, node);
  const whole = plain(blockOf(id)), o = selectionOffsets(node) || { start: whole.length, end: whole.length };
  edit({ op: "splice_text", page: page.meta.id, block: id, at: o.start, delete: o.end - o.start, insert: text });
  reload(); paintRuns(node, blockOf(id).runs); setCaret(node, o.start + text.length);
}

/* ---------- Dragging a block ---------- */
function handle(b) {
  const h = el("button", { class: "hd", tabindex: "-1", "aria-label": t("Move or change block"), "aria-haspopup": "menu", "data-tip": t("Drag to move. Click for options") }, icon("dots-six-vertical"));
  h.addEventListener("click", e => { if (e.detail === 0) blockMenu(blockOf(b.id) || b, h.getBoundingClientRect()); });
  h.addEventListener("pointerdown", e => {
    if (e.button) return;
    e.preventDefault(); try { h.setPointerCapture(e.pointerId); } catch { /* not a live pointer */ } hideTip();
    const sx = e.clientX, sy = e.clientY, source = h.closest(".b"); let drag = false, target = null, after = false, ghost = null, cancelled = false;
    const clear = () => $$(".drop-before,.drop-after").forEach(n => n.classList.remove("drop-before", "drop-after"));
    const mv = ev => {
      if (!drag && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 4) return;
      if (!drag) {
        drag = true;
        source.classList.add("lifted");
        document.body.classList.add("dragging");
        const words = plain(blockOf(b.id) || b);
        ghost = el("div", { class: "ghost", dir: "auto", text: words.slice(0, 70) || t(KINDS.find(k => k[0] === b.kind)?.[2] || "Text") });
        document.body.append(ghost);
      }
      ghost.style.left = (ev.clientX + 12) + "px"; ghost.style.top = (ev.clientY + 8) + "px";
      clear();
      const over = rowAtY(ev.clientY);
      if (!over) { target = null; return; }
      const r = over.getBoundingClientRect(); after = ev.clientY > r.top + r.height / 2; target = over.dataset.id;
      over.classList.add(after ? "drop-after" : "drop-before");
      autoScroll(ev.clientY);
    };
    const esc = ev => { if (ev.key === "Escape") { ev.stopPropagation(); ev.preventDefault(); cancelled = true; up(); } };
    const up = () => {
      h.removeEventListener("pointermove", mv); h.removeEventListener("pointerup", up); h.removeEventListener("pointercancel", up); document.removeEventListener("keydown", esc, true); clear();
      ghost?.remove(); source.classList.remove("lifted"); document.body.classList.remove("dragging");
      if (!drag) { blockMenu(blockOf(b.id) || b, h.getBoundingClientRect()); return; }
      if (cancelled) return;
      if (target && target !== b.id) {
        const list = page.blocks.filter(x => x.id !== b.id), idx = list.findIndex(x => x.id === target);
        const prev = after ? list[idx] : list[idx - 1], was = page.blocks[page.blocks.findIndex(x => x.id === b.id) - 1];
        if ((prev ? prev.id : null) !== (was ? was.id : null)) { edit({ op: "move_block", page: page.meta.id, block: b.id, after: prev ? prev.id : null }); renderPage(); }
      }
      if (textOf(b.id) && !isCoarse()) setCaret(textOf(b.id));
    };
    h.addEventListener("pointermove", mv); h.addEventListener("pointerup", up); h.addEventListener("pointercancel", up); document.addEventListener("keydown", esc, true);
  });
  return h;
}
function blockMenu(b, rect) {
  const pid = page.meta.id, items = [];
  const back = () => { if (textOf(b.id)) setCaret(textOf(b.id)); };
  if (hasText(b)) {
    const n = textOf(b.id), o = n ? selectionOffsets(n) : null;
    items.push({ section: t("Turn into") });
    KINDS.slice(0, 10).filter(k => k[0] !== b.kind).forEach(([kind, gl, label]) => items.push({ gl, label: t(label), run: () => { edit({ op: "set_block", page: pid, block: b.id, kind, checked: false }); renderPage(); const m = textOf(b.id); if (m) { if (o) setRange(m, o.start, o.end); else setCaret(m); } } }));
    items.push({ sep: true });
  }
  items.push({ gl: "copy", label: t("Duplicate"), keys: "mod+d", run: () => { duplicateBlock(b.id); } });
  const i = page.blocks.findIndex(x => x.id === b.id);
  if (i > 0) items.push({ gl: "arrow-up-right", label: t("Move up"), keys: "mod+shift+up", run: () => { moveBlockBy(b.id, -1); back(); } });
  if (i < page.blocks.length - 1) items.push({ gl: "arrow-up-right", label: t("Move down"), keys: "mod+shift+down", run: () => { moveBlockBy(b.id, 1); back(); } });
  items.push({ gl: "trash", label: t("Delete"), danger: true, run: () => deleteBlock(b.id) });
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
      edit({ op: "insert_block", page: pid, after: hr.id, block: newParagraph(nid) });
      renderPage(); setCaret(textOf(nid)); return;
    }
    ops.push({ op: "set_block", page: pid, block: b.id, kind, checked: false });
    edit(...ops); renderPage(); setCaret(textOf(b.id));
  } }));
  for (const x of ext.slash) if ((x.label + " " + t(x.label)).toLowerCase().includes(ql)) items.push({ gl: x.gl, label: t(x.label), run: () => {
    edit({ op: "splice_text", page: pid, block: b.id, at: caret - len, delete: len, insert: "" });
    reload(); x.run(blockOf(b.id) || b);
  } });
  if (!items.length && / $/.test(q)) { closeMenu(); return; } // The person is writing a sentence, not looking for a block.
  const r = getSelection().getRangeAt(0).getClientRects()[0] || node.getBoundingClientRect();
  menu(r, items, { keys: true, hdr: t("Blocks") });
}

/* ---------- Menus ---------- */
let menuState = null;
function closeMenu(returnFocus) {
  if (!menuState) return;
  const { node, opener, caret } = menuState;
  node.remove(); menuState = null;
  if (returnFocus) {
    if (caret && restoreCaret(caret)) return;
    if (opener?.isConnected) opener.focus();
  }
}
const kbd = keys => el("kbd", { dir: "ltr", text: keyHint(keys) });
function menu(rect, items, opts = {}) {
  const opener = document.activeElement, caret = captureCaret();
  closeMenu();
  const node = el("div", { class: "pop", role: "menu" });
  const actionable = items.filter(i => !i.sep && !i.section);
  menuState = { node, items: actionable, all: items, on: 0, keys: !!opts.keys, hdr: opts.hdr, opener, caret };
  paintMenu(); document.body.append(node);
  const w = node.offsetWidth, h = node.offsetHeight, rtl = document.documentElement.dir === "rtl";
  const left = rtl ? rect.right - w : rect.left;
  node.style.left = clamp(left, 8, Math.max(8, innerWidth - w - 8)) + "px";
  node.style.top = (rect.bottom + h + 8 > innerHeight ? Math.max(8, rect.top - h - 4) : rect.bottom + 4) + "px";
  if (!opts.keys) node.querySelector(".mi")?.focus();
}
// On the document, so that a feature's own handling on the menu runs first and is respected.
document.addEventListener("keydown", e => {
  if (menuState && !menuState.keys && !e.defaultPrevented && e.target === document.activeElement && menuState.node.contains(e.target)) menuKeys(e);
});
function menuKeys(e) {
  const list = $$(".mi", menuState.node), at = list.indexOf(document.activeElement);
  const go = i => list[(i + list.length) % list.length]?.focus();
  if (e.key === "ArrowDown") { e.preventDefault(); e.stopPropagation(); go(at + 1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); e.stopPropagation(); go(at - 1); }
  else if (e.key === "Home") { e.preventDefault(); go(0); }
  else if (e.key === "End") { e.preventDefault(); go(list.length - 1); }
  else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenu(true); }
  else if (e.key === "Tab") { e.preventDefault(); closeMenu(true); }
  else if (e.key.length === 1 && e.key !== " " && !e.ctrlKey && !e.metaKey) {
    const k = e.key.toLocaleLowerCase(lang), after = [...list.slice(at + 1), ...list.slice(0, at + 1)];
    after.find(x => x.textContent.trim().toLocaleLowerCase(lang).startsWith(k))?.focus();
  }
}
function paintMenu() {
  const { node, items, all, hdr, keys, on } = menuState; node.replaceChildren();
  if (hdr) node.append(el("div", { class: "hdr", text: hdr }));
  if (!items.length) node.append(el("div", { class: "hdr", text: t("No matches") }));
  let idx = 0;
  for (const it of all) {
    if (it.sep) { node.append(el("div", { class: "msep", role: "separator" })); continue; }
    if (it.section) { node.append(el("div", { class: "hdr", text: it.section })); continue; }
    const i = idx++;
    const b = el("button", { class: "mi" + (keys && i === on ? " on" : "") + (it.danger ? " danger" : ""), role: "menuitem", tabindex: keys ? null : "-1", onmousedown: e => { if (keys) e.preventDefault(); }, onclick: () => runMenuItem(it) },
      it.gl ? el("span", { class: "gl" }, icon(it.gl)) : null, el("span", { class: "ml", dir: it.user ? "auto" : null, text: it.label }), it.keys ? kbd(it.keys) : null);
    node.append(b);
    if (keys && i === on) queueMicrotask(() => b.scrollIntoView({ block: "nearest" }));
  }
}
function runMenuItem(it) {
  const { opener, caret } = menuState || {};
  closeMenu();
  it.run();
  // The action may have focused something itself. If not, nobody is left holding the keyboard.
  const a = document.activeElement;
  if (!a || a === document.body) { if (!(caret && restoreCaret(caret)) && opener?.isConnected) opener.focus(); else if (a === document.body) focusEditor(); }
}
document.addEventListener("pointerdown", e => {
  if (menuState && !e.target.closest(".pop")) closeMenu();
  if (!e.target.closest(".fmt") && !e.target.closest(".t")) $("#fmt").hidden = true;
}, true);

/* Selection toolbar */
document.addEventListener("selectionchange", () => {
  const s = getSelection(), f = $("#fmt");
  if (multi) paintInsel();
  if (composing) { f.hidden = true; return; }
  if (!s.rangeCount || s.isCollapsed) { f.hidden = true; return; }
  const a = s.anchorNode, node = (a.nodeType === 1 ? a : a.parentElement)?.closest(".t");
  if ((!node && !multi) || node?.closest(".code")) { f.hidden = true; return; }
  const r = s.getRangeAt(0).getBoundingClientRect();
  f.hidden = false;
  // On a phone the system's own selection menu sits by the text, so the bar stays by the keyboard.
  if (isPhone()) { const bar = $("#mobar").getBoundingClientRect(); f.style.left = "8px"; f.style.top = (bar.top - f.offsetHeight - 6) + "px"; return; }
  f.style.left = clamp(r.left, 8, Math.max(8, innerWidth - f.offsetWidth - 8)) + "px";
  f.style.top = (r.top - f.offsetHeight - 6 < 8 ? r.bottom + 6 : r.top - f.offsetHeight - 6) + "px";
});

/* ---------- Page ---------- */
function renderBlocks() {
  const box = $(".blocks"); if (!box) return;
  exitMulti();
  const held = captureCaret();
  let n = 0;
  const keep = new Map([...box.children].map(c => [c.dataset.id, c]));
  const want = page.blocks.map(b => {
    n = b.kind === "numbered" ? n + 1 : 0;
    const sig = JSON.stringify([b.kind, b.checked, b.runs, b.target, n]);
    const old = keep.get(b.id);
    // A block the engine has not changed keeps its element, its focus and its caret.
    if (old && old._sig === sig && hasText(b) && old.querySelector(":scope > .t, hr")?.tagName !== undefined && (b.kind === "divider" || old.querySelector(":scope > .t").textContent === plain(b))) return old;
    const row = blockEl(b, n); row._sig = sig; return row;
  });
  const wanted = new Set(want);
  for (const c of [...box.children]) if (!wanted.has(c)) c.remove();
  want.forEach((row, i) => { if (box.children[i] !== row) box.insertBefore(row, box.children[i] || null); });
  page.blocks.forEach((b, i) => { if (!plain(b)) paintDir(want[i], b); });
  if (held && document.activeElement === document.body && !composing) restoreCaret(held);
}
function tailClick() {
  const last = page.blocks.at(-1);
  if (last && hasText(last) && !plain(last)) { setCaret(textOf(last.id)); return; }
  addBlockAfter(last ? last.id : null);
}
/** Clicking the page, not its text: below the end, beside a line, in an empty workspace. */
function docPointer(e) {
  if (e.button) return;
  const tg = e.target;
  if (!$(".blocks") || tg.closest("button,a,input,select,textarea,label,.t,.title,.gutter")) return;
  const hit = tg.closest(".b");
  if (hit && !hit.querySelector(":scope > .t") && !hit.classList.contains("hr")) return; // A feature's block owns its clicks.
  e.preventDefault();
  const rows = $$(".blocks > .b"), y = e.clientY, title = $(".doc .title");
  if (title && rows[0] && y < rows[0].getBoundingClientRect().top) { placeAt(title, e.clientX, y); return; }
  const last = rows.at(-1);
  if (!last || y > last.getBoundingClientRect().bottom) { tailClick(); return; }
  const row = rowAtY(y), i = rows.indexOf(row);
  const textRow = row.querySelector(":scope > .t") ? row : rows.slice(i + 1).find(r => r.querySelector(":scope > .t")) || rows.slice(0, i).reverse().find(r => r.querySelector(":scope > .t"));
  if (!textRow) return;
  const node = textRow.querySelector(":scope > .t");
  placeAt(node, e.clientX, y);
  if (e.pointerType !== "touch") { const s = getSelection(); dragSel = { row: textRow, a: [s.anchorNode, s.anchorOffset], manual: true }; }
}
function setDocTitle(title) { document.title = (title ? title + " · " : "") + "CasualNote"; }
function retitle(id, title) {
  for (const n of $$("[data-label]")) if (n.dataset.label === id) n.textContent = title || t("Untitled");
  if (id === state.cur) setDocTitle(title);
}
function renderPage() {
  reload();
  if ($(".blocks")?.dataset.page === state.cur) { renderBlocks(); scheduleMeta(); return; } // Enter stays quick: panels catch up.
  const title = el("h1", { class: "title", contenteditable: "true", dir: "auto", role: "textbox", "aria-label": t("Page title"), spellcheck: "true", text: page.meta.title });
  title.classList.toggle("lat", LATIN.test(page.meta.title));
  title.addEventListener("input", () => {
    if (!title.textContent && title.firstChild) title.replaceChildren();
    const value = title.textContent;
    typed({ op: "set_title", id: page.meta.id, title: value }, "title:" + page.meta.id, { page: state.cur, id: "title", start: value.length, end: value.length });
    page.meta.title = value;
    retitle(page.meta.id, value);
    title.classList.toggle("lat", LATIN.test(value));
  });
  title.addEventListener("paste", e => { e.preventDefault(); document.execCommand("insertText", false, e.clipboardData.getData("text/plain").replace(/\r?\n+/g, " ")); });
  title.addEventListener("keydown", e => {
    if (e.isComposing) return;
    const first = page.blocks.find(hasText);
    if (e.key === "Enter") { e.preventDefault(); if (first) setCaret(textOf(first.id), 0); else tailClick(); }
    else if (e.key === "ArrowDown" && first) { e.preventDefault(); setCaret(textOf(first.id), 0); }
    else if (e.key === "Home" || e.key === "End") { e.preventDefault(); getSelection().modify(e.shiftKey ? "extend" : "move", e.key === "Home" ? "backward" : "forward", "lineboundary"); }
    else if (modOf(e) && ["b", "i", "u"].includes(keyName(e))) e.preventDefault();
  });
  title.addEventListener("beforeinput", e => { if (e.inputType.startsWith("format")) e.preventDefault(); });
  title.addEventListener("blur", () => { const o = selectionOffsets(title); if (o && !multi) lastCaret[state.cur] = { page: state.cur, id: "title", ...o }; });
  const blocks = el("div", { class: "blocks", "data-page": state.cur });
  const doc = el("div", { class: "doc" }, el("div", { class: "head" }, title), blocks, el("div", { class: "tail" }));
  const scroll = el("div", { class: "docscroll" }, doc);
  scroll.addEventListener("mousedown", docPointer);
  let spy = false;
  scroll.addEventListener("scroll", () => { if (spy) return; spy = true; requestAnimationFrame(() => { spy = false; paintSpy(); }); }, { passive: true });
  $("#content").replaceChildren(scroll);
  setDocTitle(page.meta.title);
  renderBlocks(); renderMeta();
}
function jumpTo(id) {
  const row = $(`.b[data-id="${CSS.escape(id)}"]`); if (!row) return;
  const n = textOf(id);
  if (n && !isCoarse()) { n.focus({ preventScroll: true }); const [tn, o] = pointIn(n, Infinity); getSelection().collapse(tn, o); }
  row.scrollIntoView({ block: "start", behavior: reducedMotion() ? "auto" : "smooth" });
  if (isOverlayPanel()) setRight(false, true);
}
function paintSpy() {
  const buttons = $$("#outline .oln"); if (!buttons.length || $("#right").hidden) return;
  const sc = $(".docscroll"); if (!sc) return;
  const top = sc.getBoundingClientRect().top + 24;
  let on = null;
  for (const btn of buttons) { const row = $(`.b[data-id="${CSS.escape(btn.dataset.block)}"]`); if (row && row.getBoundingClientRect().top <= top) on = btn; }
  for (const btn of buttons) { const yes = btn === on; if (btn.classList.contains("on") !== yes) { btn.classList.toggle("on", yes); yes ? btn.setAttribute("aria-current", "true") : btn.removeAttribute("aria-current"); } }
}
function renderMeta() {
  if (!page) return;
  const box = $("#outline"); box.replaceChildren();
  const heads = page.blocks.filter(b => /^heading/.test(b.kind) && plain(b));
  if (!heads.length) box.append(el("p", { text: t("Headings appear here.") }));
  const top = Math.min(...heads.map(b => +b.kind.slice(-1)));
  heads.forEach(b => {
    const btn = el("button", { class: "oln", dir: "auto", "data-block": b.id, "data-tipclip": "", text: plain(b), onclick: () => jumpTo(b.id) });
    btn.style.paddingInlineStart = (+b.kind.slice(-1) - top) * 12 + "px";
    box.append(btn);
  });
  paintSpy();
  for (const panel of ext.panels) panel(page);
  const text = [page.meta.title, ...page.blocks.map(plain)].join(" ");
  const words = [...new Intl.Segmenter(lang, { granularity: "word" }).segment(text)].filter(x => x.isWordLike).length;
  $("#counts").textContent = tf("Words: {n}", { n: new Intl.NumberFormat(lang).format(words) });
}

/* ---------- Pages ---------- */
function newPage(parent, title = "") {
  const id = uid();
  edit({ op: "create_page", id, parent: parent || null, title }, { op: "insert_block", page: id, after: null, block: newParagraph(uid()) });
  if (parent) state.open[parent] = true;
  openPage(id); $("#content .title")?.focus();
  return id;
}
function openPage(id) {
  if (state.cur && !state.view) { const c = captureCaret(); if (c) lastCaret[state.cur] = c; const sc = $(".docscroll"); if (sc) pageScroll[state.cur] = sc.scrollTop; }
  state.view = null; state.cur = id; if (!state.tabs.includes(id)) state.tabs.push(id);
  let q = metaOf(id), all = pages(); while (q?.parent) { state.open[q.parent] = true; q = all.find(x => x.id === q.parent); }
  ui.recent = [id, ...ui.recent.filter(x => x !== id)].slice(0, 12);
  renderAll();
  const sc = $(".docscroll"); if (sc && pageScroll[id]) sc.scrollTop = pageScroll[id];
  if (isPhone()) setSide(false);
}
function openView(id) {
  if (state.cur && !state.view) { const c = captureCaret(); if (c) lastCaret[state.cur] = c; }
  state.view = id; renderAll();
  if (isPhone()) setSide(false);
}
function trashPage(id) {
  const m = metaOf(id); if (!m) return;
  edit({ op: "set_trashed", id, value: true });
  if (state.cur === id) state.cur = null;
  renderAll();
  toast(t("Moved to Trash"), { label: t("Undo"), run: () => { edit({ op: "set_trashed", id, value: false }); openPage(id); focusEditor(); } });
  if (!state.view) focusEditor();
}
function restorePage(id) { edit({ op: "set_trashed", id, value: false }); renderAll(); }
function duplicatePage(id) {
  const p = pageData(id), nid = uid();
  const ops = [{ op: "create_page", id: nid, parent: p.meta.parent || null, title: p.meta.title ? tf("Copy of {title}", { title: p.meta.title }) : "" }];
  let after = null;
  for (const b of p.blocks) { const bid = uid(); ops.push({ op: "insert_block", page: nid, after, block: { ...b, id: bid } }); after = bid; }
  if (!p.blocks.length) ops.push({ op: "insert_block", page: nid, after: null, block: newParagraph(uid()) });
  edit(...ops); openPage(nid);
  return nid;
}
function descends(id, from) { let q = metaOf(id); const all = pages(); while (q) { if (q.id === from) return true; q = all.find(x => x.id === q.parent); } return false; }
function movePage(id, parent) {
  if (parent && (parent === id || descends(parent, id))) return false;
  const m = metaOf(id); if (!m || (m.parent || null) === (parent || null)) return false;
  edit({ op: "move_page", id, parent: parent || null });
  if (parent) state.open[parent] = true;
  saveUI(); renderSide(); renderCrumbs();
  return true;
}
function moveMenu(p, rect) {
  const others = live().filter(x => x.id !== p.id && !descends(x.id, p.id));
  menu(rect, [
    ...(p.parent ? [{ gl: "file-text", label: t("Top level"), run: () => movePage(p.id, null) }] : []),
    ...others.filter(x => x.id !== p.parent).map(x => ({ gl: "file-text", user: true, label: x.title || t("Untitled"), run: () => movePage(p.id, x.id) })),
  ], { hdr: t("Move to") });
}
function pageItems(p, rect) {
  const fav = p.favorite;
  return [
    { gl: "plus", label: t("New subpage"), run: () => newPage(p.id) },
    { gl: "pencil-simple", label: t("Rename"), keys: "f2", run: () => startRename(p.id) },
    { gl: fav ? "star-fill" : "star", label: t(fav ? "Remove from favorites" : "Add to favorites"), run: () => { edit({ op: "set_favorite", id: p.id, value: !fav }); saveUI(); renderSide(); renderCrumbs(); } },
    ...(p.database ? [] : [{ gl: "copy", label: t("Duplicate"), run: () => duplicatePage(p.id) }]),
    { gl: "arrow-up-right", label: t("Move to"), run: () => moveMenu(p, rect) },
    ...ext.pageMenu.flatMap(f => f(p)),
    { sep: true },
    { gl: "trash", label: t("Move to Trash"), danger: true, run: () => trashPage(p.id) },
  ];
}
function startRename(id) {
  if (isPhone() && $("#side").classList.contains("closed")) setSide(true);
  const row = $$("#tree .row").find(r => r.dataset.id === id && r.dataset.sec === "pages") || $$("#tree .row").find(r => r.dataset.id === id);
  if (!row) return;
  const label = row.querySelector(".open span"), m = metaOf(id), had = !!document.activeElement?.closest?.(".t,.title");
  const input = el("input", { class: "rename", dir: "auto", value: m.title, "aria-label": t("Rename") });
  input.value = m.title;
  label.replaceWith(input);
  let done = false;
  const finish = save => {
    if (done) return; done = true;
    const value = input.value.trim();
    if (save && value !== m.title) { edit({ op: "set_title", id, title: value }); if (id === state.cur) { reload(); const title = $(".doc .title"); if (title && document.activeElement !== title) title.textContent = value; } }
    renderSide(); renderTabs(); renderCrumbs();
    const next = $$("#tree .row").find(r => r.dataset.id === id && r.dataset.sec === row.dataset.sec);
    if (next) focusRow(next);
    return had;
  };
  input.addEventListener("keydown", e => {
    e.stopPropagation();
    if (e.isComposing) return;
    if (e.key === "Enter") { e.preventDefault(); finish(true); }
    if (e.key === "Escape") { e.preventDefault(); finish(false); }
  });
  input.addEventListener("blur", () => { if (!done) { done = true; const value = input.value.trim(); if (value !== m.title) { edit({ op: "set_title", id, title: value }); if (id === state.cur) { reload(); const title = $(".doc .title"); if (title) title.textContent = value; } } renderSide(); renderTabs(); renderCrumbs(); } });
  input.addEventListener("click", e => e.stopPropagation());
  input.addEventListener("pointerdown", e => e.stopPropagation());
  input.focus(); input.select();
}

/* ---------- Sidebar ---------- */
function focusRow(row) {
  $$("#tree .row").forEach(r => r.setAttribute("tabindex", r === row ? "0" : "-1"));
  row.focus();
}
function renderSide() {
  const tree = $("#tree");
  const was = document.activeElement?.closest?.("#tree .row"), wasKey = was ? was.dataset.sec + ":" + was.dataset.id : null;
  tree.replaceChildren();
  const all = pages(), active = all.filter(p => !p.trashed);
  const row = (p, depth, kidsToo, sec, into) => {
    const kids = active.filter(x => x.parent === p.id), open = !!state.open[p.id], current = p.id === state.cur && !state.view;
    const chev = el("button", { class: "chev", tabindex: "-1", "aria-hidden": kidsToo && kids.length ? null : "true", "aria-label": t(open ? "Collapse" : "Expand"), onclick: e => { e.stopPropagation(); state.open[p.id] = !open; saveUI(); renderSide(); } }, kidsToo && kids.length ? icon(open ? "caret-down" : "caret-right") : null);
    const r = el("div", { class: "row" + (current ? " on" : ""), role: "treeitem", tabindex: "-1", "data-id": p.id, "data-sec": sec, "aria-level": String(depth + 1), "aria-selected": String(current), ...(current ? { "aria-current": "page" } : {}), ...(kidsToo && kids.length ? { "aria-expanded": String(open) } : {}) }, chev,
      el("button", { class: "open", tabindex: "-1", onclick: e => { if (e.detail > 1) return; if (p.id === state.cur && !state.view) focusEditor(); else { openPage(p.id); focusEditor(); } }, ondblclick: e => { e.preventDefault(); startRename(p.id); } },
        icon(p.database ? "table" : "file-text"), el("span", { dir: "auto", "data-label": p.id, "data-tipclip": "", text: p.title || t("Untitled") })),
      el("div", { class: "acts" },
        el("button", { class: "more", tabindex: "-1", "aria-label": t("More"), "aria-haspopup": "menu", "data-tip": t("More"), onclick: e => { e.stopPropagation(); const rc = e.currentTarget.getBoundingClientRect(); menu(rc, pageItems(metaOf(p.id), rc)); } }, icon("dots-three")),
        el("button", { class: "add", tabindex: "-1", "aria-label": t("Add subpage"), "data-tip": t("Add subpage"), onclick: e => { e.stopPropagation(); newPage(p.id); } }, icon("plus"))));
    r.style.paddingInlineStart = depth * 14 + "px";
    r.addEventListener("contextmenu", e => { e.preventDefault(); const rc = { left: e.clientX, right: e.clientX, top: e.clientY, bottom: e.clientY }; menu(rc, pageItems(metaOf(p.id), rc)); });
    r.addEventListener("pointerdown", rowDrag);
    into.append(r);
    if (kidsToo && open) kids.forEach(k => row(k, depth + 1, true, sec, into));
  };
  const favs = active.filter(p => p.favorite);
  if (favs.length) {
    const g = el("div", { role: "group", "aria-label": t("Favorites") }, el("div", { class: "sec", "aria-hidden": "true", text: t("Favorites") }));
    tree.append(g); favs.forEach(p => row(p, 0, false, "fav", g));
  }
  const g = el("div", { role: "group", "aria-label": t("Pages") }, el("div", { class: "sec droproot", "aria-hidden": "true" }, el("span", { text: t("Pages") })));
  tree.append(g);
  active.filter(p => !p.parent || !active.some(x => x.id === p.parent)).forEach(p => row(p, 0, true, "pages", g));
  const rows = $$(".row", tree);
  const anchor = rows.find(r => r.dataset.sec + ":" + r.dataset.id === wasKey) || rows.find(r => r.classList.contains("on") && r.dataset.sec === "pages") || rows.find(r => r.classList.contains("on")) || rows[0];
  if (anchor) { anchor.setAttribute("tabindex", "0"); if (wasKey) anchor.focus(); }
  const trash = all.filter(p => p.trashed).length;
  $("#trashCount").textContent = trash ? new Intl.NumberFormat(lang).format(trash) : "";
  $("#trashBtn").classList.toggle("on", state.view === "trash");
  $("#settingsBtn").classList.toggle("on", state.view === "settings");
}
function treeKeys(e) {
  if (e.target.closest(".rename") || e.isComposing) return;
  const row = e.target.closest(".row"); if (!row) return;
  const rows = $$("#tree .row"), i = rows.indexOf(row), id = row.dataset.id, rtl = document.documentElement.dir === "rtl";
  const forward = rtl ? "ArrowLeft" : "ArrowRight", back = rtl ? "ArrowRight" : "ArrowLeft";
  const stop = () => { e.preventDefault(); e.stopPropagation(); };
  if (e.key === "ArrowDown") { stop(); if (rows[i + 1]) focusRow(rows[i + 1]); }
  else if (e.key === "ArrowUp") { stop(); if (rows[i - 1]) focusRow(rows[i - 1]); }
  else if (e.key === "Home") { stop(); focusRow(rows[0]); }
  else if (e.key === "End") { stop(); focusRow(rows.at(-1)); }
  else if (e.key === forward) {
    stop();
    if (row.getAttribute("aria-expanded") === "false") { state.open[id] = true; saveUI(); renderSide(); }
    else if (row.getAttribute("aria-expanded") === "true" && rows[i + 1]) focusRow(rows[i + 1]);
  } else if (e.key === back) {
    stop();
    if (row.getAttribute("aria-expanded") === "true") { state.open[id] = false; saveUI(); renderSide(); }
    else { const level = +row.getAttribute("aria-level"); const parent = rows.slice(0, i).reverse().find(r => +r.getAttribute("aria-level") < level); if (parent) focusRow(parent); }
  } else if (e.key === "Enter") { stop(); if (id !== state.cur || state.view) openPage(id); focusEditor(); }
  else if (e.key === " ") { stop(); openPage(id); const again = $$("#tree .row").find(r => r.dataset.id === id && r.dataset.sec === row.dataset.sec); if (again) focusRow(again); }
  else if (e.key === "F2") { stop(); startRename(id); }
  else if (e.key === "Delete") { stop(); const next = rows[i + 1] || rows[i - 1]; trashPage(id); const again = next && $$("#tree .row").find(r => r.dataset.id === next.dataset.id && r.dataset.sec === next.dataset.sec); if (again) focusRow(again); }
  else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) { stop(); const rc = row.getBoundingClientRect(); menu(rc, pageItems(metaOf(id), rc)); }
  else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
    const k = e.key.toLocaleLowerCase(lang), after = [...rows.slice(i + 1), ...rows.slice(0, i + 1)];
    const hit = after.find(r => r.querySelector(".open span")?.textContent.toLocaleLowerCase(lang).startsWith(k));
    if (hit) { stop(); focusRow(hit); }
  }
}
function rowDrag(e) {
  if (e.button || e.pointerType !== "mouse" || e.target.closest(".acts,.chev,.rename")) return;
  const row = e.currentTarget, id = row.dataset.id, sx = e.clientX, sy = e.clientY;
  let drag = false, ghost = null, target, valid = false, cancelled = false;
  const clear = () => $$(".drop-in").forEach(n => n.classList.remove("drop-in"));
  const mv = ev => {
    if (!drag && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
    if (!drag) {
      drag = true; hideTip();
      document.body.classList.add("dragging");
      row.classList.add("lifted");
      ghost = el("div", { class: "ghost", dir: "auto", text: metaOf(id)?.title || t("Untitled") });
      document.body.append(ghost);
    }
    ghost.style.left = (ev.clientX + 12) + "px"; ghost.style.top = (ev.clientY + 8) + "px";
    const tb = $("#tree").getBoundingClientRect();
    if (ev.clientY < tb.top + 24) $("#tree").scrollBy(0, -12); else if (ev.clientY > tb.bottom - 24) $("#tree").scrollBy(0, 12);
    clear(); target = undefined; valid = false;
    const over = document.elementFromPoint(ev.clientX, ev.clientY);
    const r = over?.closest?.("#tree .row"), root = over?.closest?.(".droproot");
    if (r && r.dataset.id !== id && !descends(r.dataset.id, id)) { target = r.dataset.id; valid = true; r.classList.add("drop-in"); }
    else if (root) { target = null; valid = true; root.classList.add("drop-in"); }
  };
  const esc = ev => { if (ev.key === "Escape") { ev.stopPropagation(); cancelled = true; up(); } };
  const up = () => {
    document.removeEventListener("pointermove", mv); document.removeEventListener("pointerup", up); document.removeEventListener("keydown", esc, true);
    clear(); ghost?.remove(); row.classList.remove("lifted"); document.body.classList.remove("dragging");
    if (!drag) return;
    // The click that ends a drag must not also open the page.
    const swallow = ev => { ev.stopPropagation(); ev.preventDefault(); };
    document.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => document.removeEventListener("click", swallow, true), 0);
    if (!cancelled && valid) movePage(id, target);
  };
  document.addEventListener("pointermove", mv); document.addEventListener("pointerup", up); document.addEventListener("keydown", esc, true);
}

/* ---------- Sidebar and panel visibility and size ---------- */
function paintChrome() {
  const side = $("#side"), right = $("#right"), phone = isPhone(), overlay = isOverlayPanel();
  side.style.width = phone ? "" : ui.sideW + "px";
  right.style.width = overlay ? "" : ui.rightW + "px";
  const sideOpen = !side.classList.contains("closed");
  document.body.classList.toggle("side-closed", !sideOpen);
  $("#sideSplit").hidden = phone || !sideOpen;
  $("#rightSplit").hidden = overlay || !state.right;
  if (right.hidden === !!state.right) right.hidden = !state.right; // Set only on a real change: a feature may be watching, and so is this file.
  $("#rightBtn").setAttribute("aria-pressed", String(!!state.right));
  const scrim = (phone && sideOpen) || (overlay && state.right);
  $("#scrim").hidden = !scrim;
  $("#center").inert = !!scrim;
  side.inert = phone && !sideOpen;
  $("#menuBtn").setAttribute("aria-expanded", String(sideOpen));
  for (const [q, v] of [["#sideSplit", ui.sideW], ["#rightSplit", ui.rightW]]) $(q).setAttribute("aria-valuenow", String(Math.round(v)));
}
function setSide(open, moveFocus) {
  const side = $("#side");
  side.classList.toggle("closed", !open);
  if (!isPhone()) { ui.side = open; saveUI(); }
  paintChrome();
  if (moveFocus) {
    if (open) ($("#tree .row[tabindex='0']") || $("#searchBtn")).focus();
    else if (isPhone()) $("#menuBtn").focus();
    else if (!document.activeElement || document.activeElement === document.body) focusEditor();
  }
}
function setRight(open, moveFocus) {
  state.right = open; saveUI(); paintChrome();
  if (moveFocus) { if (open) $("#right button:not([hidden])")?.focus(); else if (isCoarse()) $("#rightBtn").focus(); else if (!document.activeElement || document.activeElement === document.body) { focusEditor(); if (document.activeElement === document.body) $("#rightBtn").focus(); } }
}
function wireSplit(handleEl, read, write, range, sign) {
  const move = ev => {
    const rtl = document.documentElement.dir === "rtl";
    const room = innerWidth - 320; // The document always keeps a usable width.
    write(clamp(ev(rtl), range[0], Math.min(range[1], room))); paintChrome(); saveUI();
  };
  handleEl.addEventListener("pointerdown", e => {
    if (e.button) return;
    e.preventDefault(); try { handleEl.setPointerCapture(e.pointerId); } catch { /* not a live pointer */ } handleEl.classList.add("drag"); document.body.classList.add("resizing");
    const sx = e.clientX, from = read();
    const mv = m => move(rtl => from + (m.clientX - sx) * (rtl ? -1 : 1) * sign);
    const up = () => { handleEl.removeEventListener("pointermove", mv); handleEl.removeEventListener("pointerup", up); handleEl.removeEventListener("pointercancel", up); handleEl.classList.remove("drag"); document.body.classList.remove("resizing"); };
    handleEl.addEventListener("pointermove", mv); handleEl.addEventListener("pointerup", up); handleEl.addEventListener("pointercancel", up);
  });
  handleEl.addEventListener("keydown", e => {
    const rtl = document.documentElement.dir === "rtl", grow = (rtl ? "ArrowLeft" : "ArrowRight"), shrink = (rtl ? "ArrowRight" : "ArrowLeft");
    if (e.key !== grow && e.key !== shrink) return;
    e.preventDefault();
    const step = (e.key === grow ? 16 : -16) * sign;
    move(() => read() + step);
  });
  handleEl.setAttribute("aria-valuemin", String(range[0])); handleEl.setAttribute("aria-valuemax", String(range[1]));
}

/* ---------- Tabs ---------- */
function closeTab(id, focusAfter) {
  if (state.tabs.length < 2) return;
  const i = state.tabs.indexOf(id); if (i < 0) return;
  state.tabs.splice(i, 1);
  const wasCur = state.cur === id && !state.view;
  if (state.cur === id) state.cur = state.tabs[Math.max(0, i - 1)];
  renderAll();
  if (focusAfter) ($("#tabs .tab.on .tl") || $("#tabs .tl"))?.focus();
  else if (wasCur) focusEditor();
}
function renderTabs() {
  const box = $("#tabs"), keep = box.scrollLeft, was = document.activeElement?.closest?.("#tabs .tab")?.dataset.id;
  box.replaceChildren();
  const active = live();
  state.tabs = state.tabs.filter(id => active.some(p => p.id === id));
  state.tabs.forEach(id => {
    const p = active.find(x => x.id === id), on = id === state.cur && !state.view;
    const tab = el("div", { class: "tab" + (on ? " on" : ""), role: "presentation", "data-id": id },
      el("button", { class: "tl", role: "tab", "aria-selected": String(on), tabindex: "-1", dir: "auto", "data-label": id, "data-tipclip": "", text: p.title || t("Untitled"), onclick: () => { if (id === state.cur && !state.view) focusEditor(); else { openPage(id); focusEditor(); } } }),
      state.tabs.length > 1 ? el("button", { class: "cl", tabindex: "-1", "aria-label": t("Close tab"), "data-tip": t("Close tab"), onclick: e => { e.stopPropagation(); closeTab(id); } }, icon("x")) : null);
    tab.addEventListener("mousedown", e => { if (e.button === 1) e.preventDefault(); });
    tab.addEventListener("auxclick", e => { if (e.button === 1) { e.preventDefault(); closeTab(id); } });
    tab.addEventListener("contextmenu", e => {
      e.preventDefault();
      const rc = { left: e.clientX, right: e.clientX, top: e.clientY, bottom: e.clientY };
      menu(rc, [
        ...(state.tabs.length > 1 ? [{ gl: "x", label: t("Close tab"), run: () => closeTab(id) }, { gl: "x", label: t("Close other tabs"), run: () => { state.tabs = [id]; if (!state.view) state.cur = id; renderAll(); } }] : []),
        ...pageItems(metaOf(id), rc),
      ]);
    });
    box.append(tab);
  });
  const roving = $(".tab.on .tl", box) || $(".tl", box);
  roving?.setAttribute("tabindex", "0");
  box.scrollLeft = keep;
  $(".tab.on", box)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  if (was) $$(".tab", box).find(x => x.dataset.id === was)?.querySelector(".tl")?.focus();
  paintTabOverflow();
}
function paintTabOverflow() { const box = $("#tabs"); $("#tabMore").hidden = box.scrollWidth <= box.clientWidth + 1; }
function tabKeys(e) {
  const tab = e.target.closest(".tab"); if (!tab) return;
  const list = $$("#tabs .tl"), i = list.indexOf(tab.querySelector(".tl")), rtl = document.documentElement.dir === "rtl";
  const next = rtl ? "ArrowLeft" : "ArrowRight", prev = rtl ? "ArrowRight" : "ArrowLeft";
  const go = j => { const to = list[(j + list.length) % list.length]; list.forEach(x => x.setAttribute("tabindex", x === to ? "0" : "-1")); to.focus(); to.scrollIntoView({ block: "nearest", inline: "nearest" }); };
  if (e.key === next) { e.preventDefault(); go(i + 1); }
  else if (e.key === prev) { e.preventDefault(); go(i - 1); }
  else if (e.key === "Home") { e.preventDefault(); go(0); }
  else if (e.key === "End") { e.preventDefault(); go(list.length - 1); }
  else if (e.key === "Delete") { e.preventDefault(); closeTab(tab.dataset.id, true); }
}
function renderCrumbs() {
  const box = $("#crumbs"); box.replaceChildren();
  ["#favBtn", "#moreBtn"].forEach(q => { $(q).hidden = !!state.view; });
  if (state.view) { box.append(el("b", { text: t(state.view === "settings" ? "Settings" : ext.views[state.view]?.label || "Settings") })); return; }
  const all = pages(), chain = []; let p = all.find(x => x.id === state.cur);
  while (p) { chain.unshift(p); p = all.find(x => x.id === p.parent); }
  chain.forEach((q, i) => {
    if (i) box.append(el("span", { class: "csep", "aria-hidden": "true", text: "/" }));
    box.append(i === chain.length - 1 ? el("b", { dir: "auto", "data-label": q.id, text: q.title || t("Untitled") }) : el("button", { dir: "auto", "data-label": q.id, text: q.title || t("Untitled"), onclick: () => { openPage(q.id); focusEditor(); } }));
  });
  const cur = chain.at(-1);
  $("#favBtn").replaceChildren(icon(cur?.favorite ? "star-fill" : "star"));
  $("#favBtn").setAttribute("aria-pressed", String(!!cur?.favorite));
}

/* ---------- Settings and Trash ---------- */
function setTheme(mode) {
  try { mode === "system" ? localStorage.removeItem("casualnote-theme") : localStorage.setItem("casualnote-theme", mode); } catch { /* optional */ }
  if (mode === "system") delete document.documentElement.dataset.theme; else document.documentElement.dataset.theme = mode;
}
const SHORTCUTS = [
  ["Search", "mod+k"], ["New page", "new"], ["Show or hide sidebar", "mod+\\"], ["Show or hide side panel", "mod+shift+\\"], ["Settings", "mod+,"],
  ["Move between regions", "f6"], ["Undo", "mod+z"], ["Redo", "mod+shift+z"], ["Bold", "mod+b"], ["Italic", "mod+i"], ["Code", "mod+e"], ["Highlight", "mod+shift+h"],
  ["Line break in a block", "shift+enter"], ["Select the block, then the page", "mod+a"], ["Select this block", "esc"], ["Move block", "mod+shift+up"], ["Duplicate", "mod+d"], ["Block menu", "mod+shift+/"],
  ["Heading 1", "mod+shift+1"], ["Heading 2", "mod+shift+2"], ["Heading 3", "mod+shift+3"], ["Bulleted list", "mod+shift+8"], ["Numbered list", "mod+shift+7"], ["To-do", "mod+shift+9"], ["Check or uncheck", "mod+enter"],
  ["Rename page", "f2"],
];
function renderSettings() {
  const sel = el("select", { class: "langsel", id: "langSel", "aria-label": t("Language"), onchange: e => { setLang(e.target.value); renderAll(); $("#langSel")?.focus(); } }, ...LANGS.map(([c, n]) => el("option", { value: c, text: n, ...(c === lang ? { selected: true } : {}) })));
  const box = el("div", { class: "agent" }, el("h1", { class: "title", text: t("Settings") }), el("h2", { text: t("Language") }), sel);
  let cur = "system"; try { cur = localStorage.getItem("casualnote-theme") || "system"; } catch { /* optional */ }
  const seg = el("div", { class: "seg2", role: "group", "aria-label": t("Appearance") });
  for (const [mode, label] of [["system", "System"], ["light", "Light"], ["dark", "Dark"]]) {
    seg.append(el("button", { "data-theme-mode": mode, "aria-pressed": String(mode === cur), text: t(label), onclick: () => { setTheme(mode); $$("button", seg).forEach(b => b.setAttribute("aria-pressed", String(b.dataset.themeMode === mode))); } }));
  }
  box.append(el("h2", { text: t("Appearance") }), seg);
  for (const section of ext.settings) section(box);
  const list = el("dl", { class: "keys", id: "shortcuts" });
  for (const [label, keys] of SHORTCUTS) list.append(el("div", {}, el("dt", { text: t(label) }), el("dd", {}, kbd(keys))));
  box.append(el("h2", { text: t("Keyboard shortcuts") }), list);
  $("#content").replaceChildren(el("div", { class: "docscroll" }, box));
  setDocTitle(t("Settings"));
}
function renderTrash(content) {
  const items = pages().filter(p => p.trashed);
  const box = el("div", { class: "agent" }, el("h1", { class: "title", text: t("Trash") }));
  if (!items.length) box.append(el("p", { class: "empty", text: t("Trash is empty.") }));
  else {
    const list = el("ul", { class: "trashlist" });
    for (const p of items) {
      list.append(el("li", { "data-id": p.id }, icon(p.database ? "table" : "file-text"), el("span", { class: "tt", dir: "auto", text: p.title || t("Untitled") }),
        el("button", { class: "btn", onclick: () => {
          restorePage(p.id);
          const nextBtn = $(".trashlist button") || $("#trashBtn");
          nextBtn.focus();
          toast(t("Page restored"), { label: t("Open page"), run: () => { openPage(p.id); focusEditor(); } });
        } }, icon("arrow-counter-clockwise"), el("span", { text: t("Restore") }))));
    }
    box.append(list);
  }
  content.replaceChildren(el("div", { class: "docscroll" }, box));
  setDocTitle(t("Trash"));
}
function renderAll() {
  if (!live().length) { const id = uid(); engine.apply(JSON.stringify({ op: "create_page", id, parent: null, title: "" })); engine.apply(JSON.stringify({ op: "insert_block", page: id, after: null, block: newParagraph(uid()) })); store.changed(); state.cur = id; }
  if (!state.cur || !live().some(p => p.id === state.cur)) state.cur = state.tabs.find(id => live().some(p => p.id === id)) || live()[0].id;
  if (!state.tabs.includes(state.cur)) state.tabs.push(state.cur);
  renderSide(); renderTabs(); renderCrumbs();
  if (state.view === "settings") { $("#content").replaceChildren(); renderSettings(); }
  else if (ext.views[state.view]) ext.views[state.view].render($("#content"));
  else if (ext.pageView?.(pageData(state.cur))) { reload(); setDocTitle(page.meta.title); }
  else { const had = $(".blocks"); if (had) had.dataset.page = ""; renderPage(); }
  $("#right").classList.toggle("plainview", state.view === "settings" || state.view === "trash");
  if (state.view) $("#counts").textContent = "";
  paintChrome();
  saveUI();
}
function setLang(code) {
  lang = code; try { localStorage.setItem("casualnote-lang", code); } catch { /* optional */ }
  document.documentElement.lang = code; document.documentElement.dir = code === "ar" ? "rtl" : "ltr";
  document.documentElement.style.setProperty("--untitled", JSON.stringify(t("Untitled")));
  $$("[data-t]").forEach(n => { n.textContent = t(n.dataset.t); });
  $$("[data-tl]").forEach(n => { n.setAttribute("aria-label", t(n.dataset.tl)); n.dataset.tip = t(n.dataset.tl); });
  $$("[data-kbd]").forEach(n => { n.textContent = keyHint(n.dataset.kbd); n.setAttribute("dir", "ltr"); });
  const boot = $("#boot"); if (boot && !boot.classList.contains("err")) boot.textContent = t("Opening your notes");
  paintSaved();
  paintBanner();
}

/* ---------- Shortcuts as people read them ---------- */
const KEY_WORD = { mod: MAC ? "⌘" : "Ctrl", shift: MAC ? "⇧" : "Shift", alt: MAC ? "⌥" : "Alt", enter: MAC ? "↩" : "Enter", up: "↑", down: "↓", esc: "Esc", f2: "F2", f6: "F6" };
function keyHint(keys) {
  if (keys === "new") return MAC ? "⌃N" : "Alt N"; // Browsers keep Ctrl+N. The desktop shell has it too.
  return keys.split("+").map(k => KEY_WORD[k] || (k === "\\" ? "\\" : k.toUpperCase())).join(MAC ? "" : " ");
}

/* ---------- Tooltips: label and shortcut, after a short rest on the control ---------- */
let tipTimer = null, tipFor = null;
function hideTip() { clearTimeout(tipTimer); tipFor = null; $("#tip").hidden = true; }
function showTip(target) {
  const tip = $("#tip"), clipped = "tipclip" in target.dataset;
  if (clipped && target.scrollWidth <= target.clientWidth) return;
  tip.replaceChildren(el("span", { dir: "auto", text: clipped ? target.textContent : target.dataset.tip }));
  if (target.dataset.keys) tip.append(kbd(target.dataset.keys));
  tip.hidden = false;
  const r = target.getBoundingClientRect(), w = tip.offsetWidth, h = tip.offsetHeight;
  tip.style.left = clamp(r.left + r.width / 2 - w / 2, 6, Math.max(6, innerWidth - w - 6)) + "px";
  tip.style.top = (r.bottom + h + 8 > innerHeight ? r.top - h - 5 : r.bottom + 5) + "px";
}
function wireTips() {
  const pick = n => n?.closest?.("[data-tip],[data-tipclip]");
  document.addEventListener("pointerover", e => {
    if (e.pointerType !== "mouse" || document.body.classList.contains("dragging")) return;
    const target = pick(e.target);
    if (target === tipFor) return;
    hideTip();
    if (!target) return;
    tipFor = target;
    tipTimer = setTimeout(() => { if (tipFor === target && target.isConnected) showTip(target); }, 450);
  });
  document.addEventListener("focusin", e => {
    const target = pick(e.target);
    if (!target || !target.matches(":focus-visible") || !target.dataset.tip) { if (tipFor) hideTip(); return; }
    hideTip(); tipFor = target;
    tipTimer = setTimeout(() => { if (tipFor === target && target.isConnected) showTip(target); }, 450);
  });
  document.addEventListener("focusout", hideTip);
  document.addEventListener("pointerdown", hideTip, true);
  document.addEventListener("keydown", e => { if (e.key === "Escape") hideTip(); }, true);
  document.addEventListener("scroll", () => { if (!$("#tip").hidden) hideTip(); }, { capture: true, passive: true });
}

/* ---------- Toast ---------- */
let toastTimer = null;
function toast(message, action) {
  $(".toast")?.remove(); clearTimeout(toastTimer);
  const node = el("div", { class: "toast", role: "status" }, el("span", { text: message }),
    action ? el("button", { text: action.label, onclick: () => { node.remove(); action.run(); } }) : null);
  document.body.append(node);
  toastTimer = setTimeout(() => node.remove(), action ? 7000 : 2600);
  return node;
}

/* ---------- Search across every page, offline. The engine index comes with Phase 4. ---------- */
const fold = s => s.toLocaleLowerCase(lang).normalize("NFC");
const SEG = new Intl.Segmenter(undefined, { granularity: "grapheme" });
/** The range of the first match, in the original string, widened to whole characters as a reader sees them. */
function matchRange(text, q) {
  if (!q) return null;
  let foldedText = ""; const seg = [];
  for (const { segment, index } of SEG.segment(text)) { const f = fold(segment); seg.push({ at: foldedText.length, index, end: index + segment.length }); foldedText += f; }
  const hit = foldedText.indexOf(q); if (hit < 0) return null;
  const stop = hit + q.length;
  let a = seg[0], b = seg[0];
  for (const g of seg) { if (g.at <= hit) a = g; if (g.at < stop) b = g; }
  return [a.index, b.end];
}
function highlighted(text, q, clip = 0) {
  const m = matchRange(text, q), out = document.createDocumentFragment();
  if (!m) { out.append(clip && text.length > clip ? text.slice(0, clip) + "…" : text); return out; }
  let from = 0, to = text.length;
  if (clip && text.length > clip) { from = Math.max(0, m[0] - Math.floor(clip / 3)); to = Math.min(text.length, from + clip); }
  out.append((from ? "…" : "") + text.slice(from, m[0]), el("mark", { text: text.slice(m[0], m[1]) }), text.slice(m[1], to) + (to < text.length ? "…" : ""));
  return out;
}
/** Marks given by the engine's index, as ranges over the excerpt. Still only text nodes. */
function marked(text, marks, cuts) {
  const out = document.createDocumentFragment(); let at = 0;
  if (cuts?.[0]) out.append("… ");
  for (const [a, b] of marks) { if (a > at) out.append(text.slice(at, a)); out.append(el("mark", { text: text.slice(a, b) })); at = b; }
  out.append(text.slice(at));
  if (cuts?.[1]) out.append(" …");
  return out;
}
let searchReturn = null;
function openSearch() {
  const held = $("#palette") ? searchReturn : { caret: captureCaret(), focus: document.activeElement };
  closeSearch(false);
  searchReturn = held;
  const input = el("input", { id: "palette", type: "search", role: "combobox", "aria-expanded": "true", "aria-controls": "paletteList", "aria-autocomplete": "list", autocomplete: "off", spellcheck: "false", dir: "auto", placeholder: t("Search"), "aria-label": t("Search") });
  const list = el("div", { class: "list", id: "paletteList", role: "listbox", "aria-label": t("Search") });
  // Read once per opening. Typing in the palette must not re-parse every page per key.
  const metas = live(), known = new Map(metas.map(m => [m.id, m]));
  let index = null;
  const linear = () => index || (index = metas.map(m => ({ m, blocks: pageData(m.id).blocks.map(b => ({ id: b.id, text: plain(b), f: fold(plain(b)) })), f: fold(m.title || "") })));
  // The engine's index, where this build has one. Otherwise a plain scan, so that search always works.
  const engineHits = q => {
    if (typeof engine.search !== "function") return null;
    try {
      return JSON.parse(engine.search(q, 40)).filter(h => known.has(h.page)).map(h => ({ m: known.get(h.page), inTitle: h.field === "title", x: h.field === "title" ? null : h.text, marks: h.marks, cuts: [h.cut_start, h.cut_end], b: h.item || null, table: h.field === "row" || h.field === "cell", raw: h }));
    } catch (err) { console.warn("search index", err); return null; }
  };
  let on = 0, hits = [], query = "";
  const pathOf = m => { const names = []; let p = known.get(m.parent); while (p) { names.unshift(p.title || t("Untitled")); p = known.get(p.parent); } return names.join(" / "); };
  const paintOn = () => {
    const opts = $$(".mi,.mirow", list);
    opts.forEach((n, i) => { n.classList.toggle("on", i === on); n.setAttribute("aria-selected", String(i === on)); });
    const cur = opts[on];
    if (cur) { input.setAttribute("aria-activedescendant", cur.id); cur.scrollIntoView({ block: "nearest" }); } else input.removeAttribute("aria-activedescendant");
  };
  const paint = () => {
    query = fold(input.value.trim()); hits = [];
    if (!query) {
      const seen = [...ui.recent.filter(id => known.has(id)), ...metas.map(x => x.id).filter(id => !ui.recent.includes(id))];
      for (const id of seen.slice(0, 12)) hits.push({ m: known.get(id) });
    } else {
      const found = engineHits(input.value.trim());
      if (found) hits = found;
      else {
        for (const x of linear()) if (x.f.includes(query)) hits.push({ m: x.m, inTitle: true });
        for (const x of linear()) for (const b of x.blocks) if (b.f.includes(query)) hits.push({ m: x.m, x: b.text, b: b.id });
      }
    }
    hits = hits.slice(0, 40); on = Math.min(on, Math.max(0, hits.length - 1));
    const rows = [];
    if (!query && hits.length) rows.push(el("div", { class: "hdr", role: "presentation", text: t("Recent pages") }));
    hits.forEach((h, i) => {
      const name = el("span", { class: "nm palt", dir: "auto" }); name.append(h.inTitle ? (h.marks ? marked(h.raw.text, h.marks) : highlighted(h.m.title, query)) : (h.m.title || t("Untitled")));
      const small = h.x ? el("small", { dir: "auto" }, h.marks ? marked(h.x, h.marks, h.cuts) : highlighted(h.x, query, 120)) : (pathOf(h.m) ? el("small", { dir: "auto", text: pathOf(h.m) }) : null);
      rows.push(el("button", { class: "mi", role: "option", id: "palo" + i, tabindex: "-1", onmousedown: e => e.preventDefault(), onclick: () => go(h) }, el("span", { class: "gl" }, icon(h.table || h.m.database ? "table" : "file-text")), el("span", { class: "ml palb" }, name, small)));
    });
    if (!hits.length) {
      rows.push(el("div", { class: "hdr none", role: "presentation" }, ...tfNodes("Nothing matches {q}", input.value.trim())));
      rows.push(el("button", { class: "mirow", role: "option", id: "palo0", tabindex: "-1", "data-create": "", onmousedown: e => e.preventDefault(), onclick: create }, el("span", { class: "gl" }, icon("plus")), el("span", { class: "ml" }, ...tfNodes("Create page {q}", input.value.trim()))));
    }
    list.replaceChildren(...rows);
    paintOn();
  };
  const create = () => { const title = input.value.trim(); closeSearch(false); newPage(null, title); const n = page.blocks.find(hasText); if (n && !isCoarse()) setCaret(textOf(n.id), 0); };
  const go = h => {
    closeSearch(false); openPage(h.m.id);
    if (h.b) {
      const row = $(`.b[data-id="${CSS.escape(h.b)}"]`), n = textOf(h.b);
      row?.scrollIntoView({ block: "center" });
      const m = n && matchRange(n.textContent, query);
      if (n && !isCoarse()) { if (m) setRange(n, m[0], m[1]); else setCaret(n); }
    } else focusEditor();
  };
  input.addEventListener("input", () => { on = 0; paint(); });
  input.addEventListener("keydown", e => {
    if (e.isComposing) return;
    const n = $$(".mi,.mirow", list).length;
    if (e.key === "ArrowDown") { e.preventDefault(); on = n ? (on + 1) % n : 0; paintOn(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); on = n ? (on + n - 1) % n : 0; paintOn(); }
    else if (e.key === "Home" && !input.value) { e.preventDefault(); on = 0; paintOn(); }
    else if (e.key === "End" && !input.value) { e.preventDefault(); on = Math.max(0, n - 1); paintOn(); }
    else if (e.key === "Enter") { e.preventDefault(); if (hits[on]) go(hits[on]); else if (!hits.length && input.value.trim()) create(); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeSearch(true); }
    else if (e.key === "Tab") { e.preventDefault(); } // The dialog has one control. Nothing behind it should take focus.
  });
  document.body.append(el("div", { class: "modalbg", onpointerdown: e => { if (e.target.classList.contains("modalbg")) closeSearch(true); } }, el("div", { class: "modal search", role: "dialog", "aria-modal": "true", "aria-label": t("Search") }, input, list)));
  paint(); input.focus();
}
/** Interface text with a person's words inside it, kept in their own element so their direction cannot rearrange the sentence. */
function tfNodes(key, value) {
  const [a, b = ""] = t(key).split(/\{q\}/);
  return [a, el("bdi", { text: value }), b].filter(x => x !== "");
}
function closeSearch(restore = true) {
  const input = $("#palette"); if (!input) return;
  input.closest(".modalbg").remove();
  if (restore && searchReturn && !restoreCaret(searchReturn.caret) && searchReturn.focus?.isConnected) searchReturn.focus.focus();
  searchReturn = null;
}

/* ---------- Saving, told truthfully ---------- */
let saveState = "saved", savedAt = null, bannerKind = null;
const SAVE_WORDS = { saved: "Saved on this device", saving: "Saving", failed: "Not saved. Export your notes.", session: "Not saved on this device" };
function paintSaved() {
  const n = $("#saved"); if (!n) return;
  n.textContent = t(SAVE_WORDS[saveState] || SAVE_WORDS.saved);
  n.classList.toggle("bad", saveState === "failed" || saveState === "session");
  if (savedAt && saveState === "saved") { n.dataset.tip = tf("Last saved at {time}", { time: new Intl.DateTimeFormat(lang, { timeStyle: "medium" }).format(savedAt) }); } else delete n.dataset.tip;
}
function onSaveState(s) {
  const was = saveState;
  if (bannerKind === "failed" && s === "saving") return; // Failed stays said until a save has really worked.
  if (bannerKind && bannerKind !== "failed") return; // This session has no storage. Nothing here can change that.
  saveState = s;
  if (s === "saved") { savedAt = new Date(); if (bannerKind === "failed") setBanner(null); }
  if (s === "failed") setBanner("failed");
  if (was !== saveState || s === "saved") paintSaved();
}
const BANNERS = {
  failed: "Your latest changes could not be saved. Keep this tab open and export your notes.",
  unavailable: "This browser cannot keep your notes. You can work in this session, and export before you close it.",
  unreadable: "Your saved notes could not be read, and nothing has been changed. This session starts empty and is not saved.",
};
function setBanner(kind) { bannerKind = kind; paintBanner(); }
function paintBanner() {
  const b = $("#banner"); if (!b) return;
  b.hidden = !bannerKind;
  if (!bannerKind) { b.replaceChildren(); return; }
  b.replaceChildren(icon("warning"), el("span", { class: "msg", text: t(BANNERS[bannerKind]) }),
    el("button", { class: "btn primary", id: "exportBtn", onclick: exportNotes }, icon("download-simple"), el("span", { text: t("Export notes") })));
}
function memoryStore() {
  return { load: async () => null, flush: async () => {}, changed() {} };
}
/** Every page as Markdown, the walk-away format: readable with nothing but a text editor. */
function blockMarkdown(b, n) {
  const text = plain(b);
  switch (b.kind) {
    case "heading1": return "# " + text;
    case "heading2": return "## " + text;
    case "heading3": return "### " + text;
    case "bulleted": return "- " + text;
    case "numbered": return n + ". " + text;
    case "todo": return `- [${b.checked ? "x" : " "}] ` + text;
    case "quote": return "> " + text;
    case "code": return "```\n" + text + "\n```";
    case "divider": return "---";
    default: return text;
  }
}
function workspaceMarkdown() {
  const out = [];
  for (const m of live()) {
    out.push("# " + (m.title || t("Untitled")), "");
    let n = 0;
    for (const b of pageData(m.id).blocks) { n = b.kind === "numbered" ? n + 1 : 0; if (plain(b) || b.kind === "divider") out.push(blockMarkdown(b, n), ""); }
    out.push("");
  }
  return out.join("\n");
}
function saveFile(name, text, type = "text/markdown") {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = el("a", { href: url, download: name });
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}
function exportNotes() {
  saveFile(`casualnote-${new Date().toISOString().slice(0, 10)}.md`, workspaceMarkdown());
  toast(t("Exported as Markdown"));
}

/* ---------- Focus regions, F6 ---------- */
function regions() {
  const list = [];
  if (!$("#side").classList.contains("closed")) list.push({ node: $("#side"), go: () => ($("#tree .row[tabindex='0']") || $("#searchBtn")).focus() });
  list.push({ node: $("#center"), go: () => { focusEditor(); if (!$("#center").contains(document.activeElement)) ($("#tabs .tl[tabindex='0']") || $("#favBtn"))?.focus(); } });
  if (state.right) list.push({ node: $("#right"), go: () => { const f = $("#right button:not([hidden]),#right [tabindex='0'],#right input,#right textarea"); (f || $("#rightBtn")).focus(); } });
  return list;
}
function cycleRegion(back) {
  const list = regions(), a = document.activeElement, at = list.findIndex(r => r.node.contains(a));
  list[(at + (back ? list.length - 1 : 1) + list.length) % list.length].go();
}

function wire() {
  $$("[data-i]").forEach(n => n.replaceWith(icon(n.dataset.i)));
  for (const item of ext.nav) $("#tree").before(el("button", { class: "nav", id: item.id, onclick: () => { item.run(); if (isPhone()) setSide(false); } }, icon(item.icon), el("span", { "data-t": item.label, text: t(item.label) })));
  // Ahead of the document, so that there is one palette with one behaviour however a feature opens it.
  addEventListener("click", e => { if (e.target.closest?.("#searchBtn")) { e.stopPropagation(); openSearch(); } }, true);
  addEventListener("keydown", e => { if (!e.isComposing && !composing && (e.metaKey || e.ctrlKey) && !e.altKey && keyName(e) === "k") { e.preventDefault(); e.stopPropagation(); openSearch(); } }, true);
  $("#newBtn").addEventListener("click", () => newPage(null));
  $("#tabAdd").addEventListener("click", () => newPage(null));
  $("#settingsBtn").addEventListener("click", () => openView("settings"));
  $("#trashBtn").addEventListener("click", () => openView("trash"));
  $("#menuBtn").addEventListener("click", () => setSide($("#side").classList.contains("closed"), true));
  $("#collapseBtn").addEventListener("click", () => setSide(false, true));
  $("#rightBtn").addEventListener("click", () => setRight(!state.right, true));
  $("#rightClose").addEventListener("click", () => setRight(false, true));
  $("#scrim").addEventListener("click", () => { if (isPhone() && !$("#side").classList.contains("closed")) setSide(false, true); else setRight(false, true); });
  $("#favBtn").addEventListener("click", () => { const m = metaOf(state.cur); edit({ op: "set_favorite", id: m.id, value: !m.favorite }); renderSide(); renderCrumbs(); });
  $("#moreBtn").addEventListener("click", e => { const m = metaOf(state.cur), rc = e.currentTarget.getBoundingClientRect(); menu(rc, pageItems(m, rc)); });
  $("#tabMore").addEventListener("click", e => {
    const rc = e.currentTarget.getBoundingClientRect(), all = live();
    menu(rc, state.tabs.map(id => ({ gl: "file-text", user: true, label: all.find(p => p.id === id)?.title || t("Untitled"), run: () => { openPage(id); focusEditor(); } })));
  });
  const fmt = $("#fmt");
  fmt.addEventListener("mousedown", e => e.preventDefault());
  fmt.addEventListener("click", e => { const btn = e.target.closest("button"); if (btn) toggleMark(btn.dataset.f); });
  const tabs = $("#tabs");
  tabs.addEventListener("keydown", tabKeys);
  tabs.addEventListener("wheel", e => { if (!e.deltaX && e.deltaY && tabs.scrollWidth > tabs.clientWidth) { e.preventDefault(); tabs.scrollLeft += e.deltaY * (document.documentElement.dir === "rtl" ? -1 : 1); } }, { passive: false });
  if ("ResizeObserver" in self) new ResizeObserver(paintTabOverflow).observe(tabs);
  $("#tree").addEventListener("keydown", treeKeys);
  $("#tree").addEventListener("focusin", e => { const r = e.target.closest(".row"); if (r && r.getAttribute("tabindex") !== "0") { $$("#tree .row").forEach(x => x.setAttribute("tabindex", x === r ? "0" : "-1")); } });
  wireSplit($("#sideSplit"), () => ui.sideW, v => { ui.sideW = v; }, SIDE, 1);
  wireSplit($("#rightSplit"), () => ui.rightW, v => { ui.rightW = v; }, RIGHT, -1);
  wireSelection(); wireTips();
  // The phone bar. It keeps the keyboard up, and sits on top of it.
  const mob = $("#mobar");
  mob.addEventListener("mousedown", e => e.preventDefault());
  const curBlock = () => { const a = $("#mobar")._block; return a && blockOf(a); };
  $("#moMenu").addEventListener("click", () => { const b = curBlock(); if (b) blockMenu(b, mob.getBoundingClientRect()); });
  $("#moAdd").addEventListener("click", () => { const b = curBlock(); if (b) addBlockAfter(b.id); });
  $("#moUndo").addEventListener("click", () => replay(undo, redo));
  $("#moRedo").addEventListener("click", () => replay(redo, undo));
  document.addEventListener("focusin", e => {
    const tn = e.target.closest?.(".blocks .t");
    if (tn) mob._block = tn.closest(".b").dataset.id;
    mob.hidden = !tn;
  });
  document.addEventListener("focusout", e => { if (e.target.closest?.(".blocks .t") && !e.relatedTarget?.closest?.("#mobar")) setTimeout(() => { if (!document.activeElement?.closest?.(".blocks .t")) mob.hidden = true; }, 0); });
  if (window.visualViewport) {
    const lift = () => { const v = visualViewport; mob.style.transform = `translateY(${-Math.max(0, innerHeight - v.height - v.offsetTop)}px)`; };
    visualViewport.addEventListener("resize", lift); visualViewport.addEventListener("scroll", lift);
  }
  addEventListener("resize", () => {
    if (isPhone()) { if (!wasPhone) $("#side").classList.add("closed"); }
    else if (wasPhone) $("#side").classList.toggle("closed", !ui.side);
    wasPhone = isPhone();
    paintChrome();
  });
  // A feature may open or close the panel itself. The backdrop and the rest of the shell must follow.
  new MutationObserver(() => { const open = !$("#right").hidden; if (open !== !!state.right) { state.right = open; saveUI(); } paintChrome(); }).observe($("#right"), { attributes: true, attributeFilter: ["hidden"] });
  addEventListener("beforeunload", e => { if (saveState === "failed" || saveState === "session") e.preventDefault(); });
  document.addEventListener("click", e => {
    const a = e.target.closest?.("a[data-link]"); if (!a) return;
    e.preventDefault();
    if (metaOf(a.dataset.link) && !metaOf(a.dataset.link).trashed) { openPage(a.dataset.link); focusEditor(); }
  });
  document.addEventListener("keydown", e => {
    if (e.isComposing || composing) return;
    const mod = e.metaKey || e.ctrlKey, k = keyName(e), code = e.code;
    const inField = e.target.matches?.("input:not([type=checkbox]),textarea,select");
    if (e.key === "F6") { e.preventDefault(); cycleRegion(e.shiftKey); return; }
    if (e.altKey && !mod && code === "KeyN" && !MAC && !e.getModifierState?.("AltGraph")) { e.preventDefault(); newPage(null); return; }
    if (e.altKey && mod) return; // AltGr on many layouts: it types characters.
    if (mod && code === "KeyN" && !e.shiftKey) { e.preventDefault(); newPage(null); }
    else if (mod && code === "Backslash") { e.preventDefault(); const typing = !!e.target.closest?.(".t,.title"); if (e.shiftKey) setRight(!state.right, !typing); else setSide($("#side").classList.contains("closed"), isPhone() || !typing); }
    else if (mod && code === "Comma") { e.preventDefault(); openView("settings"); $("#settingsBtn").focus(); }
    else if (mod && code === "Slash" && !e.shiftKey) { e.preventDefault(); openView("settings"); $("#shortcuts")?.scrollIntoView({ block: "start" }); }
    else if (mod && k === "s") { e.preventDefault(); toast(t(SAVE_WORDS[saveState])); }
    else if (mod && k === "z" && !(inField && !e.target.closest(".dbx"))) { e.preventDefault(); e.shiftKey ? replay(redo, undo) : replay(undo, redo); }
    else if (e.ctrlKey && k === "y" && !inField) { e.preventDefault(); replay(redo, undo); }
    else if (e.key === "Escape") {
      if (menuState) { closeMenu(true); return; }
      if ($("#palette")) { closeSearch(true); return; }
      if (isPhone() && !$("#side").classList.contains("closed")) { setSide(false, true); return; }
      if (isOverlayPanel() && state.right) { setRight(false, true); return; }
    }
  });
}
let wasPhone = false;

// Everything a feature may use. Features write only through `edit`, so undo and saving work.
const api = {
  ext, state, t, el, icon, uid, $, $$, edit, run, menu, closeMenu, plain, paintRuns, setCaret, selectionOffsets, caretOffset,
  pages, live, pageData, metaOf, openPage, renderAll, renderPage, renderSide, reload, textOf,
  blockOf, currentPage: () => page, newPage, openView,
  tf, toast, saveFile, focusEditor, keyHint,
  ui: { setSide, setRight, setTheme, isPhone, movePage, trashPage, duplicatePage, startRename, closeTab, selectAllBlocks, multiActive: () => multi, MOD: MAC ? { metaKey: true } : { ctrlKey: true }, data: ui, setBanner, showFatal, workspaceMarkdown, hideTip, refreshChrome: () => paintChrome() },
  get engine() { return engine; }, get store() { return store; }, get lang() { return lang; },
};
// Trash is a view like any feature's view.
ext.views.trash = { label: "Trash", render: renderTrash };

function showFatal(err) {
  const boot = $("#boot") || el("p", { class: "boot", id: "boot" });
  if (!boot.isConnected) $("#content").replaceChildren(boot);
  boot.classList.add("err");
  boot.setAttribute("role", "alert");
  boot.replaceChildren(icon("warning"), el("b", { text: t("CasualNote could not start.") }), el("span", { text: t("Your notes are untouched. Reloading usually fixes this.") }), el("small", { dir: "ltr", text: String(err?.message || err).slice(0, 200) }), el("button", { class: "btn primary", text: t("Reload"), onclick: () => location.reload() }));
  $("#right").hidden = true;
}

function firstCode() {
  let code = null; try { code = localStorage.getItem("casualnote-lang"); } catch { /* optional */ }
  return code || LANGS.find(l => (navigator.language || "en").toLowerCase().startsWith(l[0].slice(0, 2)))?.[0] || "en";
}
async function start() {
  // Before anything slow, so that the wait itself is in the right language and direction, with no shift.
  try { const theme = localStorage.getItem("casualnote-theme"); if (theme) document.documentElement.dataset.theme = theme; } catch { /* optional */ }
  loadUI();
  setLang(firstCode());
  $$("[data-i]").forEach(n => n.replaceWith(icon(n.dataset.i)));
  await init();
  const peer = crypto.getRandomValues(new BigUint64Array(1))[0] >> 1n;
  engine = new Engine(peer);
  const remote = bytes => {
    // Another tab of the same browser saved: merge it. Nothing touches a block during its composition.
    engine.import(bytes);
    if (composing) { renderSide(); return; }
    const held = captureCaret(); renderAll(); if (held) restoreCaret(held);
  };
  let storeReady = true;
  try {
    store = await openStore(() => engine.export(), onSaveState, remote);
  } catch (err) {
    console.warn("storage unavailable", err);
    storeReady = false; store = memoryStore(); saveState = "session"; setBanner("unavailable");
  }
  let saved = null;
  if (storeReady) {
    try { saved = await store.load(); if (saved) engine.import(saved); }
    catch (err) {
      // Never let a save overwrite notes that could not be read.
      console.warn("stored notes unreadable", err);
      saved = null; store = memoryStore(); saveState = "session"; setBanner("unreadable");
    }
  }
  await loadFeatures(api);
  wire();
  wasPhone = isPhone();
  setLang(lang);
  if (uiSaved && typeof uiSaved.right === "boolean") state.right = uiSaved.right;
  if (isOverlayPanel()) state.right = false;
  $("#side").classList.toggle("closed", isPhone() || !ui.side);
  const known = new Set(pages().filter(p => !p.trashed).map(p => p.id));
  if (uiSaved && saved) {
    if (Array.isArray(uiSaved.tabs)) state.tabs = uiSaved.tabs.filter(id => known.has(id));
    if (known.has(uiSaved.cur)) state.cur = uiSaved.cur;
    if (uiSaved.open && typeof uiSaved.open === "object") state.open = Object.fromEntries(Object.entries(uiSaved.open).filter(([, v]) => v === true));
  }
  const firstRun = !known.size;
  renderAll();
  $$("[inert]").forEach(n => { if (n.id !== "center" && !(n.id === "side" && isPhone())) n.inert = false; });
  paintChrome();
  paintSaved();
  if (firstRun) $(".doc .title")?.focus();
  else focusEditor();
  if (new URLSearchParams(location.search).has("demo")) (await import("./tests/demo.js")).seedDemo(api);
  if (new URLSearchParams(location.search).has("e2e")) {
    const { runE2E } = await import("./tests/e2e.js");
    runE2E({ api, engine, store, state, undoStack: undo, replay, redoStack: redo, pageNow: () => page, openPage, setLang, renderAll, $, $$, setCaret, textOf });
  }
}
start().catch(err => { console.error(err); showFatal(err); });
