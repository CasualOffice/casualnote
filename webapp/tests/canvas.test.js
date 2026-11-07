// Page and Canvas, through real PointerEvents on the real DOM, checked against the engine.
export async function run({ api, ok, wait, key, type, pageNow, engine, state, setCaret, $, $$ }) {
  const page0 = state.cur;
  const data = () => JSON.parse(engine.page(state.cur));
  const canvas = () => data().canvas;
  const cv = () => $("#cv");
  const undo = () => key(document.body, "z", { ctrlKey: true });
  const redo = () => key(document.body, "z", { ctrlKey: true, shiftKey: true });
  const useTool = id => $(`.dock [data-tool="${id}"]`).click();
  const box = () => cv().getBoundingClientRect();
  let pid = 100;

  const ptr = (name, x, y, o = {}) => new PointerEvent(name, { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, pointerId: o.id ?? 1, pointerType: o.type || "mouse", isPrimary: true, button: name === "pointermove" ? -1 : 0, buttons: name === "pointerup" ? 0 : 1, pressure: o.pressure ?? (name === "pointerup" ? 0 : 0.5), ...o.init });
  const down = (x, y, o) => (document.elementFromPoint(x, y) || cv()).dispatchEvent(ptr("pointerdown", x, y, o));
  const move = (x, y, o) => cv().dispatchEvent(ptr("pointermove", x, y, o));
  const up = (x, y, o) => cv().dispatchEvent(ptr("pointerup", x, y, o));
  // A click as a person makes it, on whatever is under the pointer.
  const click = (x, y) => { const n = document.elementFromPoint(x, y); n.dispatchEvent(ptr("pointerdown", x, y)); n.dispatchEvent(ptr("pointerup", x, y)); };
  const stroke = (from, to, n = 20, o = {}) => {
    const id = pid++, opts = { id, ...o };
    down(from[0], from[1], opts);
    for (let i = 1; i <= n; i++) move(from[0] + (to[0] - from[0]) * i / n, from[1] + (to[1] - from[1]) * i / n, opts);
    up(to[0], to[1], opts);
  };
  const emptySpot = () => { const b = box(); return [b.right - 260, b.top + 80]; };
  const camOf = () => { const m = $(".world").style.transform.match(/translate\(([-\d.e+]+)px, ?([-\d.e+]+)px\) scale\(([-\d.e+]+)\)/); return m.slice(1, 4).map(Number); };
  const paintedPixels = c => { const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data; let n = 0; for (let i = 3; i < d.length; i += 4) if (d[i]) n++; return n; };
  const inkIds = () => canvas().ink.map(s => s.id);
  const snapshot = () => { const p = data(); return JSON.stringify([p.meta.title, p.blocks, p.canvas]); };

  const before = data();

  await ok("canvas: page and canvas switch is in the bar", () => $(".bar .cv-mode") && $(".bar .cv-mode").nextElementSibling === $("#favBtn") && $('.cv-mode [data-view="page"]').getAttribute("aria-pressed") === "true");
  await ok("canvas: switch writes set_view and shows the real blocks in the note card", () => {
    $('.cv-mode [data-view="canvas"]').click();
    return data().view === "canvas" && !!cv() && $$(".note .b").length === data().blocks.length && !$(".docscroll") && !!$(".note .title") && $('.cv-mode [data-view="canvas"]').getAttribute("aria-pressed") === "true";
  });
  await ok("canvas: dock has every tool, with a name and a key", () => {
    const tools = $$(".dock [data-tool]");
    return tools.length === 9 && tools.every(b => b.getAttribute("aria-label") && b.getAttribute("data-tip").endsWith(b.getAttribute("aria-keyshortcuts").toUpperCase()));
  });
  await ok("canvas: the world stays left to right, moved by a style property", () => getComputedStyle($(".world")).direction === "ltr" && $(".world").style.transform.includes("scale"));
  await ok("canvas: empty surface shows one plain sentence", () => canvas().ink.length || canvas().items.length || !$(".cv-hint").hidden);

  await ok("canvas: pen stroke reaches the engine with pressure", () => {
    useTool("pen"); const n = inkIds().length, b = box();
    stroke([b.left + 300, b.top + 200], [b.left + 500, b.top + 200], 40);
    const s = canvas().ink.at(-1);
    return inkIds().length === n + 1 && s.kind === "pen" && s.points.length >= 40 && s.points.every(p => p.length === 3 && p[2] > 0 && p[2] <= 1);
  });
  await ok("canvas: the finished stroke is painted on the dry layer, wet is clear", () => paintedPixels($(".cv-dry")) > 50 && paintedPixels($(".cv-wet")) === 0);
  await ok("canvas: coalesced samples are all stored", () => {
    useTool("pen"); const b = box(), id = pid++, o = { id };
    const sub = i => new PointerEvent("pointermove", { clientX: b.left + 300 + i, clientY: b.top + 240, pointerId: id, pointerType: "mouse" });
    down(b.left + 300, b.top + 240, o);
    const hasInit = new PointerEvent("pointermove", { coalescedEvents: [sub(1)] }).getCoalescedEvents().length === 1;
    cv().dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: b.left + 305, clientY: b.top + 240, pointerId: id, pointerType: "mouse", coalescedEvents: [1, 2, 3, 4, 5].map(sub) }));
    up(b.left + 305, b.top + 240, o);
    const pts = canvas().ink.at(-1).points;
    undo();
    return hasInit ? pts.length === 6 : pts.length >= 2;
  });
  await ok("canvas: undo removes the stroke, redo brings it back", () => {
    const n = inkIds().length, id = canvas().ink.at(-1).id;
    undo(); const gone = !inkIds().includes(id) && inkIds().length === n - 1;
    redo(); return gone && inkIds().includes(id);
  });
  await ok("canvas: mouse pressure varies with speed", () => {
    useTool("pen"); const b = box(), id = pid++, o = { id };
    down(b.left + 300, b.top + 280, o);
    for (let i = 1; i <= 8; i++) move(b.left + 300 + i * 40, b.top + 280, o);
    up(b.left + 620, b.top + 280, o);
    const qs = canvas().ink.at(-1).points.map(p => p[2]);
    undo();
    return new Set(qs).size > 1;
  });
  await ok("canvas: highlighter stores flat ink, and pen colours stick", () => {
    useTool("highlighter"); const b = box();
    stroke([b.left + 300, b.top + 320], [b.left + 500, b.top + 320]);
    const h = canvas().ink.at(-1);
    const hiPainted = paintedPixels($(".cv-hi")) > 50;
    useTool("pen");
    $(".dock-sub .k-red").click();
    stroke([b.left + 300, b.top + 360], [b.left + 500, b.top + 360]);
    const r = canvas().ink.at(-1);
    $(".dock-sub .k-ink").click();
    const blend = getComputedStyle($(".cv-hi")).mixBlendMode;
    undo(); undo();
    return h.kind === "highlighter" && h.width === 18 && r.color === "red" && hiPainted && blend !== "normal";
  });

  await ok("canvas: erasing the middle gives two pieces in one undo step", () => {
    useTool("pen"); const b = box(), y = b.top + 420;
    stroke([b.left + 300, y], [b.left + 600, y], 60);
    const orig = canvas().ink.at(-1), others = inkIds().length - 1;
    useTool("eraser");
    stroke([b.left + 450, y - 40], [b.left + 450, y + 40], 16);
    const ink = canvas().ink;
    const pieces = ink.filter(s => !inkIds().includes(orig.id) && Math.abs(s.points[0][1] - orig.points[0][1]) < 1);
    const allKept = pieces.flatMap(s => s.points).length;
    const twoPieces = ink.length === others + 2 && pieces.length === 2 && allKept < orig.points.length && pieces.every(s => s.kind === "pen" && s.width === orig.width);
    undo();
    const back = inkIds().includes(orig.id) && canvas().ink.length === others + 1;
    return twoPieces && back;
  });
  await ok("canvas: erasing an end leaves one piece, and a drag across everything removes it", () => {
    const b = box(), y = b.top + 420, n = inkIds().length;
    useTool("eraser");
    stroke([b.left + 590, y - 30], [b.left + 590, y + 30], 12);
    const one = canvas().ink.at(-1);
    const onePiece = inkIds().length === n && one.points.every(p => p[0] !== undefined) && one.points.length > 10;
    undo();
    stroke([b.left + 290, y], [b.left + 610, y], 80);
    const removed = inkIds().length === n - 1;
    undo();
    useTool("select");
    return onePiece && removed && inkIds().length === n;
  });

  let stickyId;
  await ok("canvas: sticky is added on click, and returns to select with the caret in it", async () => {
    const n = canvas().items.length, [x, y] = emptySpot();
    useTool("sticky");
    click(x, y);
    await wait(10);
    const it = canvas().items.at(-1); stickyId = it?.id;
    return canvas().items.length === n + 1 && it.kind === "sticky" && it.width === 170 && it.height === 130 && it.color === "amber"
      && $('.dock [data-tool="select"]').getAttribute("aria-pressed") === "true" && document.activeElement === $(`.ci[data-id="${stickyId}"] .txt`);
  });
  await ok("canvas: sticky text in Chinese is written to the engine as text", async () => {
    type("你好，画布");
    await wait(0);
    key(document.activeElement, "Escape");
    const it = canvas().items.find(i => i.id === stickyId);
    return it.text === "你好，画布" && $(`.ci[data-id="${stickyId}"] .txt`).textContent === "你好，画布" && !$(`.ci[data-id="${stickyId}"] .txt`).children.length;
  });
  await ok("canvas: an input method composes without writing half a word", async () => {
    const node = $(`.ci[data-id="${stickyId}"]`), r = node.getBoundingClientRect();
    node.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: r.left + 20, clientY: r.top + 20 }));
    const txt = node.querySelector(".txt");
    if (document.activeElement !== txt) return false;
    const undoDepthText = canvas().items.find(i => i.id === stickyId).text;
    txt.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    txt.textContent += "nihao";
    txt.dispatchEvent(new InputEvent("input", { bubbles: true, isComposing: true }));
    await wait(600);
    const during = canvas().items.find(i => i.id === stickyId).text;
    api.renderAll(); // A merge from another tab during composition must not take the node away.
    const stillThere = txt.isConnected && document.activeElement === txt;
    txt.textContent = undoDepthText;
    txt.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
    key(txt, "Escape");
    return during === undoDepthText && stillThere && !$(`.ci[data-id="${stickyId}"]`).classList.contains("editing");
  });
  await ok("canvas: dragging moves a sticky in one op, with no jump at the start", () => {
    const it0 = canvas().items.find(i => i.id === stickyId), node = $(`.ci[data-id="${stickyId}"]`), r = node.getBoundingClientRect();
    const x = r.left + 40, y = r.top + 40, z = camOf()[2], id = pid++, o = { id };
    down(x, y, o);
    move(x + 2, y + 1, o); // Under the threshold nothing moves.
    const held = node.getBoundingClientRect().left === r.left;
    move(x + 30, y + 20, o);
    const shifted = Math.abs(node.getBoundingClientRect().left - (r.left + 30)) < 1;
    const notWritten = canvas().items.find(i => i.id === stickyId).x === it0.x;
    up(x + 30, y + 20, o);
    const it = canvas().items.find(i => i.id === stickyId);
    return held && shifted && notWritten && Math.abs(it.x - (it0.x + 30 / z)) < 0.01 && Math.abs(it.y - (it0.y + 20 / z)) < 0.01 && node.classList.contains("sel");
  });
  await ok("canvas: undo puts the sticky back where it was", () => {
    const moved = canvas().items.find(i => i.id === stickyId).x;
    undo();
    const was = canvas().items.find(i => i.id === stickyId).x;
    const shown = Math.abs(parseFloat($(`.ci[data-id="${stickyId}"]`).style.left) - was) < 0.01;
    redo();
    return was !== moved && shown && canvas().items.find(i => i.id === stickyId).x === moved;
  });
  await ok("canvas: corner resize writes the new size", () => {
    const node = $(`.ci[data-id="${stickyId}"]`), it0 = canvas().items.find(i => i.id === stickyId);
    const r = node.querySelector(".rz").getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2, z = camOf()[2], o = { id: pid++ };
    down(x, y, o); move(x + 40, y + 30, o); up(x + 40, y + 30, o);
    const it = canvas().items.find(i => i.id === stickyId);
    return Math.abs(it.width - (it0.width + 40 / z)) < 0.01 && Math.abs(it.height - (it0.height + 30 / z)) < 0.01;
  });
  await ok("canvas: a click on a selected item edits it, Escape leaves it selected", () => {
    const node = $(`.ci[data-id="${stickyId}"]`), r = node.getBoundingClientRect();
    click(r.left + 30, r.top + 30);
    const editing = node.classList.contains("editing") && document.activeElement === node.querySelector(".txt");
    key(document.activeElement, "Escape");
    return editing && node.classList.contains("sel") && !node.classList.contains("editing");
  });
  await ok("canvas: Delete removes the selection, undo restores it with its text", () => {
    key(document.body, "Delete");
    const gone = !canvas().items.some(i => i.id === stickyId) && !$(`.ci[data-id="${stickyId}"]`);
    undo();
    const back = canvas().items.find(i => i.id === stickyId);
    return gone && back?.text === "你好，画布" && !!$(`.ci[data-id="${stickyId}"]`);
  });
  await ok("canvas: shape tools make items, drag sets the size, empty text box is not kept", async () => {
    const b = box(), n = canvas().items.length;
    useTool("rect");
    stroke([b.right - 500, b.top + 300], [b.right - 300, b.top + 400], 6);
    const z = camOf()[2], rect = canvas().items.at(-1);
    key(document.activeElement, "Escape");
    useTool("ellipse"); click(b.right - 200, b.top + 420); await wait(0);
    const ellipse = canvas().items.at(-1);
    key(document.activeElement, "Escape");
    useTool("text"); click(b.right - 500, b.top + 500); await wait(0);
    const drafted = !!document.activeElement.closest(".ci.text") && canvas().items.length === n + 2;
    key(document.activeElement, "Escape");
    const noneLeft = canvas().items.length === n + 2 && !$(".ci.text");
    useTool("text"); click(b.right - 500, b.top + 500); await wait(0);
    type("Hello"); key(document.activeElement, "Escape");
    const text = canvas().items.at(-1);
    const good = rect.kind === "rect" && Math.abs(rect.width - 200 / z) < 1 && Math.abs(rect.height - 100 / z) < 1 && ellipse.kind === "ellipse" && drafted && noneLeft && text.kind === "text" && text.text === "Hello";
    api.edit(...[rect, ellipse, text].map(i => ({ op: "delete_item", page: state.cur, id: i.id })));
    api.renderAll();
    return good;
  });
  await ok("canvas: arrow is ink with two points; short ones are discarded; it selects, moves and deletes", () => {
    const b = box(), n = inkIds().length, y = b.top + 470;
    useTool("arrow");
    stroke([b.left + 300, y], [b.left + 302, y], 2);
    const discarded = inkIds().length === n;
    stroke([b.left + 300, y], [b.left + 500, y], 8);
    const a = canvas().ink.at(-1);
    useTool("select");
    const o = { id: pid++ };
    const mid = [b.left + 400, y];
    if (document.elementFromPoint(mid[0], mid[1])?.closest(".note")) return false;
    down(mid[0], mid[1], o); move(mid[0] + 10, mid[1] + 30, o); up(mid[0] + 10, mid[1] + 30, o);
    const m = canvas().ink.at(-1), z = camOf()[2];
    const movedOk = m.id !== a.id && Math.abs(m.points[0][1] - a.points[0][1] - 30 / z) < 0.01;
    key(document.body, "Delete");
    const gone = inkIds().length === n;
    undo();
    const back = inkIds().length === n + 1;
    undo(); undo();
    return discarded && a.kind === "arrow" && a.points.length === 2 && movedOk && gone && back && inkIds().length === n;
  });

  await ok("canvas: the note card edits the real blocks, in both views", () => {
    const t = $(".note .blocks .t"), id = t.closest(".b").dataset.id;
    useTool("select");
    setCaret(t); type("注记");
    const hit = data().blocks.find(x => x.id === id).runs.map(r => r.text).join("").endsWith("注记");
    const n = data().blocks.length;
    key(document.activeElement, "Enter");
    const split = data().blocks.length === n + 1 && !!$(".note").contains(document.activeElement) && $$(".note .b").length === n + 1;
    key(document.activeElement, "s"); // A letter typed in the editor does not change tool.
    const stays = $('.dock [data-tool="select"]').getAttribute("aria-pressed") === "true";
    key(document.activeElement, "Escape");
    undo(); undo();
    return hit && split && stays && !$(".docscroll") && data().blocks.length === n && !data().blocks.find(x => x.id === id).runs.map(r => r.text).join("").endsWith("注记");
  });
  await ok("canvas: note card moves by its grip and resizes by its handle, set_note only", () => {
    const n0 = canvas().note, g = $(".note .grip").getBoundingClientRect(), z = camOf()[2], blocks = JSON.stringify(data().blocks);
    let o = { id: pid++ };
    down(g.left + 50, g.top + 5, o); move(g.left + 80, g.top + 25, o); up(g.left + 80, g.top + 25, o);
    const n1 = canvas().note;
    const rz = $(".note > .rz"); rz.style.display = "block";
    const r = rz.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2; o = { id: pid++ };
    down(x, y, o); move(x + 50, y, o); up(x + 50, y, o);
    rz.style.display = "";
    const n2 = canvas().note;
    const small = Math.abs(n1.x - n0.x - 30 / z) < 0.01 && Math.abs(n1.y - n0.y - 20 / z) < 0.01 && Math.abs(n2.width - n0.width - 50 / z) < 0.01;
    const cam = camOf().join();
    undo(); undo();
    return small && camOf().join() === cam && JSON.stringify(data().blocks) === blocks && canvas().note.width === n0.width && canvas().note.x === n0.x;
  });
  await ok("canvas: wheel pans, ctrl wheel zooms around the pointer, buttons and Fit work", () => {
    const b = box(), [x0, y0, z0] = camOf();
    cv().dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaX: 30, deltaY: 40, clientX: b.left + 400, clientY: b.top + 300 }));
    const [x1, y1, z1] = camOf(); const panned = x1 === x0 - 30 && y1 === y0 - 40 && z1 === z0;
    const px = 400, py = 300, wx = (px - x1) / z1, wy = (py - y1) / z1;
    cv().dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, ctrlKey: true, deltaY: -100, clientX: b.left + px, clientY: b.top + py }));
    const [x2, y2, z2] = camOf();
    const zoomed = z2 > z1 && Math.abs(wx * z2 + x2 - px) < 0.01 && Math.abs(wy * z2 + y2 - py) < 0.01;
    $('.zoom [aria-label="Zoom in"]').click();
    const up1 = camOf()[2] > z2;
    for (let i = 0; i < 20; i++) $('.zoom [aria-label="Zoom out"]').click();
    const floor = camOf()[2] === 0.25 && $(".cv-pct").textContent.includes("25");
    $(".cv-fit").click();
    const r = $(".note").getBoundingClientRect(), bb = box();
    const inView = r.left >= bb.left - 1 && r.right <= bb.right + 1 && r.top >= bb.top - 1;
    return panned && zoomed && up1 && floor && inView;
  });
  await ok("canvas: select tool drag on empty space pans and writes nothing", () => {
    useTool("select");
    const [x, y] = emptySpot(), s = snapshot(), [cx, cy] = camOf(), o = { id: pid++ };
    if (document.elementFromPoint(x, y)?.closest(".note,.ci")) { $('.zoom [aria-label="Reset zoom"]').click(); }
    down(x, y, o); const grabbing = cv().classList.contains("grabbing");
    move(x - 50, y - 20, o); up(x - 50, y - 20, o);
    const [nx, ny] = camOf();
    return grabbing && !cv().classList.contains("grabbing") && Math.abs(nx - cx + 50) < 1 && Math.abs(ny - cy + 20) < 1 && snapshot() === s;
  });
  await ok("canvas: tool keys work only when not typing, Escape returns to select", () => {
    const pressed = () => $('.dock [aria-pressed="true"]').dataset.tool;
    document.activeElement?.blur?.();
    const results = [];
    for (const [k, id] of [["p", "pen"], ["h", "highlighter"], ["e", "eraser"], ["s", "sticky"], ["r", "rect"], ["o", "ellipse"], ["a", "arrow"], ["t", "text"], ["v", "select"]]) { key(document.body, k); results.push(pressed() === id); }
    key(document.body, "p");
    const drawCursor = cv().classList.contains("draw") && getComputedStyle(cv()).cursor === "crosshair";
    key(document.body, "Escape");
    const back = pressed() === "select" && !cv().classList.contains("draw");
    setCaret($(".note .t")); key(document.activeElement, "p");
    const stayed = pressed() === "select";
    key(document.activeElement, "Escape");
    return results.every(Boolean) && drawCursor && back && stayed && !document.activeElement?.isContentEditable;
  });
  await ok("canvas: a long gesture splits at 65,536 samples with nothing lost", () => {
    useTool("pen");
    const b = box(), n = inkIds().length, o = { id: pid++ }, total = 65700, x0 = b.left + 300, y0 = b.top + 300;
    down(x0, y0, o);
    for (let i = 1; i <= total; i++) move(x0 + (i % 200) / 2, y0 + (i % 317) / 3, o);
    up(x0, y0, o);
    const added = canvas().ink.slice(n);
    const counts = added.map(s => s.points.length);
    const last = added[0].points.at(-1), first = added[1]?.points[0];
    undo(); // One gesture is one undo step, however many strokes it became.
    useTool("select");
    return added.length === 2 && counts[0] === 65536 && counts[1] > 100 && last[0] === first[0] && last[1] === first[1] && inkIds().length === n;
  });
  await ok("canvas: coordinates stay finite and inside the limit far from home", () => {
    const camera0 = camOf();
    cv().dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaX: 1e15, deltaY: -1e15, clientX: 500, clientY: 300 }));
    useTool("pen"); const b = box(), n = inkIds().length;
    stroke([b.left + 300, b.top + 200], [b.left + 340, b.top + 240], 5);
    const s = canvas().ink.at(-1), okay = inkIds().length === n + 1 && s.points.every(p => p.every(v => Number.isFinite(v) && Math.abs(v) <= 1e9));
    undo();
    useTool("select");
    $(".cv-fit").click();
    return okay && camOf().every(Number.isFinite) && camera0.length === 3;
  });
  await ok("canvas: Arabic mirrors the tools, and not the world", () => {
    document.documentElement.dir = "rtl";
    const b = box(), dock = $(".dock").getBoundingClientRect(), zoom = $(".zoom").getBoundingClientRect();
    const r = dock.right > b.right - 80 && zoom.left < b.left + 240 && getComputedStyle($(".world")).direction === "ltr";
    document.documentElement.dir = "ltr";
    return r;
  });
  await ok("canvas: switching views any number of times loses nothing", () => {
    const s = snapshot();
    for (let i = 0; i < 5; i++) { $('.cv-mode [data-view="page"]').click(); if (cv() || !$(".docscroll")) return false; $('.cv-mode [data-view="canvas"]').click(); if (!cv()) return false; }
    return snapshot() === s && $(`.ci[data-id="${stickyId}"] .txt`).textContent === "你好，画布";
  });
  await ok("canvas: page view names what is on the canvas, with a way back", async () => {
    $('.cv-mode [data-view="page"]').click();
    await wait(0);
    const line = $(".cv-truth");
    const says = !!line && line.textContent.length > 0;
    if (!says) return false;
    line.querySelector("button").click();
    const opened = !!cv();
    $('.cv-mode [data-view="page"]').click();
    return opened && !!$(".docscroll .blocks") && data().view === "page";
  });
  await ok("canvas: the switch hides on Settings, and comes back", async () => {
    $("#settingsBtn").click(); await wait(0);
    const hidden = $(".cv-mode").hidden;
    api.openPage(state.cur); await wait(0);
    return hidden && !$(".cv-mode").hidden;
  });
  await ok("canvas: touch draws until a pen is seen, then it pans", async () => {
    $('.cv-mode [data-view="canvas"]').click();
    useTool("pen");
    const b = box(), n = inkIds().length;
    stroke([b.left + 300, b.top + 150], [b.left + 400, b.top + 150], 8, { type: "touch" });
    const touched = inkIds().length === n + 1;
    undo(); useTool("pen");
    stroke([b.left + 300, b.top + 150], [b.left + 400, b.top + 150], 8, { type: "pen", pressure: 0.9 });
    const s = canvas().ink.at(-1), pressure = s.points.every(p => p[2] === 0.9);
    undo(); useTool("pen");
    await wait(600);
    const [cx, cy] = camOf(), m = inkIds().length;
    stroke([b.left + 300, b.top + 150], [b.left + 360, b.top + 190], 8, { type: "touch" });
    const [nx, ny] = camOf();
    const panned = inkIds().length === m && Math.abs(nx - cx - 60) < 1 && Math.abs(ny - cy - 40) < 1;
    // Two fingers always pan and zoom.
    const z0 = camOf()[2], f = [{ id: 901, type: "touch" }, { id: 902, type: "touch" }];
    down(b.left + 400, b.top + 300, f[0]); down(b.left + 500, b.top + 300, f[1]);
    move(b.left + 350, b.top + 300, f[0]); move(b.left + 550, b.top + 300, f[1]);
    up(b.left + 350, b.top + 300, f[0]); up(b.left + 550, b.top + 300, f[1]);
    const pinched = camOf()[2] > z0 && inkIds().length === m;
    key(document.body, "Escape");
    return touched && pressure && panned && pinched;
  });

  // Put everything back for the tests that follow.
  await ok("canvas: leaves the page as it found it", () => {
    const c = canvas(), ops = [];
    if (c.ink.length !== before.canvas.ink.length) ops.push({ op: "change_ink", page: state.cur, add: [], remove: c.ink.filter(s => !before.canvas.ink.some(o => o.id === s.id)).map(s => s.id) });
    for (const it of c.items) if (!before.canvas.items.some(o => o.id === it.id)) ops.push({ op: "delete_item", page: state.cur, id: it.id });
    if (data().view !== "page") ops.push({ op: "set_view", page: state.cur, view: "page" });
    if (ops.length) api.edit(...ops);
    api.openPage(page0);
    return !cv() && !!$(".docscroll .blocks") && pageNow().meta.id === page0 && canvas().items.length === before.canvas.items.length && canvas().ink.length === before.canvas.ink.length && !$(".cv-truth");
  });
}
