// Every animation here demonstrates something the product does. No libraries.
const root = document.documentElement;
root.classList.add("js");
const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
if (still) root.classList.add("still");
const $ = id => document.getElementById(id);
const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const ease = t => 1 - Math.pow(1 - t, 3);
const inkColor = () => getComputedStyle(root).getPropertyValue("--ink").trim();

/* ---------- One frame loop, alive only while something it drives is on screen ---------- */
const active = new Set();
let frame = 0;
const tick = t => { frame = 0; active.forEach(f => f(t)); if (active.size) frame = requestAnimationFrame(tick); };
function whileVisible(el, fn, margin = "10%") {
  new IntersectionObserver(([e]) => {
    if (e.isIntersecting) { active.add(fn); frame ||= requestAnimationFrame(tick); } else active.delete(fn);
  }, { rootMargin: margin }).observe(el);
}
function once(selector, cls, amount = .25) {
  const io = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) { e.target.classList.add(cls); io.unobserve(e.target); } }), { threshold: amount });
  document.querySelectorAll(selector).forEach(el => io.observe(el));
}
once(".reveal", "in", .12);
once(".end .marked", "drawn", .6);
once(".unpack", "drawn", .3);
requestAnimationFrame(() => requestAnimationFrame(() => document.querySelector(".hero-copy")?.classList.add("drawn")));
setTimeout(() => $("penHint")?.classList.add("drawn"), 1300);

/* ---------- Hero: real ink, thinner when the pen moves fast ---------- */
{
  const hero = $("hero"), canvas = $("heroInk"), ctx = canvas.getContext("2d");
  let strokes = [], cur = null;
  const dpr = () => Math.min(devicePixelRatio || 1, 2);
  const size = () => { canvas.width = hero.clientWidth * dpr(); canvas.height = hero.clientHeight * dpr(); paint(); };
  function segment(a, b) {
    ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.lineWidth = b.w; ctx.stroke();
  }
  function paint() {
    ctx.setTransform(dpr(), 0, 0, dpr(), 0, 0);
    ctx.clearRect(0, 0, hero.clientWidth, hero.clientHeight);
    ctx.lineCap = "round"; ctx.strokeStyle = inkColor();
    for (const s of strokes) for (let i = 1; i < s.length; i++) segment(s[i - 1], s[i]);
  }
  const at = e => { const r = hero.getBoundingClientRect(); return { x: e.clientX - r.left, y: e.clientY - r.top }; };
  const blocked = e => e.target.closest("a,button") || e.pointerType === "touch" || e.button !== 0;
  hero.addEventListener("pointerdown", e => {
    if (blocked(e)) return;
    e.preventDefault(); getSelection()?.removeAllRanges();
    hero.setPointerCapture(e.pointerId);
    const p = at(e);
    cur = [{ ...p, w: e.pointerType === "pen" ? 1 + e.pressure * 6 : 3, t: e.timeStamp }];
    strokes.push(cur);
    $("penHintText").textContent = "Pens feel pressure in the real app.";
    $("penClear").hidden = false;
  });
  hero.addEventListener("pointermove", e => {
    if (!cur) return;
    for (const ev of e.getCoalescedEvents?.() ?? [e]) {
      const p = at(ev), prev = cur.at(-1), d = Math.hypot(p.x - prev.x, p.y - prev.y);
      if (d < 1) continue;
      const speed = d / Math.max(1, ev.timeStamp - prev.t);
      const target = ev.pointerType === "pen" ? 1 + ev.pressure * 6 : clamp(4.6 - speed * 1.5, 1.4, 4.6);
      const next = { ...p, w: prev.w + (target - prev.w) * .28, t: ev.timeStamp };
      cur.push(next);
      ctx.setTransform(dpr(), 0, 0, dpr(), 0, 0); ctx.lineCap = "round"; ctx.strokeStyle = inkColor();
      segment(prev, next);
    }
  });
  const end = () => { cur = null; };
  hero.addEventListener("pointerup", end); hero.addEventListener("pointercancel", end);
  $("penClear").addEventListener("click", () => { strokes = []; paint(); $("penClear").hidden = true; $("penHintText").textContent = "This page takes ink. Draw on it."; });
  new ResizeObserver(size).observe(hero);
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", paint);

  // The product settles flat as you reach it.
  if (!still) {
    const pic = document.querySelector("#heroStage picture");
    const stage = $("heroStage");
    whileVisible(stage, () => {
      const r = stage.getBoundingClientRect(), p = clamp(1 - (r.top - innerHeight * .08) / (innerHeight * .72));
      const e = ease(p);
      pic.style.transform = `rotateX(${(1 - e) * 16}deg) scale(${.93 + e * .07})`;
    });
  }
}

