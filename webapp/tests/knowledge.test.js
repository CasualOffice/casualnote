// Search, the graph view and the side panel, through real input events in a real browser.
const GOLDEN = ["import:g1::3405921", "import:g2:g2b:808393", "import:g1:g1b:406154", "导入:g1::3405921", "导入:g2:g2b:549973", "导入:g1:g1b:406154", "العربية:g1:g1b:902634", "한국어:g1:g1b:1041306", "नमस्ते:g1:g1b:902634", "こんにちは:g1:g1b:3610535", "nino:g1:g1b:520653"];

export async function run({ api, ok, wait, key, openPage, $, $$, engine, state }) {
  const page = (id, title, text, extra = []) => {
    api.edit({ op: "create_page", id, parent: null, title }, { op: "insert_block", page: id, after: null, block: { id: id + "b", kind: "paragraph", checked: false, runs: [{ text, marks: [] }] } }, ...extra);
  };
  const home = state.cur;
  const openPalette = () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true, bubbles: true, cancelable: true }));
  const query = q => { const i = $("#palette"); i.value = q; i.dispatchEvent(new Event("input", { bubbles: true })); return i; };
  const closePalette = () => { key($("#palette"), "Escape"); };

  /* Search */
  await ok("search: native and wasm give the same scores", async () => {
    const { Engine } = await import("../pkg/casual_note_wasm.js");
    const e = new Engine(9n);
    for (const [id, title, text] of [["g1", "Import 导入", "导入 العربية नमस्ते こんにちは 한국어 niño import"], ["g2", "Notes", "import import plan 导入数据"], ["g3", "Other", "nothing here"]]) {
      e.apply(JSON.stringify({ op: "create_page", id, parent: null, title }));
      e.apply(JSON.stringify({ op: "insert_block", page: id, after: null, block: { id: id + "b", kind: "paragraph", checked: false, runs: [{ text, marks: [] }] } }));
    }
    const got = ["import", "导入", "العربية", "한국어", "नमस्ते", "こんにちは", "nino"].flatMap(q => JSON.parse(e.search(q, 10)).map(r => `${q}:${r.page}:${r.item}:${r.score}`));
    return JSON.stringify(got) === JSON.stringify(GOLDEN);
  });
  page("kn-roll", "Rollout schedule", "Open once the مرحبا import works and नमस्ते 日本語のノート 知识图谱 한국어를 niño");
  await ok("search: Ctrl K opens the engine's palette, and every launch script finds its page", () => {
    openPalette();
    if (!$("#palette") || !$("#palette").hasAttribute("role")) return false;
    for (const q of ["مرحبا", "नमस्ते", "日本語", "知识", "한국어", "NIÑO", "Rollout"]) {
      query(q);
      if (!$$(".modal .mi").some(m => m.textContent.includes("Rollout schedule"))) return false;
    }
    closePalette();
    return !$(".modalbg");
  });
  await ok("search: the Search button opens the same palette", () => {
    $("#searchBtn").click();
    const r = $("#palette")?.getAttribute("role") === "combobox";
    closePalette(); return r;
  });
  await ok("search: a match is highlighted as text, in the excerpt", () => {
    openPalette(); query("niño");
    const mark = $(".modal mark");
    const r = mark?.textContent === "niño" && !!$(".modal .mi.on");
    closePalette(); return r;
  });
  await ok("search: keyboard moves and opens, and empty results say so", async () => {
    page("kn-other", "Second page", "unusualword");
    openPage(home);
    openPalette(); query("zzznothingzzz");
    if (!$$(".modal .hdr").some(n => n.textContent)) return false;
    const i = query("unusualword");
    key(i, "ArrowDown"); key(i, "Enter"); await wait(5);
    return state.cur === "kn-other" && !$(".modalbg");
  });
  await ok("search: an edit is found at once, and the trash leaves results", () => {
    api.edit({ op: "splice_text", page: "kn-other", block: "kn-otherb", at: 0, delete: 0, insert: "freshlytyped " });
    openPalette(); query("freshlytyped");
    if ($$(".modal .mi").length !== 1) return false;
    closePalette();
    api.edit({ op: "set_trashed", id: "kn-other", value: true });
    openPalette(); query("freshlytyped");
    const gone = $$(".modal .mi").length === 0;
    closePalette();
    api.edit({ op: "set_trashed", id: "kn-other", value: false });
    openPage(home);
    return gone;
  });
  await ok("search: a database row finds its page", () => {
    api.edit({ op: "create_database", database: { id: "kn-db", name: "Tracker", view: "table", statuses: [{ id: "s1", name: "Open", group: "todo" }], columns: [{ id: "owner", name: "Owner", kind: "text", hidden: false }], rows: [{ id: "r1", name: "Findablerow task", status: "s1", values: { owner: "Mei" } }, { id: "r2", name: "Other row", status: "s1", values: { owner: "mei" } }] } },
      { op: "create_page", id: "kn-tracker", parent: null, title: "Tracker", database: "kn-db" });
    openPalette(); query("findablerow");
    const hit = $$(".modal .mi").find(m => m.textContent.includes("Tracker"));
    closePalette();
    return !!hit;
  });
  await ok("search: document text cannot become HTML", () => {
    page("kn-xss", "<img src=x onerror=alert(1)>", "<script>alert(1)</script> findhtml");
    openPalette(); query("findhtml");
    const clean = !$(".modal img") && !$(".modal script") && $(".modal .mi").textContent.includes("<img");
    closePalette();
    return clean;
  });

  /* A workspace with one of everything */
  const shared = "bridge pylon tension cable span anchor deck rivet";
  page("kn-a", "Bridge notes", shared + " #Plan");
  page("kn-b", "Tower notes", shared + " #plan");
  page("kn-c", "مرحبا العالم", "#مرحبا text");
  api.edit({ op: "insert_block", page: "kn-a", after: "kn-ab", block: { id: "kn-link", kind: "paragraph", checked: false, runs: [{ text: "see ", marks: [] }, { text: "arabic", marks: [], link: "kn-c" }] } });

  const G = () => JSON.parse(engine.graph());
  await ok("graph: engine derives page, tag, row and person nodes and made edges", () => {
    const g = G(), has = id => g.nodes.some(n => n.id === id), edge = (k, a, b) => g.edges.some(e => e.kind === k && e.from === a && e.to === b);
    return has("page:kn-a") && has("tag:plan") && has("tag:مرحبا") && has("row:kn-db:r1") && has("person:mei")
      && edge("link", "page:kn-a", "page:kn-c") && edge("tag", "page:kn-a", "tag:plan") && edge("row", "page:kn-tracker", "row:kn-db:r1") && edge("owner", "row:kn-db:r1", "person:mei");
  });
  await ok("graph: similar edges come from word overlap, computed and named so", () => {
    const s = G().edges.find(e => e.kind === "similar" && e.from === "page:kn-a" && e.to === "page:kn-b");
    return s?.origin === "computed" && s.method === "word-overlap-v1" && s.score > 0;
  });

  const openGraph = async () => { $("#graphBtn").click(); await wait(30); };
  const nodeEl = id => $(`.n[data-id="${CSS.escape(id)}"]`);
  const pointer = (type, target, x, y, o = {}) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 7, clientX: x, clientY: y, button: 0, ...o }));
  const center = n => { const r = n.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
  const clickNode = async id => { const n = nodeEl(id), [x, y] = center(n.querySelector(".sh")); pointer("pointerdown", n, x, y); pointer("pointerup", n, x, y); await wait(5); };

  await ok("graph: the nav item opens a full view with nodes and edges", async () => {
    await openGraph();
    const label = nodeEl("page:kn-c")?.querySelector("text")?.textContent;
    return state.view === "graph" && !!$("#gsvg") && $$("#gsvg .n").length >= 8 && $$("#gsvg line").length >= 5 && label === "مرحبا العالم";
  });
  await ok("graph: layout is on screen and every node is inside the view after Fit", () => {
    const box = $("#gsvg").getBoundingClientRect();
    return $$("#gsvg .n .sh").every(s => { const r = s.getBoundingClientRect(); return r.left >= box.left - 1 && r.right <= box.right + 1 && r.top >= box.top - 1 && r.bottom <= box.bottom + 1; });
  });
  await ok("graph: similar edges are dashed, and made edges are not", () => {
    const sim = $("#gsvg line.e-similar"), made = $("#gsvg line.e-link");
    return !!sim && !!made && getComputedStyle(sim).strokeDasharray !== "none" && getComputedStyle(made).strokeDasharray === "none";
  });
  await ok("graph: hover highlights neighbours and dims the rest", async () => {
    const n = nodeEl("page:kn-c"), [x, y] = center(n.querySelector(".sh"));
    pointer("pointermove", n, x, y); await wait(5);
    const on = $("#gsvg").classList.contains("focus") && nodeEl("page:kn-a").classList.contains("hl") && !nodeEl("row:kn-db:r1").classList.contains("hl") && !!$("#gsvg line.hl");
    pointer("pointermove", $("#gsvg"), 2, 2); await wait(5);
    return on && !$("#gsvg").classList.contains("focus");
  });
  await ok("graph: click selects, lists relations in groups, and opens the page", async () => {
    await clickNode("page:kn-a");
    const side = $(".kgside");
    if (!side.classList.contains("open") || !nodeEl("page:kn-a").classList.contains("sel")) return false;
    const groups = $$(".rp h4", side).map(n => n.textContent);
    if (!groups.includes("links to") || !groups.includes("Similar, computed") || !$(".kgcomputed .rel", side)?.textContent.includes("Tower notes")) return false;
    $(".kgopen", side).click(); await wait(5);
    const opened = state.cur === "kn-a" && state.view === null;
    await openGraph();
    return opened && $(".kgside").classList.contains("open");
  });
  await ok("graph: a click on the background clears the selection", async () => {
    const r = $("#gsvg").getBoundingClientRect(), x = r.right - 4, y = r.top + 4;
    pointer("pointerdown", $("#gsvg"), x, y); pointer("pointerup", $("#gsvg"), x, y); await wait(5);
    return !$(".kgside").classList.contains("open") && !$("#gsvg .n.sel");
  });
  await ok("graph: dragging a node moves it, and it stays after a re-render", async () => {
    const n = nodeEl("page:kn-b"), before = n.getAttribute("transform"), [x, y] = center(n.querySelector(".sh"));
    pointer("pointerdown", n, x, y); pointer("pointermove", $("#gsvg"), x + 40, y + 30); pointer("pointerup", $("#gsvg"), x + 40, y + 30); await wait(5);
    const after = nodeEl("page:kn-b").getAttribute("transform");
    if (after === before) return false;
    api.renderAll(); await wait(5);
    const [ax, ay] = after.match(/-?[\d.]+/g).map(Number), [bx, by] = nodeEl("page:kn-b").getAttribute("transform").match(/-?[\d.]+/g).map(Number);
    return Math.hypot(ax - bx, ay - by) < 1;
  });
  await ok("graph: panning moves the camera, and the buttons and wheel zoom", async () => {
    const w = () => $("#gsvg > g").getAttribute("transform"), start = w(), r = $("#gsvg").getBoundingClientRect(), x = r.right - 4, y = r.bottom - 60;
    pointer("pointerdown", $("#gsvg"), x, y); pointer("pointermove", $("#gsvg"), x - 30, y - 30); pointer("pointerup", $("#gsvg"), x - 30, y - 30);
    if (w() === start) return false;
    const pan = w();
    $('.zoom button[aria-label="Zoom in"]').click();
    if (w() === pan) return false;
    const zoomed = w();
    $("#gsvg").dispatchEvent(new WheelEvent("wheel", { deltaY: -100, ctrlKey: true, clientX: r.left + 50, clientY: r.top + 50, bubbles: true, cancelable: true }));
    if (w() === zoomed) return false;
    $(".zoom .kgfit").click();
    return true;
  });
  await ok("graph: filter checkboxes remove nodes, and empty states explain", async () => {
    const toggle = async id => { const c = $("#gf-" + id); c.checked = !c.checked; c.dispatchEvent(new Event("change", { bubbles: true })); await wait(10); };
    await toggle("tag");
    if (nodeEl("tag:plan")) return false;
    await toggle("similar");
    if ($("#gsvg line.e-similar")) return false;
    await toggle("page"); await toggle("row"); await toggle("person");
    const empty = !!$(".kgempty p")?.textContent;
    for (const id of ["tag", "similar", "page", "row", "person"]) await toggle(id);
    return empty && !!nodeEl("tag:plan") && !!$("#gsvg line.e-similar");
  });
  await ok("graph: the find box marks matches, says when nothing matches, and Enter selects", async () => {
    const q = $("#gq"), set = v => { q.value = v; q.dispatchEvent(new Event("input", { bubbles: true })); };
    set("tower");
    if (!nodeEl("page:kn-b").classList.contains("found") || !nodeEl("page:kn-a").classList.contains("miss")) return false;
    set("zzzz");
    if ($(".kgmiss").hidden) return false;
    set("مرحبا");
    q.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); await wait(5);
    const selected = $(".kgside h3")?.textContent.includes("مرحبا");
    set(""); key($("#gsvg .n"), "Escape"); await wait(5);
    return selected && !$(".kgside").classList.contains("open");
  });
  await ok("graph: keyboard walks, selects and clears, with one tab stop", async () => {
    if ($$('#gsvg .n[tabindex="0"]').length !== 1) return false;
    const n = $('#gsvg .n[tabindex="0"]'); n.focus();
    key(n, "ArrowRight"); key(document.activeElement, "ArrowDown"); key(document.activeElement, "ArrowLeft");
    const now = document.activeElement;
    if (!now.classList.contains("n")) return false;
    key(now, "Enter"); await wait(5);
    if (!$(".kgside").classList.contains("open")) return false;
    key(now, "Escape"); await wait(5);
    return !$(".kgside").classList.contains("open");
  });
  await ok("graph: a list form exists for screen readers", async () => {
    $(".kgaslist").click(); await wait(5);
    const has = $$(".kgrow").some(r => r.textContent.includes("Tower notes"));
    $(".kgaslist").click(); await wait(10);
    return has && !!$("#gsvg");
  });
  await ok("graph: labels cannot become HTML", () => !$("#gsvg img") && !$("#gsvg script") && nodeEl("page:kn-xss").querySelector("text").textContent.includes("<img"));
  await ok("graph: the same notes lay out the same way every time", async () => {
    // A cold layout from the spiral has no chance in it.
    const { Engine } = await import("../pkg/casual_note_wasm.js");
    const a = new Engine(3n), b = new Engine(4n);
    for (const e of [a, b]) for (const id of ["x", "y"]) e.apply(JSON.stringify({ op: "create_page", id, parent: null, title: id }));
    return a.graph() === b.graph();
  });

  /* The side panel */
  await ok("side panel: Related lists similar pages with a score, and Link adds a real link", async () => {
    openPage("kn-a"); await wait(350);
    const rel = $$("#rpRelated .rel").find(r => r.textContent.includes("Tower notes"));
    if (!rel || !/%|٪/.test(rel.textContent)) return false;
    $(".lk", rel).click(); await wait(350);
    const made = api.pageData("kn-a").blocks.some(b => b.runs.some(r => r.link === "kn-b"));
    return made && !G().edges.some(e => e.kind === "similar" && e.from === "page:kn-a" && e.to === "page:kn-b");
  });
  await ok("side panel: the local graph draws neighbours, and a click opens one", async () => {
    await wait(350);
    if ($$("#rpLocal circle").length < 3 || !$("#rpLocal line")) return false;
    const dot = $$("#rpLocal circle[role=button]")[0];
    dot.dispatchEvent(new MouseEvent("click", { bubbles: true })); await wait(5);
    const moved = state.cur !== "kn-a";
    openPage("kn-a"); await wait(350);
    return moved;
  });
  await ok("side panel: Full graph opens the view with this page selected", async () => {
    $("#rpLocal .textlink").click(); await wait(20);
    const r = state.view === "graph" && nodeEl("page:kn-a").classList.contains("sel");
    openPage(home);
    return r;
  });
  await ok("side panel: a page with nothing says so", async () => {
    page("kn-alone", "Alone", "qqqqq"); openPage("kn-alone"); await wait(350);
    const r = !!$("#rpRelated p") && !!$("#rpLocal p");
    return r;
  });
  await ok("graph: phone width keeps controls reachable with no page scroll", async () => {
    await openGraph();
    const kg = $(".kg");
    kg.style.width = "375px";
    await wait(10);
    const ctl = $(".gctl").getBoundingClientRect(), zoom = $(".zoom").getBoundingClientRect(), box = $(".gv").getBoundingClientRect();
    const r = ctl.left >= box.left && zoom.right <= box.right + 1 && document.documentElement.scrollWidth <= innerWidth;
    kg.style.width = "";
    return r;
  });
  // Leaves the graph open for `tools/e2e.mjs --shot`.
  window.__showGraph = async () => { state.view = "graph"; api.renderAll(); $(".zoom .kgfit")?.click(); await wait(30); };
}
