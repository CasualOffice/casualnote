# 47 — UX and feature map

**Status: design only, nothing built.** The [prototype](prototypes/casualnote-prototype.html) is the reference for layout and interaction. It covers the shell, blocks, databases, canvas, graph, assistant, agent access, settings and package export. Screens marked "not in prototype" are specified here for the first time. Shortcut conflicts are **unverified** until the browser gate runs them. Visual rules are in [49](49-DESIGN-SYSTEM.md).

## Outcome

A person opens the app and is writing straight away. Everything the product can do is reachable three ways: where you expect it on screen, by keyboard, and from the palette. Nothing depends on hover or colour alone. Every state is designed, including empty, loading and error, because that is where trust is won or lost (PR-1, PR-2).

## Shell

```
┌──────────┬───────────────────────────────────────┬──────────┐
│          │ tabs                                  │          │
│ Sidebar  ├───────────────────────────────────────┤  Panel   │
│          │ bar: crumbs, view switch, more, panel │ Details  │
│ search   ├───────────────────────────────────────┤    or    │
│ new      │                                       │Assistant │
│ graph    │              content                  │          │
│ agents   │     page · canvas · database · graph  │          │
│ settings │     agent access · settings · trash   │          │
│ tree     │                                       │          │
├──────────┴───────────────────────────────────────┴──────────┤
│ status line: word count · save and sync state                │
└──────────────────────────────────────────────────────────────┘
```

| Region | Holds | Under 1120px | Under 760px | Tracker |
| --- | --- | --- | --- | --- |
| **Sidebar** | Workspace name, Search, New page, Knowledge graph, Agent access, Settings, then Favorites, Pages tree, Trash | Same | Overlay, from the menu button | `WEB-02` |
| **Tabs** | Open pages and open special views, new tab | Same | Same | `WEB-02` |
| **Bar** | Crumbs, view switch, favourite, more menu, panel toggle | Same | Labels hide on the view switch | `WEB-02` |
| **Content** | One view at a time | Same | Same | `WEB-03` |
| **Panel** | Details (Outline, Backlinks, Related, Connections), or Assistant | Overlay | Overlay | `WEB-05`, `AST-01` |
| **Status line** | Word count; save state; sync state from Phase 7 | Same | Counts hide | `WEB-02`, `SYN-03` |
| **Overlays** | Palette, slash menu, link picker, format bar, modals, toasts | Same | Same | `WEB-04` |

Landmarks: `nav` for the sidebar, `main` for the content, `complementary` for the panel, `contentinfo` for the status line. `F6` moves between them.

**First run has no welcome page** ([49](49-DESIGN-SYSTEM.md)). A new workspace opens to one empty, untitled page, with the caret in the title, and the placeholder line showing `/` and `[[`. There is no tour, sample content, sign-in or account. Help is one command away (`help.shortcuts`, `help.open`).

## Screens and their states

Every screen is in the pseudo-locale gate in each of these states ([50](50-INTERNATIONALIZATION.md)). "Loading" follows the motion rule: nothing for a proposed 300 ms, then a plain line of text. Copy here is English source text.

### Core

| Screen | Normal | Empty | Loading | Error | In prototype |
| --- | --- | --- | --- | --- | --- |
| **Page view** | Title and blocks. A line at the end if the canvas has content | Title placeholder, first-block placeholder | Title first, blocks as projected. Layout does not jump | Page failed verification: read-only copy of what can be read, with "Export what is readable" (`CN-*` shown). Never a blank page | Yes |
| **Canvas view** | Surface, note card, items, ink, dock, zoom ([43](43-CANVAS-AND-INK.md)) | Note card and dock, no prompt text | Note card first, ink as tiles arrive | A stroke that failed to decode is reported, and the rest is shown | Yes |
| **Database view and block** | Name, view tabs, table or board, New | Two blank rows, as in the prototype | Header with a text line | "This database was removed." with its name, if known | Yes |
| **Embed block** | Live content, header with Open | "This page is empty" | Text line | "Page removed", or a cycle message | Yes |
| **Palette: search and commands** | Input, grouped results, Related | Recent pages | Results stream. Nothing is blocked | `Nothing matches "…"`, and it offers "Create page". Index rebuilding: "Search is rebuilding. Results may be incomplete" | Search yes, commands no |
| **Find bar** | Query, count, next, previous, replace | No matches: count shows 0 | No state | No state | No |
| **Slash menu** | Filtered block types | "No matching block" | No state | No state | Yes |
| **Link picker** | Page results as you type, "Create page" | "Create page" only | No state | No state | Yes |
| **Format bar** | Bold, italic, code, highlight | Hidden with no selection, or during composition | No state | No state | Yes |

