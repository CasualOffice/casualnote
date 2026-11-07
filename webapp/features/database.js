// Databases: a block or a full page, with a table and a board. Every write goes through api.edit.
// Row, column and status names only reach the DOM through textContent or an input's value.
import { customBlock, keyMenu, placeBlock } from "./links.js";

let api;
let rendering = false;
let selfWrite = false;

const GROUP_ICON = { todo: "circle-dashed", active: "circle-half", review: "eye", done: "check-circle" };
const COLUMN_TYPES = [["text", "Text"], ["number", "Number"], ["date", "Date"]];

const read = id => { try { return JSON.parse(api.engine.database(id)); } catch { return null; } };
const count = n => new Intl.NumberFormat(api.lang).format(n);
const statusOf = (db, row) => db.statuses.find(s => s.id === row.status) || db.statuses[0];
const roots = id => api.$$(`.db[data-db="${CSS.escape(id)}"]`);
const keySel = k => `[data-k="${CSS.escape(k)}"]`;

function blankDatabase(id, name = "") {
  const t = api.t;
  return {
    id, name, view: "table",
    statuses: [
      { id: api.uid(), name: t("To do"), group: "todo" },
      { id: api.uid(), name: t("In progress"), group: "active" },
      { id: api.uid(), name: t("Done"), group: "done" },
    ],
    columns: [{ id: api.uid(), name: t("Date"), kind: "date", hidden: false }],
    rows: [],
  };
}

/* ---------- Writing ---------- */
// Runs edits as one undo step. A failure shows in words beside the database it came from.
function write(id, from, ...ops) {
  selfWrite = true;
  try {
    api.edit(...ops);
    setMessage(from, "");
    // Other copies of this database on screen. The caller repaints its own.
    for (const r of roots(id)) if (r !== from && !r.contains(document.activeElement)) redraw(r);
    return true;
  } catch (err) {
    console.warn("database write refused", err);
    setMessage(from, api.t("That change could not be saved. Try again, or undo the last step."));
    return false;
  } finally { selfWrite = false; }
}
function setMessage(root, text) {
  const box = root?.querySelector?.(":scope > .dbmsg"); if (!box) return;
  box.hidden = !text;
  box.replaceChildren(...(text ? [api.icon("x"), api.el("span", { text })] : []));
}
function redraw(root, focusKey) {
  const fresh = drawDb(root.dataset.db, root.dataset.full === "1");
  rendering = true;
  try { root.replaceWith(fresh); } finally { rendering = false; }
  if (focusKey) fresh.querySelector(keySel(focusKey))?.focus();
  return fresh;
}

/* ---------- Editable fields ---------- */
// Enter commits, Escape cancels, leaving commits. Pending text is never lost to a repaint.
function field({ root, value = "", key, type = "text", label, placeholder, cls, commit, validate, onEnter }) {
  const input = api.el("input", { type, class: cls || "", "aria-label": label, "data-k": key, placeholder, dir: type === "text" ? "auto" : null, inputmode: type === "number" ? "decimal" : null, spellcheck: "false" });
  if (type === "number") input.type = "text";
  input.value = value;
  let saved = value;
  const paintEmpty = () => { if (type === "date") input.toggleAttribute("data-empty", !input.value); };
  const save = () => {
    const v = input.value;
    if (v === saved) { clearError(); return true; }
    const problem = validate?.(v);
    if (problem) { input.setAttribute("aria-invalid", "true"); setMessage(root(), problem); return false; }
    clearError();
    if (commit(v) === false) { input.value = saved; paintEmpty(); return false; }
    saved = v; paintEmpty();
    return true;
  };
  const clearError = () => { if (input.hasAttribute("aria-invalid")) { input.removeAttribute("aria-invalid"); setMessage(root(), ""); } };
  input.addEventListener("blur", () => { if (!rendering) save(); });
  if (type === "date") input.addEventListener("change", save);
  input.addEventListener("keydown", e => {
    if (e.isComposing || e.keyCode === 229) return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === "Enter") { e.preventDefault(); if (save()) onEnter?.(input, e); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); input.value = saved; paintEmpty(); clearError(); input.select(); }
    else if (mod && e.key.toLowerCase() === "z" && input.value !== saved) e.stopPropagation(); // The browser undoes uncommitted typing.
  });
  paintEmpty();
  return input;
}
const isNumber = v => !v.trim() || Number.isFinite(Number(v.replace(",", ".")));

