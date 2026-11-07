// Search through Ctrl K, the knowledge graph view, and the side panel's related pages and local
// graph. Everything is read from the engine, which derives it from the notes, so nothing here can
// disagree with them. Nothing is written except by an explicit Link. Document text only ever
// reaches the page through textContent or SVG text content.
import { I18N } from "../data.js";

let api;
const SVG = "http://www.w3.org/2000/svg";
const MAX_NODES = 2000, MAX_EDGES = 10000, SYNC_LAYOUT_NODES = 200, LOCAL_NODES = 14;
const REDUCED = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

// Drafts for the strings this feature adds, not yet native-reviewed (I18N-05).
const STRINGS = {
  "zh-Hans": { "Similar pages are found by shared words, not by meaning.": "相似页面按共同词语判断，而不是按含义。", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "还没有关联。用 [[ 链接页面、添加 #标签或数据库后，关联就会显示在这里。", "Everything is filtered out. Turn a filter back on.": "全部已被筛选隐藏。请重新打开一个筛选项。", "The graph could not be built.": "无法生成图谱。", "Try again": "重试", "Show as list": "以列表显示", "Show as graph": "以图谱显示", "Filters": "筛选", "Zoom in": "放大", "Zoom out": "缩小", "Fit": "适应", "Arranging {n} nodes": "正在排列 {n} 个节点", "Showing {n} of {m} nodes, the most connected.": "显示连接最多的 {n} 个节点，共 {m} 个。", "Select a node to see what it connects to.": "选择一个节点以查看它的关联。", "Similar, computed": "相似（计算得出）", "Open its page": "打开所在页面", "Clear selection": "取消选择", "and {n} more": "还有 {n} 个", "Page": "页面", "Tag": "标签", "Database row": "数据库行", "Person": "人物", "links to": "链接到", "linked from": "被链接自", "embeds": "嵌入", "embedded in": "被嵌入于", "has subpage": "子页面", "subpage of": "上级页面", "tagged": "标签", "tag on": "使用此标签的页面", "contains": "包含", "row in": "所在页面", "involves": "涉及", "owner or author of": "负责或撰写", "Links to removed pages: {n}": "指向已删除页面的链接：{n}", "Search failed.": "搜索失败。", "Pages and items connected to this page": "与此页面相关的页面和条目", "Knowledge graph of pages, tags, rows and people": "页面、标签、行和人物的知识图谱", "Link to {page}": "链接到 {page}" },
  ja: { "Similar pages are found by shared words, not by meaning.": "類似ページは意味ではなく、共通の語句で判断しています。", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "まだつながりがありません。[[ でページをリンクするか、#タグやデータベースを追加すると、ここに表示されます。", "Everything is filtered out. Turn a filter back on.": "すべて絞り込みで非表示です。フィルターを戻してください。", "The graph could not be built.": "グラフを作成できませんでした。", "Try again": "再試行", "Show as list": "リストで表示", "Show as graph": "グラフで表示", "Filters": "フィルター", "Zoom in": "拡大", "Zoom out": "縮小", "Fit": "全体表示", "Arranging {n} nodes": "{n} 個のノードを配置中", "Showing {n} of {m} nodes, the most connected.": "つながりの多い {n} 個を表示中（全 {m} 個）。", "Select a node to see what it connects to.": "ノードを選ぶと、つながりを表示します。", "Similar, computed": "類似（計算による）", "Open its page": "そのページを開く", "Clear selection": "選択を解除", "and {n} more": "他 {n} 件", "Page": "ページ", "Tag": "タグ", "Database row": "データベースの行", "Person": "人物", "links to": "リンク先", "linked from": "リンク元", "embeds": "埋め込み先", "embedded in": "埋め込み元", "has subpage": "サブページ", "subpage of": "親ページ", "tagged": "タグ", "tag on": "このタグのページ", "contains": "含む", "row in": "行のあるページ", "involves": "関係者", "owner or author of": "担当または作成", "Links to removed pages: {n}": "削除されたページへのリンク: {n}", "Search failed.": "検索に失敗しました。", "Pages and items connected to this page": "このページにつながるページと項目", "Knowledge graph of pages, tags, rows and people": "ページ、タグ、行、人物のナレッジグラフ", "Link to {page}": "{page} にリンク" },
  es: { "Similar pages are found by shared words, not by meaning.": "Las páginas similares se detectan por palabras compartidas, no por su significado.", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "Aún no hay conexiones. Enlaza páginas con [[, añade #etiquetas o una base de datos, y aparecerán aquí.", "Everything is filtered out. Turn a filter back on.": "Todo está filtrado. Vuelve a activar un filtro.", "The graph could not be built.": "No se pudo crear el grafo.", "Try again": "Reintentar", "Show as list": "Ver como lista", "Show as graph": "Ver como grafo", "Filters": "Filtros", "Zoom in": "Acercar", "Zoom out": "Alejar", "Fit": "Ajustar", "Arranging {n} nodes": "Colocando {n} nodos", "Showing {n} of {m} nodes, the most connected.": "Se muestran {n} de {m} nodos, los más conectados.", "Select a node to see what it connects to.": "Selecciona un nodo para ver sus conexiones.", "Similar, computed": "Similar, calculado", "Open its page": "Abrir su página", "Clear selection": "Quitar selección", "and {n} more": "y {n} más", "Page": "Página", "Tag": "Etiqueta", "Database row": "Fila de base de datos", "Person": "Persona", "links to": "enlaza a", "linked from": "enlazada desde", "embeds": "inserta", "embedded in": "insertada en", "has subpage": "tiene subpágina", "subpage of": "subpágina de", "tagged": "etiquetada", "tag on": "etiqueta en", "contains": "contiene", "row in": "fila en", "involves": "implica a", "owner or author of": "responsable o autor de", "Links to removed pages: {n}": "Enlaces a páginas eliminadas: {n}", "Search failed.": "La búsqueda falló.", "Pages and items connected to this page": "Páginas y elementos conectados a esta página", "Knowledge graph of pages, tags, rows and people": "Grafo de páginas, etiquetas, filas y personas", "Link to {page}": "Enlazar a {page}" },
  ar: { "Similar pages are found by shared words, not by meaning.": "تُحدَّد الصفحات المتشابهة بالكلمات المشتركة، لا بالمعنى.", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "لا توجد روابط بعد. اربط الصفحات باستخدام [[ أو أضف #وسومًا أو قاعدة بيانات، وستظهر الروابط هنا.", "Everything is filtered out. Turn a filter back on.": "كل شيء مُخفى بالتصفية. أعد تشغيل أحد عوامل التصفية.", "The graph could not be built.": "تعذر إنشاء الرسم البياني.", "Try again": "إعادة المحاولة", "Show as list": "عرض كقائمة", "Show as graph": "عرض كرسم بياني", "Filters": "التصفية", "Zoom in": "تكبير", "Zoom out": "تصغير", "Fit": "ملاءمة", "Arranging {n} nodes": "جارٍ ترتيب {n} عقدة", "Showing {n} of {m} nodes, the most connected.": "عرض {n} من {m} عقدة، الأكثر ارتباطًا.", "Select a node to see what it connects to.": "حدد عقدة لرؤية ما ترتبط به.", "Similar, computed": "مشابه، محسوب", "Open its page": "فتح صفحتها", "Clear selection": "إلغاء التحديد", "and {n} more": "و{n} أخرى", "Page": "صفحة", "Tag": "وسم", "Database row": "صف قاعدة بيانات", "Person": "شخص", "links to": "يرتبط بـ", "linked from": "مرتبط من", "embeds": "يضمّن", "embedded in": "مضمّن في", "has subpage": "له صفحة فرعية", "subpage of": "صفحة فرعية من", "tagged": "موسوم", "tag on": "وسم على", "contains": "يحتوي", "row in": "صف في", "involves": "يخص", "owner or author of": "مالك أو مؤلف", "Links to removed pages: {n}": "روابط إلى صفحات محذوفة: {n}", "Search failed.": "فشل البحث.", "Pages and items connected to this page": "الصفحات والعناصر المرتبطة بهذه الصفحة", "Knowledge graph of pages, tags, rows and people": "رسم بياني معرفي للصفحات والوسوم والصفوف والأشخاص", "Link to {page}": "ربط بـ {page}" },
  hi: { "Similar pages are found by shared words, not by meaning.": "समान पेज साझा शब्दों से पहचाने जाते हैं, अर्थ से नहीं।", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "अभी कोई संबंध नहीं है। [[ से पेज जोड़ें, #टैग या डेटाबेस जोड़ें, और संबंध यहाँ दिखेंगे।", "Everything is filtered out. Turn a filter back on.": "सब कुछ फ़िल्टर से छिपा है। कोई फ़िल्टर फिर चालू करें।", "The graph could not be built.": "ग्राफ़ नहीं बन सका।", "Try again": "फिर कोशिश करें", "Show as list": "सूची में दिखाएँ", "Show as graph": "ग्राफ़ में दिखाएँ", "Filters": "फ़िल्टर", "Zoom in": "बड़ा करें", "Zoom out": "छोटा करें", "Fit": "फ़िट करें", "Arranging {n} nodes": "{n} नोड व्यवस्थित हो रहे हैं", "Showing {n} of {m} nodes, the most connected.": "{m} में से {n} नोड दिख रहे हैं, सबसे अधिक जुड़े हुए।", "Select a node to see what it connects to.": "संबंध देखने के लिए कोई नोड चुनें।", "Similar, computed": "समान, गणना से", "Open its page": "उसका पेज खोलें", "Clear selection": "चयन हटाएँ", "and {n} more": "और {n}", "Page": "पेज", "Tag": "टैग", "Database row": "डेटाबेस पंक्ति", "Person": "व्यक्ति", "links to": "लिंक करता है", "linked from": "से लिंक", "embeds": "एम्बेड करता है", "embedded in": "में एम्बेड", "has subpage": "उप-पेज", "subpage of": "का उप-पेज", "tagged": "टैग", "tag on": "टैग इन पर", "contains": "में शामिल", "row in": "पंक्ति इसमें", "involves": "संबंधित", "owner or author of": "स्वामी या लेखक", "Links to removed pages: {n}": "हटाए गए पेजों के लिंक: {n}", "Search failed.": "खोज विफल रही।", "Pages and items connected to this page": "इस पेज से जुड़े पेज और आइटम", "Knowledge graph of pages, tags, rows and people": "पेज, टैग, पंक्ति और लोगों का नॉलेज ग्राफ़", "Link to {page}": "{page} से लिंक करें" },
};
for (const [lang, table] of Object.entries(STRINGS)) if (I18N[lang]) for (const [k, v] of Object.entries(table)) I18N[lang][k] ??= v;

