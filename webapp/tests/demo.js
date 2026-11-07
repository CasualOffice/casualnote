// Example content for the website's screenshots and the hosted demo. Opened with ?demo=1.
export async function seedDemo(api) {
  if (api.live().some(p => p.title === "Beta launch plan")) return;
  const uid = api.uid;
  // The empty first page has no place in an example workspace.
  for (const p of api.live()) if (!p.title) api.edit({ op: "set_trashed", id: p.id, value: true });
  const blk = (kind, text, extra = {}) => ({ id: uid(), kind, checked: false, runs: text ? [{ text }] : [], ...extra });
  const page = (id, parent, title) => ({ op: "create_page", id, parent, title });
  const put = (id, blocks) => { let after = null; const ops = []; for (const b of blocks) { ops.push({ op: "insert_block", page: id, after, block: b }); after = b.id; } return ops; };
  const row = (name, status, owner, due) => ({ id: uid(), name, status, values: { owner, due } });
  const d = "launch-tracker";
  const rows = [row("Write the release notes", "s1", "Mei", "2026-11-20"), row("Record the import walkthrough", "s1", "Tomasz", "2026-11-24"), row("Translate the docs into 中文", "s2", "Lin", "2026-11-18"), row("Finish Evernote import", "s2", "Priya", "2026-11-16"), row("Review the file format spec", "s3", "Tomasz", "2026-11-12"), row("Open the beta waitlist", "s4", "Mei", "2026-11-03")];
  const plan = "launch", why = "why-switch", fmt = "file-format", sync = "sync-notes", read = "reading";
  const intro = blk("paragraph", "We open the beta once import is solid. Everything still open is below.");
  const linked = blk("paragraph", "The thinking behind it is in Why would someone switch? and File format. #launch");
  api.edit(
    { op: "create_database", database: { id: d, name: "Beta tracker", view: "board", statuses: [{ id: "s1", name: "To do", group: "todo" }, { id: "s2", name: "In progress", group: "active" }, { id: "s3", name: "In review", group: "review" }, { id: "s4", name: "Done", group: "done" }], columns: [{ id: "owner", name: "Owner", kind: "text", hidden: false }, { id: "due", name: "Due", kind: "date", hidden: false }], rows } },
    page(plan, null, "Beta launch plan"), page(why, plan, "Why would someone switch?"), page(fmt, plan, "File format"), page(sync, plan, "Sync across devices"), page(read, null, "读书笔记"),
    { op: "set_favorite", id: plan, value: true },
    ...put(plan, [intro, linked, blk("database", "", { target: d }), blk("heading2", "What has to be true"), blk("numbered", "Old notes arrive complete, attachments included"), blk("numbered", "Typing stays instant on a long page"), blk("numbered", "The file format is published before anyone depends on it"), blk("heading2", "Open questions"), blk("todo", "Do we import Notion before or after Evernote?"), blk("todo", "Agree the folder layout", { checked: true })]),
    ...put(fmt, [blk("paragraph", "A notebook is an append-only log for each page, with snapshots and a readable Markdown mirror. Export is one documented file. #format"), blk("bulleted", "Each device writes only its own new segments"), blk("bulleted", "The schema is published with the spec")]),
    ...put(sync, [blk("paragraph", "Devices exchange encrypted segments through a folder they already have. Offline edits merge when a device returns. #format")]),
    ...put(read, [blk("paragraph", "中文和英文可以写在一起，搜索也能找到。 #reading"), blk("quote", "给时光以生命，而不是给生命以时光。")]),
    ...put(why, [blk("paragraph", "Notes from Tuesday. Two reasons came up every time:"), blk("bulleted", "Import has to work the first time"), blk("bulleted", "It must feel fast on an old laptop")]),
    { op: "set_view", page: why, view: "canvas" },
    { op: "set_note", page: why, note: { x: 40, y: 30, width: 430 } },
    { op: "put_item", page: why, item: { id: uid(), kind: "sticky", x: 530, y: 40, width: 180, height: 120, color: "amber", text: "Show the import report before anything changes" } },
    { op: "put_item", page: why, item: { id: uid(), kind: "sticky", x: 730, y: 40, width: 180, height: 120, color: "grey", text: "Share one page with a single link" } },
    { op: "put_item", page: why, item: { id: uid(), kind: "sticky", x: 630, y: 190, width: 180, height: 120, color: "amber", text: "离线也能用" } },
    { op: "put_item", page: why, item: { id: uid(), kind: "rect", x: 80, y: 340, width: 170, height: 66, color: "", text: "Old notes app" } },
    { op: "put_item", page: why, item: { id: uid(), kind: "ellipse", x: 380, y: 330, width: 170, height: 86, color: "", text: "Your notes, intact" } },
    { op: "change_ink", page: why, add: [{ id: uid(), kind: "arrow", color: "", width: 2, points: [[252, 372, 0.5], [376, 372, 0.5]] }, { id: uid(), kind: "pen", color: "red", width: 3, points: Array.from({ length: 45 }, (_, i) => { const a = i / 44 * 6.6; return [720 + 112 * Math.cos(a), 250 + 78 * Math.sin(a), 0.35 + 0.35 * Math.sin(i / 44 * Math.PI)]; }) }], remove: [] },
  );
  const text = "Why would someone switch?", plain = intro.runs[0].text.length; void plain;
  const at = linked.runs[0].text.indexOf(text);
  api.edit({ op: "set_link", page: plan, block: linked.id, start: at, end: at + text.length, target: why }, { op: "set_link", page: plan, block: linked.id, start: linked.runs[0].text.indexOf("File format"), end: linked.runs[0].text.indexOf("File format") + 11, target: fmt });
  await api.store.flush();
  api.openPage(plan);
}
