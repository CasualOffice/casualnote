// Page and Canvas: one page, two views over the same content (docs/43). The note card holds the
// app's real editor, moved in from renderPage, so typing, undo and input methods are the same code.
// Layers, bottom to top: dots, world (note card and items, DOM), highlighter, pen, wet ink, tools.
// Everything written goes through api.edit, one undo step per gesture. The camera is per device.
import { I18N } from "../data.js";

const MIN_Z = 0.25, MAX_Z = 3, GRID = 22, LIMIT = 1e9, MAX_SAMPLES = 65536;
const PEN_W = 3, HI_W = 18, ARROW_W = 1.5, ERASE_R = 11, HI_ALPHA = 0.55, PALM_MS = 500;
const MIN_W = 100, MIN_H = 36, NOTE_MIN = 280, DRAG_PX = 4, BAKE_EVERY = 64;
const SIZES = { sticky: [170, 130], rect: [170, 80], ellipse: [160, 90], text: [190, 36] };
const TOOLS = [
  ["select", "Select", "cursor", "v"], null,
  ["pen", "Pen", "pen-nib", "p"], ["highlighter", "Highlighter", "highlighter", "h"], ["eraser", "Eraser", "eraser", "e"], null,
  ["sticky", "Sticky note", "note", "s"], ["rect", "Rectangle", "rectangle", "r"], ["ellipse", "Ellipse", "circle", "o"], ["arrow", "Arrow", "arrow-up-right", "a"], ["text", "Text", "text-t", "t"],
];
const PEN_COLORS = [["ink", "Black"], ["red", "Red"], ["blue", "Blue"]];
const STICKY_COLORS = [["amber", "Amber"], ["grey", "Grey"]];
const KNOWN_KINDS = new Set(Object.keys(SIZES));
const UI = ".dock,.dock-sub,.zoom";

// Drafts, not yet native-reviewed (I18N-05). Only fills gaps, so the catalogue always wins.
const STRINGS = {
  "zh-Hans": { "Page view": "页面视图", "Canvas view": "画布视图", "Canvas tools": "画布工具", Select: "选择", Pen: "钢笔", Highlighter: "荧光笔", Eraser: "橡皮擦", "Sticky note": "便签", Rectangle: "矩形", Ellipse: "椭圆", Arrow: "箭头", Text: "文本", Black: "黑色", Red: "红色", Blue: "蓝色", Amber: "琥珀色", Grey: "灰色", "Zoom in": "放大", "Zoom out": "缩小", Fit: "适合", "Reset zoom": "重置缩放", "Move note": "移动笔记", "Resize": "调整大小", "Type here": "在此输入", "Pick a tool to draw or add a note.": "选择工具来绘画或添加便签。", "On the canvas. Items: {items}. Strokes: {strokes}.": "画布上的内容。项目：{items}。笔画：{strokes}。", "Open canvas": "打开画布" },
  ja: { "Page view": "ページ表示", "Canvas view": "キャンバス表示", "Canvas tools": "キャンバスツール", Select: "選択", Pen: "ペン", Highlighter: "蛍光ペン", Eraser: "消しゴム", "Sticky note": "付箋", Rectangle: "長方形", Ellipse: "楕円", Arrow: "矢印", Text: "テキスト", Black: "黒", Red: "赤", Blue: "青", Amber: "黄", Grey: "灰色", "Zoom in": "拡大", "Zoom out": "縮小", Fit: "全体表示", "Reset zoom": "ズームをリセット", "Move note": "ノートを移動", "Resize": "サイズ変更", "Type here": "ここに入力", "Pick a tool to draw or add a note.": "ツールを選んで描くか、付箋を追加します。", "On the canvas. Items: {items}. Strokes: {strokes}.": "キャンバス上の内容。項目: {items}。線: {strokes}。", "Open canvas": "キャンバスを開く" },
  es: { "Page view": "Vista de página", "Canvas view": "Vista de lienzo", "Canvas tools": "Herramientas del lienzo", Select: "Seleccionar", Pen: "Lápiz", Highlighter: "Resaltador", Eraser: "Borrador", "Sticky note": "Nota adhesiva", Rectangle: "Rectángulo", Ellipse: "Elipse", Arrow: "Flecha", Text: "Texto", Black: "Negro", Red: "Rojo", Blue: "Azul", Amber: "Ámbar", Grey: "Gris", "Zoom in": "Acercar", "Zoom out": "Alejar", Fit: "Ajustar", "Reset zoom": "Restablecer zoom", "Move note": "Mover la nota", "Resize": "Cambiar tamaño", "Type here": "Escribe aquí", "Pick a tool to draw or add a note.": "Elige una herramienta para dibujar o añadir una nota.", "On the canvas. Items: {items}. Strokes: {strokes}.": "En el lienzo. Elementos: {items}. Trazos: {strokes}.", "Open canvas": "Abrir lienzo" },
  ar: { "Page view": "عرض الصفحة", "Canvas view": "عرض اللوحة", "Canvas tools": "أدوات اللوحة", Select: "تحديد", Pen: "قلم", Highlighter: "قلم تمييز", Eraser: "ممحاة", "Sticky note": "ملاحظة لاصقة", Rectangle: "مستطيل", Ellipse: "شكل بيضاوي", Arrow: "سهم", Text: "نص", Black: "أسود", Red: "أحمر", Blue: "أزرق", Amber: "كهرماني", Grey: "رمادي", "Zoom in": "تكبير", "Zoom out": "تصغير", Fit: "ملاءمة", "Reset zoom": "إعادة ضبط التكبير", "Move note": "نقل الملاحظة", "Resize": "تغيير الحجم", "Type here": "اكتب هنا", "Pick a tool to draw or add a note.": "اختر أداة للرسم أو لإضافة ملاحظة.", "On the canvas. Items: {items}. Strokes: {strokes}.": "على اللوحة. العناصر: {items}. الخطوط: {strokes}.", "Open canvas": "فتح اللوحة" },
  hi: { "Page view": "पेज दृश्य", "Canvas view": "कैनवस दृश्य", "Canvas tools": "कैनवस उपकरण", Select: "चुनें", Pen: "पेन", Highlighter: "हाइलाइटर", Eraser: "इरेज़र", "Sticky note": "स्टिकी नोट", Rectangle: "आयत", Ellipse: "दीर्घवृत्त", Arrow: "तीर", Text: "पाठ", Black: "काला", Red: "लाल", Blue: "नीला", Amber: "कहरुवा", Grey: "धूसर", "Zoom in": "ज़ूम इन", "Zoom out": "ज़ूम आउट", Fit: "फ़िट करें", "Reset zoom": "ज़ूम रीसेट करें", "Move note": "नोट हिलाएँ", "Resize": "आकार बदलें", "Type here": "यहाँ लिखें", "Pick a tool to draw or add a note.": "चित्र बनाने या नोट जोड़ने के लिए उपकरण चुनें।", "On the canvas. Items: {items}. Strokes: {strokes}.": "कैनवस पर। आइटम: {items}। रेखाएँ: {strokes}।", "Open canvas": "कैनवस खोलें" },
};

const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const safe = v => (Number.isFinite(v) ? clamp(v, -LIMIT, LIMIT) : 0);
const milli = v => Math.round(safe(v) * 1000) / 1000;
const store = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* optional */ } },
};