const svgEl = (tag, attrs = {}) => { const n = document.createElementNS(SVG, tag); for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v); return n; };
const num = n => new Intl.NumberFormat(api.lang).format(n);
const fill = (s, map) => s.replace(/\{(\w+)\}/g, (_, k) => map[k] ?? "");
const graphemes = text => { try { return [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(text)].map(x => x.segment); } catch { return [...text]; } };
const clip = (text, max) => { const g = graphemes(text); return g.length > max ? g.slice(0, max - 1).join("") + "…" : text; };
const match = (hay, q) => hay.normalize("NFC").toLocaleLowerCase().includes(q);
const labelOf = n => n.label || api.t("Untitled");

/* ---------- The derived graph, cached by the engine's revision ---------- */
let cache = { rev: -1, graph: null };
function currentGraph() {
  const rev = api.engine.revision();
  if (cache.rev !== rev || !cache.graph) {
    const g = JSON.parse(api.engine.graph());
    const nodes = new Map(g.nodes.map(n => [n.id, n]));
    const around = new Map(g.nodes.map(n => [n.id, []]));
    for (const e of g.edges) { around.get(e.from)?.push({ id: e.to, e, forward: true }); around.get(e.to)?.push({ id: e.from, e, forward: false }); }
    cache = { rev, graph: { nodes, edges: g.edges, around } };
  }
  return cache.graph;
}
const RELATIONS = { link: ["links to", "linked from"], embed: ["embeds", "embedded in"], child: ["has subpage", "subpage of"], tag: ["tagged", "tag on"], row: ["contains", "row in"], owner: ["involves", "owner or author of"] };
const KIND_LABEL = { page: "Page", tag: "Tag", row: "Database row", person: "Person" };
const percent = score => new Intl.NumberFormat(api.lang, { style: "percent" }).format(score / 1000);

