// Databases, driven through the real slash menu, inputs and pointer events.
export async function run({ api, ok, wait, key, type, first, pageNow, engine, state, setCaret, textOf, openPage, $, $$ }) {
  const home = state.cur;
  const undoKey = () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "z", metaKey: true, bubbles: true, cancelable: true }));
  const db = id => JSON.parse(engine.database(id));
  const fill = (input, value) => { input.focus(); input.value = value; input.dispatchEvent(new Event("input", { bubbles: true })); };
  // A headless window has no focus to lose, so the event is sent as well as the call.
  const leave = n => { n.dispatchEvent(new FocusEvent("blur")); n.blur(); };
  const k = id => $(`[data-k="${id}"]`);
  const slash = async word => { type("/" + word); await wait(5); key(document.activeElement, "Enter"); await wait(5); };
  const page = title => {
    const id = api.uid();
    api.edit({ op: "create_page", id, parent: null, title }, { op: "insert_block", page: id, after: null, block: { id: api.uid(), kind: "paragraph", checked: false, runs: [] } });
    openPage(id); setCaret(first());
    return id;
  };
  const menuPick = label => { const item = $$(".pop .mi").find(n => n.textContent === label); if (!item) throw new Error("no menu item " + label); item.click(); };
  const pointer = (n, type, x, y) => n.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 1, button: 0, clientX: x, clientY: y, isPrimary: true }));
  const center = n => { const r = n.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };

  const host = page("Database host");
  let id;
  await ok("db: slash inserts a block with a new database and three statuses", async () => {
    await slash("database");
    const b = pageNow().blocks.find(x => x.kind === "database");
    if (!b?.target) return false;
    id = b.target;
    const d = db(id);
    return d.statuses.map(s => s.name).join() === "To do,In progress,Done" && d.statuses.map(s => s.group).join() === "todo,active,done" && d.view === "table" && !!$(`.b.db .db[data-db="${id}"]`) && document.activeElement === k("dbname");
  });
  await ok("db: text can still follow the block", () => pageNow().blocks.at(-1).kind === "paragraph" && !!textOf(pageNow().blocks.at(-1).id));
  await ok("db: an empty table says what to do", () => $(".dbempty")?.textContent === "No rows yet. Add the first one below." && $$(".grid tbody tr").length === 0);
  await ok("db: the name commits on enter and cancels on escape", () => {
    const n = k("dbname");
    fill(n, "Tasks"); key(n, "Enter");
    fill(n, "Wrong"); key(n, "Escape");
    return db(id).name === "Tasks" && n.value === "Tasks";
  });
  await ok("db: add row, focused on its name", async () => {
    k("addrow").click();
    await wait(5); // Focus must survive whatever runs after the click.
    const d = db(id);
    return d.rows.length === 1 && document.activeElement === k(`name:${d.rows[0].id}`) && !$(".dbempty");
  });
  await ok("db: new from the head adds another row", () => { k("new").click(); return db(id).rows.length === 2; });
  let r1, r2;
  await ok("db: a cell edit commits on enter and moves down", () => {
    [r1, r2] = db(id).rows.map(r => r.id);
    const n = k(`name:${r1}`);
    fill(n, "First task"); key(n, "Enter");
    return db(id).rows[0].name === "First task" && document.activeElement === k(`name:${r2}`);
  });
  await ok("db: escape cancels a cell edit", () => {
    const n = k(`name:${r1}`);
    fill(n, "Nope"); key(n, "Escape");
    return n.value === "First task" && db(id).rows[0].name === "First task";
  });
  await ok("db: leaving a cell commits", () => {
    const n = k(`name:${r2}`);
    fill(n, "Second مهمة"); leave(n);
    return db(id).rows[1].name === "Second مهمة";
  });
  await ok("db: cell edits undo", async () => {
    undoKey(); await wait(5);
    const undone = db(id).rows[1].name === "";
    const n = k(`name:${r2}`); fill(n, "Second مهمة"); leave(n);
    return undone && !!$(`.b.db .db[data-db="${id}"]`);
  });
  await ok("db: a date column uses a date input and stores the date", () => {
    const col = db(id).columns.find(c => c.kind === "date");
    const n = k(`cell:${r1}:${col.id}`);
    if (n.type !== "date" || !n.hasAttribute("data-empty")) return false;
    n.focus(); n.value = "2026-12-01"; n.dispatchEvent(new Event("change", { bubbles: true }));
    return db(id).rows[0].values[col.id] === "2026-12-01" && !n.hasAttribute("data-empty");
  });
  await ok("db: the status menu changes status, by icon and word", async () => {
    const b = k(`status:${r1}`);
    if (b.querySelector(".st svg") == null || b.textContent !== "To do") return false;
    b.click();
    if (document.activeElement !== $(".pop .mi")) return false;
    const pop = $(".pop");
    pop.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    if (document.activeElement.textContent !== "In progress") return false;
    document.activeElement.click();
    const d = db(id), after = k(`status:${r1}`);
    return d.statuses.find(s => s.id === d.rows[0].status).name === "In progress" && after.textContent === "In progress" && document.activeElement === after;
  });
  await ok("db: escape leaves the status menu and returns focus", () => {
    const b = k(`status:${r1}`); b.click();
    $(".pop").dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    return !$(".pop") && document.activeElement === b;
  });
  await ok("db: the status is plain, not a pill", () => {
    const st = getComputedStyle($(".st"));
    return st.backgroundColor === "rgba(0, 0, 0, 0)" && parseFloat(st.borderRadius) <= 4;
  });
  let cText, cNum;
  await ok("db: add a column with a name and type", () => {
    k("addcol").click();
    const form = $(".colform"), name = $(".colform .colname");
    if (document.activeElement !== name) return false;
    key(name, "Enter");
    const refused = form.querySelector(".dberror") && !form.querySelector(".dberror").hidden && db(id).columns.length === 1;
    fill(name, "Points"); $(".colform select").value = "number"; key(name, "Enter");
    const c = db(id).columns.find(x => x.name === "Points");
    cNum = c?.id;
    return !!refused && c?.kind === "number" && !$(".colform") && !!k(`cell:${r1}:${c.id}`);
  });
  await ok("db: escape cancels the column form", () => {
    k("addcol").click(); fill($(".colform .colname"), "Nope"); key($(".colform .colname"), "Escape");
    return !$(".colform") && document.activeElement === k("addcol") && !db(id).columns.some(c => c.name === "Nope");
  });
  await ok("db: a number column refuses text, with words", () => {
    const n = k(`cell:${r1}:${cNum}`);
    fill(n, "abc"); key(n, "Enter");
    const shown = n.getAttribute("aria-invalid") === "true" && !$(".db .dbmsg").hidden && $(".db .dbmsg").textContent.includes("Points");
    const unsaved = db(id).rows[0].values[cNum] === undefined;
    key(n, "Escape");
    const cleared = !n.hasAttribute("aria-invalid") && $(".db .dbmsg").hidden;
    fill(n, "12.5"); key(n, "Enter");
    return shown && unsaved && cleared && db(id).rows[0].values[cNum] === "12.5";
  });
  await ok("db: tab order runs through the row's cells", () => {
    const stops = [...$$("tr[data-row] input, tr[data-row] button")].filter(n => n.tabIndex >= 0 && n.closest(`[data-row="${r1}"]`)).map(n => n.dataset.k.split(":")[0]);
    return stops.join() === "name,status,cell,cell,del";
  });
  await ok("db: hide a column, keep its cells, restore it", () => {
    k(`col:${cNum}`).click(); menuPick("Hide column");
    const hidden = db(id).columns.find(c => c.id === cNum).hidden && !k(`cell:${r1}:${cNum}`) && db(id).rows[0].values[cNum] === "12.5";
    const b = k("hidden");
    if (!hidden || b.textContent !== "1 hidden") return false;
    b.click(); menuPick("Points");
    return !db(id).columns.find(c => c.id === cNum).hidden && k(`cell:${r1}:${cNum}`).value === "12.5" && !k("hidden");
  });
  await ok("db: delete a row and undo it", async () => {
    k("addrow").click();
    const r3 = db(id).rows[2].id;
    k(`del:${r3}`).click();
    if (db(id).rows.length !== 2) return false;
    undoKey(); await wait(5);
    if (db(id).rows.length !== 3) return false;
    k(`del:${r3}`).click();
    return db(id).rows.length === 2;
  });
  await ok("db: a hostile value stays text", () => {
    const n = k(`name:${r2}`);
    fill(n, "<img src=x onerror=alert(3)>"); leave(n);
    api.renderPage();
    return !$(".db img") && k(`name:${r2}`).value.includes("<img");
  });

  /* Board */
  const lane = name => $$(".lane").find(l => l.querySelector("header .st").textContent === name);
  const cards = name => [...lane(name).querySelectorAll(".card:not(.adding)")].map(c => c.querySelector("b").textContent);
  await ok("db: the board has a lane per status, with counts", () => {
    k("view:board").click();
    const names = $$(".lane header .st").map(n => n.textContent).join();
    return db(id).view === "board" && names === "To do,In progress,Done" && cards("In progress").join() === "First task" && lane("In progress").querySelector("header > span:last-child").textContent === "1";
  });
  await ok("db: an empty lane says so", () => lane("Done").querySelector(".laneempty")?.textContent === "No cards");
  await ok("db: dragging a card lifts it, lights the lane and moves it", async () => {
    const card = $$(".card").find(c => c.textContent.includes("First task"));
    const [x, y] = center(card), [tx, ty] = center(lane("Done"));
    pointer(card, "pointerdown", x, y);
    pointer(card, "pointermove", x + 10, y);
    const lift = $(".card.lift");
    if (!lift || getComputedStyle(lift).position !== "fixed") return false;
    pointer(card, "pointermove", tx, ty);
    if (!lane("Done").classList.contains("over")) return false;
    if (Math.abs(parseFloat(lift.style.left) - (tx - (x - card.getBoundingClientRect().left))) > 1) return false;
    pointer(card, "pointerup", tx, ty);
    const d = db(id), row = d.rows.find(r => r.id === r1);
    return d.statuses.find(s => s.id === row.status).name === "Done" && cards("Done").includes("First task") && !$(".card.lift") && !$(".lane.over");
  });
  await ok("db: a board drag undoes", async () => {
    undoKey(); await wait(5);
    const back = cards("In progress").includes("First task");
    const d = db(id), row = d.rows.find(r => r.id === r1);
    const done = d.statuses.find(s => s.group === "done").id;
    api.edit({ op: "set_row_field", id, row: row.id, key: "status", value: done });
    api.renderAll();
    return back && d.view === "board" && cards("Done").includes("First task");
  });
  await ok("db: escape cancels a drag", () => {
    const card = $$(".card").find(c => c.textContent.includes("First task"));
    const [x, y] = center(card), [tx, ty] = center(lane("To do"));
    pointer(card, "pointerdown", x, y); pointer(card, "pointermove", tx, ty);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    const clear = !$(".card.lift") && !$(".lane.over");
    pointer(card, "pointerup", tx, ty);
    return clear && cards("Done").includes("First task");
  });
  await ok("db: clicking a card edits it in place", () => {
    const card = $$(".card").find(c => c.textContent.includes("First task"));
    const [x, y] = center(card);
    pointer(card, "pointerdown", x, y); pointer(card, "pointerup", x, y);
    const input = card.querySelector(".cardedit");
    if (!input || document.activeElement !== input) return false;
    input.value = "Renamed"; key(input, "Enter");
    return db(id).rows.find(r => r.id === r1).name === "Renamed" && card.querySelector("b").textContent === "Renamed" && document.activeElement === card;
  });
  await ok("db: escape cancels a card edit", () => {
    const card = $$(".card").find(c => c.textContent.includes("Renamed"));
    key(card, "Enter");
    const input = card.querySelector(".cardedit"); input.value = "Nope"; key(input, "Escape");
    return card.querySelector("b").textContent === "Renamed" && db(id).rows.find(r => r.id === r1).name === "Renamed" && document.activeElement === card;
  });
  await ok("db: arrow keys move a card between lanes", () => {
    const card = k(`card:${r1}`); card.focus();
    key(card, "ArrowLeft");
    return cards("In progress").includes("Renamed") && document.activeElement === k(`card:${r1}`);
  });
  await ok("db: each lane adds a card, and stays open for the next", () => {
    k(`add:${db(id).statuses[0].id}`).click();
    let input = lane("To do").querySelector(".cardedit");
    input.value = "Lane card"; key(input, "Enter");
    input = lane("To do").querySelector(".cardedit");
    if (!input || document.activeElement !== input || !cards("To do").includes("Lane card")) return false;
    key(input, "Escape");
    return !lane("To do").querySelector(".cardedit") && document.activeElement === k(`add:${db(id).statuses[0].id}`);
  });
  await ok("db: an empty new card is not created", () => {
    const before = db(id).rows.length;
    k("new").click();
    const input = lane("To do").querySelector(".cardedit");
    leave(input);
    return db(id).rows.length === before && !lane("To do").querySelector(".cardedit");
  });
  await ok("db: lanes follow the writing direction", () => {
    const ltr = lane("To do").getBoundingClientRect().left < lane("Done").getBoundingClientRect().left;
    document.documentElement.dir = "rtl";
    const rtl = lane("To do").getBoundingClientRect().left > lane("Done").getBoundingClientRect().left;
    document.documentElement.dir = "ltr";
    return ltr && rtl;
  });
  await ok("db: a write from elsewhere repaints the board", async () => {
    document.activeElement?.blur();
    api.edit({ op: "add_row", id, after: null, row: { id: api.uid(), name: "From the assistant", status: db(id).statuses[0].id, values: {} } });
    await wait(5);
    return cards("To do").includes("From the assistant");
  });

  /* Page and block interplay */
  await ok("db: a missing database says so, without breaking the page", () => {
    api.edit({ op: "insert_block", page: host, after: pageNow().blocks.at(-1).id, block: { id: api.uid(), kind: "database", checked: false, runs: [], target: "missing" } });
    api.renderPage();
    const shown = $$(".db .dberror").some(n => n.textContent.includes("not on this device"));
    undoKey();
    return shown;
  });
  await ok("db: backspace below a database keeps the text", async () => {
    openPage(host);
    const blocks = pageNow().blocks, at = blocks.findIndex(b => b.kind === "database"), below = blocks[at + 1];
    setCaret(textOf(below.id), 0); type("stays");
    setCaret(textOf(below.id), 0); key(textOf(below.id), "Backspace");
    const b = pageNow().blocks.find(x => x.id === below.id);
    return b.runs[0]?.text === "stays" && !pageNow().blocks[at].runs.length && document.activeElement === textOf(blocks[at].id);
  });

  /* Full page */
  let pageId, full;
  await ok("db: the sidebar creates a full-page database", () => {
    $("#newDbBtn").click();
    const m = api.metaOf(state.cur);
    pageId = m.id; full = m.database;
    return !!full && !$(".blocks") && !!$(`.dbpage .db[data-db="${full}"]`) && document.activeElement === $(".dbtitle") && !$(".dbpage .dbname");
  });
  await ok("db: the page title names the page and the database", () => {
    const n = $(".dbtitle");
    fill(n, "Roadmap"); key(n, "Enter");
    return api.metaOf(pageId).title === "Roadmap" && db(full).name === "Roadmap" && $(".tab.on .tl").textContent === "Roadmap" && $$("#tree .row.on .open").some(x => x.textContent === "Roadmap");
  });
  await ok("db: a full-page database works and undoes", async () => {
    $(".dbpage [data-k=addrow]").click();
    const r = db(full).rows[0].id, n = k(`name:${r}`);
    fill(n, "Row on a page"); key(n, "Enter");
    undoKey(); await wait(5);
    const undone = db(full).rows[0].name === "" && !!$(".dbpage .db");
    return undone && document.querySelector(`.dbpage [data-k="name:${r}"]`).value === "";
  });
  await ok("db: leaving and returning draws the right thing", async () => {
    openPage(host);
    const doc = !!$(".blocks") && !$(".dbpage");
    openPage(pageId);
    return doc && !!$(".dbpage") && !$(".blocks") && api.currentPage().meta.id === pageId;
  });
  await ok("db: the side panel follows a full-page database", () => $("#outline").textContent === "Headings appear here." && !!$("#rpBacklinks"));

  api.edit({ op: "set_trashed", id: pageId, value: true }, { op: "set_trashed", id: host, value: true });
  openPage(home);
}
