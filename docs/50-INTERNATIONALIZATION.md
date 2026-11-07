# 50 — Internationalization

**Status: design only, nothing built.** The [prototype](prototypes/casualnote-prototype.html) implements six interface languages: English, Simplified Chinese, Japanese, Spanish, Arabic and Hindi, at 101 translated strings each. **All five translations are drafts by the design author, unreviewed by native speakers** (`PROTO-03`), and the prototype's language picker does not yet label them as drafts. Many of its strings, such as accessible names and the agent view, are still English. It has none of the catalogue machinery below. Browser and `Intl` behaviour is **unverified** until the test sets run.

## Outcome

Owner instruction 7: CasualNote is primarily a multilingual product (ADR-018, PR-5, FR-18). A person can read the interface, write, search, sort, export and talk to the assistant in their own language, and mix languages in one note, with no language treated as a second-class add-on. English is the source language for translators and one language among equals for people.

Languages are not a later layer: they start in Phase 2, and are in each task's Definition of Done ([05](05-PHASES-AND-TASKS.md)).

## Design

### Launch languages

Set by `DEC-09`, which is open. The recommendation covers eleven.

| Language | Tag | Direction | Plural categories | Why it matters | Prototype |
| --- | --- | --- | --- | --- | --- |
| English | `en` | LTR | one, other | Source | Yes |
| Simplified Chinese | `zh-Hans` | LTR | other | Input method, no word spaces | Draft |
| Traditional Chinese | `zh-Hant` | LTR | other | Different glyph forms and input methods | **No** |
| Japanese | `ja` | LTR | other | Input method, three scripts | Draft |
| Korean | `ko` | LTR | other | Syllable composition, word spacing | **No** |
| Spanish | `es` | LTR | one, many, other | Expansion, dead keys | Draft |
| French | `fr` | LTR | one, many, other | Zero is singular, spaces before punctuation | **No** |
| German | `de` | LTR | one, other | Long compounds | **No** |
| Portuguese | `pt` | LTR | one, many, other | Dead keys | **No** |
| Arabic | `ar` | **RTL** | zero, one, two, few, many, other | Direction, shaping, six plural forms | Draft |
| Hindi | `hi` | LTR | one, other | Complex shaping, conjuncts | Draft |

**Open within `DEC-09`:** which Portuguese and Spanish. The proposal is `pt-BR` and a neutral `es` (leaning to `es-419`), and to resolve this with the first translators. Plural columns come from CLDR as read here, and tests take the real values from `Intl.PluralRules`.

"Supported" has a precise meaning: see "Translation workflow".

### Message catalogue

**Format.** ICU MessageFormat, the widely implemented syntax. MessageFormat 2 is not used, because its browser and tool support is not settled as far as this document can verify.

**Files.**

```
webapp/i18n/
  en.json            source messages, the key set
  en.meta.json       per key: description, screenshot, max length if any
  ar.json, ...       one per locale: key to message
  status/ar.json     per key: state and the hash of the source it was translated from
  generated/         compiled ES modules, one per locale, not hand-edited
```

**No runtime library.** ADR-025 forbids a bundler, not a build step for data. `tools/compile-i18n.py`, or a small script using `@messageformat/core` (MIT, not yet in [12](12-OPEN-SOURCE-AUDIT.md)) as a development dependency, compiles each locale into a plain ES module of functions using `Intl.PluralRules`, `Intl.NumberFormat` and others. Only the current locale loads. Syntax errors fail the build, and argument names are checked against the source.

**Keys.** `area.element.meaning`, in lower snake case, stable, and never derived from English text. The prototype keys on the English string, so a change to English would orphan every translation. That does not carry over.

```
shell.sidebar.new_page          "New page"
editor.placeholder.empty        "Type / for blocks, [[ to link a page"
canvas.tool.highlighter         "Highlighter"
editor.page_view.canvas_counts  see below
error.CN-EDT-0004.title         "Could not paste all of this"
```

**Rules.** `check-i18n.py` enforces these.