/* ---------- Layout: deterministic, because start positions come from a spiral and never from chance ---------- */
const SPRINGS = { child: [110, .05], link: [170, .03], embed: [150, .03], tag: [90, .04], row: [64, .08], owner: [76, .05], similar: [240, .008] };
function settleStep(nodes, edges, alpha) {
  for (let i = 0; i < nodes.length; i++) {
    const a = nodes[i];
    for (let j = i + 1; j < nodes.length; j++) {
      const b = nodes[j]; let dx = a.x - b.x, dy = a.y - b.y; const d2 = dx * dx + dy * dy || 1;
      if (d2 > 500000) continue;
      const f = 3600 / d2 * alpha, d = Math.sqrt(d2); dx /= d; dy /= d;
      a.vx += dx * f; a.vy += dy * f; b.vx -= dx * f; b.vy -= dy * f;
    }
  }
  for (const e of edges) {
    const a = e.na, b = e.nb, [len, s] = SPRINGS[e.kind] || [90, .04];
    const dx = b.x - a.x, dy = b.y - a.y, d = Math.hypot(dx, dy) || 1, f = (d - len) * s * alpha;
    a.vx += dx / d * f; a.vy += dy / d * f; b.vx -= dx / d * f; b.vy -= dy / d * f;
  }
  for (const n of nodes) {
    n.vx -= n.x * .008 * alpha; n.vy -= n.y * .008 * alpha; n.vx *= .82; n.vy *= .82;
    if (!n.pin) { n.x += n.vx; n.y += n.vy; }
  }
}
const iterationsFor = n => Math.max(40, Math.min(420, Math.floor(4e7 / Math.max(1, n * n))));

/* ---------- The whole graph ---------- */
const view = {
  filter: { page: true, tag: true, row: true, person: true, similar: true },
  positions: new Map(), camera: null, selected: null, hover: null, query: "", list: false, filtersOpen: null, cancel: null,
};

function pickShown(g) {
  const wanted = [...g.nodes.values()].filter(n => view.filter[n.kind]);
  const made = new Map();
  for (const e of g.edges) if (e.kind !== "similar") { made.set(e.from, (made.get(e.from) || 0) + 1); made.set(e.to, (made.get(e.to) || 0) + 1); }
  let shown = wanted;
  if (wanted.length > MAX_NODES) shown = [...wanted].sort((a, b) => (made.get(b.id) || 0) - (made.get(a.id) || 0) || (a.id < b.id ? -1 : 1)).slice(0, MAX_NODES);
  const ids = new Set(shown.map(n => n.id));
  const edges = g.edges.filter(e => ids.has(e.from) && ids.has(e.to) && (e.kind !== "similar" || view.filter.similar)).slice(0, MAX_EDGES);
  return { shown, edges, total: wanted.length };
}