### Panel

| Screen | Normal | Empty | Loading | Error | In prototype |
| --- | --- | --- | --- | --- | --- |
| **Outline** | Headings, click to jump | "Headings appear here." | No state | No state | Yes |
| **Backlinks** | Linking pages with a snippet | "No pages link here yet." | Text line | "Could not load links." Retry | Yes |
| **Related** | Pages with a score | "Nothing similar yet." Before model chosen: "Choose a search model in Settings" | "Finding related pages" | Model failed: named, with a link to Settings | Yes, stand-in |
| **Connections** | Local graph | "No connections yet." | Text line | Link to full graph | Yes |
| **Node details** | Page, tag, row or person with neighbours | Not reachable | Text line | "This item no longer exists" | Yes |
| **Assistant** | Log, composer, model line | AI off: one sentence saying it is off and nothing leaves the device, with a Settings link. On: starter prompts as plain text | Stop button, streamed text | Named cause: key refused, model not running, network down, provider limit. Message kept for retry. Never a generic "Something went wrong" | Yes |
| **Approval card** | Exactly what will change, who can read the target, and which pages were read this turn, all from the engine ([63](63-AGENT-TOOLS-AND-MCP.md)); Allow, Decline. Large text: bounded preview and "Review all text". Text could reach more readers: a distinct warning, and Allow disabled until the full preview has been read to its end | No state | No state | Stale target or changed audience: "This page changed. Ask again" | Part: no audience or sources |

### Views in tabs

| Screen | Normal | Empty | Loading | Error | In prototype |
| --- | --- | --- | --- | --- | --- |
| **Knowledge graph** | Nodes, filters, search; links and similarity kept visibly apart ([61](61-KNOWLEDGE-GRAPH.md)) | "Link two pages to start the graph" | Text line (PERF-10) | "Could not build the graph." Pages unaffected | Yes |
| **Agent access** | Connect instructions, tool list, recent calls ([63](63-AGENT-TOOLS-AND-MCP.md)). The setup text says plainly that a note can make a client read everything under its roots and send it out, that `--read-only` does not stop this, and to use one `--root` per notebook | "No recent calls" | No state | Server not installed, path refused, or encrypted notebook with no opt-in | Yes, command marked as planned |
| **Settings** | Language, theme, assistant, search model, export and import, storage, sync, shortcuts, pen, about | No state | Per section | Per section, in place | Part |
| **Trash** | Items with their date and days left. Restore, delete forever | "Trash is empty" | Text line | Named | Part: sidebar list |
| **Version history** ([24](24-OPERATIONS-AND-MERGE.md)) | Timeline with named merges, preview, restore | "No earlier versions yet" | Text line | "This version cannot be read." Others remain | No |
| **Import report** ([37](37-IMPORT-FORMATS.md)) | Pending: what will arrive, what cannot, conflicts. Commit or cancel | "Nothing to import in this file" | Progress with counts | Refusal code and reason. Nothing changed, by design | Part: package |
| **Export dialog** ([36](36-EXPORT-FORMAT.md)) | Scope, format, sensitive-content warning, saved path | No state | Progress, off main thread | Named. Partial files removed. PDF: font request, and what could not be drawn | Yes: package |

### Later phases

| Screen | Normal | Empty | Loading | Error | Phase |
| --- | --- | --- | --- | --- | --- |
| **Model download** ([62](62-AI-ASSISTANT-AND-PROVIDERS.md)) | Name, size, licence first; Download, Pause, Resume, Use, Remove. Add from a local file only, with a warning. States when this computer cannot sandbox the model (`CN-DSK-0019`). First start of a file: native confirmation on desktop | "No models yet" | Real progress, including the hash check before each start | Hash failed, disk full, network lost: resumable. Nothing is used until the whole file is verified. After a crash, Start: never an automatic restart | 6, simulated in prototype |
| **Provider connect** | Provider list, endpoint, key, what is sent where. Web: the key is kept for this session only, and "Remember on this browser" is an explicit choice with a plain warning. Desktop: a native confirmation shows the real address, and the key goes in a separate small window | "Assistant is off" | "Checking" | Named, per provider. A declined confirmation changes nothing | 5, preview in prototype |
| **Sync state** ([56](56-SYNC-DESIGN.md)) | Per device: last seen, what is unsynced; per storage: state. Desktop: connecting storage shows a native confirmation with the real address | "Sync is off. Your notes stay on this device" | Live | Every fault named in words (PR-2) | 7 |
| **Merge review** | Marker in the page, both versions side by side, keep either or both | Marker absent | Text line | "Cannot compare" with both kept | 7 |
| **Restored copy notice** | A device past a limit gets a visible copy (PR-1) | No state | No state | No state | 7 |
| **Share and permissions** ([57](57-SERVER-DESIGN.md)) | People, roles, link | "Only you" | Text line | Refused, by whom | 8 |
| **Comments** | Thread by block | "No comments" | Text line | Kept as draft, retry | 8 |
| **Presence** | Names with carets. Names, not decorative dots | Absent | No state | "Offline. Your edits are safe here" | 8 |
| **Admin** ([10](10-ENTERPRISE-REQUIREMENTS.md)) | SSO, audit, retention | Per section | Per section | Per section | 10 |