/* ---------- Page to canvas, driven by the scrollbar or the switch ---------- */
{
  const section = $("page"), sheet = $("sheet"), world = $("world"), dots = $("dots");
  const items = Object.fromEntries([...world.querySelectorAll(".m-item")].map(el => [el.dataset.k, el]));
  const inkEl = $("worldInk"), arrow = $("wArrow"), head = $("wHead"), circle = $("wCircle");
  const wide = { W: 1120, H: 640, col: { x: 300, w: 520, y: 70 },
    canvas: {
      title: { x: 60, y: 52, w: 400 }, p1: { x: 60, y: 138, w: 370 },
      t1: { x: 620, y: 368, w: 400 }, t2: { x: 620, y: 410, w: 400 }, t3: { x: 620, y: 452, w: 400 },
      s1: { x: 530, y: 56, w: 240, r: -2.5 }, s2: { x: 830, y: 92, w: 190, r: 3 },
      r1: { x: 70, y: 420, w: 170, h: 84 }, r2: { x: 340, y: 404, w: 180, h: 116 } },
    arrow: "M244 462C276 452 304 452 334 460", head: "M322 448l13 13-16 8", circle: "M640 346C790 318 1010 312 1040 372C1066 440 1040 510 940 524C820 540 640 536 590 482C556 432 566 360 665 340" };
  const narrow = { W: 560, H: 860, col: { x: 40, w: 480, y: 56 },
    canvas: {
      title: { x: 34, y: 34, w: 360 }, p1: { x: 34, y: 116, w: 320 },
      t1: { x: 58, y: 648, w: 450 }, t2: { x: 58, y: 692, w: 450 }, t3: { x: 58, y: 736, w: 450 },
      s1: { x: 340, y: 262, w: 190, r: -2.5 }, s2: { x: 44, y: 276, w: 180, r: 3 },
      r1: { x: 44, y: 440, w: 170, h: 84 }, r2: { x: 330, y: 424, w: 180, h: 116 } },
    arrow: "M218 482C256 472 290 472 322 480", head: "M310 468l13 13-16 8", circle: "M120 622C250 604 490 600 520 668C540 730 520 790 420 806C300 820 90 816 36 760C10 700 30 640 140 618" };
  let L = wide, heights = {}, target = 0, shown = 0, forced = null;

  function measure() {
    const w = sheet.clientWidth, h = sheet.clientHeight;
    L = w / h < .92 ? narrow : wide;
    const s = Math.min(w / L.W, h / L.H);
    world.style.width = `${L.W}px`; world.style.height = `${L.H}px`;
    world.style.transform = `translate(${(w - L.W * s) / 2}px,${(h - L.H * s) / 2}px) scale(${s})`;
    inkEl.setAttribute("width", L.W); inkEl.setAttribute("height", L.H); inkEl.setAttribute("viewBox", `0 0 ${L.W} ${L.H}`);
    arrow.setAttribute("d", L.arrow); head.setAttribute("d", L.head); circle.setAttribute("d", L.circle);
    // The column is laid out in the world's own units, so it matches at every size.
    const gaps = { title: 24, p1: 26, t1: 14, t2: 14, t3: 0 };
    let y = L.col.y;
    for (const k of ["title", "p1", "t1", "t2", "t3"]) {
      items[k].style.width = `${L.col.w}px`;
      heights[k] = { x: L.col.x, y, w: L.col.w };
      y += items[k].offsetHeight + gaps[k];
    }
    for (const k of ["s1", "s2", "r1", "r2"]) {
      const c = L.canvas[k];
      items[k].style.width = `${c.w}px`;
      if (c.h) items[k].style.height = `${c.h}px`;
    }
    shown = -1;
  }
  const lerp = (a, b, t) => a + (b - a) * t;
  function draw(p) {
    const order = ["title", "p1", "t1", "t2", "t3"];
    order.forEach((k, i) => {
      const c = L.canvas[k], a = heights[k], t = ease(clamp((p - i * .05) / .62));
      // Widths stay as written while a block moves, so its text does not rewrap in flight.
      items[k].style.transform = `translate(${lerp(a.x, c.x, t)}px,${lerp(a.y, c.y, t)}px)`;
    });
    ["s1", "s2", "r1", "r2"].forEach((k, i) => {
      const c = L.canvas[k], t = ease(clamp((p - .42 - i * .07) / .26));
      items[k].style.opacity = t;
      items[k].style.transform = `translate(${c.x}px,${c.y + (1 - t) * 26}px) rotate(${(c.r || 0) * t}deg) scale(${.9 + t * .1})`;
    });
    dots.style.opacity = clamp((p - .2) / .35) * .9;
    arrow.style.strokeDashoffset = 1 - clamp((p - .72) / .12);
    head.style.strokeDashoffset = 1 - clamp((p - .83) / .05);
    circle.style.strokeDashoffset = 1 - clamp((p - .78) / .2);
    const isCanvas = p > .5;
    $("toPage").setAttribute("aria-pressed", String(!isCanvas));
    $("toCanvas").setAttribute("aria-pressed", String(isCanvas));
    $("switchThumb").style.transform = `translateX(${isCanvas ? "100%" : "0"})`;
  }
  measure();
  draw(0);
  new ResizeObserver(() => { measure(); draw(target); }).observe(sheet);
  document.fonts?.ready.then(() => { measure(); draw(shown < 0 ? target : shown); });

  if (still) {
    // No flight: the switch simply changes the view.
    $("toPage").onclick = () => { target = 0; draw(0); };
    $("toCanvas").onclick = () => { target = 1; draw(1); };
  } else {
    whileVisible(section, () => {
      const r = section.getBoundingClientRect(), travel = section.offsetHeight - innerHeight;
      target = forced ?? clamp((-r.top - travel * .12) / (travel * .76));
      if (Math.abs(target - shown) < .0005) return;
      shown = shown < 0 ? target : shown + (target - shown) * .16;
      draw(shown);
    }, "20%");
    const go = v => { const travel = section.offsetHeight - innerHeight; scrollTo({ top: section.getBoundingClientRect().top + scrollY + (v ? travel * .9 : travel * .08), behavior: "smooth" }); };
    $("toPage").onclick = () => go(0);
    $("toCanvas").onclick = () => go(1);
  }
}

