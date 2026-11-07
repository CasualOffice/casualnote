# 49 — Design system

**Status: design only, nothing built.** The tokens and components below are taken from the [prototype](prototypes/casualnote-prototype.html), which is the current visual and interaction reference. The prototype's CSS has not been audited in a browser for contrast or target size: ratios here are calculated from hex values with the WCAG formula. The accent is open under `DEC-11`.

## Outcome

CasualNote looks like a working document made by people who care, in every language. Someone who likes SiYuan, AppFlowy and AFFiNE ([03](03-OWNER-INSTRUCTIONS.md) instruction 4) should feel at home with its density, while the visual language stays its own. Tokens are the only source of colour, size and motion, so that a change lands everywhere, and a CI gate holds the line.

## The rule

**Owner instruction 5: no AI slop.** The owner dislikes basic AI-generated design. In practice:

> A working document, not a dashboard. Full-bleed panes split by hairlines. Text keeps a reading measure while tables and boards take the full width. One radius, one accent, no decoration.

That is the line at the top of the prototype's stylesheet, and it is the brief. If an element exists to look designed rather than to help a person read, write or find, it goes. When a choice is in doubt, the plainer one wins.

### Banned

Each of these fails review, and those marked "gate" also fail CI (`check-design.py`, proposed).

| Banned | Why | What to do instead | Gate |
| --- | --- | --- | --- |
| **Emoji as interface icons** | Render differently per platform, and read as a template | Phosphor icons, ADR-026. People may type emoji in their own notes | Yes: emoji code points in catalogue strings and templates |
| **Tutorial welcome page** | Fills a new workspace with content that the person must delete | Open on an empty page with the caret in the title ([47](47-UX-AND-FEATURE-MAP.md)) | Review |
| **Coloured-bar callouts** | The most recognised template look | Callout is `--tint` background, `--radius`, a muted icon. No side bar, and no per-type colour | Yes: inline-start border on `.callout`, `.quote` |
| **Lavender links** | Default AI palette | Links in `--fg` with a thin `--faint` underline, `--accent` on hover | Review. See the note on dark accent below |
| **Floating rounded cards** | Dashboard look. Hides the structure of the page | Hairlines and full-bleed panes. A card exists only where a thing is literally movable: board card, sticky, note card. Those have `--radius`, a border and no resting shadow | Yes: `border-radius` other than the token; `box-shadow` other than `--shadow` |
| **Pastel status pills** | Template look, and colour alone carries meaning | Icon and text: `.st`, with a Phosphor icon by state, and no fill | Yes: pill radius |
| **Purple gradients** | Default AI palette | No gradients of any colour | Yes: `linear-gradient`, `conic-gradient`. The dot grid's single `radial-gradient` is the named exception |
| **Decorative status dots** | Coloured dots beside everything, carrying no information | A status is said in words with an icon. Graph nodes are data, and are not decoration | Review |
| **Em dashes in product copy** | A strong tell of machine-written text | Full stop, colon, or a shorter sentence | Yes: U+2014 in every catalogue, in every language, by default |
| **Mono-caps eyebrows** | Small uppercase monospace labels above headings | Plain small text in `--faint` or `--muted`, sentence case. `text-transform` also breaks in most launch languages | Yes: `text-transform: uppercase` |

**Also out,** in the same spirit: glassmorphism and backdrop blur; large drop shadows on resting surfaces; hero illustrations in empty states; confetti; "sparkle" AI icons; marketing copy inside the app; exclamation marks in interface text; Title Case; animated loaders on anything under the delay in Motion; an assistant avatar.

**This document follows the em dash rule,** apart from the required title line.

## Tokens

These match the prototype's `:root`, and are the only colours in `webapp/`. Any change goes through this table. `--bg` and friends are stable names that embedding hosts may rely on ([55](55-SDK-AND-EMBEDDING.md)).