### App-wide failure states

| State | What the person sees |
| --- | --- |
| **Storage unavailable** (private mode, denied OPFS) | Banner: "This browser cannot keep your notes." Offers working in a session, with export. No fake "saved" (PR-1) |
| **Storage full** | Status line and banner, with what is unsaved, and ways out. Editing stays possible in memory, and export is offered |
| **Save failed** (`CN-EDT-0005`) | Status line in words, not a toast alone |
| **Engine stopped** (`CN-EDT-0007`) | Editing pauses, with a restart. Unsaved text remains visible and can be copied |
| **Recovered text** (`CN-EDT-0001`) | A target was truly gone, so typed text could not be placed. A notice at that place, or at the top of the page or workspace, holds the text. Insert here, Copy, Move to a new page, Discard. It stays across restarts until the person chooses ([42](42-EDITOR-AND-RENDERING.md)) |
| **Newer format than this app** | Workspace opens read-only, with a message to update, and no write. Never a silent upgrade |
| **Offline** | Nothing changes for local use. Assistant says its provider is unreachable. A local model works |
| **Second tab on the web** | Works, or says clearly which tab owns writing ([44](44-DESKTOP-SHELL.md), [23](23-STORAGE-FORMAT.md)) |

## Commands

**Every verb is a registered command.** Sidebar buttons, menus, shortcuts, the palette, the slash menu and the dock all call the registry. There are no inline handlers doing product work, so nothing can be reachable by mouse and not by keyboard.

```js
registerCommand({
  id: "page.new",            // stable, never reused, never translated
  title: "cmd.page_new",     // catalogue key
  icon: "plus",
  when: ctx => ctx.canWrite, // context predicate; false hides it from menus
  keys: { mac: "Cmd+N", other: "Ctrl+N", web: null },
  run: (ctx, args) => ...    // calls -ops through the engine client. Never the DOM
});
```

**Rules.**

- A command changes state only through ops ([42](42-EDITOR-AND-RENDERING.md)), and each is one undo step.
- `id` is `area.verb_object`, in lower snake case after the dot. Stable once released.
- A gate (`check-commands.py`, proposed) requires, for each command: a catalogue key, a test, and an entry in this table. It also refuses a duplicate shortcut in one context, and any listener in `webapp/` that calls `-ops` outside a command, beyond the editor's input module.
- The palette lists commands that are currently possible, and also shows others with why not, so a person learns what exists.
- Shortcuts are shown from the registry: palette, tooltips, `aria-keyshortcuts`, `help.shortcuts`.
- **Commands are not agent tools.** Tools ([63](63-AGENT-TOOLS-AND-MCP.md)) are a smaller, closed, versioned set with schemas, for agents. Both write through `-ops`. An agent cannot call `app.*`, `settings.*` or `agent.*`.
- **Editor intents through `beforeinput`** (typing, delete, split) are not commands, as they map from browser events. Their menu forms are.

### Command list

Phase is when it first ships. Keys are in the next table.