/* ---------- Languages: typed through the input method each script really uses ---------- */
{
  const typed = $("typed"), cands = $("cands"), typer = $("typer"), tabs = $("tongueTabs");
  const L = [
    { id: "zh-Hans", name: "中文", dir: "ltr", final: "离线也能用，笔记归你所有。", steps: [
      { c: "lixian", o: ["离线", "立宪", "里线"] }, { c: "yenengyong", o: ["也能用", "也能拥"] }, { t: "，" },
      { c: "biji", o: ["笔记", "比基", "必及"] }, { c: "guini", o: ["归你", "贵你"] }, { c: "suoyou", o: ["所有", "索尤"] }, { t: "。" }] },
    { id: "ar", name: "العربية", dir: "rtl", final: "ملاحظاتك تبقى لك، حتى دون اتصال." },
    { id: "ja", name: "日本語", dir: "ltr", final: "ノートはあなたのもの。", steps: [
      { c: "のーと", o: ["ノート", "のーと"] }, { t: "は" }, { c: "あなたの", o: ["あなたの", "貴方の"] }, { c: "もの", o: ["もの", "物", "者"] }, { t: "。" }] },
    { id: "hi", name: "हिन्दी", dir: "ltr", final: "आपके नोट्स आपके ही रहते हैं।" },
    { id: "es", name: "Español", dir: "ltr", final: "Tus notas siguen siendo tuyas." },
  ];
  const graphemes = s => Intl.Segmenter ? [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(s)].map(x => x.segment) : [...s];
  let run = 0, index = 0, visible = false;
  const wait = (ms, id) => new Promise(res => setTimeout(() => res(id === run), ms));
  const buttons = L.map((l, i) => {
    const b = document.createElement("button");
    b.type = "button"; b.lang = l.id; b.textContent = l.name;
    b.addEventListener("click", () => { index = i; play(true); });
    tabs.append(b);
    return b;
  });
  function render(done, comp, showCaret) {
    typed.textContent = done;
    if (comp) { const s = document.createElement("span"); s.className = "comp"; s.textContent = comp; typed.append(s); }
    if (showCaret) { const c = document.createElement("span"); c.className = "caret"; typed.append(c); }
  }
  function showCands(options) {
    if (!options) { cands.hidden = true; return; }
    cands.replaceChildren(...options.map((o, i) => {
      const s = document.createElement("span"), n = document.createElement("small");
      n.textContent = String(i + 1); s.append(n, o);
      return s;
    }));
    cands.hidden = false;
    const c = typed.querySelector(".caret")?.getBoundingClientRect(), b = typer.getBoundingClientRect();
    if (c) {
      cands.style.top = `${c.bottom - b.top + 10}px`;
      cands.style.left = `${clamp(c.left - b.left - 40, 0, Math.max(0, b.width - cands.offsetWidth))}px`;
    }
  }
  function setLang(l) {
    typed.lang = l.id; typed.dir = l.dir;
    buttons.forEach((b, i) => b.setAttribute("aria-pressed", String(L[i] === l)));
  }
  async function play(chosen) {
    const id = ++run, l = L[index];
    setLang(l);
    showCands(null);
    if (still) { render(l.final); return; }
    let done = "";
    render("", "", true);
    if (!await wait(chosen ? 150 : 350, id)) return;
    if (l.steps) {
      for (const st of l.steps) {
        if (st.t) { done += st.t; render(done, "", true); if (!await wait(170, id)) return; continue; }
        const letters = graphemes(st.c);
        for (let i = 1; i <= letters.length; i++) {
          render(done, letters.slice(0, i).join(""), true);
          showCands(st.o);
          if (!await wait(85, id)) return;
        }
        if (!await wait(320, id)) return;
        done += st.o[0];
        showCands(null);
        render(done, "", true);
        if (!await wait(120, id)) return;
      }
    } else {
      for (const g of graphemes(l.final)) {
        done += g;
        render(done, "", true);
        if (!await wait(g === " " ? 90 : 52, id)) return;
      }
    }
    if (!await wait(2400, id)) return;
    if (visible) { index = (index + 1) % L.length; play(false); }
  }
  new IntersectionObserver(([e]) => {
    const was = visible; visible = e.isIntersecting;
    if (visible && !was) play(false); else if (!visible) { run++; showCands(null); render(L[index].final); }
  }, { threshold: .25 }).observe(typer);
  setLang(L[0]);
}