### Colour

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--bg` | `#fbfbfa` | `#17181a` | Page, panes, menus |
| `--side` | `#f3f3f1` | `#1d1e21` | Sidebar, tab bar, panel tab bar |
| `--hover` | `#e9e9e6` | `#2a2b2f` | Hover on rows and buttons |
| `--active` | `#e1e1dd` | `#33353a` | Current row, pressed view tab |
| `--fg` | `#17181a` | `#ececea` | Body text |
| `--muted` | `#62656b` | `#a3a5aa` | Secondary text, icons |
| `--faint` | `#67696f` | `#919399` | Placeholders, section labels. See Accessibility |
| `--line` | `#dededa` | `#2f3034` | All hairlines |
| `--strong` | `#17181a` | `#ececea` | Pressed tool and mode, format bar, toast, approval border |
| `--accent` | `#1f47d6` | `#6c9bff` | `DEC-11`. See below |
| `--accent-fg` | `#ffffff` | `#10131d` | Text on accent |
| `--accent-soft` | `#e4e9fb` | `#252c47` | Drop target lane |
| `--tint` | `#f0f0ed` | `#222327` | Callout, code, user message |
| `--mark` | `#ffe680` | `#6b5a0c` | Inline highlight, search hit |
| `--sticky` | `#fbeea6` | `#4a4315` | Amber sticky |
| `--sticky-2` | `#e7e7e3` | `#313236` | Grey sticky |
| `--sticky-fg` | `#17181a` | `#ececea` | Text on stickies |
| `--dot` | `#d9d9d5` | `#303135` | Canvas grid |
| `--ink` | `#17181a` | `#ececea` | Default pen |
| `--ink-red` | `#c9341f` | `#f08a78` | Red pen. Also danger text, and "writes" labels |
| `--ink-blue` | `#1f47d6` | `#8ba4ff` | Blue pen |
| `--hi` | `#ffd92e` | `#8f7a10` | Highlighter |
| `--hi-blend` | `multiply` | `screen` | Highlighter blending |
| `--shadow` | `0 10px 30px rgb(23 24 26 / .16)` | `0 10px 30px rgb(0 0 0 / .55)` | Menus, modals, lifted card, overlay panels only |
| `--radius` | `4px` | `4px` | **The only radius** |

The modal scrim is `rgb(23 24 26 / .3)`, a literal in the prototype, and becomes `--scrim`. `--ink-red` doing double duty as danger is also a prototype shortcut, and a separate `--danger` with the same values is added, so that a pen colour change cannot change error text.

**Accent.** Cobalt `#1f47d6` is the prototype's value and the recommendation in `DEC-11`, chosen to stand apart from OpenCalc and OpenPresent. The accent appears **only** for: the primary action, focus ring, drop indicator, selected item outline, and hover on links. There is one accent. Status, graph and sticky colours never borrow it as decoration.

**Dark accent needs a decision, under `DEC-11`.** `#8ba4ff` is a pale periwinkle. Used on `.textlink` and `.rel .lk` in dark mode, it comes close to the banned lavender link. References already avoid it (`--fg` with underline). The proposal is to move these text links to the same treatment, and keep `--accent` in dark mode for controls.

### Type

**UI and content font: Geist,** weights 400, 500, 600. **Code: Geist Mono,** 400. The product bundles both, and loads nothing from a CDN (SEC-C03): the prototype's Google Fonts link does not carry over. Licences go through ADR-027. Geist has no CJK, Arabic or Devanagari, and fallback stacks by script are in [50](50-INTERNATIONALIZATION.md). The token forms are:

```
--ui:   "Geist", <script fallbacks>, system-ui, sans-serif
--mono: "Geist Mono", ui-monospace, Menlo, monospace
```

| Step | Size | Used for (from the prototype) |
| --- | --- | --- |
| `--t-11` | `.6875rem` | Graph legend, tool log |
| `--t-12` | `.75rem` | Section labels, metadata, status line, zoom percent |
| `--t-13` | `.8125rem` | Panel lists, table headers, code block, field labels |
| `--t-14` | `.875rem` | **UI base** at line height 1.5. Menus, buttons, database cells |
| `--t-15` | `.9375rem` | Workspace name, database name, item text, dialog heading |
| `--t-16` | `1rem` | **Content body** at line height 1.6. H3 weight 600 |
| `--t-17` | `1.0625rem` | Settings section heading |
| `--t-18` | `1.125rem` | Text item on canvas |
| `--t-19` | `1.1875rem` | H2 at weight 600, line height 1.35, tracking -.01em |
| `--t-22` | `1.375rem` | Note card title |
| `--t-24` | `1.5rem` | H1 at weight 600, line height 1.3, tracking -.02em |
| `--t-28` | `1.75rem` | Page title under 760px |
| `--t-36` | `2.25rem` | Page title at weight 600, line height 1.15, tracking -.025em |

**Rules.**