| Id | Does | Phase | Tracker |
| --- | --- | --- | --- |
| `palette.open` | Open search and commands | 2 | `WEB-04` |
| `palette.commands` | Open palette in command mode | 2 | `WEB-04` |
| `page.new` | New page | 2 | `WEB-02` |
| `page.new_sub` | New page under this one | 2 | `WEB-02` |
| `page.new_database` | New database page | 2 | `DB-02` |
| `page.rename` | Focus the title | 2 | `WEB-02` |
| `page.duplicate` | Duplicate page | 2 | `WEB-02` |
| `page.move` | Move to another parent | 2 | `WEB-02` |
| `page.trash` | Move to Trash | 2 | `WEB-02` |
| `page.restore` | Restore from Trash | 2 | `WEB-02` |
| `page.delete_forever` | Delete from Trash, after a confirmation | 2 | `WEB-02` |
| `page.favorite_toggle` | Add to or remove from Favorites | 2 | `WEB-02` |
| `page.history_open` | Open version history | 2 | `STO-05` |
| `page.history_restore` | Restore a version | 2 | `STO-05` |
| `page.set_language` | Set the page's language | 2 | `I18N-04` |
| `page.view_markdown` | View as Markdown | 2 | `WEB-02` |
| `view.page` | Switch to page view | 3 | `CNV-01` |
| `view.canvas` | Switch to canvas view | 3 | `CNV-01` |
| `view.toggle_sidebar` | Show or hide sidebar | 2 | `WEB-02` |
| `view.toggle_panel` | Show or hide panel | 2 | `WEB-02` |
| `view.panel_details` | Open Details | 2 | `WEB-05` |
| `view.panel_assistant` | Open Assistant | 5 | `AST-01` |
| `view.next_region` | Move focus to next region | 2 | `A11Y-01` |
| `view.previous_region` | Move focus to previous region | 2 | `A11Y-01` |
| `tab.new` | New tab | 2 | `WEB-02` |
| `tab.close` | Close tab | 2 | `WEB-02` |
| `tab.next` | Next tab | 2 | `WEB-02` |
| `tab.previous` | Previous tab | 2 | `WEB-02` |
| `edit.undo` | Undo | 2 | `OPS-02` |
| `edit.redo` | Redo | 2 | `OPS-02` |
| `edit.cut` | Cut | 2 | `WEB-04` |
| `edit.copy` | Copy | 2 | `WEB-04` |
| `edit.paste` | Paste | 2 | `WEB-04` |
| `edit.paste_plain` | Paste as plain text | 2 | `WEB-04` |
| `edit.select_all` | Select block text, then the page | 2 | `WEB-03` |
| `edit.find` | Find in page | 2 | `WEB-03` |
| `edit.replace` | Find and replace | 2 | `WEB-03` |
| `edit.find_next` | Next match | 2 | `WEB-03` |
| `edit.find_previous` | Previous match | 2 | `WEB-03` |
| `edit.show_save_state` | Show what is saved and where | 2 | `WEB-02` |
| `edit.recovered_text` | Show recovered text | 2 | `WEB-03` |
| `block.to_paragraph` | Turn into paragraph | 2 | `WEB-04` |
| `block.to_heading_1` | Turn into heading 1 | 2 | `WEB-04` |
| `block.to_heading_2` | Turn into heading 2 | 2 | `WEB-04` |
| `block.to_heading_3` | Turn into heading 3 | 2 | `WEB-04` |
| `block.to_bulleted` | Turn into bulleted list | 2 | `WEB-04` |
| `block.to_numbered` | Turn into numbered list | 2 | `WEB-04` |
| `block.to_todo` | Turn into to-do | 2 | `WEB-04` |
| `block.to_quote` | Turn into quote | 2 | `WEB-04` |
| `block.to_callout` | Turn into callout | 2 | `WEB-04` |
| `block.to_code` | Turn into code | 2 | `WEB-04` |
| `block.insert_divider` | Insert divider | 2 | `WEB-04` |
| `block.insert_database` | Insert database | 2 | `DB-02` |
| `block.insert_embed` | Embed a page | 4 | `BLK-01` |
| `block.insert_attachment` | Attach a file | 4 | `FIL-01` |
| `block.toggle_todo` | Check or uncheck | 2 | `WEB-04` |
| `block.move_up` | Move block up | 2 | `WEB-03` |
| `block.move_down` | Move block down | 2 | `WEB-03` |
| `block.duplicate` | Duplicate block | 2 | `WEB-03` |
| `block.delete` | Delete block | 2 | `WEB-03` |
| `block.select` | Select block | 2 | `WEB-03` |
| `block.open_menu` | Open block menu | 2 | `WEB-04` |
| `block.set_language` | Set block's language | 2 | `I18N-04` |
| `block.set_direction` | Set direction: auto, left to right, right to left. If adopted ([50](50-INTERNATIONALIZATION.md)) | 2 | `I18N-02` |
| `inline.bold` | Bold | 2 | `WEB-04` |
| `inline.italic` | Italic | 2 | `WEB-04` |
| `inline.code` | Inline code | 2 | `WEB-04` |
| `inline.highlight` | Highlight | 2 | `WEB-04` |
| `inline.clear` | Clear formatting | 2 | `WEB-04` |
| `inline.link_page` | Link to a page | 2 | `WEB-05` |
| `inline.add_tag` | Add a tag | 2 | `WEB-05` |
| `db.add_row` | New row | 2 | `DB-02` |
| `db.delete_row` | Delete row | 2 | `DB-02` |
| `db.add_column` | New column | 2 | `DB-02` |
| `db.delete_column` | Delete column | 2 | `DB-02` |
| `db.rename` | Rename database | 2 | `DB-02` |
| `db.view_table` | Table view | 2 | `DB-02` |
| `db.view_board` | Board view | 2 | `DB-02` |
| `db.sort` | Sort a view | 2 | `DB-02` |
| `db.filter` | Filter a view | 2 | `DB-02` |
| `db.set_status` | Change a row's status | 2 | `DB-02` |
| `canvas.tool_select` | Select tool | 3 | `CNV-01` |
| `canvas.tool_pen` | Pen | 3 | `INK-02` |
| `canvas.tool_highlighter` | Highlighter | 3 | `INK-02` |
| `canvas.tool_eraser` | Eraser | 3 | `INK-03` |
| `canvas.tool_sticky` | Sticky note | 3 | `CNV-02` |
| `canvas.tool_rect` | Rectangle | 3 | `CNV-02` |
| `canvas.tool_ellipse` | Ellipse | 3 | `CNV-02` |
| `canvas.tool_arrow` | Arrow | 3 | `CNV-02` |
| `canvas.tool_text` | Text | 3 | `CNV-02` |
| `canvas.ink_color_next` | Next ink colour | 3 | `INK-01` |
| `canvas.zoom_in` | Zoom in | 3 | `CNV-01` |
| `canvas.zoom_out` | Zoom out | 3 | `CNV-01` |
| `canvas.zoom_reset` | Zoom to 100% | 3 | `CNV-01` |
| `canvas.zoom_fit` | Fit everything | 3 | `CNV-01` |
| `canvas.item_delete` | Delete selected item | 3 | `CNV-02` |
| `canvas.item_bring_forward` | Bring forward | 3 | `CNV-02` |
| `canvas.item_send_back` | Send back | 3 | `CNV-02` |
| `canvas.next_item` | Move focus to next item | 3 | `A11Y-01` |
| `canvas.toggle_finger_draw` | Draw with finger | 3 | `INK-02` |
| `canvas.toggle_erase_mode` | Partial or whole-stroke erase | 3 | `INK-03` |
| `canvas.import_pdf` | Add a PDF to annotate | 3 | `INK-04` |
| `search.in_workspace` | Search everything | 4 | `SRCH-02` |
| `search.by_meaning` | Search by meaning | 4 | `EMB-02` |
| `graph.open` | Open knowledge graph | 4 | `GRF-02` |
| `graph.open_local` | Show this page in the graph | 4 | `GRF-02` |
| `assistant.ask` | Ask the assistant | 5 | `AST-01` |
| `assistant.stop` | Stop the reply | 5 | `AST-01` |
| `assistant.approve` | Allow a pending write | 5 | `TOOL-02` |
| `assistant.decline` | Decline a pending write | 5 | `TOOL-02` |
| `assistant.review_change` | Review all text of a pending write | 5 | `TOOL-02` |
| `assistant.clear` | Clear the conversation | 5 | `AST-01` |
| `assistant.toggle` | Turn the assistant on or off | 5 | `AST-01` |
| `assistant.connect_provider` | Connect a provider | 5 | `AST-03` |
| `assistant.remove_key` | Remove provider key | 5 | `AST-03` |
| `model.download` | Download a local model | 6 | `DSK-02` |
| `model.pause` | Pause a download | 6 | `DSK-02` |
| `model.resume` | Resume a download | 6 | `DSK-02` |
| `model.use` | Use this model | 6 | `DSK-03` |
| `model.remove` | Remove a local model | 6 | `DSK-02` |
| `agent.open` | Open Agent access | 5 | `MCP-01` |
| `agent.copy_connect` | Copy the connect command, after showing the setup text | 5 | `MCP-01` |
| `export.page_package` | Export this page as a package | 2 | `PKG-05` |
| `export.workspace_package` | Export the workspace | 2 | `PKG-05` |
| `export.page_markdown` | Export as Markdown | 2 | `PKG-05` |
| `export.page_html` | Export as HTML | 2 | `PKG-05` |
| `export.page_pdf` | Export as PDF | 3 | `RND-01` |
| `export.page_png` | Export as PNG | 3 | `RND-01` |
| `import.package` | Import a package | 2 | `PKG-06` |
| `import.markdown` | Import Markdown | 6 | `IMP-01` |
| `import.html` | Import HTML | 6 | `IMP-01` |
| `import.evernote` | Import Evernote | 6 | `IMP-02` |
| `sync.open_state` | Open sync state | 7 | `SYN-03` |
| `sync.add_storage` | Add a storage location | 7 | `SYN-02` |
| `sync.now` | Sync now | 7 | `SYN-01` |
| `sync.review_merge` | Open next merge to review | 7 | `SYN-04` |
| `share.open` | Share this page | 8 | `SRV-03` |
| `comment.add` | Comment on selection | 8 | `SRV-05` |
| `settings.open` | Open Settings | 2 | `WEB-02` |
| `settings.language` | Change language | 2 | `I18N-01` |
| `settings.theme_system` | Follow system theme | 2 | `WEB-02` |
| `settings.theme_light` | Light theme | 2 | `WEB-02` |
| `settings.theme_dark` | Dark theme | 2 | `WEB-02` |
| `settings.search_model` | Choose search model | 4 | `EMB-02` |
| `settings.fonts_download` | Download fonts for export (desktop) | 3 | `I18N-04` |
| `help.shortcuts` | Show keyboard shortcuts | 2 | `WEB-02` |
| `help.open` | Open help | 2 | `WEB-02` |
| `help.about` | About, licences | 2 | `WEB-02` |