/* ---------- Outline ---------- */
/** Fills a pen stroke, or strokes a highlighter's centre line at its flat width. `pts` are [x, y, pressure]. */
export function outline(pts, w, flat) {
  const path = new Path2D();
  if (!pts.length) return path;
  // Smoothing is derived at draw time. The stored samples stay as they arrived.
  const clean = [];
  for (let i = 0; i < pts.length; i++) {
    const p = pts[i], l = clean[clean.length - 1];
    if (!l || i === pts.length - 1 || Math.hypot(p[0] - l[0], p[1] - l[1]) >= 0.25) clean.push(p);
  }
  const n = clean.length;
  const c = clean.map((p, i) => {
    if (i === 0 || i === n - 1) return p;
    const a = clean[i - 1], b = clean[i + 1];
    return [(a[0] + 2 * p[0] + b[0]) / 4, (a[1] + 2 * p[1] + b[1]) / 4, (a[2] + 2 * p[2] + b[2]) / 4];
  });
  const through = pair => {
    for (let i = 1; i < pair.length - 1; i++) {
      const [x, y] = pair[i], [u, v] = pair[i + 1];
      path.quadraticCurveTo(x, y, (x + u) / 2, (y + v) / 2);
    }
    path.lineTo(pair[pair.length - 1][0], pair[pair.length - 1][1]);
  };
  if (flat) {
    path.moveTo(c[0][0], c[0][1]);
    if (n < 2) path.lineTo(c[0][0] + 0.01, c[0][1]); else through(c);
    return path;
  }
  const radius = q => w * (0.28 + 0.62 * clamp(q, 0, 1));
  if (n < 2) { path.arc(c[0][0], c[0][1], radius(c[0][2]), 0, Math.PI * 2); return path; }
  const L = [], R = [];
  let start = 0, end = 0;
  for (let i = 0; i < n; i++) {
    const a = c[Math.max(0, i - 1)], b = c[Math.min(n - 1, i + 1)], p = c[i];
    let dx = b[0] - a[0], dy = b[1] - a[1];
    const d = Math.hypot(dx, dy) || 1; dx /= d; dy /= d;
    const r = radius(p[2]);
    L.push([p[0] - dy * r, p[1] + dx * r]); R.push([p[0] + dy * r, p[1] - dx * r]);
    if (i === 0) start = Math.atan2(dx, -dy);
    if (i === n - 1) end = Math.atan2(dx, -dy);
  }
  path.moveTo(L[0][0], L[0][1]);
  through(L);
  path.arc(c[n - 1][0], c[n - 1][1], radius(c[n - 1][2]), end, end - Math.PI, true);
  through(R.reverse());
  path.arc(c[0][0], c[0][1], radius(c[0][2]), start + Math.PI, start, true);
  path.closePath();
  return path;
}

function arrowPath(s) {
  const [ax, ay] = s.points[0], [bx, by] = s.points[s.points.length - 1];
  const ang = Math.atan2(by - ay, bx - ax), l = 12;
  const line = new Path2D(), head = new Path2D();
  line.moveTo(ax, ay); line.lineTo(bx - Math.cos(ang) * 6, by - Math.sin(ang) * 6);
  head.moveTo(bx, by);
  head.lineTo(bx - l * Math.cos(ang - 0.45), by - l * Math.sin(ang - 0.45));
  head.lineTo(bx - l * Math.cos(ang + 0.45), by - l * Math.sin(ang + 0.45));
  head.closePath();
  return { line, head };
}

const cache = new WeakMap();
function entry(s) {
  let e = cache.get(s);
  if (e) return e;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of s.points) { x0 = Math.min(x0, p[0]); y0 = Math.min(y0, p[1]); x1 = Math.max(x1, p[0]); y1 = Math.max(y1, p[1]); }
  const m = s.kind === "arrow" ? 14 : (s.width || PEN_W) + 2;
  e = { box: [x0 - m, y0 - m, x1 + m, y1 + m] };
  if (s.kind === "arrow") Object.assign(e, arrowPath(s));
  else e.path = outline(s.points, s.width || PEN_W, s.kind === "highlighter");
  cache.set(s, e);
  return e;
}

function segDist(px, py, a, b) {
  const dx = b[0] - a[0], dy = b[1] - a[1];
  const k = clamp(((px - a[0]) * dx + (py - a[1]) * dy) / (dx * dx + dy * dy || 1), 0, 1);
  return Math.hypot(a[0] + k * dx - px, a[1] + k * dy - py);
}