- Numbers in columns and counts use `font-variant-numeric: tabular-nums`.
- Reading measure: content at `68ch`, title at `22ch` with `text-wrap: balance`. Databases and boards are not limited.
- Sentence case everywhere. No uppercase transforms.
- **Tracking and italics are per script,** because they break other scripts ([50](50-INTERNATIONALIZATION.md)). The prototype zeroes tracking only under `[dir="rtl"]`, which misses an Arabic title in an English interface, and all Devanagari. The product applies tracking and the quote's italic only when the block's script is Latin, Greek or Cyrillic.

### Space and layout

**Spacing steps,** from values in use: `2, 4, 6, 8, 10, 12, 16, 22, 28` px, as `--s-*`. Blocks have 2px of vertical padding. Heading padding above is 24, 22 and 12 px. Anything else needs a reason.

| Region | Size |
| --- | --- |
| Sidebar | 248px, fixed in version 1 |
| Details and assistant panel | 300px; `min(340px, 100%)` as an overlay |
| Document inline padding | 72px start, for the gutter; 40px end; 16px under 760px |
| Document block padding | 28px top, 120px bottom |
| Document max width | 1120px, with text held to the measure |
| Status line | One line, 4px block padding |
| Breakpoint: panel becomes overlay | 1120px |
| Breakpoint: sidebar becomes overlay; gutter hides | 760px |

All safe-area insets are honoured with `env(safe-area-inset-*)`.

**Stacking,** as named tokens: dock and zoom 5; panel overlay 55; sidebar overlay 60; format bar 70; menu 80; modal 85; lifted card 90; toast 95.

### Icons

**Phosphor Icons (MIT),** version 2.1.1 in the prototype, regular weight, 256 viewBox, inlined as static SVG (ADR-026). No icon font, and no fetch. Fill variant only where it carries meaning, such as a set favourite. Default 16px, with `currentColor`. Sizes in use: 11 and 12 (chevrons, close), 15 (status), 16, 18 (dock).

Icons in the interface use: `magnifying-glass`, `plus`, `graph`, `link-simple`, `gear-six`, `file-text`, `kanban`, `frame-corners`, `dots-three`, `sidebar-simple`, `list`, `star`, `star-fill`, `trash`, `x`, `caret-right`, `caret-down`, `dots-six`, `dots-six-vertical`, `text-t`, `text-h-one`, `text-h-two`, `text-h-three`, `list-bullets`, `list-numbers`, `check-square`, `quotes`, `lightbulb`, `code`, `minus`, `table`, `stack`, `text-b`, `text-italic`, `highlighter`, `cursor`, `pen-nib`, `eraser`, `note`, `rectangle`, `circle`, `arrow-up-right`, `arrow-square-out`, `circle-dashed`, `circle-half`, `eye`, `check-circle`, `download-simple`, `markdown-logo`, `copy`, `monitor`, `globe-simple`. Adding one means adding it to the inlined set, and only that file.

**Mirroring in right-to-left** is a fixed list in [50](50-INTERNATIONALIZATION.md). A pen, a magnifying glass or a check mark does not mirror.

**Pages have no chosen icon.** A page's icon comes from what it is (document, database, canvas). An emoji picker for pages is out, under the first banned item.

## Components

Each spec is as in the prototype, with sizes in px. **Every interactive component has all the states in the matrix below.** Differences from the prototype are named.

### Shell

| Component | Spec |
| --- | --- |
| Nav row | `--t-14`, icon 16, gap 8, padding 4 8, `--muted` text. Hover: `--hover`, `--fg`. Shortcut hint right-aligned logically, in `--faint` `--t-12`, shown per platform (the prototype always shows "Ctrl K") |
| Section label | `--t-12`, `--faint`, sentence case, padding 16 8 4. No uppercase |
| Tree row | Chevron 18 by 24, label truncates with an ellipsis. Hover and focus reveal the add button. Current: `--active`, `--fg`, weight 500. Indent 14 per level, with `padding-inline-start` |
| Tab | Max width 200, padding 6 and start 12, ending hairline. On: `--bg`, joining the pane below by hiding the bottom border |
| Bar | Min height 28, padding 6 12, gap 4 |
| Breadcrumb | `--muted`, separator `/`, current in `--fg`, truncates. Each title is isolated ([50](50-INTERNATIONALIZATION.md)) |
| Mode switch | Page and Canvas, in a bordered group with a hairline between. Pressed: `--strong` fill, `--bg` text, never accent |
| Icon button | 28 by 28, `--muted`. Hover: `--hover`, `--fg` |
| Status line | `--t-12`, ending with the sync or save state in words. Word count via `Intl`. No decorative dot |