## Keyboard shortcuts

**Proposed, all to be tested** in the browser gate ([15](15-CI-AND-RELEASE-GATES.md)). `Mod` is Cmd on macOS and Ctrl elsewhere.

**Rules that matter for a multilingual product.**

- Match on `event.key` when ASCII, otherwise `event.code`, so that shortcuts work on Arabic, Hindi, Korean and other layouts ([42](42-EDITOR-AND-RENDERING.md)). The prototype's `e.key` check fails there.
- **No Ctrl+Alt combinations on Windows and Linux.** On many European layouts that is AltGr, which types characters. This excludes the common editor pattern of Ctrl+Alt+1.
- During composition, all shortcuts return without acting.
- Single-letter canvas keys work only when no text has focus.
- On the web, browsers reserve Ctrl+N, Ctrl+T and Ctrl+W. Those commands have no default there, and they are still in the palette and on buttons. Desktop has them.
- Avoid VoiceOver's Ctrl+Option, and the macOS screenshot keys Cmd+Shift+3, 4 and 5.
- All are user-remappable later. The registry already allows that.

| Command | macOS | Windows and Linux | Web note |
| --- | --- | --- | --- |
| `palette.open` | Cmd K | Ctrl K | As in the prototype |
| `palette.commands` | Cmd Shift P | Ctrl Shift P | Firefox uses this for private window on some platforms; test |
| `page.new` | Cmd N | Ctrl N | Desktop only |
| `tab.new` | Cmd T | Ctrl T | Desktop only |
| `tab.close` | Cmd W | Ctrl W | Desktop only |
| `tab.next` | Ctrl Tab | Ctrl Tab | Desktop only |
| `tab.previous` | Ctrl Shift Tab | Ctrl Shift Tab | Desktop only |
| `settings.open` | Cmd , | Ctrl , | |
| `help.shortcuts` | Cmd / | Ctrl / | |
| `view.toggle_sidebar` | Cmd \ | Ctrl \ | By `event.code` |
| `view.toggle_panel` | Cmd Shift \ | Ctrl Shift \ | By `event.code` |
| `view.panel_assistant` | Cmd Shift . | Ctrl Shift . | By `event.code` |
| `view.next_region` | F6 | F6 | |
| `view.previous_region` | Shift F6 | Shift F6 | |
| `edit.undo` | Cmd Z | Ctrl Z | |
| `edit.redo` | Cmd Shift Z | Ctrl Y, Ctrl Shift Z | |
| `edit.cut`, `edit.copy`, `edit.paste` | Cmd X, C, V | Ctrl X, C, V | Native events |
| `edit.paste_plain` | Cmd Shift V | Ctrl Shift V | |
| `edit.select_all` | Cmd A | Ctrl A | Twice for the page |
| `edit.find` | Cmd F | Ctrl F | |
| `edit.replace` | Cmd Shift H | Ctrl H | |
| `edit.find_next`, `edit.find_previous` | Cmd G, Cmd Shift G | F3, Shift F3 | |
| `edit.show_save_state` | Cmd S | Ctrl S | Nothing needs saving. It shows the state instead of the browser's save dialog |
| `inline.bold` | Cmd B | Ctrl B | |
| `inline.italic` | Cmd I | Ctrl I | |
| `inline.code` | Cmd E | Ctrl E | |
| `inline.highlight` | Cmd Shift M | Ctrl Shift H | |
| `inline.clear` | Cmd Shift Space | Ctrl Shift Space | Not Ctrl Shift X, which switches text direction in Firefox. Test |
| `inline.link_page` | `[[` | `[[` | Also `【【`, `「「` |
| `block.to_paragraph` | Cmd Option 0 | Ctrl Shift 0 | Windows may reserve this one; test |
| `block.to_heading_1` to `_3` | Cmd Option 1 to 3 | Ctrl Shift 1 to 3 | |
| `block.to_bulleted` | Cmd Option 8 | Ctrl Shift 8 | |
| `block.to_numbered` | Cmd Option 7 | Ctrl Shift 7 | |
| `block.to_todo` | Cmd Option 9 | Ctrl Shift 9 | |
| `block.toggle_todo` | Cmd Enter | Ctrl Enter | |
| `block.open_menu` | `/` in an empty block, or Cmd Shift / | `/` in an empty block, or Ctrl Shift / | Also `／`, `、` |
| `block.select` | Escape | Escape | Second Escape clears |
| `block.move_up`, `block.move_down` | Cmd Ctrl Up, Down | Ctrl Shift Up, Down | Works on selected blocks |
| `block.duplicate` | Cmd D | Ctrl D | Replaces browser's bookmark |
| `block.delete` | Delete with block selected | Delete with block selected | |
| `view.page`, `view.canvas` | Cmd Shift E toggles | Ctrl Shift E toggles | Test against browser sidebars |
| `canvas.tool_select`, `_pen`, `_highlighter`, `_eraser` | V, P, H, E | V, P, H, E | No text focus. By `event.code` |
| `canvas.tool_sticky`, `_rect`, `_ellipse`, `_arrow`, `_text` | N, R, O, A, T | N, R, O, A, T | No text focus |
| `canvas.zoom_in`, `_out`, `_reset` | Cmd =, Cmd -, Cmd 0 | Ctrl =, Ctrl -, Ctrl 0 | Replaces browser zoom over the canvas only |
| `canvas.zoom_fit` | Shift 1 | Shift 1 | No text focus |
| `canvas.item_delete` | Delete, Backspace | Delete, Backspace | No text focus |
| `canvas.next_item` | Tab | Tab | In canvas |
| Arrows on a selected item | Moves the item | Moves the item | Physical arrows: the world does not mirror |
| Arrows, nothing selected | Pans | Pans | |
| `assistant.ask` | Enter in composer. Shift Enter for a new line | Same | Never on a confirming Enter during composition |
| `assistant.stop` | Escape in panel | Escape in panel | |