/* ---------- Status ---------- */
function statusMark(status) {
  const group = status?.group || "todo";
  return api.el("span", { class: "st" + (group === "done" ? " done" : "") }, api.icon(GROUP_ICON[group] || GROUP_ICON.todo), api.el("span", { dir: "auto", text: status?.name || api.t("No status") }));
}
function statusButton(db, row, getRoot) {
  const key = `status:${row.id}`;
  return api.el("button", { class: "stbtn", "data-k": key, "aria-haspopup": "menu", "aria-label": api.t("Change status") + ": " + (statusOf(db, row)?.name || ""),
    onclick: e => keyMenu(api, e.currentTarget, db.statuses.map(s => ({ gl: GROUP_ICON[s.group] || GROUP_ICON.todo, label: s.name, run: () => {
      const root = getRoot();
      if (s.id !== row.status && write(db.id, root, { op: "set_row_field", id: db.id, row: row.id, key: "status", value: s.id })) redraw(root, key);
      else root.querySelector(keySel(key))?.focus();
    } }))),
  }, statusMark(statusOf(db, row)));
}

/* ---------- Head ---------- */
function drawHead(db, root, full) {
  const head = api.el("div", { class: "dbhead" });
  if (!full) head.append(field({ root: () => root, value: db.name, key: "dbname", cls: "dbname", label: api.t("Database name"), placeholder: api.t("Untitled"),
    commit: v => write(db.id, root, { op: "set_database_field", id: db.id, key: "name", value: v }),
    onEnter: () => root.querySelector("tbody input, .card, [data-k=addrow]")?.focus() }));
  const tab = (view, gl, label) => api.el("button", { "aria-pressed": String(db.view === view), "aria-label": api.t(label), "data-k": `view:${view}`, onclick: () => {
    if (db.view === view) return;
    if (write(db.id, root, { op: "set_database_field", id: db.id, key: "view", value: view })) redraw(root, `view:${view}`);
  } }, api.icon(gl), api.el("span", { text: api.t(label) }));
  head.append(api.el("div", { class: "vtabs", role: "group", "aria-label": api.t("View") }, tab("table", "table", "Table"), tab("board", "kanban", "Board")));
  const hidden = db.columns.filter(c => c.hidden);
  if (hidden.length && db.view === "table") head.append(api.el("button", { class: "hiddenbtn", "data-k": "hidden", "aria-haspopup": "menu", onclick: e => {
    const restore = cols => { if (write(db.id, root, ...cols.map(c => ({ op: "set_column_hidden", id: db.id, column: c.id, hidden: false })))) redraw(root, `col:${cols[0].id}`); };
    keyMenu(api, e.currentTarget, [...hidden.map(c => ({ gl: "eye", label: c.name || api.t("Untitled"), run: () => restore([c]) })),
      ...(hidden.length > 1 ? [{ gl: "plus", label: api.t("Show all columns"), run: () => restore(hidden) }] : [])], { hdr: api.t("Hidden columns") });
  } }, api.icon("eye"), api.el("span", { text: api.t("{n} hidden").replace("{n}", count(hidden.length)) })));
  head.append(api.el("button", { class: "newbtn", "data-k": "new", onclick: () => {
    if (db.view === "board") { root.querySelector(keySel(`add:${db.statuses[0]?.id}`))?.click(); return; }
    addRow(db, root);
  } }, api.el("span", { text: api.t("New") })));
  return head;
}