/* ---------- The feature ---------- */
export function install(api) {
  const { t, el, icon, uid, $ } = api;
  for (const [lang, strings] of Object.entries(STRINGS)) { if (I18N[lang]) for (const [k, v] of Object.entries(strings)) I18N[lang][k] ??= v; }

  let S = null;          // The open surface.
  let act = null;        // The gesture in progress.
  let tool = "select", penColor = "ink", stickyColor = "amber";
  let penSeen = store.get("casualnote-pen-seen") === "1", penDown = false, penAt = 0;
  let lastKeyAt = -1e9, pointerHeld = false;
  const touches = new Map(), cams = new Map();

  const isOpen = () => !!S && S.cv.isConnected && !api.state.view;
  const commit = (...ops) => {
    try { api.edit(...ops); return true; }
    catch (err) { console.warn("canvas edit refused", err); api.renderAll(); return false; }
  };
  const typing = () => {
    const a = document.activeElement;
    return !!a && (a.isContentEditable || a.matches?.("input,textarea,select"));
  };

  /* ----- Switch ----- */
  const modeBtn = (view, gl, label) => el("button", { "data-view": view, "aria-label": t(view === "page" ? "Page view" : "Canvas view"), onclick: () => setView(view) }, icon(gl), el("span", { "data-t": label, text: t(label) }));
  const mode = el("div", { class: "mode cv-mode", role: "group" }, modeBtn("page", "file-text", "Page"), modeBtn("canvas", "frame-corners", "Canvas"));
  $(".bar")?.insertBefore(mode, $("#favBtn"));

  function setView(view) {
    const cur = api.state.cur;
    endEdit();
    if (api.pageData(cur).view === view) return;
    if (commit({ op: "set_view", page: cur, view })) api.renderAll();
  }
  function curPage() { try { return api.state.cur ? api.pageData(api.state.cur) : null; } catch { return null; } }
  function sync() {
    const page = api.state.view ? null : curPage();
    mode.hidden = !page;
    if (page) for (const b of mode.children) {
      b.setAttribute("aria-pressed", String((page.view === "canvas" ? "canvas" : "page") === b.dataset.view));
      b.setAttribute("aria-label", t(b.dataset.view === "page" ? "Page view" : "Canvas view"));
    }
    paintTruth(page);
  }
  // Page view tells the truth: content a person cannot see is named, with a way to it.
  function paintTruth(page) {
    const had = $(".cv-truth");
    const doc = $(".docscroll .doc");
    const n = page && page.view !== "canvas" && doc ? page.canvas.items.length + page.canvas.ink.length : 0;
    if (!n) { had?.remove(); return; }
    const num = new Intl.NumberFormat(api.lang);
    const text = t("On the canvas. Items: {items}. Strokes: {strokes}.").replace("{items}", num.format(page.canvas.items.length)).replace("{strokes}", num.format(page.canvas.ink.length));
    const line = had || el("div", { class: "cv-truth" }, el("span"), el("button", { text: t("Open canvas"), onclick: () => setView("canvas") }));
    line.firstChild.textContent = text;
    if (!had || !doc.contains(had)) doc.querySelector(".tail")?.before(line);
  }
  new MutationObserver(sync).observe($("#crumbs"), { childList: true });
  let syncQueued = false;
  api.ext.onChange.push(op => {
    if (!["set_view", "put_item", "delete_item", "change_ink"].includes(op.op) || syncQueued) return;
    syncQueued = true;
    queueMicrotask(() => { syncQueued = false; sync(); });
  });

  /* ----- Mounting ----- */
  const prev = api.ext.pageView;
  api.ext.pageView = page => {
    mode.hidden = !!page.meta.database;
    if (prev?.(page)) { drop(); return true; }
    if (page.view !== "canvas") { drop(); sync(); return false; }
    // An input method is mid-word in an item. Never pull the node out from under it.
    if (S?.composing && S.pageId === page.meta.id && S.cv.isConnected) return true;
    draw(page);
    sync();
    return true;
  };

  function drop() {
    if (!S) return;
    if (act) abort();
    if (S.editing) commitText(S.editing);
    S.editing = null;
    S.ro.disconnect();
    cams.set(S.pageId, { ...S.cam }); // A repaint, such as undo, must not move the view.
    if (S.raf) cancelAnimationFrame(S.raf);
    S = null;
    touches.clear();
  }

  function draw(page) {
    const same = S && S.pageId === page.meta.id, sel = same ? S.sel : null;
    const resume = same && S.editing ? { id: S.editing.it.id, caret: api.caretOffset(S.editing.txt), draft: S.drafts } : null;
    drop();
    // Read after drop: finishing an edit there may have written.
    const fresh = api.pageData(page.meta.id);
    const had = $(".blocks"); if (had) had.dataset.page = "";
    api.renderPage();
    const doc = $("#content .doc");

    const cv = el("div", { class: "canvas cv", id: "cv", role: "application", "aria-label": t("Canvas") });
    const world = el("div", { class: "world" });
    const note = el("div", { class: "note" },
      el("div", { class: "grip", role: "button", "aria-label": t("Move note") }, icon("dots-six")),
      el("div", { class: "body", dir: document.documentElement.dir || "ltr" }, doc),
      el("div", { class: "rz", "aria-label": t("Resize") }));
    world.append(note);
    const layer = name => el("canvas", { class: "cv-layer " + name, "aria-hidden": "true" });
    const hi = layer("cv-hi"), dry = layer("cv-dry"), wet = layer("cv-wet");
    const hint = el("div", { class: "cv-hint", text: t("Pick a tool to draw or add a note.") });
    const ring = el("div", { class: "cv-ring", hidden: true });
    const dock = el("div", { class: "dock", role: "toolbar", "aria-label": t("Canvas tools") });
    const sub = el("div", { class: "dock-sub", role: "group", hidden: true });
    for (const row of TOOLS) {
      if (!row) { dock.append(el("span", { class: "sep" })); continue; }
      const [id, label, gl, kbd] = row;
      dock.append(el("button", { "data-tool": id, "aria-label": t(label), "aria-keyshortcuts": kbd, "data-tip": `${t(label)}  ${kbd.toUpperCase()}`, "aria-pressed": "false", onclick: () => setTool(id) }, icon(gl)));
    }
    const pct = el("button", { class: "cv-pct", "data-tip": t("Reset zoom"), "aria-label": t("Reset zoom"), onclick: () => zoomTo(1) }, el("span"));
    const zoom = el("div", { class: "zoom" },
      el("button", { "aria-label": t("Zoom out"), "data-tip": t("Zoom out"), onclick: () => zoomBy(0.8) }, icon("minus")), pct,
      el("button", { "aria-label": t("Zoom in"), "data-tip": t("Zoom in"), onclick: () => zoomBy(1.25) }, icon("plus")),
      el("button", { class: "cv-fit", text: t("Fit"), onclick: fit }));
    cv.append(world, hi, dry, wet, ring, hint, dock, sub, zoom);
    $("#content").replaceChildren(cv);

    S = {
      pageId: fresh.meta.id, cv, world, note, hi, dry, wet, ring, hint, dock, sub, pct,
      noteData: { ...fresh.canvas.note }, items: fresh.canvas.items.map(i => ({ ...i })), ink: fresh.canvas.ink,
      nodes: new Map(), drafts: resume?.draft || new Set(), sel: null, editing: null, composing: false, space: false,
      rect: cv.getBoundingClientRect(), cam: null, dpr: 1, raf: 0, dryDirty: true, bake: null, ro: null, colors: null,
    };
    placeNote();
    for (const it of S.items) mountItem(it);
    S.cam = loadCam(S.pageId) || homeCam();
    S.ro = new ResizeObserver(() => { sizeLayers(); paintNow(); });
    S.ro.observe(cv);
    sizeLayers();
    applyCam();
    setTool(tool);
    if (sel && (sel.arrow ? S.ink.some(s => s.id === sel.id) : S.nodes.has(sel.id))) select(sel);
    paintNow();
    if (resume && S.nodes.has(resume.id)) { select({ id: resume.id }); startEdit(resume.id); api.setCaret(S.editing.txt, resume.caret); }

    cv.addEventListener("pointerdown", onDown);
    cv.addEventListener("pointermove", onMove);
    cv.addEventListener("pointerup", onUp);
    cv.addEventListener("pointercancel", onUp);
    cv.addEventListener("pointerleave", () => { ring.hidden = true; });
    cv.addEventListener("wheel", onWheel, { passive: false });
    // A click on the padding of the item being edited keeps the caret where it is.
    cv.addEventListener("mousedown", e => { if (e.target.closest?.(".ci.editing") && !e.target.closest(".txt")) e.preventDefault(); });
    cv.addEventListener("dblclick", e => { const ci = e.target.closest?.(".ci"); if (ci && tool === "select" && !ci.classList.contains("editing")) startEdit(ci.dataset.id, e.clientX, e.clientY); });
    // Safari's trackpad pinch.
    let gestureZ = 1;
    cv.addEventListener("gesturestart", e => { e.preventDefault(); gestureZ = S.cam.z; });
    cv.addEventListener("gesturechange", e => { e.preventDefault(); const r = S.rect; zoomAround(e.clientX - r.left, e.clientY - r.top, gestureZ * e.scale); });
  }

  /* ----- Camera ----- */
  const camKey = id => "casualnote-cam-" + id;
  function loadCam(id) {
    if (cams.has(id)) return cams.get(id);
    try {
      const c = JSON.parse(store.get(camKey(id)));
      if (c && [c.x, c.y, c.z].every(Number.isFinite)) return { x: c.x, y: c.y, z: clamp(c.z, MIN_Z, MAX_Z) };
    } catch { /* a bad preference is no camera */ }
    return null;
  }
  let camTimer = 0;
  function saveCam() {
    if (!S) return;
    const id = S.pageId, cam = { ...S.cam };
    cams.set(id, cam);
    clearTimeout(camTimer);
    camTimer = setTimeout(() => store.set(camKey(id), JSON.stringify(cam)), 300);
  }
  const margin = () => (S.rect.width < 760 ? 12 : 64);
  function homeCam() {
    const { width } = S.rect, w = S.noteData.width, m = margin();
    const z = clamp(Math.min(1, (width - 2 * m) / w), MIN_Z, MAX_Z);
    return { x: Math.max(m, (width - w * z) / 2) - S.noteData.x * z, y: (width < 760 ? 56 : 24) - S.noteData.y * z, z };
  }
  function applyCam() {
    const c = S.cam;
    c.z = clamp(c.z, MIN_Z, MAX_Z);
    c.x = clamp(c.x, -LIMIT * c.z, LIMIT * c.z);
    c.y = clamp(c.y, -LIMIT * c.z, LIMIT * c.z);
    S.world.style.transform = `translate(${c.x}px,${c.y}px) scale(${c.z})`;
    S.world.style.setProperty("--cz", String(c.z));
    const g = GRID * c.z;
    S.cv.classList.toggle("nogrid", g < 8);
    S.cv.style.backgroundSize = `${g}px ${g}px`;
    S.cv.style.backgroundPosition = `${c.x}px ${c.y}px`;
    S.pct.firstChild.textContent = new Intl.NumberFormat(api.lang, { style: "percent", maximumFractionDigits: 0 }).format(c.z);
    invalidate();
  }
  function zoomAround(x, y, z) {
    const c = S.cam; z = clamp(z, MIN_Z, MAX_Z);
    c.x = x - (x - c.x) * (z / c.z); c.y = y - (y - c.y) * (z / c.z); c.z = z;
    applyCam(); saveCam();
  }
  const zoomTo = z => { S.rect = S.cv.getBoundingClientRect(); zoomAround(S.rect.width / 2, S.rect.height / 2, z); };
  const zoomBy = f => zoomTo(S.cam.z * f);
  function fit() {
    S.rect = S.cv.getBoundingClientRect();
    const n = S.noteData;
    let x0 = n.x, y0 = n.y, x1 = n.x + n.width, y1 = n.y + S.note.offsetHeight;
    for (const it of S.items) { x0 = Math.min(x0, it.x); y0 = Math.min(y0, it.y); x1 = Math.max(x1, it.x + it.width); y1 = Math.max(y1, it.y + it.height); }
    for (const s of S.ink) { const b = entry(s).box; x0 = Math.min(x0, b[0]); y0 = Math.min(y0, b[1]); x1 = Math.max(x1, b[2]); y1 = Math.max(y1, b[3]); }
    const phone = S.rect.width < 760, m = margin(), top = phone ? 56 : 24, bottom = phone ? 62 : 56;
    const z = clamp(Math.min(1, (S.rect.width - 2 * m) / (x1 - x0), (S.rect.height - top - bottom) / (y1 - y0)), MIN_Z, MAX_Z);
    S.cam = { z, x: (S.rect.width - (x1 - x0) * z) / 2 - x0 * z, y: top + Math.max(0, (S.rect.height - top - bottom - (y1 - y0) * z) / 2) - y0 * z };
    applyCam(); saveCam();
  }
  const toWorld = (cx, cy) => [safe((cx - S.rect.left - S.cam.x) / S.cam.z), safe((cy - S.rect.top - S.cam.y) / S.cam.z)];
  function onWheel(e) {
    e.preventDefault();
    if (act && act.k !== "pan") return; // The camera is frozen during ink and moves.
    S.rect = S.cv.getBoundingClientRect();
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? S.rect.height : 1;
    if (e.ctrlKey || e.metaKey) zoomAround(e.clientX - S.rect.left, e.clientY - S.rect.top, S.cam.z * Math.exp(-e.deltaY * unit * 0.004));
    else { S.cam.x -= e.deltaX * unit; S.cam.y -= e.deltaY * unit; applyCam(); saveCam(); }
  }

  /* ----- Ink layers ----- */
  function sizeLayers() {
    S.rect = S.cv.getBoundingClientRect();
    S.dpr = Math.min(window.devicePixelRatio || 1, 3);
    const w = Math.max(1, Math.round(S.rect.width * S.dpr)), h = Math.max(1, Math.round(S.rect.height * S.dpr));
    for (const c of [S.hi, S.dry, S.wet]) if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
    S.colors = null;
  }
  function colors() {
    if (S.colors) return S.colors;
    const css = getComputedStyle(S.cv), v = n => css.getPropertyValue(n).trim();
    return (S.colors = { ink: v("--ink"), red: v("--ink-red"), blue: v("--ink-blue"), hi: v("--hi"), blend: v("--hi-blend") || "multiply", strong: v("--strong"), accent: v("--accent"), bg: v("--bg") });
  }
  const worldCtx = canvas => {
    const ctx = canvas.getContext("2d"), { x, y, z } = S.cam, d = S.dpr;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(d * z, 0, 0, d * z, d * x, d * y);
    return ctx;
  };
  function paintDry() {
    S.dryDirty = false;
    const col = colors(), hi = worldCtx(S.hi), pen = worldCtx(S.dry), { x, y, z } = S.cam;
    const x0 = -x / z, y0 = -y / z, x1 = (S.rect.width - x) / z, y1 = (S.rect.height - y) / z;
    hi.globalCompositeOperation = col.blend; hi.globalAlpha = HI_ALPHA; hi.strokeStyle = col.hi; hi.lineCap = hi.lineJoin = "round";
    pen.lineCap = "round";
    for (const s of S.ink) {
      if (!s.points.length) continue;
      const e = entry(s), b = e.box;
      if (b[2] < x0 || b[0] > x1 || b[3] < y0 || b[1] > y1) continue;
      if (s.kind === "highlighter") { hi.lineWidth = s.width || HI_W; hi.stroke(e.path); }
      else if (s.kind === "arrow") { pen.strokeStyle = pen.fillStyle = col.strong; pen.lineWidth = s.width || ARROW_W; pen.stroke(e.line); pen.fill(e.head); }
      else { pen.fillStyle = col[s.color] || col.ink; pen.fill(e.path); }
    }
  }
  function strokeInto(ctx, pts, a) {
    if (a.flat) { ctx.lineWidth = a.width; ctx.lineCap = ctx.lineJoin = "round"; ctx.stroke(outline(pts, a.width, true)); }
    else ctx.fill(outline(pts, a.width, false));
  }
  function paintWet() {
    const ctx = worldCtx(S.wet), col = colors();
    if (act?.k === "ink") {
      const paint = act.flat ? col.hi : col[act.color] || col.ink;
      ctx.fillStyle = ctx.strokeStyle = paint;
      if (S.bake) { ctx.save(); ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.drawImage(S.bake, 0, 0); ctx.restore(); }
      const tail = act.pts.slice(Math.max(0, act.baked - 2));
      strokeInto(ctx, tail.concat(act.predicted), act);
    } else if (act?.k === "arrow") {
      ctx.fillStyle = ctx.strokeStyle = col.strong; ctx.lineWidth = ARROW_W;
      const e = arrowPath({ points: [act.a, act.b] });
      ctx.stroke(e.line); ctx.fill(e.head);
    }
    const s = S.sel?.arrow && S.ink.find(i => i.id === S.sel.id);
    if (s) {
      const e = arrowPath(s), z = S.cam.z, h = 5 / z;
      ctx.strokeStyle = ctx.fillStyle = col.accent; ctx.lineWidth = 2.5 / z; ctx.lineCap = "round";
      ctx.stroke(e.line); ctx.fill(e.head);
      ctx.fillStyle = col.bg; ctx.lineWidth = 1.5 / z;
      for (const p of [s.points[0], s.points[s.points.length - 1]]) { ctx.fillRect(p[0] - h, p[1] - h, 2 * h, 2 * h); ctx.strokeRect(p[0] - h, p[1] - h, 2 * h, 2 * h); }
    }
  }
  function invalidate(dry = true) {
    if (!S) return;
    if (dry) S.dryDirty = true;
    if (!S.raf) S.raf = requestAnimationFrame(() => { if (!S) return; S.raf = 0; if (S.dryDirty) paintDry(); paintWet(); });
  }
  function paintNow() { paintDry(); paintWet(); }
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => { if (S) { S.colors = null; paintNow(); } });

  /* ----- Note card and items ----- */
  function placeNote() {
    const n = S.noteData, st = S.note.style;
    st.left = n.x + "px"; st.top = n.y + "px"; st.width = n.width + "px";
  }
  function placeItem(node, it) {
    const st = node.style;
    st.left = it.x + "px"; st.top = it.y + "px"; st.width = it.width + "px"; st.height = it.height + "px";
  }
  function itemNode(it) {
    const kind = KNOWN_KINDS.has(it.kind) ? it.kind : "cv-unknown";
    const color = it.kind === "sticky" ? (STICKY_COLORS.some(c => c[0] === it.color) ? ` c-${it.color}` : " c-other") : "";
    const node = el("div", { class: `ci ${kind}${color}`, "data-id": it.id, tabindex: "0", role: "group", "aria-label": t(({ sticky: "Sticky note", rect: "Rectangle", ellipse: "Ellipse", text: "Text" })[it.kind] || "Text") });
    placeItem(node, it);
    node.append(el("div", { class: "txt", dir: "auto", "data-ph": t("Type here"), text: it.text }), el("div", { class: "rz", "aria-label": t("Resize") }));
    return node;
  }
  function mountItem(it) {
    const node = itemNode(it);
    S.world.append(node); S.nodes.set(it.id, node);
    return node;
  }
  const itemOf = id => S.items.find(i => i.id === id);
  function removeItem(id) {
    S.nodes.get(id)?.remove(); S.nodes.delete(id);
    S.items = S.items.filter(i => i.id !== id);
    if (S.sel?.id === id) select(null);
    hintState();
  }
  function select(sel) {
    if (S.sel && !S.sel.arrow) S.nodes.get(S.sel.id)?.classList.remove("sel");
    S.sel = sel;
    if (sel && !sel.arrow) S.nodes.get(sel.id)?.classList.add("sel");
    paintSub();
    invalidate(false);
  }
  function hintState() { S.hint.hidden = S.items.length > 0 || S.ink.length > 0; }

  function deleteSelected() {
    const sel = S.sel; if (!sel) return;
    if (sel.arrow) {
      const s = S.ink.find(i => i.id === sel.id);
      select(null);
      if (s && commit({ op: "change_ink", page: S.pageId, add: [], remove: [s.id] })) S.ink = S.ink.filter(i => i !== s);
      hintState(); paintNow();
      return;
    }
    if (S.drafts.has(sel.id)) { S.drafts.delete(sel.id); removeItem(sel.id); return; }
    if (commit({ op: "delete_item", page: S.pageId, id: sel.id })) removeItem(sel.id);
  }
  const putItem = it => (S.drafts.delete(it.id), commit({ op: "put_item", page: S.pageId, item: { ...it } }));

  /* ----- Text on items ----- */
  function startEdit(id, cx, cy) {
    const node = S.nodes.get(id), it = itemOf(id);
    if (!node || !it) return;
    if (S.editing?.it === it) return;
    endEdit();
    const txt = node.querySelector(".txt");
    txt.contentEditable = "plaintext-only";
    if (txt.contentEditable !== "plaintext-only") txt.contentEditable = "true";
    txt.spellcheck = true;
    txt.setAttribute("role", "textbox");
    txt.setAttribute("aria-label", t("Text"));
    node.classList.add("editing");
    const ed = S.editing = { it, txt, node, timer: 0 };
    ed.onInput = () => { if (S?.composing) return; clearTimeout(ed.timer); ed.timer = setTimeout(() => { if (S?.editing === ed) commitText(ed); }, 500); };
    ed.onStart = () => { S.composing = true; };
    ed.onEnd = () => { if (S) S.composing = false; ed.onInput(); };
    ed.onBlur = () => { if (S?.editing === ed) endEdit(); };
    ed.onKey = e => {
      if (e.isComposing) return;
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); endEdit(); node.focus({ preventScroll: true }); }
      else if (e.key === "Enter" && txt.contentEditable !== "plaintext-only") { e.preventDefault(); document.execCommand("insertText", false, "\n"); }
    };
    ed.onPaste = e => { e.preventDefault(); document.execCommand("insertText", false, e.clipboardData.getData("text/plain")); };
    txt.addEventListener("input", ed.onInput);
    txt.addEventListener("compositionstart", ed.onStart);
    txt.addEventListener("compositionend", ed.onEnd);
    txt.addEventListener("blur", ed.onBlur);
    txt.addEventListener("keydown", ed.onKey);
    txt.addEventListener("paste", ed.onPaste);
    txt.focus({ preventScroll: true });
    const at = cx != null && document.caretRangeFromPoint?.(cx, cy);
    if (at && txt.contains(at.startContainer)) { const s = getSelection(); s.removeAllRanges(); s.addRange(at); }
    else api.setCaret(txt);
  }
  function commitText(ed) {
    clearTimeout(ed.timer);
    const text = ed.txt.textContent;
    if (text === ed.it.text) return;
    ed.it.text = text;
    if (!(S.drafts.has(ed.it.id) && !text)) putItem(ed.it);
  }
  function endEdit() {
    const ed = S?.editing; if (!ed) return;
    commitText(ed);
    S.editing = null; S.composing = false;
    const { txt, node, it } = ed;
    for (const [type, f] of [["input", ed.onInput], ["compositionstart", ed.onStart], ["compositionend", ed.onEnd], ["blur", ed.onBlur], ["keydown", ed.onKey], ["paste", ed.onPaste]]) txt.removeEventListener(type, f);
    txt.removeAttribute("contenteditable"); txt.removeAttribute("role"); txt.removeAttribute("aria-label");
    node.classList.remove("editing");
    if (document.activeElement === txt) txt.blur();
    getSelection()?.removeAllRanges();
    // An empty text box is not content. Leave nothing behind.
    if (it.kind === "text" && !it.text) {
      if (S.drafts.has(it.id)) { S.drafts.delete(it.id); removeItem(it.id); }
      else if (commit({ op: "delete_item", page: S.pageId, id: it.id })) removeItem(it.id);
    }
  }

  /* ----- Tools ----- */
  function setTool(id) {
    if (!S) { tool = id; return; }
    tool = id;
    if (id !== "select") { select(null); endEdit(); }
    for (const b of S.dock.querySelectorAll("[data-tool]")) b.setAttribute("aria-pressed", String(b.dataset.tool === id));
    S.cv.className = S.cv.className.replace(/\bt-\S+/g, "").trim();
    S.cv.classList.add("t-" + id);
    S.cv.classList.toggle("draw", id !== "select");
    if (id !== "eraser") S.ring.hidden = true;
    hintState(); paintSub();
  }
  function paintSub() {
    const stickySel = S.sel && !S.sel.arrow && itemOf(S.sel.id)?.kind === "sticky";
    const set = tool === "pen" ? PEN_COLORS : tool === "sticky" || stickySel ? STICKY_COLORS : null;
    S.sub.hidden = !set;
    if (!set) { S.sub.replaceChildren(); return; }
    const current = tool === "pen" ? penColor : stickySel ? itemOf(S.sel.id).color : stickyColor;
    S.sub.replaceChildren(...set.map(([c, label]) => el("button", {
      class: "sw k-" + c, "aria-label": t(label), "data-tip": t(label), "aria-pressed": String(current === c), onclick: () => pickColor(c),
    }, el("i"))));
  }
  function pickColor(c) {
    if (tool === "pen") penColor = c;
    else {
      stickyColor = c;
      const it = S.sel && !S.sel.arrow && itemOf(S.sel.id);
      if (it?.kind === "sticky" && it.color !== c) {
        it.color = c; putItem(it);
        const node = S.nodes.get(it.id);
        node.classList.remove("c-amber", "c-grey", "c-other"); node.classList.add("c-" + c);
      }
    }
    paintSub();
  }

  /* ----- Gestures ----- */
  const capture = e => { try { S.cv.setPointerCapture(e.pointerId); } catch { /* not a live pointer */ } };
  const blurEditor = () => { endEdit(); const a = document.activeElement; if (a?.isContentEditable) a.blur(); };

  function onDown(e) {
    if (!S || e.target.closest(UI)) return;
    S.rect = S.cv.getBoundingClientRect();
    const type = e.pointerType || "mouse", now = performance.now();
    if (type === "pen") {
      if (!penSeen) { penSeen = true; store.set("casualnote-pen-seen", "1"); }
      penDown = true; penAt = now;
      if (act?.type === "touch") abort(); // A pen arrived during a touch stroke: the touch was a palm.
    }
    if (type === "touch") {
      if (penDown || now - penAt < PALM_MS) return;
      touches.set(e.pointerId, [e.clientX, e.clientY]);
      if (touches.size >= 2) { if (act) abort(); startPinch(); return; }
    }
    if (act) return;
    if (type === "mouse" && e.button === 1) { e.preventDefault(); startPan(e); return; }
    if (type === "mouse" && e.button !== 0) return;
    if (S.space) { e.preventDefault(); startPan(e); return; }
    if (tool !== "select") {
      if (type === "touch" && penSeen) { startPan(e); return; } // After a pen, fingers pan and never draw.
      e.preventDefault(); capture(e); blurEditor();
      startTool(e, type);
      return;
    }
    downSelect(e, type);
  }

  function startPan(e) {
    capture(e);
    S.cv.classList.add("grabbing", "moving");
    act = { k: "pan", id: e.pointerId, type: e.pointerType, cx: e.clientX, cy: e.clientY, x: S.cam.x, y: S.cam.y, moved: false };
  }
  function startPinch() {
    const [a, b] = [...touches.values()];
    S.cv.classList.add("moving");
    act = { k: "pinch", type: "touch", d: Math.hypot(a[0] - b[0], a[1] - b[1]) || 1, cx: (a[0] + b[0]) / 2 - S.rect.left, cy: (a[1] + b[1]) / 2 - S.rect.top, cam: { ...S.cam } };
  }
  function movePinch() {
    const [a, b] = [...touches.values()]; if (!b) return;
    const d = Math.hypot(a[0] - b[0], a[1] - b[1]) || 1, cx = (a[0] + b[0]) / 2 - S.rect.left, cy = (a[1] + b[1]) / 2 - S.rect.top;
    const z = clamp(act.cam.z * d / act.d, MIN_Z, MAX_Z);
    const wx = (act.cx - act.cam.x) / act.cam.z, wy = (act.cy - act.cam.y) / act.cam.z;
    S.cam = { z, x: cx - wx * z, y: cy - wy * z };
    applyCam();
  }

  function startTool(e, type) {
    const p = toWorld(e.clientX, e.clientY);
    if (tool === "pen" || tool === "highlighter") { startInk(e, type, p); return; }
    if (tool === "eraser") {
      act = { k: "erase", id: e.pointerId, type, orig: S.ink, last: p };
      S.ink = S.ink.slice();
      eraseTo(p);
      return;
    }
    if (tool === "arrow") { act = { k: "arrow", id: e.pointerId, type, a: p, b: p }; invalidate(false); return; }
    const color = tool === "sticky" ? stickyColor : "";
    const it = { id: uid(), kind: tool, x: 0, y: 0, width: SIZES[tool][0], height: SIZES[tool][1], color, text: "" };
    const node = itemNode(it);
    node.classList.add("cv-preview");
    S.world.append(node);
    act = { k: "place", id: e.pointerId, type, it, node, start: p, cx: e.clientX, cy: e.clientY, moved: false };
    sizePlace(p);
  }
  function sizePlace(p) {
    const { it, start, node } = act;
    if (act.moved) {
      it.width = Math.max(MIN_W, Math.abs(p[0] - start[0])); it.height = Math.max(MIN_H, Math.abs(p[1] - start[1]));
      it.x = safe(p[0] < start[0] ? start[0] - it.width : start[0]); it.y = safe(p[1] < start[1] ? start[1] - it.height : start[1]);
    } else {
      [it.width, it.height] = SIZES[it.kind];
      it.x = safe(start[0] - it.width / 2); it.y = safe(start[1] - it.height / 2);
    }
    placeItem(node, it);
  }

  function startInk(e, type, p) {
    const flat = tool === "highlighter";
    S.wet.classList.toggle("hi", flat);
    act = {
      k: "ink", id: e.pointerId, type, flat, color: flat ? "" : penColor, kind: flat ? "highlighter" : "pen", width: flat ? HI_W : PEN_W,
      pts: [], done: [], predicted: [], baked: 0, t: e.timeStamp, q: 0.5,
    };
    S.bake = document.createElement("canvas");
    S.bake.width = S.wet.width; S.bake.height = S.wet.height;
    addSample(e, p);
    invalidate(false);
  }
  function addSample(ev, p = toWorld(ev.clientX, ev.clientY)) {
    const a = act, l = a.pts[a.pts.length - 1];
    let q;
    if (a.type === "pen") q = clamp(ev.pressure || 0.5, 0, 1);
    else {
      // No real pressure from this device: slower is heavier, as the prototype drew it.
      const d = l ? Math.hypot(p[0] - l[0], p[1] - l[1]) * S.cam.z : 0;
      const speed = d / Math.max(1, ev.timeStamp - a.t);
      a.q = a.q * 0.6 + clamp(0.85 - speed * 0.28, 0.2, 0.85) * 0.4;
      q = a.q;
    }
    a.t = ev.timeStamp;
    const point = [milli(p[0]), milli(p[1]), Math.round(q * 1000) / 1000];
    if (l && l[0] === point[0] && l[1] === point[1] && l[2] === point[2]) return;
    if (a.pts.length >= MAX_SAMPLES) {
      // Over the limit: this stroke ends and the next begins at the same place. Nothing shows.
      bake(a.pts.length);
      a.done.push(finishStroke(a));
      a.pts = [l]; a.baked = 0;
    }
    a.pts.push(point);
  }
  function bake(upTo) {
    const a = act, col = colors();
    const ctx = S.bake.getContext("2d"), { x, y, z } = S.cam, d = S.dpr;
    ctx.setTransform(d * z, 0, 0, d * z, d * x, d * y);
    ctx.fillStyle = ctx.strokeStyle = a.flat ? col.hi : col[a.color] || col.ink;
    strokeInto(ctx, a.pts.slice(Math.max(0, a.baked - 2), upTo), a);
    a.baked = upTo;
  }
  const finishStroke = a => ({ id: uid(), kind: a.kind, color: a.color, width: a.width, points: a.pts });

  function moveInk(e) {
    const list = e.getCoalescedEvents?.() || [];
    for (const ev of list.length ? list : [e]) addSample(ev);
    const pred = e.getPredictedEvents?.() || [];
    const last = act.pts[act.pts.length - 1];
    act.predicted = pred.map(ev => { const p = toWorld(ev.clientX, ev.clientY); return [p[0], p[1], last[2]]; });
    if (act.pts.length - act.baked >= BAKE_EVERY + 6) bake(act.pts.length - 6);
    invalidate(false);
  }

  function eraseTo(p) {
    const R = ERASE_R / S.cam.z, a = act.last, steps = Math.max(1, Math.ceil(Math.hypot(p[0] - a[0], p[1] - a[1]) / (R / 2)));
    let changed = false;
    for (let i = 1; i <= steps; i++) changed = eraseAt(a[0] + (p[0] - a[0]) * i / steps, a[1] + (p[1] - a[1]) * i / steps, R) || changed;
    act.last = p;
    if (changed) invalidate();
  }
  function eraseAt(x, y, R) {
    let changed = false;
    const out = [];
    for (const s of S.ink) {
      const r = R + (s.kind === "highlighter" ? (s.width || HI_W) / 2 : 0), b = entry(s).box;
      if (x < b[0] - r || x > b[2] + r || y < b[1] - r || y > b[3] + r) { out.push(s); continue; }
      if (s.kind === "arrow") {
        if (segDist(x, y, s.points[0], s.points[s.points.length - 1]) < r) changed = true; else out.push(s);
        continue;
      }
      let cut = false, run = [];
      const pieces = [], flush = () => { if (run.length > 1) pieces.push(run); run = []; };
      s.points.forEach((q, i) => {
        if (Math.hypot(q[0] - x, q[1] - y) < r) { cut = true; flush(); return; }
        // A sparse stroke can pass through the eraser between two samples.
        if (run.length && i && segDist(x, y, s.points[i - 1], q) < r) { cut = true; flush(); }
        run.push(q);
      });
      flush();
      if (!cut) { out.push(s); continue; }
      changed = true;
      for (const points of pieces) out.push({ ...s, id: uid(), points });
    }
    if (changed) S.ink = out;
    return changed;
  }
  function finishErase() {
    const before = new Set(act.orig), after = new Set(S.ink);
    const add = S.ink.filter(s => !before.has(s)), remove = act.orig.filter(s => !after.has(s)).map(s => s.id);
    if (add.length || remove.length) {
      if (!commit({ op: "change_ink", page: S.pageId, add, remove })) return;
    }
    hintState();
  }

  function arrowAt(cx, cy) {
    const [x, y] = toWorld(cx, cy), r = 7 / S.cam.z;
    for (let i = S.ink.length - 1; i >= 0; i--) {
      const s = S.ink[i];
      if (s.kind === "arrow" && s.points.length > 1 && segDist(x, y, s.points[0], s.points[s.points.length - 1]) < r) return s;
    }
    return null;
  }
  function arrowEnd(s, cx, cy) {
    const [x, y] = toWorld(cx, cy), r = 10 / S.cam.z, end = s.points.length - 1;
    if (Math.hypot(s.points[0][0] - x, s.points[0][1] - y) < r) return 0;
    if (Math.hypot(s.points[end][0] - x, s.points[end][1] - y) < r) return end;
    return -1;
  }
  function replaceStroke(next) { S.ink = S.ink.map(s => (s.id === next.id ? next : s)); }

  function downSelect(e, type) {
    const target = e.target, p = toWorld(e.clientX, e.clientY);
    const rz = target.closest(".rz");
    if (rz) {
      e.preventDefault(); capture(e); blurEditor();
      const ci = rz.closest(".ci");
      if (ci) {
        const it = itemOf(ci.dataset.id);
        act = { k: "resize", id: e.pointerId, type, it, node: ci, p, w: it.width, h: it.height };
      } else act = { k: "resize", id: e.pointerId, type, it: null, node: S.note, p, w: S.noteData.width };
      return;
    }
    if (target.closest(".grip")) {
      e.preventDefault(); capture(e); blurEditor(); select(null);
      S.cv.classList.add("grabbing");
      S.note.classList.add("lifted");
      act = { k: "note", id: e.pointerId, type, p, x: S.noteData.x, y: S.noteData.y };
      return;
    }
    const ci = target.closest(".ci");
    if (ci?.classList.contains("editing")) return; // A click inside the text being edited stays native.
    const inNote = !!target.closest(".note");
    if (!inNote) {
      const sel = S.sel?.arrow && S.ink.find(s => s.id === S.sel.id);
      const end = sel ? arrowEnd(sel, e.clientX, e.clientY) : -1;
      const hit = end >= 0 ? sel : arrowAt(e.clientX, e.clientY);
      if (hit) {
        e.preventDefault(); capture(e); blurEditor();
        select({ arrow: true, id: hit.id });
        act = { k: "arrow-edit", id: e.pointerId, type, orig: hit, end, p, moved: false };
        return;
      }
    }
    if (ci) {
      const was = S.sel?.id === ci.dataset.id;
      endEdit();
      // The item may have been an empty text box that endEdit removed.
      const it = itemOf(ci.dataset.id); if (!it) return;
      select({ id: it.id });
      act = { k: "drag", id: e.pointerId, type, it, node: ci, p, x: it.x, y: it.y, cx: e.clientX, cy: e.clientY, moved: false, was };
      return;
    }
    if (inNote) {
      endEdit();
      if (S.sel) select(null);
      // With a finger, dragging over the note card pans. A tap still reaches the editor.
      if (type === "touch") act = { k: "pan", id: e.pointerId, type, cx: e.clientX, cy: e.clientY, x: S.cam.x, y: S.cam.y, moved: false, soft: true };
      return;
    }
    e.preventDefault(); blurEditor();
    startPan(e);
  }

  function onMove(e) {
    if (!S) return;
    if (touches.has(e.pointerId)) touches.set(e.pointerId, [e.clientX, e.clientY]);
    if (act?.k === "pinch") { if (e.pointerType === "touch") movePinch(); return; }
    if (tool === "eraser" && e.pointerType !== "touch") moveRing(e);
    if (!act) { if (tool === "select" && e.pointerType === "mouse") S.cv.classList.toggle("over-arrow", !e.target.closest(".note") && !!arrowAt(e.clientX, e.clientY)); return; }
    if (e.pointerId !== act.id) return;
    const p = () => toWorld(e.clientX, e.clientY);
    switch (act.k) {
      case "pan": {
        const dx = e.clientX - act.cx, dy = e.clientY - act.cy;
        if (act.soft && !act.moved) {
          if (Math.hypot(dx, dy) < DRAG_PX * 2) return;
          capture(e); blurEditor(); getSelection()?.removeAllRanges();
          S.cv.classList.add("moving");
        }
        act.moved = act.moved || Math.hypot(dx, dy) >= DRAG_PX;
        S.cam.x = act.x + dx; S.cam.y = act.y + dy;
        applyCam();
        break;
      }
      case "ink": moveInk(e); break;
      case "erase": eraseTo(p()); break;
      case "arrow": act.b = p(); invalidate(false); break;
      case "place":
        if (!act.moved && Math.hypot(e.clientX - act.cx, e.clientY - act.cy) >= DRAG_PX) act.moved = true;
        sizePlace(p());
        break;
      case "drag": {
        if (!act.moved) {
          if (Math.hypot(e.clientX - act.cx, e.clientY - act.cy) < DRAG_PX) return;
          act.moved = true; capture(e);
          S.cv.classList.add("grabbing");
          act.node.classList.add("lifted");
        }
        const q = p();
        act.it.x = safe(act.x + q[0] - act.p[0]); act.it.y = safe(act.y + q[1] - act.p[1]);
        placeItem(act.node, act.it);
        break;
      }
      case "note": {
        const q = p(), n = S.noteData;
        n.x = safe(act.x + q[0] - act.p[0]); n.y = safe(act.y + q[1] - act.p[1]);
        placeNote();
        break;
      }
      case "resize": {
        const q = p();
        if (act.it) {
          act.it.width = clamp(act.w + q[0] - act.p[0], MIN_W, LIMIT);
          act.it.height = clamp(act.h + q[1] - act.p[1], MIN_H, LIMIT);
          placeItem(act.node, act.it);
        } else {
          S.noteData.width = clamp(act.w + q[0] - act.p[0], NOTE_MIN, LIMIT);
          placeNote();
        }
        break;
      }
      case "arrow-edit": {
        const q = p(), o = act.orig, dx = q[0] - act.p[0], dy = q[1] - act.p[1];
        if (!act.moved && Math.hypot(dx, dy) * S.cam.z < DRAG_PX) return;
        act.moved = true;
        const points = act.end >= 0
          ? o.points.map((pt, i) => (i === act.end ? [milli(q[0]), milli(q[1]), pt[2]] : pt))
          : o.points.map(pt => [milli(pt[0] + dx), milli(pt[1] + dy), pt[2]]);
        replaceStroke({ ...o, points });
        invalidate();
        break;
      }
    }
  }

  function moveRing(e) {
    const r = S.rect, st = S.ring.style;
    S.ring.hidden = e.target.closest(UI) != null;
    st.transform = `translate(${e.clientX - r.left - ERASE_R}px,${e.clientY - r.top - ERASE_R}px)`;
  }

  function onUp(e) {
    if (!S) return;
    const cancelled = e.type === "pointercancel";
    touches.delete(e.pointerId);
    if (e.pointerType === "pen") { penDown = false; penAt = performance.now(); }
    if (act?.k === "pinch") { if (touches.size < 2) { act = null; S.cv.classList.remove("moving"); saveCam(); } return; }
    if (!act || e.pointerId !== act.id) return;
    if (cancelled && !(act.k === "ink" && act.type !== "touch")) { abort(); return; }
    finish(e);
  }

  function clearGesture() {
    act = null;
    S.bake = null;
    S.wet.classList.remove("hi");
    S.cv.classList.remove("grabbing", "moving");
    S.note.classList.remove("lifted");
    for (const n of S.nodes.values()) n.classList.remove("lifted");
  }
  function finish(e) {
    const a = act;
    switch (a.k) {
      case "pan":
        clearGesture();
        if (!a.moved && !a.soft && S.sel) select(null);
        saveCam();
        return;
      case "ink": {
        const add = [...a.done, finishStroke(a)];
        // Dry first, then clear wet in the same task, so there is no frame without the stroke.
        if (commit({ op: "change_ink", page: S.pageId, add, remove: [] })) S.ink = [...S.ink, ...add];
        clearGesture(); hintState(); paintNow();
        return;
      }
      case "erase":
        finishErase();
        clearGesture(); paintNow();
        return;
      case "arrow": {
        const long = Math.hypot(a.b[0] - a.a[0], a.b[1] - a.a[1]) >= 8;
        if (long) {
          const s = { id: uid(), kind: "arrow", color: "", width: ARROW_W, points: [[milli(a.a[0]), milli(a.a[1]), 0.5], [milli(a.b[0]), milli(a.b[1]), 0.5]] };
          if (commit({ op: "change_ink", page: S.pageId, add: [s], remove: [] })) S.ink = [...S.ink, s];
        }
        clearGesture(); hintState(); paintNow();
        return;
      }
      case "place": {
        const { it, node } = a;
        node.remove();
        clearGesture();
        S.items.push(it);
        if (it.kind === "text") S.drafts.add(it.id); else putItem(it);
        mountItem(it);
        setTool("select");
        select({ id: it.id });
        startEdit(it.id);
        return;
      }
      case "drag":
        clearGesture();
        if (a.moved) { if (S.drafts.has(a.it.id)) return; putItem(a.it); return; }
        if (a.was && e.type === "pointerup") startEdit(a.it.id, e.clientX, e.clientY);
        return;
      case "note":
        clearGesture();
        if (S.noteData.x !== a.x || S.noteData.y !== a.y) commit({ op: "set_note", page: S.pageId, note: { ...S.noteData } });
        return;
      case "resize":
        clearGesture();
        if (a.it) { if (!S.drafts.has(a.it.id)) putItem(a.it); }
        else if (S.noteData.width !== a.w) commit({ op: "set_note", page: S.pageId, note: { ...S.noteData } });
        return;
      case "arrow-edit": {
        clearGesture();
        if (!a.moved) { paintNow(); return; }
        const now = S.ink.find(s => s.id === a.orig.id), next = { ...now, id: uid() };
        if (commit({ op: "change_ink", page: S.pageId, add: [next], remove: [a.orig.id] })) {
          S.ink = S.ink.map(s => (s === now ? next : s));
          select({ arrow: true, id: next.id });
        }
        paintNow();
        return;
      }
    }
  }
  /** Ends a gesture with nothing written and everything put back. */
  function abort() {
    const a = act; if (!a || !S) { act = null; return; }
    if (a.k === "erase") S.ink = a.orig;
    if (a.k === "place") a.node.remove();
    if (a.k === "drag") { a.it.x = a.x; a.it.y = a.y; placeItem(a.node, a.it); }
    if (a.k === "note") { S.noteData.x = a.x; S.noteData.y = a.y; placeNote(); }
    if (a.k === "resize") {
      if (a.it) { a.it.width = a.w; a.it.height = a.h; placeItem(a.node, a.it); }
      else { S.noteData.width = a.w; placeNote(); }
    }
    if (a.k === "arrow-edit") replaceStroke(a.orig);
    if (a.k === "pan" && !a.soft) { S.cam.x = a.x; S.cam.y = a.y; applyCam(); }
    clearGesture();
    paintNow();
  }

  /* ----- Keyboard ----- */
  const KEYS = Object.fromEntries(TOOLS.filter(Boolean).flatMap(r => [[r[3], r[0]], ["Key" + r[3].toUpperCase(), r[0]]]));
  let nudge = null;
  document.addEventListener("keydown", e => {
    lastKeyAt = performance.now();
    // Undo must see text still waiting to be written.
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z" && S?.editing) commitText(S.editing);
  }, true);
  document.addEventListener("keydown", e => {
    if (!isOpen() || e.isComposing || e.defaultPrevented) return;
    if (document.querySelector(".modalbg,.pop")) return;
    if (e.key === "Escape" && typing()) { document.activeElement.blur(); return; }
    if (typing() || e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "Escape") {
      if (act) abort();
      else if (tool !== "select") setTool("select");
      else select(null);
      return;
    }
    if (e.key === " " && !document.activeElement?.matches?.("button")) { e.preventDefault(); S.space = true; S.cv.classList.add("spacepan"); return; }
    if ((e.key === "Delete" || e.key === "Backspace") && S.sel) { e.preventDefault(); deleteSelected(); return; }
    if (e.key === "Enter" && S.sel && !S.sel.arrow) { e.preventDefault(); startEdit(S.sel.id); return; }
    if (e.key.startsWith("Arrow")) { arrowKey(e); return; }
    if (e.key === "+" || e.key === "=") { zoomBy(1.25); return; }
    if (e.key === "-") { zoomBy(0.8); return; }
    if (e.key === "0") { zoomTo(1); return; }
    const next = e.shiftKey ? null : KEYS[e.key.toLowerCase()] || KEYS[e.code];
    if (next) { e.preventDefault(); setTool(next); }
  });
  function arrowKey(e) {
    e.preventDefault();
    const step = e.shiftKey ? 10 : 1, dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0, dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
    const it = S.sel && !S.sel.arrow ? itemOf(S.sel.id) : null;
    if (it) {
      nudge = it;
      it.x = safe(it.x + dx); it.y = safe(it.y + dy);
      placeItem(S.nodes.get(it.id), it);
    } else {
      S.cam.x -= dx * 4; S.cam.y -= dy * 4;
      applyCam(); saveCam();
    }
  }
  document.addEventListener("keyup", e => {
    if (e.key === " " && S) { S.space = false; S.cv.classList.remove("spacepan"); }
    if (e.key.startsWith("Arrow") && nudge) { const it = nudge; nudge = null; if (S && itemOf(it.id) === it && !S.drafts.has(it.id)) putItem(it); }
  });
  document.addEventListener("focusin", e => {
    if (!isOpen() || pointerHeld) return;
    const ci = e.target.closest?.(".ci");
    if (ci && e.target === ci && tool === "select" && S.sel?.id !== ci.dataset.id) select({ id: ci.dataset.id });
  });

  /* ----- Keep the caret on screen while typing in the note card ----- */
  document.addEventListener("pointerdown", () => { pointerHeld = true; }, true);
  document.addEventListener("pointerup", () => { pointerHeld = false; }, true);
  document.addEventListener("pointercancel", () => { pointerHeld = false; }, true);
  document.addEventListener("selectionchange", () => {
    if (!isOpen() || act || pointerHeld || performance.now() - lastKeyAt > 300) return;
    const a = document.activeElement;
    if (!a?.isContentEditable || !S.cv.contains(a)) return;
    const s = getSelection(); if (!s.rangeCount) return;
    const box = s.getRangeAt(0).getClientRects()[0] || a.getBoundingClientRect();
    const r = S.rect = S.cv.getBoundingClientRect(), m = 48;
    let dx = 0, dy = 0;
    if (box.bottom > r.bottom - m) dy = r.bottom - m - box.bottom; else if (box.top < r.top + m) dy = r.top + m - box.top;
    if (box.right > r.right - m) dx = r.right - m - box.right; else if (box.left < r.left + m) dx = r.left + m - box.left;
    if (dx || dy) { S.cam.x += dx; S.cam.y += dy; applyCam(); saveCam(); }
  });
}