/* ---------- The graph: a real force layout you can pull around ---------- */
{
  const section = $("knowledge"), canvas = $("graphCanvas"), ctx = canvas.getContext("2d");
  const names = [
    ["Beta launch plan", 9, "page"], ["读书笔记", 7, "page"], ["File format", 7, "page"], ["Why switch?", 7, "page"], ["Sync across devices", 6, "page"],
    ["ملاحظات الاجتماع", 6, "page"], ["議事録", 6, "page"], ["#launch", 4.5, "tag"], ["#format", 4.5, "tag"], ["#reading", 4.5, "tag"], ["Mei", 4.5, "person"], ["Tomasz", 4.5, "person"], ["Lin", 4.5, "person"], ["Priya", 4.5, "person"],
    ["Write the release notes", 3.5, "task"], ["Translate the docs", 3.5, "task"], ["Finish Evernote import", 3.5, "task"], ["Review the format spec", 3.5, "task"], ["Open the waitlist", 3.5, "task"],
    ["Import report", 5, "page"], ["Offline first", 5, "page"], ["デザインメモ", 5, "page"], ["Roadmap notes", 5, "page"], ["विचार", 5, "page"],
  ];
  const made = [[0,3],[0,2],[0,7],[0,14],[0,15],[0,16],[0,18],[2,8],[4,8],[2,17],[1,9],[3,7],[14,10],[15,12],[16,13],[17,11],[18,10],[0,19],[19,16],[4,20],[3,20],[6,21],[5,0],[22,0],[23,1],[22,4],[6,22]];
  const guessed = [[1,5],[20,2],[19,3],[21,23],[5,6]];
  const edges = [...made.map(([a, b]) => ({ a, b, g: false })), ...guessed.map(([a, b]) => ({ a, b, g: true }))];
  let w = 0, h = 0, born = 0, drag = null, hover = -1, started = false;
  const nodes = names.map(([label, r, kind], i) => {
    const a = i * 2.399963;
    return { label, r, kind, x: 0, y: 0, vx: 0, vy: 0, a, d: 30 + Math.sqrt(i) * 62, n: new Set() };
  });
  edges.forEach(e => { nodes[e.a].n.add(e.b); nodes[e.b].n.add(e.a); });
  const centre = () => {
    const narrowView = w < 860;
    return { x: narrowView ? w * .5 : w * .62, y: narrowView ? h * .66 : h * .52 };
  };
  function size() {
    const d = Math.min(devicePixelRatio || 1, 2), r = canvas.getBoundingClientRect();
    w = r.width; h = r.height; canvas.width = w * d; canvas.height = h * d;
    ctx.setTransform(d, 0, 0, d, 0, 0);
    const c = centre(), spread = Math.min(1.25, Math.min(w, h * 1.3) / 780);
    if (!started) { nodes.forEach(n => { n.x = c.x + Math.cos(n.a) * n.d * spread; n.y = c.y + Math.sin(n.a) * n.d * spread * .78; }); }
  }
  function step() {
    const c = centre(), rest = Math.min(120, Math.max(60, w / 12));
    for (let i = 0; i < nodes.length; i++) {
      const a = nodes[i];
      a.vx += (c.x - a.x) * .0016; a.vy += (c.y - a.y) * .0024;
      for (let j = i + 1; j < nodes.length; j++) {
        const b = nodes[j], dx = a.x - b.x, dy = a.y - b.y, q = dx * dx + dy * dy + .01;
        if (q > 90000) continue;
        const f = 2600 / q;
        a.vx += dx * f * .02; a.vy += dy * f * .02; b.vx -= dx * f * .02; b.vy -= dy * f * .02;
      }
    }
    for (const e of edges) {
      const a = nodes[e.a], b = nodes[e.b], dx = b.x - a.x, dy = b.y - a.y, d = Math.hypot(dx, dy) || 1, f = (d - rest) * (e.g ? .0008 : .0028);
      a.vx += dx / d * f * d * .02; a.vy += dy / d * f * d * .02; b.vx -= dx / d * f * d * .02; b.vy -= dy / d * f * d * .02;
    }
    for (const n of nodes) {
      if (n === drag) { n.vx = n.vy = 0; continue; }
      n.vx *= .86; n.vy *= .86; n.x += n.vx; n.y += n.vy;
      n.x = clamp(n.x, 14, w - 14); n.y = clamp(n.y, 14, h - 14);
    }
  }
  function draw(t) {
    if (!born) born = t;
    const style = getComputedStyle(root);
    const ink = style.getPropertyValue("--ink").trim(), fg = style.getPropertyValue("--fg").trim(), muted = style.getPropertyValue("--muted").trim(), paper = style.getPropertyValue("--paper").trim();
    ctx.clearRect(0, 0, w, h);
    const grow = still ? 1 : clamp((t - born) / 1400), focus = hover >= 0 ? hover : drag ? nodes.indexOf(drag) : -1;
    for (const e of edges) {
      const a = nodes[e.a], b = nodes[e.b], lit = focus >= 0 && (e.a === focus || e.b === focus);
      ctx.globalAlpha = (focus >= 0 && !lit ? .12 : lit ? 1 : e.g ? .7 : .34) * grow;
      ctx.strokeStyle = lit || e.g ? ink : muted; ctx.lineWidth = lit ? 1.8 : 1.1;
      ctx.setLineDash(e.g ? [4, 5] : []);
      ctx.beginPath(); ctx.moveTo(a.x, a.y);
      const end = grow; ctx.lineTo(a.x + (b.x - a.x) * end, a.y + (b.y - a.y) * end); ctx.stroke();
    }
    ctx.setLineDash([]);
    nodes.forEach((n, i) => {
      const lit = focus < 0 || i === focus || nodes[focus].n.has(i);
      const appear = ease(clamp(grow * 1.6 - i * .03));
      ctx.globalAlpha = (lit ? 1 : .22) * appear;
      ctx.beginPath(); ctx.arc(n.x, n.y, n.r * appear, 0, 7);
      if (n.kind === "page") { ctx.fillStyle = i === focus ? ink : fg; ctx.fill(); }
      else { ctx.fillStyle = paper; ctx.fill(); ctx.strokeStyle = n.kind === "tag" ? ink : fg; ctx.lineWidth = 1.6; ctx.stroke(); }
      const label = n.r >= 5 || i === focus || (focus >= 0 && nodes[focus].n.has(i));
      if (label && w >= 560 || i === focus) {
        ctx.font = `${n.r >= 7 ? 500 : 400} ${n.r >= 7 ? 14 : 13}px Geist, -apple-system, "PingFang SC", "Noto Sans SC", system-ui, sans-serif`;
        ctx.fillStyle = lit ? fg : muted; ctx.textAlign = "left"; ctx.textBaseline = "middle";
        ctx.lineWidth = 4; ctx.strokeStyle = paper; ctx.lineJoin = "round";
        ctx.strokeText(n.label, n.x + n.r + 7, n.y); ctx.fillText(n.label, n.x + n.r + 7, n.y);
      }
    });
    ctx.globalAlpha = 1;
  }
  const pick = e => {
    const r = canvas.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top;
    let best = -1, min = 26 * 26;
    nodes.forEach((n, i) => { const q = (n.x - x) ** 2 + (n.y - y) ** 2; if (q < min) { min = q; best = i; } });
    return { best, x, y };
  };
  canvas.addEventListener("pointerdown", e => {
    const { best } = pick(e);
    if (best < 0 || e.pointerType === "touch") return;
    drag = nodes[best]; canvas.setPointerCapture(e.pointerId); canvas.classList.add("drag");
  });
  canvas.addEventListener("pointermove", e => {
    const { best, x, y } = pick(e);
    if (drag) { drag.x = x; drag.y = y; return; }
    hover = best;
  });
  const release = () => { drag = null; canvas.classList.remove("drag"); };
  canvas.addEventListener("pointerup", release); canvas.addEventListener("pointercancel", release);
  canvas.addEventListener("pointerleave", () => { hover = -1; });
  new ResizeObserver(() => { size(); if (still) draw(1); }).observe(section);
  size();
  whileVisible(section, t => { started = true; if (!still || drag) step(); draw(t); }, "0%");
}