1. **No user-visible literal outside the catalogue,** including `aria-label`, `title`, `placeholder`, `alt` and `document.title`.
2. **No concatenation.** A sentence is one message. The prototype's `n + " " + (LANG === "en" ? "words" : "")` shows exactly what goes wrong: every other language loses the unit.
3. **Plurals and selects are inside the message,** with whole phrases inside each branch.
4. **Numbers are arguments,** never pre-formatted, so that the locale formats them.
5. **No HTML in messages.** Rich parts use tag placeholders mapped to DOM builders, as in `Press <kbd>Ctrl K</kbd>`. The compiled output never yields markup (QR-7).
6. **Never reuse a key because the English matches.** "Page" the noun and "Page" the view may differ elsewhere.
7. **No word order assumed.** Every argument is named.
8. **User text as an argument is always isolated** (see "Bidi").
9. **No em dashes and no emoji** in any catalogue, by default ([49](49-DESIGN-SYSTEM.md)).

**Plural, with two counts.** Combined counts are written so that each grammatical unit has its own plural:

```
"editor.page_view.items":   "{count, plural, one {# item} other {# items}}",
"editor.page_view.strokes": "{count, plural, one {# stroke} other {# strokes}}",
"editor.page_view.canvas_counts": "{items} and {strokes} on the canvas"
```

Here `{items}` and `{strokes}` are formatted messages passed as arguments, which is the one allowed form of composition: the outer message owns word order and connectives. Arabic's translation has six branches for each count, and Chinese has one.

**Fallback.** `zh-Hant` never falls back to `zh-Hans` silently. Scripts do not stand in for each other. The chain is: exact locale, its parent where it is the same script (`pt-BR` to `pt`), then `en`. A missing key in a released locale fails CI. In development, it shows the English and logs. The source of a key missing in `en` fails CI.

**Engine and agents.** Engine crates hold no display strings (ADR-018): refusals are error codes with arguments ([20](20-ERROR-CODE-REGISTRY.md)), and the host maps them to `error.<code>.title` and `.detail`. Op names, tool names and tool schemas are English identifiers, not translated. **Open:** the CLI and the MCP tool descriptions are English in Phase 5, since a Rust host has no catalogue runtime here. That is a named exception to review in [63](63-AGENT-TOOLS-AND-MCP.md).

### Locale detection and switching

**Detection, first run only.**

1. Stored preference, if any.
2. Web: walk `navigator.languages` in order. Desktop: the OS language list through a host seam ([44](44-DESKTOP-SHELL.md)).
3. Match with script awareness: `zh-TW`, `zh-HK`, `zh-MO`, `zh-Hant` map to `zh-Hant`; `zh`, `zh-CN`, `zh-SG`, `zh-Hans` to `zh-Hans`; others by language, then `en`.

**The prototype's check is wrong for this:** it matches the first two letters, so someone in Taiwan would get Simplified Chinese. The product must not.

**Switching.** Settings and the `settings.language` command. No restart and no reload: the compiled module loads, `lang` and `dir` on `<html>` are set, and the shell re-renders. An editor's document content is untouched, and a composition in progress finishes first. The picker shows each language in its own name, and **a language that is not yet reviewed is labelled as a draft, in that language and in English.**

**Scope.** Interface language is a per-device preference. It is never in a package, and export bytes do not depend on it ([36](36-EXPORT-FORMAT.md)). It is a separate thing from the language of the content, which may be many.

**Region.** `Intl` formats use the full BCP 47 tag from the OS or browser, even when the catalogue is coarser. A person with `es-MX` gets `es` strings with Mexican dates.

### Logical CSS and direction in the interface

**All layout is logical.** `check-css-logical.py` refuses, in `webapp/`:

```
margin-left, margin-right, padding-left, padding-right, border-left*, border-right*,
border-*-left-radius, border-*-right-radius, left, right (in position rules),
float: left|right, clear: left|right, text-align: left|right
```

**Exception:** a line ending in `/* physical: <reason> */`, for the canvas world and for the graph, which do not mirror. Reasons are listed in the gate's output, so that they stay visible.

**The prototype is not clean, and is not the baseline for this.** Physical rules at CSS lines for `.mode` (`margin-right`), `.b.drop-*`, `.b.ol > .mk` (`text-align: right`), `.rz`, `.dock`, `.zoom`, `.composer`, `.status`, `.toast` all need replacing. `.world` and `.inklayer` are legitimately physical.

**What mirrors.**