## Feature map by phase

Tracker rows in [14](14-EXECUTION-TRACKER.md), and phase rules in [06](06-ROADMAP-AND-DELIVERY.md). Nothing below is built beyond the prototype.

| Phase | A person gains | Requirements | Key rows |
| --- | --- | --- | --- |
| **Prototype** | A clickable idea of the product | | `PROTO-01` |
| **1 Engine** | Nothing on screen. Notes are safe, merge and export headless | FR-12, FR-13 | `MOD-01`, `OPS-01`, `MRG-01`, `STO-01`, `PKG-05` |
| **2 Web editor** | Write in a browser in any launch language: blocks, pages, tabs, trash, slash, Markdown shortcuts, links, backlinks, outline, tags, databases, undo, history, package export and import | FR-01, FR-02, FR-03, FR-05, FR-18 | `WEB-01` to `WEB-06`, `DB-01`, `DB-02`, `I18N-01` to `I18N-05`, `A11Y-01` |
| **3 Canvas and ink** | Same page as a canvas. Pen, highlighter, partial erase, PDF annotation, PDF and PNG export | FR-06, FR-07 | `CNV-01`, `CNV-02`, `INK-01` to `INK-04`, `RND-01` |
| **4 Knowledge** | Search everything, in every language. Related notes, the graph, embeds, attachments | FR-04, FR-08 to FR-11 | `SRCH-01`, `SRCH-02`, `EMB-01`, `EMB-02`, `GRF-01`, `GRF-02`, `BLK-01`, `FIL-01` |
| **5 Assistant and agents** | Own provider on the web, approval on every write, MCP, CLI | FR-15, FR-17 | `TOOL-01`, `TOOL-02`, `AST-01` to `AST-05`, `MCP-01`, `MCP-02` |
| **6 First release** | Desktop app, local models, Markdown, HTML and Evernote import, signed builds | FR-14, FR-16, FR-22 | `DSK-01` to `DSK-04`, `IMP-01`, `IMP-02`, `REL-01` |
| **7 Sync** | Encrypted sync through a folder, WebDAV or S3. Per-device state. Merge review | FR-19 | `SYN-01` to `SYN-05`, `CRY-01`, `CRY-02` |
| **8 Server** | Co-editing, comments, sharing, permissions | FR-20 | `SRV-01` to `SRV-06` |
| **9 Tablet and mobile** | Tablet-first interface; mobile shells if the prototype supports them | FR-22 | `MOB-01`, `MOB-02` |
| **10 Platform** | SDK, more importers, SSO, audit. Optional OCR and handwriting after review | FR-21, FR-23 | `SDK-02`, `IMP-03`, `ENT-01`, `OCR-01`, `HWR-01` |

