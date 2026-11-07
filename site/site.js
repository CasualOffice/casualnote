// Every animation here demonstrates something the product does. No libraries.
const root = document.documentElement;
root.classList.add("js");
const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
if (still) root.classList.add("still");
const $ = id => document.getElementById(id);
const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const inkColor = () => getComputedStyle(root).getPropertyValue("--ink").trim();

function once(selector, cls, amount = .25) {
  const io = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) { e.target.classList.add(cls); io.unobserve(e.target); } }), { threshold: amount });
  document.querySelectorAll(selector).forEach(el => io.observe(el));
}
once(".reveal", "in", .12);
once(".end .marked", "drawn", .6);
once(".unpack", "drawn", .3);
requestAnimationFrame(() => requestAnimationFrame(() => document.querySelector(".hero-copy")?.classList.add("drawn")));

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

}

/* ---------- The tour: one application in one place, and CSS moves everything ---------- */
{
  const marks = [...document.querySelectorAll(".tour-marks .mark")];
  const parts = [...document.querySelectorAll(".tour [data-step]")], navs = [...document.querySelectorAll(".tour-nav button")];
  const show = n => {
    parts.forEach(el => el.classList.toggle("on", +el.dataset.step === n));
    navs.forEach((b, i) => b.setAttribute("aria-pressed", String(i === n)));
  };
  show(0);
  const io = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) show(marks.indexOf(e.target)); }), { rootMargin: "-50% 0px -50% 0px" });
  marks.forEach(m => io.observe(m));
  navs.forEach((b, i) => b.addEventListener("click", () => marks[i].scrollIntoView({ block: "center", behavior: still ? "auto" : "smooth" })));
  // Decoded ahead of time, so the first change of view has nothing left to wait for.
  document.querySelectorAll(".shot img").forEach(img => img.decode?.().catch(() => {}));
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