| Mirrors with interface direction | Does not mirror |
| --- | --- |
| Shell: sidebar, panels, tabs, menus | Canvas world, ink, items' positions |
| Dock, zoom control, breadcrumbs (separator too) | Knowledge graph layout |
| Database columns, board lanes | Code blocks (`dir="ltr"` by default) |
| Progress bar direction | Media controls, charts by time |
| Directional icons: `caret-right`, `arrow-square-out`, `arrow-up-right`, `list-bullets`, `list-numbers`, `sidebar-simple` | Everything else: `magnifying-glass`, `pen-nib`, `check-*`, `x`, `plus`, `gear-six` and so on |

**Keyboard follows.** In a tree, the key towards the inline end expands, so ArrowLeft in right-to-left. Horizontal caret movement in text is always native ([42](42-EDITOR-AND-RENDERING.md)). Positioned menus compute from the physical rectangle of their anchor, align to the inline start, and flip by direction.

### Bidi

**Every block is its own paragraph for direction,** so an Arabic line and an English line sit together correctly on one page.

| Where | Rule |
| --- | --- |
| Text block | `dir="auto"` on the block row, so that marker, checkbox and text go together. `text-align: start` |
| Page title, item text, database cell, assistant message | `dir="auto"` |
| Code block | `dir="ltr"`, because of what its content is. Comments in Arabic stay readable through the browser's bidi handling |
| Empty block | `dir="auto"` with no strong character resolves to LTR, which puts an Arabic writer's caret on the wrong side. An empty block takes the direction of the previous block, or the interface direction for the first, until a strong character arrives |
| Stored | **Nothing,** per [36](36-EXPORT-FORMAT.md): "Direction is not stored." |

**A real limit of first-strong-character.** "Python هو لغة" is an Arabic sentence that starts with a Latin word, and `dir="auto"` makes it left-to-right. A person needs a way to say which it is. **The proposal is an optional per-block direction override** (`auto` by default), with the command `block.set_direction`. It needs an optional field in [22](22-DATA-MODEL.md) and [36](36-EXPORT-FORMAT.md), which currently has no such field. Row proposed.

**Isolates in the interface.** Any user text placed inside interface text is isolated, so that its direction cannot rearrange the sentence around it.

| Sink | Method |
| --- | --- |
| DOM | `<bdi>` around the argument, built by the message's DOM builder |
| Text-only: `aria-label`, `title`, `document.title`, `<option>` | U+2068 and U+2069 around the argument, added by the formatter |
| Crumbs, tabs, tree rows, search results, backlinks, graph labels | Their own element with `dir="auto"` |

**Spoofing.** A title holding bidi overrides or invisible characters could misrepresent itself in an assistant's approval card (ADR-013, [07](07-SECURITY-THREAT-MODEL.md)). Approval cards and import reports isolate all user text, and show U+202A to U+202E, U+2066 to U+2069, and zero-width format characters as visible marks. Notes themselves stay as written.

**Numbers and dates in Arabic text.** `Intl` output is used as it is, in its own isolate. Whether `ar` shows Arabic-Indic or Latin digits depends on locale and region data, and differs across browsers. A test records it, and we do not override it.

### Script-aware typography

Decoration that is harmless in Latin breaks other scripts. Rules for [49](49-DESIGN-SYSTEM.md).

| Rule | Reason | Prototype |
| --- | --- | --- |
| No `letter-spacing` unless the script is Latin, Greek or Cyrillic | Breaks Arabic joining and Devanagari's headline | Only zeroed under `[dir="rtl"]`, so misses Arabic in an English interface, and all Hindi |
| No italics unless Latin, Greek or Cyrillic. Quotes use inset and `--muted` | Synthetic slant harms CJK, Arabic and Devanagari | Quotes are italic everywhere |
| Line height floors: at least 1.3 in headings for Arabic and Devanagari | Marks above and below clip | Page title is 1.15 |
| `word-break: keep-all` for Korean. `line-break: strict` for Japanese and Chinese content | Word spacing and line-start rules | Not set |
| Underline `text-decoration-skip-ink: auto`; Arabic and Devanagari keep a clear offset | Marks below the line | Partly |
| Bold from real faces. Avoid synthetic bold for CJK | Blurred strokes | Mixed |

The script of a block comes from a small cheap scan of its first strong characters when it changes, and is stored only in the view-model, as a `data-script` attribute that CSS can use.

### Fonts by script

