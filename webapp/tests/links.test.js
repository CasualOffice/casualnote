// Links, backlinks, tags and embeds, driven through the real editor.
export async function run({ api, ok, wait, key, type, first, pageNow, state, setCaret, textOf, openPage, $, $$ }) {
  const home = state.cur;
  const undoKey = () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "z", metaKey: true, bubbles: true, cancelable: true }));
  const page = (title, line) => {
    const id = api.uid(), block = api.uid();
    api.edit({ op: "create_page", id, parent: null, title }, { op: "insert_block", page: id, after: null, block: { id: block, kind: "paragraph", checked: false, runs: line ? [{ text: line }] : [] } });
    return { id, block };
  };
  const items = () => $$(".pop .mi").map(n => n.textContent);
  const on = () => $(".pop .mi.on")?.textContent;
  const fresh = title => { const p = page(title); openPage(p.id); setCaret(first()); return p; };
  const firstRuns = () => pageNow().blocks[0].runs;

  const alpha = page("Alpha target", "alpha body line");
  const alps = page("Alpine notes");
  const beta = page("Beta target");
  const src = fresh("Link source");

  await ok("links: typing [[ opens the page picker", () => { type("see [["); return !!$(".pop") && items().includes("Alpha target") && items().includes("Beta target"); });
  await ok("links: the picker filters as you type", () => { type("alp"); return items().includes("Alpha target") && items().includes("Alpine notes") && !items().includes("Beta target"); });
  await ok("links: arrows move the highlight and wrap", () => {
    const n = first(), start = on();
    key(n, "ArrowDown"); const second = on();
    key(n, "ArrowUp"); const back = on();
    key(n, "ArrowUp"); const wrapped = on();
    key(n, "ArrowDown");
    return start !== second && back === start && wrapped === items().at(-1);
  });
  await ok("links: escape closes the picker and it stays shut", () => {
    const n = first(); key(n, "Escape");
    if ($(".pop")) return false;
    type("h"); if ($(".pop")) return false;
    document.execCommand("delete");
    return !$(".pop") && n.textContent === "see [[alp";
  });
  await ok("links: a new [[ opens it again", () => { type(" [["); const shown = !!$(".pop"); key(first(), "Escape"); document.execCommand("delete"); document.execCommand("delete"); document.execCommand("delete"); return shown && first().textContent === "see [[alp"; });

  const linked = fresh("Link source two");
  await ok("links: selecting inserts title text and a set_link run", () => {
    type("go [[alpha"); key(first(), "Enter");
    const r = firstRuns();
    return first().textContent === "go Alpha target" && r.length === 2 && r[1].text === "Alpha target" && r[1].link === alpha.id && !$(".pop");
  });
  await ok("links: one undo removes the link and the title together", async () => {
    undoKey(); await wait(10);
    const r = firstRuns();
    const undone = r.every(x => !x.link) && r.map(x => x.text).join("") === "go [[alpha";
    // Type again to reopen the picker and put the link back.
    setCaret(first()); type("x"); document.execCommand("delete"); key(first(), "Enter");
    return undone && firstRuns()[1]?.link === alpha.id;
  });
  await ok("links: text typed after a link is not part of it", () => {
    setCaret(first()); type(" more");
    const r = firstRuns(), a = $(".blocks a[data-link]");
    return r[1].text === "Alpha target" && r[2].text === " more" && !r[2].link && a.textContent === "Alpha target" && first().textContent === "go Alpha target more";
  });
  await ok("links: a link is clickable", () => { $(".blocks a[data-link]").click(); const went = state.cur === alpha.id; openPage(linked.id); return went; });
  await ok("links: a trashed target shows as removed, and comes back", async () => {
    api.edit({ op: "set_trashed", id: beta.id, value: true });
    openPage(alpha.id); openPage(linked.id);
    const a = $(".blocks a[data-link]");
    if (a.classList.contains("gone")) return false;
    api.edit({ op: "set_trashed", id: alpha.id, value: true });
    await wait(5);
    const gone = $(".blocks a[data-link]").classList.contains("gone");
    $(".blocks a[data-link]").click();
    const stayed = state.cur === linked.id;
    api.edit({ op: "set_trashed", id: alpha.id, value: false }); api.edit({ op: "set_trashed", id: beta.id, value: false });
    await wait(5);
    return gone && stayed && !$(".blocks a[data-link]").classList.contains("gone");
  });
  await ok("links: a link to a page that does not exist shows as removed", async () => {
    const p = page("Dangling", "ghost");
    api.edit({ op: "set_link", page: p.id, block: p.block, start: 0, end: 5, target: "no-such-page" });
    openPage(p.id); await wait(5);
    const a = $(".blocks a[data-link]");
    const r = a.classList.contains("gone");
    api.edit({ op: "set_trashed", id: p.id, value: true }); openPage(linked.id);
    return r;
  });
  await ok("links: the query can create a new page, undoable as one step", async () => {
    const f = fresh("Creator");
    type("[[Brand new idea");
    if (!items().some(x => x.includes("Brand new idea"))) return false;
    const last = items().length - 1;
    for (let i = 0; i < last; i++) key(first(), "ArrowDown");
    key(first(), "Enter");
    const r = firstRuns()[0], m = api.live().find(x => x.title === "Brand new idea");
    if (!m || r.link !== m.id || r.text !== "Brand new idea") return false;
    if (!api.pageData(m.id).blocks.length) return false;
    const inTree = $$("#tree .row .open").some(n => n.textContent === "Brand new idea");
    undoKey(); await wait(10);
    const gone = !api.live().some(x => x.title === "Brand new idea") && first().textContent === "[[Brand new idea";
    openPage(f.id);
    return inTree && gone;
  });
  await ok("links: an exact title does not offer to create a duplicate", () => {
    fresh("Dup check"); type("[[Beta target");
    const offers = items().some(x => x.includes("⁨"));
    key(first(), "Escape");
    return !offers && items().length === 0;
  });
  await ok("links: a hostile title stays text in picker and link", () => {
    page("<img src=x onerror=alert(1)>");
    fresh("Hostile"); type("[[<img");
    const pick = $$(".pop img").length === 0 && items()[0].includes("<img src=x");
    key(first(), "Enter");
    return pick && !$(".blocks img") && first().textContent === "<img src=x onerror=alert(1)>";
  });

  /* Backlinks */
  await ok("links: backlinks list the linking page with its line, once", async () => {
    openPage(alpha.id); await wait(5);
    const secs = $$("#rpDetails #rpBacklinks"), bl = $$("#rpBacklinks .bl");
    const hit = bl.find(n => n.textContent.includes("Link source two"));
    openPage(linked.id); openPage(alpha.id);
    return secs.length === 1 && $$("#rpBacklinks").length === 1 && !!hit && hit.querySelector("small").textContent.startsWith("go Alpha target");
  });
  await ok("links: a backlink opens its page", () => {
    $$("#rpBacklinks .bl").find(n => n.textContent.includes("Link source two")).click();
    return state.cur === linked.id && !$$("#rpBacklinks .bl").some(n => n.textContent.includes("Link source two"));
  });
  await ok("links: a page nobody links to says so", async () => {
    openPage(alps.id); await wait(5);
    const p = $("#rpBacklinks p");
    return !!p && !$("#rpBacklinks .bl") && p.textContent === "No pages link here yet.";
  });
  await ok("links: backlinks follow an edit that adds a link", async () => {
    openPage(alps.id);
    const other = page("Late linker", "see it");
    api.edit({ op: "set_link", page: other.id, block: other.block, start: 4, end: 6, target: alps.id });
    openPage(other.id); openPage(alps.id); await wait(5);
    return $$("#rpBacklinks .bl").some(n => n.textContent.includes("Late linker"));
  });

  /* Tags */
  await ok("links: tags appear in the sidebar and open a search", async () => {
    fresh("Tagged"); type("about #roadmap and #2026 and #قواعد");
    await wait(450);
    const rows = $$("#tagList .open").map(n => n.textContent);
    if (!rows.includes("#roadmap") || !rows.includes("#قواعد") || rows.includes("#2026")) return false;
    $$("#tagList .open").find(n => n.textContent === "#roadmap").click();
    const shown = $$(".modal .mi").length >= 1 && $("#palette").value === "#roadmap";
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    return shown && !$(".modalbg");
  });

  /* Embeds */
  const slash = async word => { type("/" + word); await wait(5); key(document.activeElement, "Enter"); await wait(5); };
  const host = fresh("Embed host");
  await ok("links: the embed command opens a filtered page picker", async () => {
    await slash("embed");
    const input = $("#embedPick");
    if (!input) return false;
    input.value = "alpha target"; input.dispatchEvent(new Event("input"));
    return $$(".modal .mi").map(n => n.textContent).join("|") === "Alpha target";
  });
  await ok("links: escape leaves the picker without inserting", () => {
    key($("#embedPick"), "Escape");
    return !$(".modalbg") && pageNow().blocks.every(b => b.kind !== "embed");
  });
  await ok("links: embedding refuses the page itself", async () => {
    setCaret(first()); await slash("embed");
    const input = $("#embedPick");
    input.value = "embed host"; input.dispatchEvent(new Event("input"));
    const row = $(".modal .mi");
    key(input, "Enter"); row.click();
    const refused = row.getAttribute("aria-disabled") === "true" && !!$(".modalbg") && pageNow().blocks.every(b => b.kind !== "embed");
    key(input, "Escape");
    return refused;
  });
  await ok("links: an embed shows the page, with a header link", async () => {
    setCaret(first()); await slash("embed");
    const input = $("#embedPick");
    input.value = "alpha target"; input.dispatchEvent(new Event("input"));
    key(input, "Enter"); await wait(5);
    const b = pageNow().blocks.find(x => x.kind === "embed");
    return !!b && b.target === alpha.id && $(".b.embed .embedbox .line").textContent === "alpha body line" && $(".b.embed .embedopen").textContent === "Alpha target" && document.activeElement === textOf(pageNow().blocks.at(-1).id);
  });
  await ok("links: an embed is read only and live", async () => {
    if ($(".embedbox [contenteditable]") || $(".embedbox .t")) return false;
    api.edit({ op: "splice_text", page: alpha.id, block: alpha.block, at: 0, delete: 0, insert: "LIVE " });
    await wait(120);
    return $(".embedbox .line").textContent === "LIVE alpha body line";
  });
  await ok("links: the embed header opens the page", () => { $(".embedopen").click(); const r = state.cur === alpha.id; openPage(host.id); return r; });
  await ok("links: a page that embeds this one cannot be embedded back", async () => {
    openPage(alpha.id); setCaret(first(), Infinity);
    key(first(), "Enter"); await slash("embed");
    const input = $("#embedPick");
    input.value = "embed host"; input.dispatchEvent(new Event("input"));
    const row = $(".modal .mi"), refused = row.getAttribute("aria-disabled") === "true";
    key(input, "Enter");
    const stays = !!$(".modalbg");
    key(input, "Escape"); openPage(host.id);
    return refused && stays;
  });
  await ok("links: a cycle from another device stops instead of hanging", async () => {
    api.edit({ op: "insert_block", page: alpha.id, after: alpha.block, block: { id: api.uid(), kind: "embed", checked: false, runs: [], target: host.id } });
    openPage(host.id); await wait(5);
    const msg = $(".embedbox .embedmsg");
    undoKey(); await wait(5);
    openPage(host.id);
    return !!msg && msg.textContent === "A page cannot embed itself." && !$(".embedbox .embedmsg");
  });
  await ok("links: a trashed embed target says what to do", async () => {
    api.edit({ op: "set_trashed", id: alpha.id, value: true });
    await wait(120);
    const m = $(".embedbox .embedmsg")?.textContent;
    api.edit({ op: "set_trashed", id: alpha.id, value: false });
    await wait(120);
    return m === "This page is in the Trash. Restore it to show it here." && !$(".embedbox .embedmsg");
  });
  await ok("links: an embedded hostile title stays text", async () => {
    const bad = page("<img src=x onerror=alert(2)>", "<b>no</b>");
    const b = pageNow().blocks.at(-1);
    api.edit({ op: "insert_block", page: host.id, after: b.id, block: { id: api.uid(), kind: "embed", checked: false, runs: [], target: bad.id } });
    api.renderPage();
    const r = !$(".embedbox img") && !$(".embedbox .line b") && [...$$(".embedbox .line")].some(n => n.textContent === "<b>no</b>");
    undoKey(); await wait(5);
    return r;
  });
  await ok("links: arrows and backspace pass safely around an embed", async () => {
    openPage(host.id);
    const blocks = pageNow().blocks, em = blocks.findIndex(x => x.kind === "embed"), after = blocks[em + 1];
    setCaret(textOf(after.id), 0); type("kept");
    setCaret(textOf(after.id), 0); key(textOf(after.id), "Backspace");
    const kept = pageNow().blocks[em + 1]?.id === after.id && textOf(after.id).textContent === "kept";
    const stop = textOf(blocks[em].id), onStop = document.activeElement === stop;
    key(stop, "ArrowDown");
    const down = document.activeElement === textOf(after.id);
    setCaret(textOf(after.id), 0); key(textOf(after.id), "Backspace");
    key(textOf(blocks[em].id), "Backspace");
    return kept && onStop && down && pageNow().blocks.every(b => b.kind !== "embed") && document.activeElement?.classList.contains("t");
  });

  openPage(home);
}