function renderGraph(content) {
  view.cancel?.(); view.cancel = null; view.hover = null;
  let g;
  try { g = currentGraph(); } catch (err) {
    console.warn("graph", err);
    content.replaceChildren(api.el("div", { class: "kg" }, api.el("div", { class: "kgempty", role: "alert" }, api.el("p", { text: api.t("The graph could not be built.") }), api.el("button", { class: "btn", text: api.t("Try again"), onclick: () => { cache.rev = -1; renderGraph(content); } }))));
    return;
  }
  if (view.selected && !g.nodes.has(view.selected)) view.selected = null;
  const { shown, edges: shownEdges, total } = pickShown(g);
  const nodes = shown.map((n, i) => {
    const p = view.positions.get(n.id), a = i * 2.399963, r = 40 * Math.sqrt(i + 1);
    return { ...n, x: p?.x ?? r * Math.cos(a), y: p?.y ?? r * Math.sin(a), vx: 0, vy: 0, pin: !!p?.pin };
  });
  const byId = new Map(nodes.map(n => [n.id, n]));
  const edges = shownEdges.map(e => ({ ...e, na: byId.get(e.from), nb: byId.get(e.to) }));
  const degree = new Map();
  for (const e of edges) { degree.set(e.from, (degree.get(e.from) || 0) + 1); degree.set(e.to, (degree.get(e.to) || 0) + 1); }

  const box = api.el("div", { class: "gv" });
  const side = api.el("aside", { class: "kgside", "aria-live": "polite", "aria-label": api.t("Details") });
  const root = api.el("div", { class: "kg" }, box, side);

  /* The controls */
  const count = k => [...g.nodes.values()].filter(n => n.kind === k).length;
  const similarCount = g.edges.filter(e => e.kind === "similar").length;
  const check = (key, label, n, cls = "") => api.el("label", { class: cls }, api.el("input", { type: "checkbox", id: "gf-" + key, ...(view.filter[key] ? { checked: true } : {}), onchange: e => { view.filter[key] = e.target.checked; renderGraph(content); } }), api.el("span", { text: label }), api.el("span", { class: "k", text: num(n) }));
  const missNote = api.el("p", { class: "kgmiss", role: "status", hidden: true, text: api.t("No matches") });
  const findBox = api.el("input", { type: "search", id: "gq", placeholder: api.t("Find in graph"), "aria-label": api.t("Find in graph"), value: view.query,
    oninput: e => { view.query = e.target.value.trim().normalize("NFC").toLocaleLowerCase(); paintFocus(); },
    onkeydown: e => {
      if (e.key !== "Enter") return;
      e.preventDefault();
      const hit = view.query && nodes.find(n => match(labelOf(n), view.query));
      if (hit) { select(hit.id); centerOn(hit); }
    } });
  const opened = view.filtersOpen ?? !matchMedia("(max-width: 760px)").matches;
  const body = api.el("div", { class: "gctlbody" },
    check("page", api.t("Pages"), count("page")), check("tag", api.t("Tags"), count("tag")), check("row", api.t("Database rows"), count("row")), check("person", api.t("People"), count("person")),
    check("similar", api.t("Similar pages"), similarCount, "gsimilar"),
    api.el("p", { text: api.t("Dashed lines are computed similarity. Solid lines come from your links, tags and data.") }),
    api.el("p", { text: api.t("Similar pages are found by shared words, not by meaning.") }),
    total > shown.length ? api.el("p", { class: "kgshowing", text: fill(api.t("Showing {n} of {m} nodes, the most connected."), { n: num(shown.length), m: num(total) }) }) : null,
    api.el("button", { class: "textlink kgaslist", text: api.t(view.list ? "Show as graph" : "Show as list"), onclick: () => { view.list = !view.list; renderGraph(content); } }));
  body.hidden = !opened;
  const toggle = api.el("button", { class: "gtoggle", "aria-expanded": String(opened), text: api.t("Filters"), onclick: () => { body.hidden = !body.hidden; view.filtersOpen = !body.hidden; toggle.setAttribute("aria-expanded", String(!body.hidden)); } });
  const ctl = api.el("div", { class: "gctl" }, findBox, missNote, toggle, body);

  /* The text alternative: every graph view has a list form (QR-5). */
  if (view.list) {
    const list = api.el("div", { class: "kglist", role: "list" });
    if (!nodes.length) list.append(api.el("p", { text: api.t("Everything is filtered out. Turn a filter back on.") }));
    for (const n of nodes) {
      const rels = (g.around.get(n.id) || []).filter(x => byId.has(x.id));
      list.append(api.el("button", { class: "kgrow" + (n.id === view.selected ? " on" : ""), role: "listitem", "data-id": n.id, onclick: () => { view.selected = n.id; renderGraph(content); } },
        api.el("span", { class: "kgrowt", dir: "auto", text: labelOf(n) }), api.el("small", { text: `${api.t(KIND_LABEL[n.kind])}, ${num(rels.length)}` })));
    }
    box.append(list, ctl);
    box.classList.add("listed");
    content.replaceChildren(root);
    paintDetail();
    return;
  }

  /* The drawing */
  const svg = svgEl("svg", { id: "gsvg", role: "group", "aria-label": api.t("Knowledge graph of pages, tags, rows and people") });
  const world = svgEl("g"); svg.append(world);
  const lines = edges.map(e => { const l = svgEl("line", { class: "e-" + e.kind }); if (e.kind === "similar") l.setAttribute("data-score", e.score); world.append(l); return l; });
  const shapes = nodes.map((n, i) => {
    const deg = degree.get(n.id) || 0;
    const r = n.kind === "page" ? 5 + Math.min(6, deg * .6) : n.kind === "row" ? 3.5 : 5;
    const gn = svgEl("g", { class: `n k-${n.kind}`, tabindex: i === 0 ? "0" : "-1", "data-id": n.id, role: "button", "aria-label": `${labelOf(n)}, ${api.t(KIND_LABEL[n.kind])}, ${num(deg)}` });
    if (n.kind === "tag") gn.append(svgEl("rect", { class: "sh", x: -r, y: -r, width: 2 * r, height: 2 * r }));
    else if (n.kind === "person") gn.append(svgEl("path", { class: "sh", d: `M0 ${-r - 1}L${r + 1} 0L0 ${r + 1}L${-r - 1} 0Z` }));
    else gn.append(svgEl("circle", { class: "sh", r }));
    const label = svgEl("text", { x: r + 4, y: 4 }); label.textContent = clip(labelOf(n), 28); gn.append(label);
    world.append(gn);
    return gn;
  });
  const empty = !nodes.length ? "Everything is filtered out. Turn a filter back on." : (g.edges.length ? null : "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.");
  const emptyNote = empty ? api.el("div", { class: "kgempty" }, api.el("p", { text: api.t(empty) })) : null;
  const status = api.el("div", { class: "kgstatus", role: "status", hidden: true });
  const zoomLabel = api.el("span");
  const zoom = api.el("div", { class: "zoom" },
    api.el("button", { "aria-label": api.t("Zoom out"), text: "−", onclick: () => zoomBy(.8) }), zoomLabel,
    api.el("button", { "aria-label": api.t("Zoom in"), text: "+", onclick: () => zoomBy(1.25) }),
    api.el("button", { class: "kgfit", text: api.t("Fit"), onclick: () => fit() }));
  box.append(svg, ctl, zoom, status);
  if (emptyNote) box.append(emptyNote);
  content.replaceChildren(root);

  const draw = () => {
    edges.forEach((e, i) => { const l = lines[i]; l.setAttribute("x1", e.na.x); l.setAttribute("y1", e.na.y); l.setAttribute("x2", e.nb.x); l.setAttribute("y2", e.nb.y); });
    nodes.forEach((n, i) => shapes[i].setAttribute("transform", `translate(${n.x.toFixed(1)} ${n.y.toFixed(1)})`));
  };
  const remember = () => { for (const n of nodes) view.positions.set(n.id, { x: n.x, y: n.y, pin: n.pin }); };
  const applyCamera = () => {
    const c = view.camera;
    world.setAttribute("transform", `translate(${c.x} ${c.y}) scale(${c.z})`);
    svg.classList.toggle("zoomed", c.z >= 1.25);
    zoomLabel.textContent = new Intl.NumberFormat(api.lang, { style: "percent" }).format(c.z);
  };
  const zoomAt = (x, y, z) => {
    const c = view.camera; z = Math.max(.15, Math.min(4, z));
    c.x = x - (x - c.x) * (z / c.z); c.y = y - (y - c.y) * (z / c.z); c.z = z; applyCamera();
  };
  const zoomBy = f => { const r = svg.getBoundingClientRect(); zoomAt(r.width / 2, r.height / 2, view.camera.z * f); };
  function fit() {
    const r = svg.getBoundingClientRect();
    if (!nodes.length || !r.width) { view.camera = { x: r.width / 2, y: r.height / 2, z: 1 }; applyCamera(); return; }
    const xs = nodes.map(n => n.x), ys = nodes.map(n => n.y), minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
    // Keep clear of the controls when there is room beside them. They sit on the inline start.
    const inset = r.width > 480 ? ctl.getBoundingClientRect().width + 24 : 0, rtl = document.documentElement.dir === "rtl", room = r.width - inset;
    const w = maxX - minX + 220, h = maxY - minY + 80, z = Math.max(.15, Math.min(1.4, Math.min(room / w, r.height / h)));
    view.camera = { z, x: (rtl ? 0 : inset) + room / 2 - z * (maxX + minX + 180) / 2 + z * 90, y: r.height / 2 - z * (maxY + minY) / 2 };
    applyCamera();
  }
  function centerOn(n) { const r = svg.getBoundingClientRect(), c = view.camera; c.x = r.width / 2 - n.x * c.z; c.y = r.height / 2 - n.y * c.z; applyCamera(); }
  function paintFocus() {
    const f = view.hover || view.selected;
    const near = f && byId.has(f) ? new Set([f, ...(g.around.get(f) || []).filter(x => x.e.kind !== "similar" || view.filter.similar).map(x => x.id)]) : null;
    svg.classList.toggle("focus", !!near);
    let hits = 0;
    nodes.forEach((n, i) => {
      const miss = !!view.query && !match(labelOf(n), view.query);
      if (view.query && !miss) hits++;
      const c = shapes[i].classList;
      c.toggle("hl", !!near && near.has(n.id)); c.toggle("sel", n.id === view.selected); c.toggle("miss", miss); c.toggle("found", !!view.query && !miss);
    });
    edges.forEach((e, i) => lines[i].classList.toggle("hl", !!near && (e.from === f || e.to === f)));
    missNote.hidden = !view.query || hits > 0;
  }
  function select(id) { view.selected = id; paintFocus(); paintDetail(); }
  view.select = select;
  view.reselect = id => { const n = byId.get(id); select(id); if (n) centerOn(n); };

  /* Pointers: drag a node, pan the background, pinch or wheel to zoom. */
  const pointers = new Map();
  let act = null, pinch = null;
  const toWorld = e => { const r = svg.getBoundingClientRect(), c = view.camera; return [(e.clientX - r.left - c.x) / c.z, (e.clientY - r.top - c.y) / c.z]; };
  svg.addEventListener("wheel", e => {
    e.preventDefault();
    const r = svg.getBoundingClientRect();
    if (e.ctrlKey || e.metaKey || e.deltaMode === 1) zoomAt(e.clientX - r.left, e.clientY - r.top, view.camera.z * Math.exp(-e.deltaY * (e.deltaMode === 1 ? .08 : .006)));
    else { view.camera.x -= e.deltaX; view.camera.y -= e.deltaY; applyCamera(); }
  }, { passive: false });
  svg.addEventListener("pointerdown", e => {
    pointers.set(e.pointerId, [e.clientX, e.clientY]);
    try { svg.setPointerCapture(e.pointerId); } catch { /* a synthetic pointer */ }
    if (pointers.size === 2) { const [a, b] = [...pointers.values()]; pinch = { d: Math.hypot(a[0] - b[0], a[1] - b[1]), z: view.camera.z }; act = null; return; }
    const gn = e.target.closest?.(".n");
    if (gn) act = { n: byId.get(gn.dataset.id), moved: false, sx: e.clientX, sy: e.clientY };
    else { svg.classList.add("grabbing"); act = { pan: true, x: e.clientX, y: e.clientY, vx: view.camera.x, vy: view.camera.y, moved: false }; }
  });
  svg.addEventListener("pointermove", e => {
    if (pointers.has(e.pointerId)) pointers.set(e.pointerId, [e.clientX, e.clientY]);
    if (pinch && pointers.size === 2) {
      const [a, b] = [...pointers.values()], r = svg.getBoundingClientRect();
      zoomAt((a[0] + b[0]) / 2 - r.left, (a[1] + b[1]) / 2 - r.top, pinch.z * Math.hypot(a[0] - b[0], a[1] - b[1]) / (pinch.d || 1));
      return;
    }
    if (!act) { const id = e.target.closest?.(".n")?.dataset.id || null; if (id !== view.hover) { view.hover = id; paintFocus(); } return; }
    if (act.pan) {
      if (Math.hypot(e.clientX - act.x, e.clientY - act.y) > 3) act.moved = true;
      view.camera.x = act.vx + e.clientX - act.x; view.camera.y = act.vy + e.clientY - act.y; applyCamera();
      return;
    }
    if (!act.moved && Math.hypot(e.clientX - act.sx, e.clientY - act.sy) < 3) return;
    act.moved = true;
    const [x, y] = toWorld(e); act.n.x = x; act.n.y = y; act.n.pin = true;
    if (nodes.length <= 600) for (let i = 0; i < 2; i++) settleStep(nodes, edges, .3);
    remember(); draw();
  });
  const release = e => {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinch = null;
    if (!act) return;
    svg.classList.remove("grabbing");
    if (act.n && !act.moved) select(act.n.id);
    else if (act.pan && !act.moved && view.selected) select(null);
    act = null;
  };
  svg.addEventListener("pointerup", release);
  svg.addEventListener("pointercancel", release);
  svg.addEventListener("pointerleave", () => { if (!act && view.hover) { view.hover = null; paintFocus(); } });

  /* Keyboard: one tab stop, then arrows walk to the nearest node that way. */
  const focusNode = n => { shapes.forEach(s => s.setAttribute("tabindex", "-1")); const s = shapes[nodes.indexOf(n)]; s.setAttribute("tabindex", "0"); s.focus(); view.hover = n.id; paintFocus(); };
  svg.addEventListener("focusin", e => { const id = e.target.closest?.(".n")?.dataset.id; if (id && id !== view.hover) { view.hover = id; paintFocus(); } });
  svg.addEventListener("focusout", () => { view.hover = null; paintFocus(); });
  svg.addEventListener("keydown", e => {
    const gn = e.target.closest?.(".n"), n = gn && byId.get(gn.dataset.id);
    if (e.key === "+" || e.key === "=") { e.preventDefault(); zoomBy(1.25); return; }
    if (e.key === "-") { e.preventDefault(); zoomBy(.8); return; }
    if (e.key === "0") { e.preventDefault(); fit(); return; }
    if (e.key === "Escape" && view.selected) { e.stopPropagation(); select(null); return; }
    if (!n) return;
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); select(n.id); return; }
    const dir = { ArrowRight: [1, 0], ArrowLeft: [-1, 0], ArrowDown: [0, 1], ArrowUp: [0, -1] }[e.key];
    if (!dir) return;
    e.preventDefault();
    let best = null, bestScore = Infinity;
    for (const o of nodes) {
      if (o === n) continue;
      const dx = o.x - n.x, dy = o.y - n.y, along = dx * dir[0] + dy * dir[1];
      if (along <= 0) continue;
      const score = Math.hypot(dx, dy) + 2 * Math.abs(dx * dir[1] - dy * dir[0]);
      if (score < bestScore) { bestScore = score; best = o; }
    }
    if (best) { focusNode(best); const r = svg.getBoundingClientRect(), c = view.camera, sx = best.x * c.z + c.x, sy = best.y * c.z + c.y; if (sx < 20 || sy < 20 || sx > r.width - 20 || sy > r.height - 20) centerOn(best); }
  });

  /* Settling. A warm start needs little. A big graph settles in slices, so the app stays responsive. */
  const warm = nodes.length > 0 && nodes.every(n => view.positions.has(n.id));
  const total_iters = warm ? 30 : iterationsFor(nodes.length);
  const alphaAt = it => Math.max(warm ? .03 : .06, (warm ? .25 : 1) * (1 - it / total_iters));
  const finish = () => { remember(); draw(); if (!warm || !view.camera) fit(); else applyCamera(); paintFocus(); paintDetail(); status.hidden = true; root.dataset.settled = "1"; };
  if (!view.camera) view.camera = { x: 0, y: 0, z: 1 };
  if (nodes.length <= SYNC_LAYOUT_NODES || REDUCED()) {
    for (let it = 0; it < total_iters; it++) settleStep(nodes, edges, alphaAt(it));
    finish();
  } else {
    status.hidden = false; status.textContent = fill(api.t("Arranging {n} nodes"), { n: num(nodes.length) });
    let it = 0, stop = false;
    view.cancel = () => { stop = true; };
    draw(); fit(); paintFocus(); paintDetail();
    const slice = () => {
      if (stop || !root.isConnected) return;
      const end = performance.now() + 12;
      while (it < total_iters && performance.now() < end) settleStep(nodes, edges, alphaAt(it++));
      draw();
      if (it < total_iters) requestAnimationFrame(slice); else finish();
    };
    requestAnimationFrame(slice);
  }

  function paintDetail() {
    side.replaceChildren();
    const n = view.selected && g.nodes.get(view.selected);
    side.classList.toggle("open", !!n);
    root.classList.toggle("withdetail", !!n);
    if (!n) return;
    const close = api.el("button", { class: "ibtn kgclose", "aria-label": api.t("Clear selection"), onclick: () => { view.selected = null; if (view.list) renderGraph(content); else select(null); } }, api.icon("x"));
    side.append(api.el("div", { class: "nodehead" }, close, api.el("small", { text: api.t(KIND_LABEL[n.kind]) + (n.status ? `, ${n.status}` : "") }), api.el("h3", { dir: "auto", text: labelOf(n) })));
    if (n.page) side.append(api.el("button", { class: "btn primary kgopen", text: api.t(n.kind === "page" ? "Open page" : "Open its page"), onclick: () => api.openPage(n.page) }));
    const groups = new Map(), computed = [];
    for (const x of g.around.get(n.id) || []) {
      if (x.e.kind === "similar") { computed.push(x); continue; }
      const label = RELATIONS[x.e.kind]?.[x.forward ? 0 : 1];
      if (!groups.has(label)) groups.set(label, []);
      groups.get(label).push(x);
    }
    const row = x => api.el("div", { class: "rel" },
      api.el("button", { dir: "auto", text: labelOf(g.nodes.get(x.id)), onclick: () => { if (view.list) { view.selected = x.id; renderGraph(content); } else if (byId.has(x.id)) view.reselect(x.id); else select(x.id); } }),
      x.e.kind === "similar" ? api.el("span", { text: percent(x.e.score) }) : x.e.count > 1 ? api.el("span", { text: num(x.e.count) }) : null);
    if (!groups.size && !computed.length) side.append(api.el("div", { class: "rp" }, api.el("p", { text: api.t("No connections yet.") })));
    for (const [label, list] of groups) side.append(api.el("div", { class: "rp" }, api.el("h4", { text: api.t(label) }), ...list.map(row)));
    if (computed.length) side.append(api.el("div", { class: "rp kgcomputed" }, api.el("h4", { text: api.t("Similar, computed") }), ...computed.map(row)));
    if (n.dangling) side.append(api.el("div", { class: "rp" }, api.el("p", { text: fill(api.t("Links to removed pages: {n}"), { n: num(n.dangling) }) })));
  }
}

