// Page links, backlinks, tags and embeds. Document strings only reach the DOM through textContent.
let api;

const CUSTOM = new Set(["database", "embed"]);
const MAX_EMBED_DEPTH = 4;
const PICKER_LIMIT = 8;
const isolate = s => `⁨${s}⁩`;
const norm = s => (s || "").toLocaleLowerCase(api.lang).normalize("NFC");
const titleOf = m => m.title || api.t("Untitled");
const blank = () => ({ id: api.uid(), kind: "paragraph", checked: false, runs: [] });

/* ---------- Shared with database.js: blocks that are not text ---------- */

// app.js moves the caret into `.b > .t`, so a custom block carries an empty one. It gives the block
// a keyboard stop: arrows pass through, Enter adds a line below, Backspace removes the block.
export function customBlock(a, box, block, cls) {
  const stop = a.el("div", { class: "t bfocus", tabindex: "-1", "aria-label": a.t(cls === "db" ? "Database" : "Embed page") });
  stop.addEventListener("keydown", e => blockStopKeys(a, e, block.id));
  queueMicrotask(() => { const row = stop.parentElement; if (row) { row.classList.remove("undefined"); row.classList.add(cls); } });
  const frag = document.createDocumentFragment();
  frag.append(stop, box);
  return frag;
}
function blockStopKeys(a, e, id) {
  if (e.target !== e.currentTarget) return;
  const page = a.currentPage(), pid = page.meta.id, i = page.blocks.findIndex(x => x.id === id);
  if (i < 0) return;
  const focusBlock = (b, atEnd) => { const n = b && a.textOf(b.id); if (n) a.setCaret(n, atEnd ? Infinity : 0); };
  const stepTo = list => list.find(x => x.kind !== "divider");
  if (e.key === "ArrowUp") { e.preventDefault(); focusBlock(stepTo(page.blocks.slice(0, i).reverse()), true); }
  else if (e.key === "ArrowDown") { e.preventDefault(); focusBlock(stepTo(page.blocks.slice(i + 1)), false); }
  else if (e.key === "Enter") {
    e.preventDefault();
    const next = blank();
    a.edit({ op: "insert_block", page: pid, after: id, block: next });
    a.renderPage(); a.setCaret(a.textOf(next.id), 0);
  } else if (e.key === "Backspace" || e.key === "Delete") {
    e.preventDefault();
    const ops = [{ op: "delete_block", page: pid, block: id }];
    let target = stepTo(page.blocks.slice(0, i).reverse()) || stepTo(page.blocks.slice(i + 1));
    if (page.blocks.length === 1) { target = blank(); ops.push({ op: "insert_block", page: pid, after: null, block: target }); }
    a.edit(...ops); a.renderPage(); focusBlock(target, true);
  }
}
// Puts a custom block after `b`. An empty paragraph is replaced, and text always follows the block.
export function placeBlock(a, b, block, before = []) {
  a.reload();
  const page = a.currentPage(), pid = page.meta.id, i = page.blocks.findIndex(x => x.id === b.id);
  const cur = page.blocks[i], next = page.blocks[i + 1];
  const ops = [...before, { op: "insert_block", page: pid, after: b.id, block }];
  const tail = !next || next.kind === "divider" || CUSTOM.has(next.kind) ? blank() : null;
  if (tail) ops.push({ op: "insert_block", page: pid, after: block.id, block: tail });
  if (cur && cur.kind === "paragraph" && !a.plain(cur)) ops.push({ op: "delete_block", page: pid, block: b.id });
  a.edit(...ops);
  return tail ? tail.id : next.id;
}
// api.menu with keyboard control, for menus opened away from a text block.
export function keyMenu(a, anchor, items, opts = {}) {
  a.menu(anchor.getBoundingClientRect(), items, opts);
  const pop = a.$$(".pop").at(-1); if (!pop) return;
  const buttons = () => [...pop.querySelectorAll(".mi")];
  buttons()[0]?.focus();
  pop.addEventListener("keydown", e => {
    const list = buttons(), at = list.indexOf(document.activeElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      list[(at + (e.key === "ArrowDown" ? 1 : list.length - 1)) % list.length]?.focus();
    } else if (e.key === "Home") { e.preventDefault(); list[0]?.focus(); }
    else if (e.key === "End") { e.preventDefault(); list.at(-1)?.focus(); }
    else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault(); e.stopPropagation();
      a.closeMenu(); anchor.focus();
    }
  });
}