/* ---------- Table ---------- */
function addRow(db, root) {
  const id = api.uid(), last = db.rows.at(-1);
  if (write(db.id, root, { op: "add_row", id: db.id, after: last ? last.id : null, row: { id, name: "", status: db.statuses[0]?.id || "", values: {} } })) redraw(root, `name:${id}`);
}
function moveRow(input, step) {
  const cell = input.closest("td"), row = cell?.parentElement; if (!row) return;
  const at = [...row.children].indexOf(cell);
  let target = step > 0 ? row.nextElementSibling : row.previousElementSibling;
  const next = target?.children[at]?.querySelector("input,button");
  if (next) { next.focus(); next.select?.(); } else input.select?.();
}
function cellKeys(input) {
  input.addEventListener("keydown", e => {
    if (e.isComposing || input.type === "date") return;
    if (e.key === "ArrowDown") { e.preventDefault(); input.blur(); moveRow(input, 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); input.blur(); moveRow(input, -1); }
  });
  return input;
}
function drawTable(db, root) {
  const cols = db.columns.filter(c => !c.hidden);
  const getRoot = () => root;
  const headRow = api.el("tr", {}, api.el("th", { scope: "col", class: "cname", text: api.t("Name") }), api.el("th", { scope: "col", class: "cstatus", text: api.t("Status") }));
  for (const c of cols) headRow.append(api.el("th", { scope: "col" }, api.el("button", { class: "colbtn", "data-k": `col:${c.id}`, "aria-haspopup": "menu", onclick: e => {
    const type = COLUMN_TYPES.find(x => x[0] === c.kind)?.[1] || "Text";
    const th = e.currentTarget.closest("th");
    keyMenu(api, e.currentTarget, [{ gl: "pencil-simple", label: api.t("Rename column"), run: () => {
      const input = api.el("input", { class: "colrename", "aria-label": api.t("Rename column"), dir: "auto" });
      input.value = c.name;
      let done = false;
      const finish = commit => {
        if (done) return; done = true;
        const value = input.value.trim();
        if (commit && value && value !== c.name) write(db.id, root, { op: "set_column_name", id: db.id, column: c.id, name: value });
        redraw(root, `col:${c.id}`);
      };
      input.addEventListener("keydown", ev => {
        if (ev.isComposing) return;
        if (ev.key === "Enter") { ev.preventDefault(); finish(true); }
        if (ev.key === "Escape") { ev.preventDefault(); ev.stopPropagation(); finish(false); }
      });
      input.addEventListener("blur", () => finish(true));
      th.replaceChildren(input); input.select();
    } }, { gl: "eye", label: api.t("Hide column"), run: () => {
      if (write(db.id, root, { op: "set_column_hidden", id: db.id, column: c.id, hidden: true })) redraw(root, "addcol");
    } }], { hdr: api.t(type) });
  } }, api.el("span", { dir: "auto", text: c.name || api.t("Untitled") }))));
  headRow.append(api.el("th", { class: "cend" }, api.el("button", { class: "coladd", "data-k": "addcol", "aria-label": api.t("Add column"), title: api.t("Add column"), onclick: e => columnForm(db, root, e.currentTarget) }, api.icon("plus"))));
  const tbody = api.el("tbody");
  db.rows.forEach((row, i) => {
    const tr = api.el("tr", { "data-row": row.id });
    tr.append(api.el("td", { class: "cname" }, cellKeys(field({ root: getRoot, value: row.name, key: `name:${row.id}`, label: api.t("Name"), placeholder: api.t("Untitled"),
      commit: v => write(db.id, root, { op: "set_row_field", id: db.id, row: row.id, key: "name", value: v }),
      onEnter: inp => moveRow(inp, 1) }))));
    tr.append(api.el("td", { class: "pcell cstatus" }, statusButton(db, row, getRoot)));
    for (const c of cols) {
      const kind = c.kind === "date" || c.kind === "number" ? c.kind : "text";
      const input = field({ root: getRoot, value: row.values[c.id] || "", key: `cell:${row.id}:${c.id}`, type: kind, label: c.name || api.t("Untitled"), cls: `k-${kind}`,
        commit: v => write(db.id, root, { op: "set_cell", id: db.id, row: row.id, column: c.id, value: v }),
        validate: kind === "number" ? v => (isNumber(v) ? null : api.t("{name} takes a number, like 12.5.").replace("{name}", c.name)) : null,
        onEnter: inp => moveRow(inp, 1) });
      tr.append(api.el("td", {}, cellKeys(input)));
    }
    tr.append(api.el("td", { class: "cend" }, api.el("button", { class: "rowdel", "data-k": `del:${row.id}`, "aria-label": api.t("Delete row") + ": " + (row.name || api.t("Untitled")), title: api.t("Delete row"), onclick: () => {
      const after = db.rows[i + 1] || db.rows[i - 1];
      if (write(db.id, root, { op: "delete_row", id: db.id, row: row.id })) redraw(root, after ? `del:${after.id}` : "addrow");
    } }, api.icon("trash"))));
    tbody.append(tr);
  });
  const parts = [api.el("div", { class: "tablewrap" }, api.el("table", { class: "grid" }, api.el("thead", {}, headRow), tbody))];
  if (!db.rows.length) parts.push(api.el("p", { class: "dbempty", text: api.t("No rows yet. Add the first one below.") }));
  parts.push(api.el("button", { class: "addrow", "data-k": "addrow", onclick: () => addRow(db, root) }, api.icon("plus"), api.el("span", { text: api.t("New row") })));
  return parts;
}
function columnForm(db, root, anchor) {
  api.closeMenu();
  api.$(".colform")?.remove();
  const name = api.el("input", { class: "colname", type: "text", dir: "auto", "aria-required": "true" });
  const kind = api.el("select", { class: "langsel" }, ...COLUMN_TYPES.map(([v, l]) => api.el("option", { value: v, text: api.t(l) })));
  const problem = api.el("p", { class: "dberror", role: "alert", hidden: true });
  const close = refocus => {
    form.remove(); document.removeEventListener("pointerdown", outside, true);
    if (refocus) root.querySelector(keySel("addcol"))?.focus();
  };
  const outside = e => { if (!form.contains(e.target)) close(false); };
  const submit = () => {
    const label = name.value.trim();
    if (!label) { name.setAttribute("aria-invalid", "true"); problem.textContent = api.t("Name the column first."); problem.hidden = false; name.focus(); return; }
    const id = api.uid();
    close(false);
    if (write(db.id, root, { op: "add_column", id: db.id, column: { id, name: label, kind: kind.value, hidden: false } })) redraw(root, "addcol");
  };
  const form = api.el("div", { class: "pop colform", role: "dialog", "aria-label": api.t("Add column"),
    onkeydown: e => {
      if (e.isComposing) return;
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(true); }
      else if (e.key === "Enter" && e.target.tagName !== "BUTTON") { e.preventDefault(); submit(); }
    } },
  api.el("label", { class: "field" }, api.el("span", { text: api.t("Column name") }), name),
  api.el("label", { class: "field" }, api.el("span", { text: api.t("Type") }), kind),
  problem,
  api.el("div", { class: "colfoot" },
    api.el("button", { class: "btn", onclick: () => close(true), text: api.t("Cancel") }),
    api.el("button", { class: "btn primary", onclick: submit, text: api.t("Add column") })));
  name.addEventListener("input", () => { name.removeAttribute("aria-invalid"); problem.hidden = true; });
  document.body.append(form);
  const r = anchor.getBoundingClientRect(), w = form.offsetWidth, h = form.offsetHeight;
  form.style.left = Math.max(8, Math.min(r.left, innerWidth - w - 8)) + "px";
  form.style.top = (r.bottom + h + 8 > innerHeight ? Math.max(8, r.top - h - 4) : r.bottom + 4) + "px";
  document.addEventListener("pointerdown", outside, true);
  name.focus();
}