**Rule for the screen:** the browser draws text, Geist for Latin, then good OS fonts, then Noto as the last resort. Nothing loads from a CDN (SEC-C03, QR-6). The prototype's Google Fonts link is a prototype-only exception.

```
:lang(ja)      "Geist", "Hiragino Sans", "Yu Gothic UI", "Noto Sans CJK JP", <base>
:lang(zh-Hans) "Geist", "PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC", <base>
:lang(zh-Hant) "Geist", "PingFang TC", "Microsoft JhengHei", "Noto Sans CJK TC", <base>
:lang(ko)      "Geist", "Apple SD Gothic Neo", "Malgun Gothic", "Noto Sans CJK KR", <base>
:lang(ar)      "Geist", "Noto Sans Arabic", "Geeza Pro", "Segoe UI", <base>
:lang(hi)      "Geist", "Noto Sans Devanagari", "Kohinoor Devanagari", "Nirmala UI", <base>
```

Exact family lists are tuned in `I18N-04`, and code fonts have the same treatment.

**Han unification.** One code point can need a different glyph in Japanese and in Chinese. A single stack, as in the prototype's (`"Noto Sans SC", "Noto Sans JP"`), draws Japanese with Chinese forms. So each block needs a `lang`:

1. Stored `lang` on the block or page ([36](36-EXPORT-FORMAT.md)), if any.
2. Otherwise inferred for display only: kana means Japanese, hangul means Korean, bopomofo means Traditional Chinese.
3. Han alone: the nearest neighbouring block's inference, then the interface language if it is CJK, then the browser's language list.

Inference is **never written back.** The same attribute drives the browser's spell checker.

**What ships.**

| Host | On screen | PDF and PNG |
| --- | --- | --- |
| Web | Geist, Arabic and Devanagari Noto precached with the app. CJK Noto slices by `unicode-range` as OpenCalc's docs/101 does, but **only if precached with the app. Otherwise system fonts** | From the precache. What is missing is named in the report, and nothing is fetched for it |
| Desktop | Geist bundled. OS fonts for CJK, as OpenCalc's revised docs/101 found: inside the installer, slices added about 58 MB there. Arabic and Devanagari Noto bundled | Pack downloaded **on request**, never silently |

**No font request is caused by note content (SEC-W08).** A request for a slice as a page renders tells whoever serves the app, or watches the traffic, which scripts a note holds, even from the app's own origin. So on the web:

- A font file is fetched only by the service worker's install step (`WEB-06`), along with the rest of the app, and the set fetched is the same whatever the notes hold.
- An `@font-face` for a slice or a Noto face is declared only once the worker has confirmed that file is in the cache. Until then, including on a first visit and wherever the worker cannot run, the browser uses system fonts.
- A person's language or a note's script never changes which files are requested.

Whether CJK slices are precached at all depends on their size, which is unmeasured for the web. OpenCalc's figure of about 58 MB was for its installer. `I18N-04` decides, and if they are not, CJK on the web is system fonts only, and a machine with no CJK font shows boxes, which is then a stated limit.

**PR-6 and QR-6.** "With AI off, nothing leaves the device" must hold, so a desktop font download is an explicit step in Settings or at the point of export, with size shown. It fetches a whole pack, whatever the page holds. Without it, the export's report names every character it could not draw, per script, with a sample ([42](42-EDITOR-AND-RENDERING.md)). No silent fetch, ever. On the web, the origin that serves the app serves the precache, and that also works air-gapped.

**Unknown until tested.** How well slices work when the worker's cache becomes ready after first paint, and the reflow that causes. Measuring before a face arrives uses docs/101's stated em-width facts. The font-less test covers the fallback.

**Licences.** Geist and Noto are, to this document's knowledge, under SIL OFL 1.1, **unverified for the exact files**. ADR-023's list does not name OFL, and ADR-027 takes fonts through their own review. `DEC-08` covers this.

### Numbers, dates and collation

**All through `Intl`, none hand-made.** Stored data stays locale-neutral ([36](36-EXPORT-FORMAT.md)): dates are ISO 8601, and numbers are JSON numbers.