/* ---------- The side panel ---------- */
let panelTimer = null;
function ensureSection(id, title) {
  let box = api.$("#" + id);
  if (!box) {
    box = api.el("div", { class: "rp", id }, api.el("h4", { "data-t": title, text: api.t(title) }), api.el("div", { class: "rpbody" }));
    api.$("#rpDetails")?.append(box);
  }
  return api.$(".rpbody", box);
}
function paintPanels() {
  panelTimer = null;
  if (!api.$("#rpDetails") || api.state.view) return;
  const pageId = api.state.cur; if (!pageId) return;
  const related = ensureSection("rpRelated", "Related"), local = ensureSection("rpLocal", "Connections");
  let g;
  try { g = currentGraph(); } catch (err) { console.warn("graph", err); related.replaceChildren(api.el("p", { text: api.t("The graph could not be built.") })); local.replaceChildren(); return; }
  const me = "page:" + pageId, around = g.around.get(me) || [];

  related.replaceChildren();
  const similar = around.filter(x => x.e.kind === "similar").sort((a, b) => b.e.score - a.e.score || (a.id < b.id ? -1 : 1)).slice(0, 5);
  if (!similar.length) related.append(api.el("p", { text: api.t("Nothing similar yet.") }));
  for (const x of similar) {
    const other = g.nodes.get(x.id);
    related.append(api.el("div", { class: "rel" },
      api.el("button", { dir: "auto", text: labelOf(other), onclick: () => api.openPage(other.page) }),
      api.el("span", { text: percent(x.e.score) }),
      api.el("button", { class: "lk", text: api.t("Link"), "aria-label": fill(api.t("Link to {page}"), { page: labelOf(other) }), onclick: () => addLink(pageId, other) })));
  }

  local.replaceChildren();
  const near = [...around].sort((a, b) => (a.e.kind === "similar") - (b.e.kind === "similar") || (a.id < b.id ? -1 : 1));
  const seen = new Set(), unique = near.filter(x => !seen.has(x.id) && seen.add(x.id));
  if (!unique.length) { local.append(api.el("p", { text: api.t("No connections yet.") })); }
  else {
    const W = 270, H = 170, cx = W / 2, cy = H / 2, draw = unique.slice(0, LOCAL_NODES);
    const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}`, role: "group", "aria-label": api.t("Pages and items connected to this page") });
    draw.forEach((x, i) => {
      const a = i / draw.length * Math.PI * 2 - Math.PI / 2, r = draw.length > 8 ? 62 : 58, nx = cx + r * 1.45 * Math.cos(a), ny = cy + r * Math.sin(a);
      const line = svgEl("line", { x1: cx, y1: cy, x2: nx, y2: ny }); if (x.e.kind === "similar") line.setAttribute("class", "sim"); svg.append(line);
      const other = g.nodes.get(x.id), open = other.page || null;
      const dot = svgEl("circle", { cx: nx, cy: ny, r: 4.5, tabindex: open ? "0" : "-1", role: open ? "button" : "img", "aria-label": `${labelOf(other)}, ${api.t(KIND_LABEL[other.kind])}` });
      if (other.kind !== "page") dot.classList.add("k-other");
      const tip = svgEl("title"); tip.textContent = labelOf(other); dot.append(tip);
      if (open) { dot.addEventListener("click", () => api.openPage(open)); dot.addEventListener("keydown", e => { if (e.key === "Enter") { e.preventDefault(); api.openPage(open); } }); }
      svg.append(dot);
      if (draw.length <= 8) {
        const label = svgEl("text", { x: nx + (nx >= cx ? 8 : -8), y: ny + 3.5, "text-anchor": nx >= cx ? "start" : "end" });
        label.textContent = clip(labelOf(other), 12); svg.append(label);
      }
    });
    svg.append(svgEl("circle", { cx, cy, r: 6, class: "cur" }));
    local.append(api.el("div", { class: "localg" }, svg));
    const more = unique.length - draw.length;
    if (more > 0) local.append(api.el("p", { text: fill(api.t("and {n} more"), { n: num(more) }) }));
  }
  local.append(api.el("button", { class: "textlink", text: api.t("Full graph"), onclick: () => { view.selected = me; api.openView("graph"); } }));
}
function addLink(pageId, other) {
  const page = api.pageData(pageId), last = page.blocks[page.blocks.length - 1];
  api.edit({ op: "insert_block", page: pageId, after: last ? last.id : null, block: { id: api.uid(), kind: "paragraph", checked: false, runs: [{ text: labelOf(other), marks: [], link: other.page }] } });
  if (api.state.cur === pageId && !api.state.view) api.renderPage();
  schedulePanels();
}
function schedulePanels() { if (!panelTimer) panelTimer = setTimeout(paintPanels, 300); }

/* ---------- Search: the engine's index behind the existing Ctrl K palette ---------- */
function paintExcerpt(hit) {
  const small = api.el("small", { dir: "auto" });
  if (hit.cut_start) small.append("… ");
  let at = 0;
  for (const [a, b] of hit.marks) { if (a > at) small.append(hit.text.slice(at, a)); small.append(api.el("mark", { text: hit.text.slice(a, b) })); at = b; }
  small.append(hit.text.slice(at));
  if (hit.cut_end) small.append(" …");
  return small;
}
function openPalette() {
  api.$(".modalbg")?.remove();
  const before = document.activeElement;
  const input = api.el("input", { id: "palette", type: "search", placeholder: api.t("Search"), "aria-label": api.t("Search"), role: "combobox", "aria-expanded": "true", "aria-controls": "paletteList", autocomplete: "off" });
  const list = api.el("div", { class: "list", id: "paletteList", role: "listbox" });
  let on = 0, hits = [];
  const close = () => { api.$(".modalbg")?.remove(); if (before?.isConnected) before.focus?.(); };
  const go = h => { api.$(".modalbg")?.remove(); api.openPage(h.page); if (h.item) api.$(`.b[data-id="${CSS.escape(h.item)}"]`)?.scrollIntoView({ block: "center" }); };
  const paint = () => {
    const q = input.value.trim(), titles = new Map(api.live().map(p => [p.id, p.title]));
    list.replaceChildren();
    if (!q) hits = [...titles].slice(0, 40).map(([page, title]) => ({ page, title, item: "", plain: true }));
    else {
      try { hits = JSON.parse(api.engine.search(q, 40)); }
      catch (err) { console.warn("search", err); hits = []; list.append(api.el("div", { class: "hdr", role: "alert", text: api.t("Search failed.") })); return; }
      hits = hits.filter(h => titles.has(h.page));
      for (const h of hits) h.title = titles.get(h.page);
    }
    on = Math.min(on, Math.max(0, hits.length - 1));
    input.removeAttribute("aria-activedescendant");
    hits.forEach((h, i) => {
      const isTitle = h.field === "title";
      const gl = h.field === "row" || h.field === "cell" ? "table" : "file-text";
      const label = api.el("span", { class: "palt", dir: "auto" });
      if (isTitle) { let at = 0; for (const [a, b] of h.marks) { if (a > at) label.append(h.text.slice(at, a)); label.append(api.el("mark", { text: h.text.slice(a, b) })); at = b; } label.append(h.text.slice(at)); }
      else label.append(h.title || api.t("Untitled"));
      const btn = api.el("button", { class: "mi" + (i === on ? " on" : ""), id: "palo" + i, role: "option", "aria-selected": String(i === on), onclick: () => go(h), onmousedown: e => e.preventDefault() },
        api.el("span", { class: "gl" }, api.icon(gl)), api.el("span", { class: "palb" }, label, !h.plain && !isTitle ? paintExcerpt(h) : null));
      list.append(btn);
      if (i === on) { input.setAttribute("aria-activedescendant", btn.id); requestAnimationFrame(() => btn.scrollIntoView?.({ block: "nearest" })); }
    });
    if (!hits.length) list.append(api.el("div", { class: "hdr", text: api.t("No matches") }));
  };
  input.addEventListener("input", () => { on = 0; paint(); });
  input.addEventListener("keydown", e => {
    if (e.isComposing) return;
    if (e.key === "ArrowDown") { e.preventDefault(); on = Math.min(hits.length - 1, on + 1); paint(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); on = Math.max(0, on - 1); paint(); }
    else if (e.key === "Enter" && hits[on]) { e.preventDefault(); go(hits[on]); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
  });
  document.body.append(api.el("div", { class: "modalbg", onpointerdown: e => { if (e.target.classList.contains("modalbg")) close(); } }, api.el("div", { class: "modal", role: "dialog", "aria-label": api.t("Search") }, input, list)));
  paint(); input.focus();
}

export function install(a) {
  api = a;
  const ext = api.ext;
  ext.views.graph = { label: "Knowledge graph", render: content => renderGraph(content) };
  ext.nav.push({ id: "graphBtn", icon: "graph", label: "Knowledge graph", run: () => api.openView("graph") });
  ext.panels.push(schedulePanels);
  // app.js opens its own linear search on these. The capture phase reaches them first.
  document.addEventListener("keydown", e => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); e.stopPropagation(); openPalette(); }
  }, true);
  document.addEventListener("click", e => {
    if (e.target.closest?.("#searchBtn")) { e.stopPropagation(); openPalette(); }
  }, true);
}