/* ---------- Board ---------- */
function formatDate(v) {
  const d = new Date(v + "T00:00:00");
  return Number.isNaN(d.getTime()) ? v : new Intl.DateTimeFormat(api.lang, { dateStyle: "medium" }).format(d);
}
function cardFacts(db, row) {
  const cols = db.columns.filter(c => !c.hidden && row.values[c.id]);
  const date = cols.find(c => c.kind === "date"), other = cols.find(c => c.kind !== "date");
  if (!date && !other) return null;
  return api.el("small", {}, api.el("span", { dir: "auto", text: other ? row.values[other.id] : "" }), api.el("span", { text: date ? formatDate(row.values[date.id]) : "" }));
}
function moveCard(db, root, row, status) {
  if (!status || status === row.status) return;
  if (write(db.id, root, { op: "set_row_field", id: db.id, row: row.id, key: "status", value: status })) redraw(root, `card:${row.id}`);
}
function drawCard(db, root, row) {
  const name = row.name || api.t("Untitled");
  const title = api.el("b", { dir: "auto", text: name, class: row.name ? "" : "unnamed" });
  const card = api.el("div", { class: "card", tabindex: "0", role: "button", "data-k": `card:${row.id}`, "data-row": row.id,
    "aria-label": name, "aria-description": api.t("Enter edits. Left and right arrows change status.") }, title, cardFacts(db, row),
  api.el("button", { class: "rowdel carddel", tabindex: "-1", "aria-label": api.t("Delete row") + ": " + name, title: api.t("Delete row"), onclick: () => removeCard(db, root, row) }, api.icon("trash")));

  const startEdit = () => {
    if (card.classList.contains("editing")) return;
    card.classList.add("editing");
    let done = false;
    const input = api.el("input", { class: "cardedit", type: "text", dir: "auto", "aria-label": api.t("Name"), placeholder: api.t("Untitled") });
    input.value = row.name;
    const finish = (commit, refocus) => {
      if (done) return;
      done = true;
      const v = input.value;
      if (commit && v !== row.name && write(db.id, root, { op: "set_row_field", id: db.id, row: row.id, key: "name", value: v })) row.name = v;
      title.textContent = row.name || api.t("Untitled");
      title.classList.toggle("unnamed", !row.name);
      input.replaceWith(title);
      card.classList.remove("editing");
      card.setAttribute("aria-label", row.name || api.t("Untitled"));
      if (refocus) card.focus();
    };
    input.addEventListener("keydown", e => {
      e.stopPropagation();
      if (e.isComposing || e.keyCode === 229) return;
      if (e.key === "Enter") { e.preventDefault(); finish(true, true); }
      else if (e.key === "Escape") { e.preventDefault(); finish(false, true); }
    });
    input.addEventListener("blur", () => { if (!rendering) finish(true, false); });
    title.replaceWith(input);
    input.focus(); input.select();
  };
  card.addEventListener("keydown", e => {
    if (e.target !== card) return;
    const lanes = db.statuses, at = lanes.findIndex(s => s.id === statusOf(db, row)?.id);
    const rtl = getComputedStyle(card).direction === "rtl";
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); startEdit(); }
    else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      moveCard(db, root, row, lanes[at + ((e.key === "ArrowRight") !== rtl ? 1 : -1)]?.id);
    } else if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); removeCard(db, root, row); }
  });
  card.addEventListener("pointerdown", e => dragCard(e, db, root, row, card, startEdit));
  return card;
}
function removeCard(db, root, row) {
  const lane = db.rows.filter(r => r.status === row.status), i = lane.findIndex(r => r.id === row.id);
  const after = lane[i + 1] || lane[i - 1];
  if (write(db.id, root, { op: "delete_row", id: db.id, row: row.id })) redraw(root, after ? `card:${after.id}` : `add:${statusOf(db, row)?.id}`);
}
function dragCard(e, db, root, row, card, onClick) {
  if (e.button || e.target.closest("button,input") || card.classList.contains("editing")) return;
  const start = card.getBoundingClientRect(), dx = e.clientX - start.left, dy = e.clientY - start.top, sx = e.clientX, sy = e.clientY;
  const home = card.closest(".lane"), board = card.closest(".board");
  let ghost = null, over = null;
  try { card.setPointerCapture(e.pointerId); } catch { /* A synthetic pointer has nothing to capture. */ }
  const light = lane => { if (lane === over) return; over?.classList.remove("over"); over = lane; over?.classList.add("over"); };
  const move = ev => {
    if (!ghost) {
      if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < 4) return;
      ghost = card.cloneNode(true);
      ghost.removeAttribute("data-k"); ghost.removeAttribute("tabindex"); ghost.setAttribute("aria-hidden", "true");
      ghost.classList.add("lift");
      ghost.style.width = start.width + "px";
      document.body.append(ghost);
      card.classList.add("lifted");
    }
    ghost.style.left = ev.clientX - dx + "px";
    ghost.style.top = ev.clientY - dy + "px";
    const lane = document.elementFromPoint(ev.clientX, ev.clientY)?.closest(".lane");
    light(lane && board.contains(lane) && lane !== home ? lane : null);
    const edge = board.getBoundingClientRect();
    if (ev.clientX < edge.left + 24) board.scrollLeft -= 12;
    else if (ev.clientX > edge.right - 24) board.scrollLeft += 12;
  };
  const stop = () => {
    card.removeEventListener("pointermove", move); card.removeEventListener("pointerup", up); card.removeEventListener("pointercancel", stop);
    document.removeEventListener("keydown", escape, true);
    ghost?.remove(); card.classList.remove("lifted"); light(null);
  };
  const up = () => {
    const dragged = !!ghost, target = over?.dataset.status;
    stop();
    if (!dragged) onClick();
    else if (target) moveCard(db, root, row, target);
  };
  const escape = ev => { if (ev.key === "Escape") { ev.preventDefault(); ev.stopPropagation(); stop(); } };
  card.addEventListener("pointermove", move); card.addEventListener("pointerup", up); card.addEventListener("pointercancel", stop);
  document.addEventListener("keydown", escape, true);
}
function drawLane(db, root, status) {
  const rows = db.rows.filter(r => statusOf(db, r)?.id === status.id);
  const lane = api.el("section", { class: "lane", "data-status": status.id, "aria-label": status.name },
    api.el("header", {}, statusMark(status), api.el("span", { text: count(rows.length) })));
  for (const r of rows) lane.append(drawCard(db, root, r));
  if (!rows.length) lane.append(api.el("p", { class: "laneempty", text: api.t("No cards") }));
  const add = api.el("button", { class: "addrow", "data-k": `add:${status.id}`, onclick: () => addCardForm(db, root, status, add) }, api.icon("plus"), api.el("span", { text: api.t("New") }));
  lane.append(add);
  return lane;
}
function addCardForm(db, root, status, button) {
  const input = api.el("input", { class: "cardedit", type: "text", dir: "auto", "aria-label": api.t("New card in {status}").replace("{status}", status.name), placeholder: api.t("Name") });
  const box = api.el("div", { class: "card adding" }, input);
  let done = false;
  const create = () => {
    const name = input.value.trim(); if (!name) return false;
    const rows = db.rows.filter(r => statusOf(db, r)?.id === status.id), last = (rows.at(-1) || db.rows.at(-1));
    return write(db.id, root, { op: "add_row", id: db.id, after: last ? last.id : null, row: { id: api.uid(), name, status: status.id, values: {} } });
  };
  const key = `add:${status.id}`;
  input.addEventListener("keydown", e => {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter") {
      e.preventDefault();
      done = true;
      if (create()) redraw(root).querySelector(keySel(key))?.click(); // Stays open for the next card.
      else { done = false; input.setAttribute("aria-invalid", "true"); }
    } else if (e.key === "Escape") {
      e.preventDefault(); e.stopPropagation();
      done = true;
      box.replaceWith(button); button.focus();
    }
  });
  input.addEventListener("input", () => input.removeAttribute("aria-invalid"));
  input.addEventListener("blur", () => {
    if (done || rendering) return;
    done = true;
    if (create()) {
      // Repaint this lane only: a button being clicked elsewhere on the board must survive.
      const fresh = read(db.id);
      if (fresh) box.closest(".lane")?.replaceWith(drawLane(fresh, root, status));
    } else box.replaceWith(button);
  });
  button.replaceWith(box);
  input.focus();
}
function drawBoard(db, root) {
  if (!db.statuses.length) return [api.el("p", { class: "dbempty", text: api.t("This database has no statuses.") })];
  return [api.el("div", { class: "board" }, ...db.statuses.map(s => drawLane(db, root, s)))];
}