| Need | API |
| --- | --- |
| Counts, percent, zoom | `Intl.NumberFormat` |
| File sizes | `Intl.NumberFormat` with `style: "unit"` |
| Dates and times | `Intl.DateTimeFormat`, the locale's calendar and order. No hand-written date format |
| "2 hours ago" | `Intl.RelativeTimeFormat` |
| Lists in text | `Intl.ListFormat` |
| Plural | `Intl.PluralRules`, via compiled messages |
| Sorting for people | `Intl.Collator(locale, { numeric: true })` |
| Grapheme and word boundaries | `Intl.Segmenter` |
| First day of week | `Intl.Locale` week info, where available, otherwise by a small table |

**Two orders, kept apart.**

| Anything that affects bytes | Anything a person reads |
| --- | --- |
| Code point order | `Intl.Collator` |
| Package entries, canonical JSON, op batches, `-render` output | Tree siblings (if not hand-ordered), database sorts in the view, tag lists, search facets |

ICU data differs across browsers and versions, so collation can never be part of a determinism promise (QR-3). A database sort is therefore applied in the view on the client. If the server ever needs one ([57](57-SERVER-DESIGN.md)), it says which collation it used.

### Unicode normalisation

| Thing | Rule |
| --- | --- |
| **Paths in packages and the mirror** | NFC. A package path that is not NFC is refused, `CN-PKG-0004`. Two paths equal under case or Unicode form are refused, `CN-PKG-0005` ([36](36-EXPORT-FORMAT.md)) |
| **Text in notes, titles, database values** | **Stored exactly as written.** Never normalised, transliterated or translated, by editing, paste, import or export |
| **Search and matching** | Normalised at query and index time, without changing the stored text. NFC, and marks kept where they carry meaning, which Hindi and Arabic need. Details in [60](60-SEARCH-AND-EMBEDDINGS.md) |
| **Link picker, tag matching, find bar** | The search normaliser, so `é` in either form matches |
| **Tags as identity** | NFC of the tag's text is its identity key. Its display is as first written ([22](22-DATA-MODEL.md), [61](61-KNOWLEDGE-GRAPH.md)) |
| **File names on macOS** | Imported paths may arrive as NFD. Converted to NFC for package paths, kept as written where they are text |
| **Lone surrogates** | Cannot be stored in UTF-8. Replaced by U+FFFD at the bridge, `CN-EDT-0008`. This is the only case where text does not survive exactly, and it is not valid text |
| **Control characters** | Tab and newline allowed in text. Others stored as written, and displayed visibly in approval cards and reports |

### Typed triggers, in every input method

The prototype's triggers only work with ASCII. Its `\w` in the slash pattern means that a query in Chinese or Arabic never matches. The product:

| Trigger | Also accepted |
| --- | --- |
| `/` | `／` at any position; `、` only at the start of an empty block, where Chinese and Japanese input turn the slash key into it |
| `[[` | `【【`, `「「`, which Chinese and Japanese input produce from the same key |
| `#` plus space | `＃`, with U+3000 ideographic space as a space |
| `-`, `*`, `>`, plus space | `－`, `＊`, `＞`, and U+3000 |
| `1.` plus space | `１．`, `١.`, `१.`, and `1、`, in digits of any launch language |

All triggers are evaluated **on committed text only**, never during composition ([42](42-EDITOR-AND-RENDERING.md)). Slash and palette queries match the translated label, its source key's English, and search-normalised forms, with word boundaries by `Intl.Segmenter`. The person types in their own language and finds the command, and an English habit also still works.

### Search tokenising

[60](60-SEARCH-AND-EMBEDDINGS.md) owns it, including scripts without spaces, n-gram fallback, marks in Hindi and Arabic, and the multilingual embedding model (ADR-015). This document only requires one test set per launch language, listed below (`SRCH-01`).

### Assistant reply language

[62](62-AI-ASSISTANT-AND-PROVIDERS.md) owns the design. The language rules are set here:

- **Replies in the language the person just wrote in,** decided by the model from their message. Interface language is the fallback when the message gives no signal: a short command, code or a page title.
- **Can answer from notes in another language,** and says so where it matters. Quotes from notes stay in the note's language, unchanged.
- **Page titles, tags and database names are never translated,** in replies or in tool arguments. The tool has to find the page.
- **Tool arguments for writes hold the person's language.** An approval card shows them as they will be written, isolated.
- **Local models vary widely across languages.** `SPK-07` tests tool calling and replies in each launch language, and a weak language is stated in the model picker.
- **Interface text around the assistant** is in the catalogue. Model text is not, and is never treated as a message key.

### Translation workflow