/* ---------- The assistant asks first: the approval is real on this page ---------- */
{
  const lines = [...document.querySelectorAll(".ai-line")], card = $("aiCard"), btns = $("aiBtns"), title = $("aiCardT"), ask = $("aiAsk"), dest = $("aiDest");
  const text = ask.textContent;
  let web = true;
  card.classList.add("wait");
  const btn = (label, cls, fn) => { const b = document.createElement("button"); b.type = "button"; b.className = `btn small ${cls}`; b.textContent = label; b.addEventListener("click", fn); return b; };
  function ready() {
    title.textContent = "Change 3 blocks into tasks";
    lines.forEach(l => l.classList.remove("task"));
    btns.replaceChildren(btn("Approve", "primary", approve), btn("Decline", "", decline));
  }
  function approve() {
    lines.forEach(l => l.classList.add("task"));
    title.textContent = "Done. 3 blocks are tasks now.";
    btns.replaceChildren(btn("Undo", "", ready));
  }
  function decline() {
    title.textContent = "Declined. Nothing changed.";
    btns.replaceChildren(btn("Ask again", "", ready));
  }
  ready();
  const setDest = () => { dest.textContent = web ? "Sent to: your provider" : "Stays on this computer"; };
  const mode = v => {
    web = v;
    $("modeWeb").setAttribute("aria-pressed", String(v)); $("modeLocal").setAttribute("aria-pressed", String(!v));
    $("modeNote").textContent = v ? "Connect Anthropic or anything OpenAI-compatible with your own key. Nothing is sent until you do."
      : "Download a model and run it here, so your notes never leave. An early macOS build, and no real model has been through it yet.";
    setDest();
  };
  $("modeWeb").addEventListener("click", () => mode(true));
  $("modeLocal").addEventListener("click", () => mode(false));
  const io = new IntersectionObserver(async ([e]) => {
    if (!e.isIntersecting) return;
    io.disconnect();
    if (still) { card.classList.remove("wait"); return; }
    const parts = [...text]; ask.textContent = "";
    for (let i = 1; i <= parts.length; i++) { ask.textContent = parts.slice(0, i).join(""); await new Promise(r => setTimeout(r, 38)); }
    await new Promise(r => setTimeout(r, 380));
    card.classList.remove("wait");
  }, { threshold: .35 });
  io.observe($("aiDemo"));
}