/* ---------- Drawing ---------- */
function drawDb(id, full) {
  const db = id ? read(id) : null;
  const root = api.el("div", { class: "db", "data-db": id || "", ...(full ? { "data-full": "1" } : {}) });
  if (!db) {
    root.append(api.el("p", { class: "dberror" }, api.icon("x"), api.el("span", { text: api.t("This database is not on this device yet. It appears once sync brings it here.") })));
    return root;
  }
  root.append(drawHead(db, root, full), ...(db.view === "board" ? drawBoard(db, root) : drawTable(db, root)));
  root.append(api.el("p", { class: "dberror dbmsg", role: "alert", hidden: true }));
  return root;
}
function renameEverywhere(meta, value) {
  const shown = value || api.t("Untitled");
  api.renderSide();
  const tab = api.$(".tab.on .tl"); if (tab) tab.textContent = shown;
  const crumb = api.$("#crumbs b"); if (crumb) crumb.textContent = shown;
}
function drawPage(page) {
  const meta = page.meta;
  // app.js skips its own page refresh when a feature draws the page, so keep its state in step.
  api.reload();
  api.$("#outline")?.replaceChildren(api.el("p", { text: api.t("Headings appear here.") }));
  for (const panel of api.ext.panels) panel(page);
  const db = drawDb(meta.database, true);
  const title = field({ root: () => db, value: meta.title, key: "pagetitle", cls: "title dbtitle", label: api.t("Page title"), placeholder: api.t("Untitled"),
    commit: v => {
      try {
        selfWrite = true;
        api.edit({ op: "set_title", id: meta.id, title: v }, ...(read(meta.database) ? [{ op: "set_database_field", id: meta.database, key: "name", value: v }] : []));
      } catch (err) { console.warn("title refused", err); return false; } finally { selfWrite = false; }
      renameEverywhere(meta, v);
      return true;
    },
    onEnter: () => db.querySelector("tbody input, .card, [data-k=addrow]")?.focus() });
  api.$("#content").replaceChildren(api.el("div", { class: "docscroll" }, api.el("div", { class: "dbpage" }, title, db)));
  return true;
}
function newDatabasePage() {
  const id = api.uid(), dbId = api.uid();
  api.edit({ op: "create_database", database: blankDatabase(dbId) }, { op: "create_page", id, parent: null, title: "", database: dbId });
  api.openPage(id);
  api.$(".dbtitle")?.focus();
}
function insertDatabaseBlock(b) {
  const dbId = api.uid(), id = api.uid();
  placeBlock(api, b, { id, kind: "database", checked: false, runs: [], target: dbId }, [{ op: "create_database", database: blankDatabase(dbId) }]);
  api.renderPage();
  api.$(`.b[data-id="${CSS.escape(id)}"] .dbname`)?.focus();
}

export function install(a) {
  api = a;
  const { ext } = api;
  ext.blocks.database = b => customBlock(api, drawDb(b.target, false), b, "db");
  ext.slash.push({ gl: "table", label: "Database", run: insertDatabaseBlock });
  ext.nav.push({ id: "newDbBtn", icon: "table", label: "New database", run: newDatabasePage });
  // Writes from elsewhere, such as the assistant, repaint unless someone is working in the database.
  ext.onChange.push(op => {
    if (selfWrite || !/^(add_row|delete_row|set_row_field|set_cell|add_column|set_column_hidden|set_database_field)$/.test(op.op)) return;
    queueMicrotask(() => { for (const r of roots(op.id)) if (!r.contains(document.activeElement)) redraw(r); });
  });
  const next = ext.pageView;
  ext.pageView = page => (page.meta.database ? drawPage(page) : next ? next(page) : false);
}