**Rule: a machine draft is labelled until a native speaker has reviewed it.** A language is called **supported** only after review (`I18N-05`, RISK-12). This includes the current prototype drafts, which are by the design author.

| State, per key per locale | Meaning | Shown |
| --- | --- | --- |
| `missing` | No translation | English, and CI fails for a released locale |
| `draft` | Machine or non-native | Yes, under a "draft" language label |
| `reviewed` | Read and accepted by a named native-speaking reviewer | Yes |
| `stale` | Source hash changed since translation | Yes, and counted as draft |

**Status per locale**, derived from those: **Draft** (any `draft` or `stale` key) or **Supported** (all `reviewed`). Draft languages are offered, labelled in the picker, in the docs and in release notes. Public text may not claim a language is supported otherwise.

**Flow.**

1. A developer adds a key in `en.json`, with a description and, for UI text, a screenshot in `en.meta.json`.
2. Hash recorded. Other locales become `missing` or `stale`.
3. A machine or author draft is allowed, recorded as `draft`, with the method.
4. A reviewer works from a tool or from the files, with context. The tool choice is open, and any tool's licence does not matter if the tool is not shipped. Reviewers are named per locale, with their consent. This needs a budget or a community decision by the owner.
5. A pull request carries the state change. Changing an English string resets its translations to `stale`, automatically.

**Translator guidance,** per locale: register (for example, how formal Japanese or Korean should be, and which form of "you" for Spanish, French, German and Portuguese), term list (page, block, canvas, workspace), and the copy rules from [49](49-DESIGN-SYSTEM.md). Translators may reorder anything and may change the sentence, but they must keep the meaning and arguments.

**Terminology** is kept per language next to the glossary terms ([17](17-GLOSSARY.md)).

### Pseudo-locale gate

Two fake locales, generated from `en.json`, and never shipped.

| Locale | Does | Catches |
| --- | --- | --- |
| `en-XA` | Wraps in markers, adds accents, expands by roughly 40% | Raw strings; truncation; fixed widths; concatenation, because markers appear mid-sentence |
| `ar-XB` | English text in RTL marks, `dir="rtl"`, with plural rules set to the six-form set | Mirroring errors; physical CSS the gate missed; number-handling assumptions |

**The gate** (`I18N-01`, PR-5). A browser run opens each screen, including empty, loading and error states ([47](47-UX-AND-FEATURE-MAP.md)), walks all text nodes and `aria-label`, `title`, `placeholder`, `alt`, and fails on any text without the marker. User content is exempt only under an explicit `data-user-content` attribute, set by the code that renders notes. A second pass fails if any element's content is clipped.

## Test sets

Kept in `fixtures/i18n/<locale>/`. Each locale runs every push ([15](15-CI-AND-RELEASE-GATES.md)). Text in these fixtures is real language, reviewed by the same reviewers.

**In every locale:** plural boundaries for each category; date and number snapshots; a page round-tripped through a package with bytes unchanged; a search set (`SRCH-01`); a screenshot set; sort order set.

| Locale | Adds |
| --- | --- |
| `en` | Baseline |
| `zh-Hans` | Pinyin composition, including cancel and multi-segment. No word spaces in search. `【【` and `、` triggers |
| `zh-Hant` | Zhuyin and cangjie composition. Glyph form check against `zh-Hans`. Locale detection from `zh-TW` |
| `ja` | Kana-kanji composition with segment conversion, Enter to confirm. Kana, kanji and Latin mixed in search. `「「` trigger. Glyph form check |
| `ko` | Syllable composition, including backspace within a syllable, and `compositionend` and `compositionstart` firing together between syllables ([42](42-EDITOR-AND-RENDERING.md)). Search across spacing. `keep-all` line breaks. NFD file names |
| `es`, `pt` | Dead keys; accent-insensitive search matches and accent-preserving storage; 40% expansion in controls |
| `fr` | Dead keys; narrow no-break space in `Intl` numbers; plural of zero |
| `de` | Long compounds in narrow controls; `ß` and case folding in search |
| `ar` | **Whole interface mirrored.** Six plural forms. Shaping in titles with no broken joins. Mixed bidi caret, selection and delete. Digits recorded per browser. PDF extraction. Isolates in crumbs and approval cards |
| `hi` | InScript and transliteration composition. Conjuncts survive caret movement and delete. Combining marks kept in search. No tracking on titles. PDF extraction |
| `en-XA`, `ar-XB` | Pseudo-locale gate above |