## Ideas from the reference apps

The owner admires SiYuan, AppFlowy and AFFiNE (instruction 4, with "Chinese repo" read as SiYuan, `DEC-04`). **This table is ideas only.** AppFlowy and SiYuan are AGPL-3.0: **no code and no assets**, and ADR-023 also forbids copying design. AFFiNE is MIT except its backend and native folders, and we still build our own. Per [12](12-OPEN-SOURCE-AUDIT.md), **no application was built, installed or run**. Everything here comes from public docs and issue trackers, and is unverified as to how they feel to use.

| Idea, as publicly described | Source | Our form | Deliberately different |
| --- | --- | --- | --- |
| Blocks as the unit of writing and reference | SiYuan, AppFlowy, AFFiNE | Blocks with stable ids ([22](22-DATA-MODEL.md)) | The op log is the truth, not JSON or Markdown files ([23](23-STORAGE-FORMAT.md)) |
| Own-data, readable-files mindset | SiYuan | A published package, and a one-way mirror | Third-party storage works by design (ADR-021) |
| One surface for page and whiteboard | AFFiNE | A switch over the same content (ADR-020) | Pen quality is a first-order goal, given the reported palm rejection issue |
| Databases with several views | AppFlowy, AFFiNE | Table and board first (FR-05) | Fewer views, and first-class data referenced from pages |
| Rust core with a thin host | AppFlowy | ADR-004, from OpenCalc | No Flutter. Real DOM for text, for input methods and right-to-left |
| Local AI option | AppFlowy, others | Local model on desktop (ADR-012) | Your provider on the web, off by default, approval on every write (PR-6) |
| Backlinks and graph | SiYuan, Logseq, others | Derived graph (ADR-014) | Links and similarity kept visibly apart |
| Slash menu, `[[` links, Markdown shortcuts | All three | Same conventions, which are not owned by anyone. Works through input methods ([50](50-INTERNATIONALIZATION.md)) | |