### Editor

| Component | Spec |
| --- | --- |
| Paragraph and lists | `--t-16`, 1.6. Marker width 18, `--muted`, numbers tabular |
| To-do | Checkbox 15 with `accent-color: var(--strong)`. Done: `--faint`, line-through |
| Quote | Inline-start padding 18, `--muted`. **No bar.** Italic in Latin, Greek and Cyrillic only |
| Callout | `--tint`, `--radius`, padding 10 12, muted icon 16. **No bar, and no colour variants** |
| Code | `--tint`, `--radius`, padding 10 14, `--mono` `--t-13` |
| Divider | 1px `--line`, 12 block padding |
| Inline code | `.86em` `--mono`, `--tint`, padding 1 4 |
| Inline highlight | `--mark`, text inherits |
| Page reference | `--fg`, 1px `--faint` underline, offset 3px. Hover: `--accent` for both. Target gone: `--faint`, line-through |
| Gutter | Two 22 buttons at 46 before the text, opacity 0 until hover or focus. Hidden under 760px, where the block's menu is reachable from a long press. Must be reachable by keyboard through commands |
| Drop line | 2px `--accent` |
| Slash menu, link picker, menus | `--bg`, `--radius`, `--shadow`, padding 4, min width 210, max height 320. Item padding 4 8, icon cell 18, `.on` and hover `--hover`. Sub-label `--t-12` `--faint`. Danger: `--ink-red` |
| Format bar | Fixed, `--strong` background, `--bg` icons, 28 by 26 buttons. Stays out of composition: hidden while composing |
| Placeholder | `--faint`, only in the focused empty block. Page title placeholder always |

### Data

| Component | Spec |
| --- | --- |
| Database head | Name `--t-15` 600, view tabs, one primary "New" at the inline end, 1px bottom hairline |
| Table | Full width, min 560, scrolls itself. Header `--t-13` `--faint` 400, no fill. Row hairlines only, no zebra stripes and no column rules. Cell input padding 6 8, focus `--hover` |
| Status | Icon by group and text: `circle-dashed`, `circle-half`, `eye`, `check-circle`. No fill and no pill. The icon comes from the status group, not a stored colour |
| Board | Equal lanes, min 190, with hairlines between. Lane header `--t-13`, tabular count. Drop target `--accent-soft` |
| Board card | `--bg`, 1px `--line`, `--radius`, padding 8 10, no resting shadow. Hover: `--faint` border. Lifted: `--shadow`, fixed width |

### Canvas and graph

| Component | Spec |
| --- | --- |
| Surface | `--bg` and a `--dot` grid, 22 units scaled with zoom |
| Dock | 34 by 34 buttons in a bordered column. On the inline start, mid-height. Pressed: `--strong`. Row at the bottom under 760px |
| Zoom | Bottom inline end, bordered, percent `--t-12` tabular, min width 38 |
| Note card | `--bg`, `--line`, `--radius`, no shadow, 20 grip, body padding 2 24 18 |
| Sticky | `--sticky` or `--sticky-2`, `--sticky-fg`, `--radius`, padding 12. No fake fold and no shadow |
| Shape | 1.5px `--strong`, `--bg` fill, text centred. Ellipse has 50% radius, as its shape, and this is the one allowed exception |
| Selection | 1.5px `--accent` outline, offset 2. Handle 10 by 10 |
| Graph | Node kinds are told apart by shape and fill, not hue. Link edges `--muted`. Similarity edges `--accent`, dashed, at .7, so that links and similarity stay visibly apart (FR-11). Controls in a bordered panel 180 wide |

### Panels and dialogs

| Component | Spec |
| --- | --- |
| Panel tabs | As tabs, equal width |
| Panel list | `--t-13`, heading `--t-12` `--faint` |
| User message | Inline end, `--tint`, `--radius`, padding 6 10, max 88%. No avatar |
| Assistant message | Plain text, no bubble. Tool line in `--mono` `--t-11`, `--faint`. Replies render as text (SEC-A03) |
| Approval card | 1px `--strong`, `--radius`, padding 8 10, gap 6. States exactly what will change. Two buttons. Text isolated ([50](50-INTERNATIONALIZATION.md)) |
| Composer | Top hairline, padding 8 12, textarea plus Send |
| Button | Padding 4 11, 1px `--line`, `--radius`. Primary: `--accent` fill and border, `--accent-fg`. One primary per view |
| Field | `--t-13` `--muted` label above, 1px `--line` input, padding 5 8, max width 460. Disabled: `--tint`, `--faint` |
| Setting row | Grid: 22, flexible, auto. 9 block padding, top hairline |
| Progress | 3px `--strong` bar, by `transform: scaleX`, origin by direction. No percentage without real progress |
| Modal | `--bg`, `--radius`, `--shadow`, max width 580, max height 72vh, top offset 12vh, on the scrim. Foot with top hairline |
| Toast | `--strong`, `--bg`, `--t-13`, padding 6 12, `role="status"`. Never the only record of an error |