Automated composition drives Chrome through its DevTools protocol. Firefox and Safari need scripted real input methods by hand, per release ([42](42-EDITOR-AND-RENDERING.md)).

## Failure modes and limits

| Failure | Response |
| --- | --- |
| A key is missing at runtime | English, logged. CI prevents this for released locales |
| A catalogue fails to load | Falls back to bundled `en`. The interface never shows keys |
| A font fails to load, or was never precached | System fallback, with no request made for it. No invisible text, using `font-display: swap`. Boxes in PDF are named in the report |
| A locale has no `Intl` data in an old webview | Falls back to `en` formatting, and it is a named limit |
| A wrong script inferred for Han-only text | Glyph forms may be off, and it is fixed by setting a language on the block or page. Nothing in the data changes |
| Reviewer not available for a language | It stays Draft. It is never relabelled to hit a date |
| First-strong direction is wrong for a block | Per-block override, if adopted. Otherwise a stated limit |

**Not in the first release:** vertical text (Japanese, Chinese); ruby; right-to-left languages beyond Arabic in the catalogue; non-Gregorian calendars beyond what `Intl` gives. These need their own design.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Keys as English strings (the prototype) | Changing English orphans translations, and homonyms collide |
| `gettext` PO files | Works well, but its plural model is weaker than ICU's for inline selects and it needs a runtime. JSON and compiled modules fit ADR-025 |
| Runtime ICU library (for example `intl-messageformat`) | Allowed by licence, but adds weight on each page, and it parses at runtime |
| MessageFormat 2 | Too new to depend on, as far as can be verified here |
| Bundle all CJK on desktop | OpenCalc's docs/101 records the installer growing by about 58 MB, and reversed that |
| Bundling only English first | Against ADR-018, and every string would need retrofitting |
| Normalising stored text to NFC | Against "stored as written" ([36](36-EXPORT-FORMAT.md)). Someone's text could change without them asking |
| Automatic translation of notes | Out. The product never rewrites what someone wrote |

## Acceptance gates

| Gate | Proves | Tracker |
| --- | --- | --- |
| Pseudo-locale run finds no raw string, on every screen and state | No hard-coded text | `I18N-01` |
| `check-i18n.py`: literals, missing keys, concatenation, em dashes, emoji, with seeded violations | Catalogue rules | `I18N-01` |
| `check-css-logical.py` with a seeded violation | Logical CSS | `I18N-02` |
| Arabic interaction and screenshot run | Right-to-left works | `I18N-02` |
| Composition tests in Chinese, Japanese and Korean, with no writes during composition, including over a selection and across blocks | Input methods | `I18N-03` |
| Each launch script renders on screen and in PDF, and text extracts back | Fonts | `I18N-04`, `RND-01` |
| AI-off network capture shows no font fetch on desktop | PR-6 | `I18N-04` |
| Web: after install, opening a page in each launch script makes no font request. The requests at install are identical whatever the workspace holds | SEC-W08 | `I18N-04` |
| **Font-less test.** Web, with every font file blocked and the worker's cache empty: a page in each launch script makes no font request beyond the install step, and shows real text in system fonts, with no invisible text, no crash and no change to the data | SEC-W08 | `I18N-04` |
| Status files: a released locale with `draft` or `stale` keys cannot be called Supported | Workflow | `I18N-05` |
| Export under each interface language gives identical bytes | Independence | `PKG-05` |
| Search set per launch language | Search | `SRCH-01` |
| Assistant test per language, including titles left unchanged | Replies | `AST-01`, `SPK-07` |

## ADRs

ADR-018 (multilingual first), ADR-015 (multilingual embeddings), ADR-025 (no bundler), ADR-027 (font licences), ADR-017 (package independent of interface language), ADR-013 (approval cards).

## Tracker ids

`I18N-01`, `I18N-02`, `I18N-03`, `I18N-04`, `I18N-05`, `WEB-06`, `PROTO-01`, `PROTO-03`, `DEC-09`, `DEC-08`, `SRCH-01`, `SPK-02`, `SPK-07`, `AST-01`, `RND-01`, `PKG-05`, and `RISK-12` in [06](06-ROADMAP-AND-DELIVERY.md).