**Where we mean to be different:** a permissive stack throughout, per-device sync state, visible merge review, a tested walk-away promise, right-to-left as a starting point, and a design with less decoration. [01](01-ORD.md) holds the case, and RISK-09 the competitive risk.

## Failure modes and limits

| Risk | Response |
| --- | --- |
| A command exists with no test, or a button bypasses the registry | `check-commands.py`, proposed |
| A shortcut collides with a browser, an OS or an input method | Browser gate. Fix by remapping. Palette remains |
| The palette is the only route to something | Each frequent verb also has an on-screen place. Reviewed per phase |
| Too many commands to discover | Palette search in every language, and the shortcut list |
| Hover-only actions | Gutter and row actions have commands and keyboard paths (QR-5) |
| State not designed | The pseudo-locale gate opens every state above, so an undesigned one fails |

## Acceptance gates

| Gate | Proves | Tracker |
| --- | --- | --- |
| Keyboard-only run through every screen | No keyboard trap, all reachable | `WEB-02`, `A11Y-01` |
| Screen reader run through the shell | Landmarks and names | `A11Y-01` |
| Each screen and each state, in pseudo-locale, Arabic and light and dark | States exist, and hold up | `I18N-01`, `I18N-02` |
| Fault injection shows each storage state as designed | PR-1, PR-2 | `STO-01`, `SYN-03` |
| Shortcuts on an Arabic and a Hindi layout | Layout independence | `WEB-04` |
| Shortcuts do not fire during composition | Input methods | `I18N-03` |
| A refused batch shows the recovered text state, with every typed character | PR-1 | `WEB-03` |
| The approval card's audience, sources and widening states, in Arabic and by keyboard | SEC-A08, QR-5 | `TOOL-02` |
| Command registry gate with seeded violations | All verbs registered | Row proposed |
| New workspace opens to an empty page with no sample content | No tutorial page | `WEB-02` |

## ADRs

ADR-019, ADR-020 (views), ADR-012, ADR-013 (assistant), ADR-018 (languages), ADR-023 (ideas only from AGPL projects), ADR-025, ADR-026.

## Tracker ids

Those in the command list and feature map, and `DEC-04`, `WEB-02`, `WEB-04`, `A11Y-01`, `PROTO-01`, and `RISK-09` in [06](06-ROADMAP-AND-DELIVERY.md).