/* ---------- The [[ picker ---------- */
let picker = null, dismissed = null;

function linkHook(b, node, before) {
  const m = before.match(/(?:\[\[|【【|［［)([^\[\]【】［］\n]*)$/u);
  if (!m || m[1].length > 80) { picker = null; dismissed = null; return false; }
  const at = before.length - m[0].length;
  if (dismissed && dismissed.block === b.id && dismissed.at === at) return false;
  dismissed = null;
  picker = { block: b.id, at };
  openPicker(b, node, m[1], at, before.length);
  return true;
}
function matches(q, exclude) {
  const ql = norm(q.trim());
  const hits = api.live().filter(m => m.id !== exclude && norm(titleOf(m)).includes(ql));
  const rank = m => (norm(titleOf(m)).startsWith(ql) ? 0 : 1);
  return hits.sort((x, y) => rank(x) - rank(y)).slice(0, PICKER_LIMIT);
}
function openPicker(b, node, q, at, caret) {
  const self = api.currentPage().meta.id, query = q.trim();
  const items = matches(q, self).map(m => ({ gl: m.database ? "table" : "file-text", label: titleOf(m), run: () => insertLink(b, at, caret, titleOf(m), m.id) }));
  if (query && !api.live().some(m => norm(m.title) === norm(query))) {
    items.push({ gl: "plus", label: api.t("Create page: {title}").replace("{title}", isolate(query)), run: () => {
      const id = api.uid();
      insertLink(b, at, caret, query, id, [{ op: "create_page", id, parent: null, title: query }, { op: "insert_block", page: id, after: null, block: blank() }]);
      api.renderSide();
    } });
  }
  const sel = getSelection();
  const rect = (sel.rangeCount && sel.getRangeAt(0).getClientRects()[0]) || node.getBoundingClientRect();
  api.menu(rect, items, { keys: true, hdr: api.t("Link to page") });
}
function insertLink(b, at, caret, title, target, before = []) {
  const pid = api.currentPage().meta.id;
  picker = null;
  api.edit(...before,
    { op: "splice_text", page: pid, block: b.id, at, delete: caret - at, insert: title },
    { op: "set_link", page: pid, block: b.id, start: at, end: at + title.length, target });
  api.renderPage();
  api.setCaret(api.textOf(b.id), at + title.length);
}
// The browser keeps typing inside an <a> when the caret sits at its end, but the engine does not
// extend a link. Repaint so what the page shows is what is stored.
function keepLinksTrue(b, node) {
  const shown = [...node.querySelectorAll("a[data-link]")].map(a => a.textContent).join("\0");
  const stored = b.runs.filter(r => r.link).map(r => r.text).join("\0");
  if (shown !== stored) { const at = api.caretOffset(node); api.paintRuns(node, b.runs); markGone(); api.setCaret(node, at); }
  return false;
}

/* ---------- Removed links ---------- */
function markGone(root = document) {
  const links = [...root.querySelectorAll("a[data-link]")];
  if (!links.length) return;
  const alive = new Set(api.live().map(m => m.id));
  for (const a of links) {
    const gone = !alive.has(a.dataset.link);
    a.classList.toggle("gone", gone);
    if (gone) a.title = api.t("This page was removed");
    else a.removeAttribute("title");
  }
}
function watchLinks() {
  new MutationObserver(records => {
    const found = records.some(r => [...r.addedNodes].some(n => n.nodeType === 1 && (n.matches("a[data-link]") || n.querySelector("a[data-link]"))));
    if (found) markGone(api.$("#content"));
  }).observe(api.$("#content"), { childList: true, subtree: true });
}

/* ---------- Index: backlinks and tags ---------- */
function scan() {
  const links = new Map(), tags = new Map();
  const note = (target, entry) => { if (!links.has(target)) links.set(target, []); links.get(target).push(entry); };
  for (const m of api.live()) {
    for (const b of api.pageData(m.id).blocks) {
      if (b.kind === "embed" && b.target) note(b.target, { from: m, block: b.id, line: api.t("Embeds this page") });
      const text = api.plain(b);
      for (const target of new Set(b.runs.filter(r => r.link).map(r => r.link))) note(target, { from: m, block: b.id, line: text });
      if (b.kind === "code") continue;
      for (const hit of text.matchAll(/(?:^|[\s(])#([\p{L}\p{N}_][\p{L}\p{N}_/-]*)/gu)) {
        if (/\p{L}/u.test(hit[1])) tags.set(hit[1], (tags.get(hit[1]) || 0) + 1);
      }
    }
  }
  return { links, tags };
}

/* ---------- Backlinks panel ---------- */
let shownFor = null, shownSig = null, panelTimer = null;

function backlinksPanel(page) {
  const id = page.meta.id;
  clearTimeout(panelTimer);
  if (id !== shownFor) paintBacklinks(id);
  else panelTimer = setTimeout(() => paintBacklinks(api.state.cur), 200);
}
function paintBacklinks(id) {
  let sec = api.$("#rpBacklinks");
  if (!sec) {
    sec = api.el("div", { class: "rp", id: "rpBacklinks" }, api.el("h4", { "data-t": "Backlinks", text: api.t("Backlinks") }), api.el("div", { class: "bllist" }));
    api.$("#rpDetails").append(sec);
  }
  const list = (scan().links.get(id) || []).filter(x => x.from.id !== id);
  const sig = JSON.stringify([id, api.lang, list.map(x => [x.from.id, x.from.title, x.block, x.line])]);
  shownFor = id;
  if (sig === shownSig) return;
  shownSig = sig;
  const box = sec.querySelector(".bllist");
  if (!list.length) { box.replaceChildren(api.el("p", { text: api.t("No pages link here yet.") })); return; }
  box.replaceChildren(...list.map(x => api.el("button", { class: "bl", onclick: () => {
    api.openPage(x.from.id);
    document.querySelector(`.b[data-id="${CSS.escape(x.block)}"]`)?.scrollIntoView({ block: "center" });
  } }, api.el("span", { dir: "auto", text: titleOf(x.from) }), api.el("small", { dir: "auto", text: x.line.slice(0, 200) }))));
}

/* ---------- Tags in the sidebar ---------- */
let tagTimer = null, tagSig = null;

function scheduleTags(delay = 400) { clearTimeout(tagTimer); tagTimer = setTimeout(paintTags, delay); }
function paintTags() {
  const tags = [...scan().tags].sort((x, y) => x[0].localeCompare(y[0], api.lang));
  const sig = JSON.stringify([api.lang, tags]);
  if (sig === tagSig) return;
  tagSig = sig;
  let box = api.$("#tagList");
  if (!tags.length) { box?.remove(); return; }
  if (!box) { box = api.el("div", { id: "tagList" }); api.$("#tree").after(box); }
  box.replaceChildren(api.el("div", { class: "sec" }, api.el("span", { "data-t": "Tags", text: api.t("Tags") })),
    api.el("div", { class: "taglist" }, ...tags.map(([tag, n]) => api.el("div", { class: "row" },
      api.el("button", { class: "open", onclick: () => searchTag(tag) }, api.el("span", { dir: "auto", text: "#" + tag })),
      api.el("small", { class: "tagn", text: new Intl.NumberFormat(api.lang).format(n) })))));
}
function searchTag(tag) {
  api.$("#searchBtn").click();
  const input = api.$("#palette"); if (!input) return;
  input.value = "#" + tag;
  input.dispatchEvent(new Event("input"));
}

/* ---------- Embeds ---------- */
function reaches(from, goal, seen = new Set()) {
  if (from === goal) return true;
  if (seen.has(from)) return false;
  seen.add(from);
  let page; try { page = api.pageData(from); } catch { return false; }
  return page.blocks.some(b => b.kind === "embed" && b.target && reaches(b.target, goal, seen));
}
function embedRefusal(host, m) {
  if (m.id === host) return api.t("This page");
  return reaches(m.id, host) ? api.t("Embeds this page") : null;
}
function embedPicker(b) {
  api.closeMenu();
  api.renderPage(); // app.js has removed the slash text from the engine, not yet from the page.
  const host = api.currentPage().meta.id;
  const back = () => { const n = api.textOf(b.id); if (n) api.setCaret(n); };
  const input = api.el("input", { id: "embedPick", type: "search", placeholder: api.t("Embed which page?"), "aria-label": api.t("Embed which page?") });
  const list = api.el("div", { class: "list", role: "listbox" });
  let on = 0, rows = [];
  const close = () => { bg.remove(); };
  const choose = row => {
    if (!row || row.refused) return;
    close();
    const id = api.uid();
    const next = placeBlock(api, b, { id, kind: "embed", checked: false, runs: [], target: row.m.id });
    api.renderPage();
    const n = api.textOf(next); if (n) api.setCaret(n, 0);
  };
  const paint = () => {
    const q = norm(input.value.trim());
    rows = api.live().filter(m => norm(titleOf(m)).includes(q)).map(m => ({ m, refused: embedRefusal(host, m) }));
    rows.sort((x, y) => (x.refused ? 1 : 0) - (y.refused ? 1 : 0));
    const usable = rows.filter(r => !r.refused).length;
    on = Math.min(on, Math.max(0, usable - 1));
    list.replaceChildren(...rows.map((r, i) => api.el("button", {
      class: "mi" + (i === on && !r.refused ? " on" : "") + (r.refused ? " refused" : ""),
      role: "option", "aria-selected": String(i === on && !r.refused), "aria-disabled": r.refused ? "true" : null,
      onclick: () => choose(r),
    }, api.el("span", { class: "gl" }, api.icon(r.m.database ? "table" : "file-text")),
      api.el("span", { dir: "auto" }, titleOf(r.m), r.refused ? api.el("small", { text: r.refused }) : null))));
    if (!rows.length) list.append(api.el("div", { class: "hdr", text: api.t("No matches") }));
    else if (!usable) list.prepend(api.el("div", { class: "hdr", text: api.t("No other page to embed yet. Create one first.") }));
    list.querySelector(".mi.on")?.scrollIntoView({ block: "nearest" });
  };
  input.addEventListener("input", () => { on = 0; paint(); });
  input.addEventListener("keydown", e => {
    if (e.isComposing) return;
    const usable = rows.filter(r => !r.refused).length;
    if (e.key === "ArrowDown") { e.preventDefault(); if (usable) { on = (on + 1) % usable; paint(); } }
    else if (e.key === "ArrowUp") { e.preventDefault(); if (usable) { on = (on + usable - 1) % usable; paint(); } }
    else if (e.key === "Enter") { e.preventDefault(); choose(rows[on]); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); back(); }
  });
  const bg = api.el("div", { class: "modalbg", onpointerdown: e => { if (e.target === bg) { close(); back(); } } },
    api.el("div", { class: "modal", role: "dialog", "aria-label": api.t("Embed which page?") }, input, list));
  document.body.append(bg);
  paint(); input.focus();
}
function drawEmbed(b) {
  const box = api.el("div", { class: "embedbox", "data-target": b.target || "" });
  fillEmbed(box, b.target, [api.currentPage().meta.id]);
  return box;
}
function embedNotice(text) { return api.el("p", { class: "embedmsg", text }); }
function fillEmbed(box, target, trail) {
  const meta = target ? api.metaOf(target) : null;
  const here = meta && !meta.trashed;
  const head = here
    ? api.el("button", { class: "embedopen", "aria-label": api.t("Open page") + ": " + titleOf(meta), onclick: () => api.openPage(meta.id) }, api.icon(meta.database ? "table" : "file-text"), api.el("span", { dir: "auto", text: titleOf(meta) }))
    : api.el("span", { class: "ref gone", dir: "auto", text: meta ? titleOf(meta) : api.t("Removed page") });
  box.replaceChildren(api.el("header", {}, head, here ? api.el("span", { text: api.t("Read only") }) : null));
  box.classList.toggle("broken", !here);
  if (!meta) { box.append(embedNotice(api.t("This page no longer exists. Delete this block or undo."))); return; }
  if (meta.trashed) { box.append(embedNotice(api.t("This page is in the Trash. Restore it to show it here."))); return; }
  if (target === trail[0]) { box.append(embedNotice(api.t("A page cannot embed itself."))); return; }
  if (trail.includes(target)) { box.append(embedNotice(api.t("This embed loops back to a page above, so it stops here."))); return; }
  if (trail.length > MAX_EMBED_DEPTH) { box.append(embedNotice(api.t("Embedded too deep to show here. Open the page to read it."))); return; }
  const next = [...trail, target];
  if (meta.database) { box.append(databaseLines(meta.database)); return; }
  const blocks = api.pageData(target).blocks.filter(b => b.kind === "divider" || CUSTOM.has(b.kind) || api.plain(b));
  if (!blocks.length) { box.append(embedNotice(api.t("This page is empty."))); return; }
  let n = 0;
  for (const b of blocks) { n = b.kind === "numbered" ? n + 1 : 0; box.append(embedLine(b, next, n)); }
}
function databaseLines(id) {
  const wrap = api.el("div", { class: "embeddb" });
  let db; try { db = JSON.parse(api.engine.database(id)); } catch { wrap.append(embedNotice(api.t("This database is not on this device yet."))); return wrap; }
  if (db.name) wrap.append(api.el("div", { class: "line h", dir: "auto", text: db.name }));
  if (!db.rows.length) wrap.append(embedNotice(api.t("No rows yet.")));
  for (const r of db.rows.slice(0, 50)) {
    const status = db.statuses.find(s => s.id === r.status);
    wrap.append(api.el("div", { class: "line dbline" }, api.el("span", { dir: "auto", text: r.name || api.t("Untitled") }), status ? api.el("small", { dir: "auto", text: status.name }) : null));
  }
  return wrap;
}
function embedLine(b, trail, n) {
  if (b.kind === "divider") return api.el("hr", { class: "embedhr" });
  if (b.kind === "embed") {
    const nested = api.el("div", { class: "embedbox nested", "data-target": b.target || "" });
    fillEmbed(nested, b.target, trail);
    return nested;
  }
  if (b.kind === "database") return b.target ? databaseLines(b.target) : api.el("div");
  const line = api.el("div", { class: `line k-${b.kind}${/^heading/.test(b.kind) ? " h" : ""}${b.checked ? " done" : ""}` });
  if (b.kind === "bulleted") line.append(api.el("span", { class: "mk", text: "•" }));
  if (b.kind === "numbered") line.append(api.el("span", { class: "mk", text: new Intl.NumberFormat(api.lang).format(n) + "." }));
  if (b.kind === "todo") line.append(api.el("input", { type: "checkbox", disabled: true, tabindex: "-1", "aria-label": api.t("To-do"), ...(b.checked ? { checked: true } : {}) }));
  const text = api.el("span", { class: "lt", dir: "auto" });
  api.paintRuns(text, b.runs);
  line.append(text);
  return line;
}
let embedTimer = null;
function refreshEmbeds() {
  clearTimeout(embedTimer);
  embedTimer = setTimeout(() => {
    const page = api.currentPage(); if (!page) return;
    for (const box of api.$$(".blocks .embedbox:not(.nested)")) fillEmbed(box, box.dataset.target || null, [page.meta.id]);
    markGone(api.$("#content"));
  }, 80);
}

/* ---------- Install ---------- */
export function install(a) {
  api = a;
  const { ext } = api;
  ext.textHooks.push(keepLinksTrue, linkHook);
  ext.blocks.embed = b => customBlock(api, drawEmbed(b), b, "embed");
  ext.slash.push({ gl: "frame-corners", label: "Embed page", run: embedPicker });
  ext.panels.push(backlinksPanel);
  ext.onChange.push(op => {
    if (op.op === "set_trashed" || op.op === "create_page" || op.op === "set_title") queueMicrotask(() => markGone(api.$("#content")));
    scheduleTags();
    // Typing on the page that shows the embed cannot change what the embed shows.
    if (api.$(".blocks .embedbox") && !(op.op === "splice_text" && op.page === api.state.cur)) refreshEmbeds();
  });
  // Escape or a click away dismisses the picker: it stays shut until a new [[ is typed.
  document.addEventListener("keydown", e => { if (e.key === "Escape" && picker && api.$(".pop")) { dismissed = picker; picker = null; } }, true);
  document.addEventListener("pointerdown", e => { if (picker && api.$(".pop") && !e.target.closest(".pop")) { dismissed = picker; picker = null; } }, true);
  // app.js would merge this text into the block above, where a custom block would hide it.
  document.addEventListener("keydown", e => {
    if (e.key !== "Backspace" || e.isComposing) return;
    const node = e.target.closest?.(".b > .t"); if (!node || node.classList.contains("bfocus")) return;
    const row = node.parentElement, page = api.currentPage(), i = page?.blocks.findIndex(x => x.id === row.dataset.id);
    if (i == null || i < 1 || !getSelection().isCollapsed || api.caretOffset(node) !== 0) return;
    const cur = page.blocks[i], prev = page.blocks[i - 1];
    if (cur.kind !== "paragraph" || !CUSTOM.has(prev.kind)) return;
    e.preventDefault(); e.stopPropagation();
    if (!node.textContent) { api.edit({ op: "delete_block", page: page.meta.id, block: cur.id }); api.renderPage(); }
    api.textOf(prev.id)?.focus();
  }, true);
  watchLinks();
  setTimeout(() => paintTags(), 0);
}