## States

| State | Treatment |
| --- | --- |
| Rest | As specified |
| Hover | `--hover` background and/or `--fg` text. No lift, no scale, no shadow |
| Focus | `:focus-visible`, 2px `--accent` outline, offset 1. Never removed except on editable text, where the caret shows focus, and on cell inputs, which use `--hover` |
| Pressed | `translateY(1px)` on buttons. Toggled state: `aria-pressed` with `--strong`, or `--active` in a lighter group |
| Current | `--active`, weight 500. Not colour alone |
| Selected | `--accent` outline on items. Native selection for text |
| Disabled | `--tint` and `--faint`. Prefer hiding a command that cannot apply, in menus, while keeping it visible and explained in the palette |
| Loading | See Motion. Layout does not shift, and no skeleton shimmer |
| Error | Text in `--ink-red` (`--danger`), with an icon and words saying what happened and what to do. Colour is never the only signal |
| Empty | One plain sentence in `--faint` or `--muted`, as in "No pages link here yet." No illustration |
| Unavailable offline | Same as disabled, and it says why |

## Dark mode

Three states: system (default), light, dark. Tokens switch under `@media (prefers-color-scheme: dark)` with `:root:not([data-theme="light"])`, and again under `:root[data-theme="dark"]`. **The two dark blocks must hold the same values, and a gate compares them,** which addresses the mismatch OpenCalc's docs/49 records between its media block and its manual dark theme. `color-scheme: dark` is set in both. The prototype follows the system, and has no control for this. The product adds a setting and a command.

- Tokens only, so that no component has its own dark rule.
- Stored colours in notes are names ([43](43-CANVAS-AND-INK.md)), so that ink and stickies adapt.
- Elevation in dark mode comes from the stronger `--shadow` and hairlines. Surfaces do not get lighter with height, except the fixed `--side` and `--tint`.
- Highlighter blend changes to `screen`.
- Image attachments are never inverted.
- PDF and PNG exports always use the light tokens. The theme is not part of a document, and export bytes do not depend on it (QR-3).

## Motion

**The default is none.** The prototype has no animations or transitions, only the 1px press, and it feels fast for that. Motion that is added has to show where something went, or the interface has to be waiting.

| Rule | Value (proposed) |
| --- | --- |
| Allowed | Panel and sidebar overlay slide; toast fade; lifted drag; caret in the browser's native form |
| Duration | 120 ms or less, ease-out. Nothing over 200 ms |
| Properties | `transform` and `opacity` only |
| Never on | Typing, block insertion, tab change, menus opening, page change, scrolling. Nothing is animated on the typing path (PR-4) |
| Loaders | Nothing for the first 300 ms. Then a plain line of text, with progress if it is known. No spinners on rows, no shimmer, no skeletons |
| Reduced motion | `prefers-reduced-motion: reduce` turns off all of it, including the press offset. Smooth scrolling also goes (the prototype uses it on outline jumps) |
| Decoration | No looping animation, ever |

## Writing

Interface copy is part of this system, in every language, with translation rules in [50](50-INTERNATIONALIZATION.md).

- Plain words and short sentences. Say what happened, then what to do.
- Sentence case. No exclamation marks. No "please", "oops" or "just".
- **No em dashes.** The gate reads every catalogue.
- Never claim more than is true. "Saved on this device" and not "Saved", which would suggest a sync that may not have happened. The prototype's wording here is the model.
- Name the thing: "Move to Trash", not "Delete?". Errors never show a raw code on their own, but do include it for support.
- The assistant is not a character. It has no name beyond "Assistant", no avatar and no greeting. Its starter prompts are plain text buttons.

## Accessibility

QR-5 requires WCAG 2.2 AA. **The prototype's tokens do not all meet it, and this conflicts with keeping the tokens exactly.** Calculated from hex values; not yet checked in a browser:

| Pair | Light | Dark | AA for small text (4.5:1) |
| --- | --- | --- | --- |
| `--fg` on `--bg` | 17.16 | 15.02 | Pass |
| `--muted` on `--bg` | 5.64 | 7.21 | Pass |
| `--muted` on `--side` | 5.26 | 6.76 | Pass |
| `--accent` on `--bg` | 6.92 | 6.57 | Pass |
| `--accent-fg` on `--accent` | 7.17 | 7.81 | Pass |
| `--ink-red` on `--bg` | 5.08 | 7.29 | Pass |
| **`--faint` on `--bg`** | **3.05** | **4.03** | **Fail** |
| **`--faint` on `--side`** | **2.84** | **3.78** | **Fail** |

`--faint` is used for section labels, metadata, table headers and hints at 12 and 13px. A placeholder counts as well. Decorative and disabled uses are exempt. **Tokens stay as they are in this document until the owner decides.** The proposal is to darken light `--faint` and lighten dark `--faint` until both reach 4.5:1 on `--side`, or to move informational uses to `--muted` and keep `--faint` for disabled and decorative uses only. Row proposed.

**Target size.** WCAG 2.2 asks for 24 by 24 at AA, with exceptions for spacing. The tab close button (18), tree add button (20), section add button (20) and gutter buttons (22) are under that. They need enlarged hit areas without visual change.

**Other rules.**

- Colour is never the only signal. Status has icon and text, a graph's node kinds have shapes, and errors have words.
- All icon buttons have an accessible name from the catalogue.
- Every action behind hover is also a command with a keyboard path ([47](47-UX-AND-FEATURE-MAP.md)).
- Forced colours mode: hairlines and focus remain visible, and icons use `currentColor`.
- Text resizes to 200% without loss. Sizes are in `rem`, and control sizes in px should not clip translated text.

## Failure modes and limits

| Risk | Response |
| --- | --- |
| Slow drift back to template looks, one small change at a time | Gate plus review against the banned list, with a screenshot in each UI change |
| New pattern with no spec | It is added here, in the same change, or does not ship |
| Long translations break tight controls | Pseudo-locale run ([50](50-INTERNATIONALIZATION.md)). Controls grow; text does not shrink. Nothing is truncated without a tooltip |
| A host embeds the app with its own theme | Only the colour tokens may be overridden ([55](55-SDK-AND-EMBEDDING.md)), and the banned list still applies |
| Geist or Noto licence or coverage issues | ADR-027, `DEC-08`. Fallback is the system stack |

## Alternatives considered

| Option | Why not |
| --- | --- |
| OpenCalc's design system | A different product and a different accent. Its token-gate approach is carried over. Its colours, ribbons and larger radii are not |
| A component library (Radix, Material, and others) | ADR-025 has no framework, and a library would bring its own look |
| Using the look of AppFlowy or SiYuan | Both AGPL: ideas only, no code or assets. ADR-023 also rules out copying design from them. The owner admires them, and that sets a bar for density and craft, not a style to copy |
| User-chosen accents, as OpenCalc has | Doubles the contrast matrix. Can come later |
| Multiple radii | One radius is the point. The sole exception is an ellipse being an ellipse |

## Acceptance gates

| Gate | Proves | Tracker |
| --- | --- | --- |
| `check-design.py` (proposed): literal colours, radii, shadows, gradients and uppercase transforms outside the tokens file; the two dark blocks differ; a seeded violation fails | Tokens hold | Row proposed |
| `check-i18n.py` extended: U+2014 and emoji in catalogues | Copy rules | `I18N-01` |
| Contrast check on all token pairs in use, light and dark | QR-5 | `A11Y-01` |
| Automated accessibility run on each screen, and target-size check | QR-5 | `A11Y-01` |
| `check-css-logical.py` | Direction | `I18N-02` |
| Screenshot set, light and dark, in English, Arabic, Japanese and Hindi | Consistency | `WEB-02` |
| Owner reviews against the banned list per phase | Instruction 5 | Phase exit in [06](06-ROADMAP-AND-DELIVERY.md) |

## ADRs

ADR-026 (Phosphor, no emoji icons), ADR-023 (no copied design), ADR-025 (no framework), ADR-018, ADR-027 (font licences).

## Tracker ids

`DEC-11`, `WEB-02`, `A11Y-01`, `I18N-01`, `I18N-02`, `I18N-04`, `PROTO-01`, `DEC-08`. Risk: a new row is proposed for the `--faint` contrast and target-size conflict.
