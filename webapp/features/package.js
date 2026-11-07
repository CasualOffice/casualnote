// Takes notes out and brings them back: the CasualNote Package (docs/36) and a Markdown view. The
// engine writes and reads every byte. Whatever a package says reaches the page as text only, and
// nothing in it is ever run, rendered or fetched.
import { I18N } from "../data.js";
import { package_inspect, package_entry } from "../pkg/casual_note_wasm.js";

let api;
const MAX_FILE_BYTES = 1.5 * 1024 ** 3;

// Drafts for the strings this feature adds, not yet native-reviewed (I18N-05).
const STRINGS = {
  "zh-Hans": { "Preparing the package.": "正在准备文件包。", "Reading the package.": "正在读取文件包。", "This package could not be made.": "无法生成此文件包。", "This file could not be imported.": "无法导入此文件。", "Nothing was changed.": "没有任何更改。", "Pages": "页面", "Back to files": "返回文件列表", "View as Markdown": "以 Markdown 查看", "Copy": "复制", "Copied.": "已复制。", "This could not be copied here.": "无法在此复制。", "Save as Markdown": "保存为 Markdown", "Markdown can lose canvas detail. Export a package to keep everything.": "Markdown 可能丢失画布细节。导出文件包可保留全部内容。", "Pages in the trash are not included: {n}": "未包含废纸篓中的页面：{n}", "{pages} pages, {databases} databases, {files} files, {size}.": "{pages} 个页面，{databases} 个数据库，{files} 个文件，{size}。", "All hashes match. Package version {version}.": "所有校验值一致。文件包版本 {version}。", "These pages already exist, so they will arrive as copies with new ids, and their links will follow.": "这些页面已存在，将以新编号的副本导入，链接会随之更新。", "Ids will be kept.": "将保留编号。", "Links to pages that are not here: {n}. They are kept, and show as removed.": "指向不存在页面的链接：{n}。它们会被保留，并显示为已删除。", "Blocks of a type this version does not know: {n}. They are kept, with their text visible.": "此版本不认识的块类型：{n}。它们会被保留，文字仍可见。", "Fields this version does not model and will not keep: {n}.": "此版本未建模、不会保留的字段：{n}。", "Attachments and extensions not imported: {n}.": "未导入的附件和扩展：{n}。", "Pages moved to the top level, because their parent is missing: {n}.": "因缺少上级而移到顶层的页面：{n}。", "Imported {n} pages.": "已导入 {n} 个页面。", "This file is larger than this browser can read.": "此文件超出浏览器可读取的大小。", "This is not a CasualNote Package, or it is damaged.": "这不是 CasualNote 文件包，或已损坏。", "The package's list of contents is missing or not valid.": "文件包的内容清单缺失或无效。", "This package was made by a newer version of CasualNote.": "此文件包由较新版本的 CasualNote 生成。", "A file path in this package is unsafe.": "此文件包中的文件路径不安全。", "This package has duplicate file paths.": "此文件包有重复的文件路径。", "This package has an encrypted file or a symbolic link.": "此文件包含加密文件或符号链接。", "This package is over a size limit.": "此文件包超出大小限制。", "A file in this package is damaged or missing.": "此文件包中的文件已损坏或缺失。", "This package has files its list does not mention.": "此文件包有清单未列出的文件。", "A page or database in this package is not valid.": "此文件包中的页面或数据库无效。" },
  ja: { "Preparing the package.": "パッケージを準備しています。", "Reading the package.": "パッケージを読み込んでいます。", "This package could not be made.": "パッケージを作成できませんでした。", "This file could not be imported.": "このファイルをインポートできませんでした。", "Nothing was changed.": "何も変更されていません。", "Back to files": "ファイル一覧に戻る", "View as Markdown": "Markdown で表示", "Copy": "コピー", "Copied.": "コピーしました。", "This could not be copied here.": "ここではコピーできません。", "Save as Markdown": "Markdown として保存", "Markdown can lose canvas detail. Export a package to keep everything.": "Markdown ではキャンバスの細部が失われることがあります。すべて残すにはパッケージを書き出してください。", "Pages in the trash are not included: {n}": "ゴミ箱のページは含まれません: {n}", "{pages} pages, {databases} databases, {files} files, {size}.": "{pages} ページ、{databases} データベース、{files} ファイル、{size}。", "All hashes match. Package version {version}.": "すべてのハッシュが一致しました。パッケージのバージョン {version}。", "These pages already exist, so they will arrive as copies with new ids, and their links will follow.": "これらのページは既に存在するため、新しい ID のコピーとして取り込み、リンクも更新します。", "Ids will be kept.": "ID はそのまま保持します。", "Links to pages that are not here: {n}. They are kept, and show as removed.": "ここにないページへのリンク: {n}。保持され、削除済みとして表示されます。", "Blocks of a type this version does not know: {n}. They are kept, with their text visible.": "このバージョンが知らない種類のブロック: {n}。テキストを表示したまま保持します。", "Fields this version does not model and will not keep: {n}.": "このバージョンで扱えず保持しないフィールド: {n}。", "Attachments and extensions not imported: {n}.": "取り込まれない添付と拡張: {n}。", "Pages moved to the top level, because their parent is missing: {n}.": "親がないため最上位に移したページ: {n}。", "Imported {n} pages.": "{n} ページを取り込みました。", "This file is larger than this browser can read.": "このファイルはブラウザーで読み込める大きさを超えています。", "This is not a CasualNote Package, or it is damaged.": "CasualNote パッケージではないか、破損しています。", "The package's list of contents is missing or not valid.": "パッケージの内容一覧がないか、無効です。", "This package was made by a newer version of CasualNote.": "このパッケージは新しいバージョンの CasualNote で作成されました。", "A file path in this package is unsafe.": "このパッケージのファイルパスは安全ではありません。", "This package has duplicate file paths.": "このパッケージにはファイルパスの重複があります。", "This package has an encrypted file or a symbolic link.": "このパッケージには暗号化ファイルまたはシンボリックリンクがあります。", "This package is over a size limit.": "このパッケージはサイズ制限を超えています。", "A file in this package is damaged or missing.": "このパッケージのファイルが破損しているか、見つかりません。", "This package has files its list does not mention.": "このパッケージには一覧にないファイルがあります。", "A page or database in this package is not valid.": "このパッケージのページまたはデータベースは無効です。" },
  es: { "Preparing the package.": "Preparando el paquete.", "Reading the package.": "Leyendo el paquete.", "This package could not be made.": "No se pudo crear este paquete.", "This file could not be imported.": "No se pudo importar este archivo.", "Nothing was changed.": "No se cambió nada.", "Back to files": "Volver a los archivos", "View as Markdown": "Ver como Markdown", "Copy": "Copiar", "Copied.": "Copiado.", "This could not be copied here.": "No se pudo copiar aquí.", "Save as Markdown": "Guardar como Markdown", "Markdown can lose canvas detail. Export a package to keep everything.": "Markdown puede perder detalles del lienzo. Exporta un paquete para conservarlo todo.", "Pages in the trash are not included: {n}": "No se incluyen páginas de la papelera: {n}", "{pages} pages, {databases} databases, {files} files, {size}.": "{pages} páginas, {databases} bases de datos, {files} archivos, {size}.", "All hashes match. Package version {version}.": "Todos los hashes coinciden. Versión del paquete {version}.", "These pages already exist, so they will arrive as copies with new ids, and their links will follow.": "Estas páginas ya existen, así que llegarán como copias con nuevos identificadores y sus enlaces se actualizarán.", "Ids will be kept.": "Se conservarán los identificadores.", "Links to pages that are not here: {n}. They are kept, and show as removed.": "Enlaces a páginas que no están aquí: {n}. Se conservan y aparecen como eliminadas.", "Blocks of a type this version does not know: {n}. They are kept, with their text visible.": "Bloques de un tipo que esta versión no conoce: {n}. Se conservan, con su texto visible.", "Fields this version does not model and will not keep: {n}.": "Campos que esta versión no modela y no conservará: {n}.", "Attachments and extensions not imported: {n}.": "Adjuntos y extensiones no importados: {n}.", "Pages moved to the top level, because their parent is missing: {n}.": "Páginas movidas al nivel superior porque falta su página superior: {n}.", "Imported {n} pages.": "Se importaron {n} páginas.", "This file is larger than this browser can read.": "Este archivo es más grande de lo que este navegador puede leer.", "This is not a CasualNote Package, or it is damaged.": "No es un paquete de CasualNote, o está dañado.", "The package's list of contents is missing or not valid.": "Falta la lista de contenido del paquete o no es válida.", "This package was made by a newer version of CasualNote.": "Este paquete se creó con una versión más reciente de CasualNote.", "A file path in this package is unsafe.": "Una ruta de este paquete no es segura.", "This package has duplicate file paths.": "Este paquete tiene rutas duplicadas.", "This package has an encrypted file or a symbolic link.": "Este paquete tiene un archivo cifrado o un enlace simbólico.", "This package is over a size limit.": "Este paquete supera un límite de tamaño.", "A file in this package is damaged or missing.": "Un archivo de este paquete está dañado o falta.", "This package has files its list does not mention.": "Este paquete tiene archivos que su lista no menciona.", "A page or database in this package is not valid.": "Una página o base de datos de este paquete no es válida." },
  ar: { "Preparing the package.": "جارٍ تجهيز الحزمة.", "Reading the package.": "جارٍ قراءة الحزمة.", "This package could not be made.": "تعذر إنشاء هذه الحزمة.", "This file could not be imported.": "تعذر استيراد هذا الملف.", "Nothing was changed.": "لم يتغير شيء.", "Back to files": "العودة إلى الملفات", "View as Markdown": "عرض بصيغة Markdown", "Copy": "نسخ", "Copied.": "تم النسخ.", "This could not be copied here.": "تعذر النسخ هنا.", "Save as Markdown": "حفظ بصيغة Markdown", "Markdown can lose canvas detail. Export a package to keep everything.": "قد تفقد Markdown تفاصيل اللوحة. صدّر حزمة للاحتفاظ بكل شيء.", "Pages in the trash are not included: {n}": "صفحات سلة المهملات غير مشمولة: {n}", "{pages} pages, {databases} databases, {files} files, {size}.": "الصفحات: {pages}، قواعد البيانات: {databases}، الملفات: {files}، الحجم: {size}.", "All hashes match. Package version {version}.": "كل قيم التحقق متطابقة. إصدار الحزمة {version}.", "These pages already exist, so they will arrive as copies with new ids, and their links will follow.": "هذه الصفحات موجودة بالفعل، لذا ستصل كنسخ بمعرّفات جديدة، وستتبعها روابطها.", "Ids will be kept.": "سيتم الاحتفاظ بالمعرّفات.", "Links to pages that are not here: {n}. They are kept, and show as removed.": "روابط إلى صفحات غير موجودة هنا: {n}. يُحتفظ بها وتظهر كمحذوفة.", "Blocks of a type this version does not know: {n}. They are kept, with their text visible.": "كتل من نوع لا يعرفه هذا الإصدار: {n}. يُحتفظ بها مع إظهار نصها.", "Fields this version does not model and will not keep: {n}.": "حقول لا يدعمها هذا الإصدار ولن يحتفظ بها: {n}.", "Attachments and extensions not imported: {n}.": "مرفقات وامتدادات لم تُستورد: {n}.", "Pages moved to the top level, because their parent is missing: {n}.": "صفحات نُقلت إلى المستوى الأعلى لغياب الصفحة الأم: {n}.", "Imported {n} pages.": "تم استيراد الصفحات: {n}.", "This file is larger than this browser can read.": "هذا الملف أكبر مما يستطيع هذا المتصفح قراءته.", "This is not a CasualNote Package, or it is damaged.": "هذه ليست حزمة CasualNote، أو أنها تالفة.", "The package's list of contents is missing or not valid.": "قائمة محتويات الحزمة مفقودة أو غير صالحة.", "This package was made by a newer version of CasualNote.": "أُنشئت هذه الحزمة بإصدار أحدث من CasualNote.", "A file path in this package is unsafe.": "مسار ملف في هذه الحزمة غير آمن.", "This package has duplicate file paths.": "تحتوي هذه الحزمة على مسارات مكررة.", "This package has an encrypted file or a symbolic link.": "تحتوي هذه الحزمة على ملف مشفر أو رابط رمزي.", "This package is over a size limit.": "تتجاوز هذه الحزمة حدًا للحجم.", "A file in this package is damaged or missing.": "ملف في هذه الحزمة تالف أو مفقود.", "This package has files its list does not mention.": "تحتوي هذه الحزمة على ملفات لا تذكرها قائمتها.", "A page or database in this package is not valid.": "صفحة أو قاعدة بيانات في هذه الحزمة غير صالحة." },
  hi: { "Preparing the package.": "पैकेज तैयार हो रहा है।", "Reading the package.": "पैकेज पढ़ा जा रहा है।", "This package could not be made.": "यह पैकेज नहीं बन सका।", "This file could not be imported.": "यह फ़ाइल आयात नहीं हो सकी।", "Nothing was changed.": "कुछ नहीं बदला।", "Back to files": "फ़ाइलों पर वापस", "View as Markdown": "Markdown में देखें", "Copy": "कॉपी करें", "Copied.": "कॉपी हो गया।", "This could not be copied here.": "यहाँ कॉपी नहीं हो सका।", "Save as Markdown": "Markdown के रूप में सहेजें", "Markdown can lose canvas detail. Export a package to keep everything.": "Markdown में कैनवास का विवरण छूट सकता है। सब कुछ रखने के लिए पैकेज निर्यात करें।", "Pages in the trash are not included: {n}": "ट्रैश के पेज शामिल नहीं हैं: {n}", "{pages} pages, {databases} databases, {files} files, {size}.": "{pages} पेज, {databases} डेटाबेस, {files} फ़ाइलें, {size}।", "All hashes match. Package version {version}.": "सभी हैश मेल खाते हैं। पैकेज संस्करण {version}।", "These pages already exist, so they will arrive as copies with new ids, and their links will follow.": "ये पेज पहले से हैं, इसलिए नए आईडी वाली प्रतियों के रूप में आएँगे, और उनके लिंक भी साथ बदलेंगे।", "Ids will be kept.": "आईडी बने रहेंगे।", "Links to pages that are not here: {n}. They are kept, and show as removed.": "उन पेजों के लिंक जो यहाँ नहीं हैं: {n}। वे रखे जाते हैं और हटाए गए दिखते हैं।", "Blocks of a type this version does not know: {n}. They are kept, with their text visible.": "ऐसे ब्लॉक जिन्हें यह संस्करण नहीं जानता: {n}। वे अपने दिखते पाठ के साथ रखे जाते हैं।", "Fields this version does not model and will not keep: {n}.": "ऐसे फ़ील्ड जो यह संस्करण नहीं रखेगा: {n}।", "Attachments and extensions not imported: {n}.": "आयात न हुए अनुलग्नक और एक्सटेंशन: {n}।", "Pages moved to the top level, because their parent is missing: {n}.": "मूल पेज न होने से शीर्ष स्तर पर भेजे गए पेज: {n}।", "Imported {n} pages.": "{n} पेज आयात हुए।", "This file is larger than this browser can read.": "यह फ़ाइल इस ब्राउज़र की पढ़ने की सीमा से बड़ी है।", "This is not a CasualNote Package, or it is damaged.": "यह CasualNote पैकेज नहीं है, या क्षतिग्रस्त है।", "The package's list of contents is missing or not valid.": "पैकेज की सामग्री सूची नहीं है या अमान्य है।", "This package was made by a newer version of CasualNote.": "यह पैकेज CasualNote के नए संस्करण से बना है।", "A file path in this package is unsafe.": "इस पैकेज का एक फ़ाइल पथ असुरक्षित है।", "This package has duplicate file paths.": "इस पैकेज में दोहराए गए फ़ाइल पथ हैं।", "This package has an encrypted file or a symbolic link.": "इस पैकेज में एन्क्रिप्टेड फ़ाइल या सिम्बॉलिक लिंक है।", "This package is over a size limit.": "यह पैकेज आकार सीमा से बड़ा है।", "A file in this package is damaged or missing.": "इस पैकेज की एक फ़ाइल क्षतिग्रस्त या ग़ायब है।", "This package has files its list does not mention.": "इस पैकेज में ऐसी फ़ाइलें हैं जो सूची में नहीं हैं।", "A page or database in this package is not valid.": "इस पैकेज का कोई पेज या डेटाबेस अमान्य है।" },
};
for (const [lang, table] of Object.entries(STRINGS)) if (I18N[lang]) for (const [k, v] of Object.entries(table)) I18N[lang][k] ??= v;

const REFUSALS = {
  "CN-PKG-0001": "This is not a CasualNote Package, or it is damaged.",
  "CN-PKG-0002": "The package's list of contents is missing or not valid.",
  "CN-PKG-0003": "This package was made by a newer version of CasualNote.",
  "CN-PKG-0004": "A file path in this package is unsafe.",
  "CN-PKG-0005": "This package has duplicate file paths.",
  "CN-PKG-0006": "This package has an encrypted file or a symbolic link.",
  "CN-PKG-0007": "This package is over a size limit.",
  "CN-PKG-0008": "A file in this package is damaged or missing.",
  "CN-PKG-0009": "This package has files its list does not mention.",
  "CN-PKG-0010": "A page or database in this package is not valid.",
};
const fill = (s, map) => s.replace(/\{(\w+)\}/g, (_, k) => map[k] ?? "");
const num = n => new Intl.NumberFormat(api.lang).format(n);
const codeOf = err => String(err?.message || err).match(/CN-[A-Z]+-\d+/)?.[0] || "CN-PKG-0001";
const created = () => new Date().toISOString().replace(/\.\d+Z$/, "Z");
// Long enough for the browser to paint the loading state. Not rAF, which stops in a hidden tab.
const nextFrame = () => new Promise(r => setTimeout(r, 30));
const safeFile = title => { const n = (title || "").normalize("NFC").replace(/[\\/:*?"<>|\u0000-\u001f\u007f]/g, "-").replace(/^[. ]+|[. ]+$/g, "").slice(0, 80); return n || api.t("Untitled"); };
function size(bytes) {
  const f = new Intl.NumberFormat(api.lang, { maximumFractionDigits: 1 });
  if (bytes < 1024) return f.format(bytes) + " B";
  if (bytes < 1024 ** 2) return f.format(bytes / 1024) + " KB";
  if (bytes < 1024 ** 3) return f.format(bytes / 1024 ** 2) + " MB";
  return f.format(bytes / 1024 ** 3) + " GB";
}
function toast(text) {
  api.$(".toast")?.remove();
  const node = api.el("div", { class: "toast", role: "status", text });
  document.body.append(node);
  setTimeout(() => node.remove(), 4000);
}
function save(bytes, filename, type) {
  const url = URL.createObjectURL(new Blob([bytes], { type }));
  const a = api.el("a", { href: url, download: filename });
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
}

/* ---------- A modal, keyboard-safe ---------- */
function modal(label) {
  api.$(".modalbg")?.remove();
  const before = document.activeElement;
  const title = api.el("h3", { dir: "auto", text: label });
  const body = api.el("div", { class: "pkgbody" });
  const foot = api.el("div", { class: "foot" });
  const box = api.el("div", { class: "modal pkgmodal", role: "dialog", "aria-modal": "true", "aria-label": label, tabindex: "-1" }, title, body, foot);
  const bg = api.el("div", { class: "modalbg", onpointerdown: e => { if (e.target === bg) close(); } }, box);
  const close = () => { bg.remove(); if (before?.isConnected) before.focus?.(); };
  box.addEventListener("keydown", e => {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); return; }
    if (e.key !== "Tab") return;
    const stops = [...box.querySelectorAll("button:not([disabled]),[href],input,[tabindex='0']")];
    if (!stops.length) return;
    const first = stops[0], last = stops[stops.length - 1];
    if (e.shiftKey && (document.activeElement === first || document.activeElement === box)) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  });
  document.body.append(bg);
  box.focus();
  return { box, title, body, foot, close };
}
const btn = (text, onclick, primary) => api.el("button", { class: "btn" + (primary ? " primary" : ""), text, onclick });
function showRefusal(m, err, fallback) {
  const code = codeOf(err);
  m.body.replaceChildren(
    api.el("p", { class: "pkgerror", role: "alert", dir: "auto", text: api.t(REFUSALS[code] || fallback) }),
    api.el("p", { class: "pkgnote", text: api.t("Nothing was changed.") }),
    api.el("p", { class: "pkgcode", text: String(err?.message || err) }));
  m.foot.replaceChildren(btn(api.t("Close"), m.close, true));
  m.foot.lastChild.focus();
}

/* ---------- Export ---------- */
async function exportPackage(page) {
  const name = (page ? safeFile(page.title) : "CasualNote workspace") + ".cnotepack";
  const m = modal(page ? api.t("Export this page") : api.t("Export workspace"));
  m.body.append(api.el("p", { class: "pkgnote", role: "status", text: api.t("Preparing the package.") }));
  m.foot.append(btn(api.t("Cancel"), m.close));
  await nextFrame();
  if (!m.box.isConnected) return;
  let bytes, info;
  try {
    bytes = api.engine.package_export(created(), page ? page.id : undefined);
    info = JSON.parse(package_inspect(bytes));
  } catch (err) {
    console.warn("export", err);
    m.body.replaceChildren(api.el("p", { class: "pkgerror", role: "alert", text: api.t("This package could not be made.") }), api.el("p", { class: "pkgcode", text: String(err?.message || err) }));
    m.foot.replaceChildren(btn(api.t("Close"), m.close, true));
    return;
  }
  m.title.textContent = name;
  const listView = () => {
    const files = api.el("div", { class: "pkgfiles", role: "list", tabindex: "0", "aria-label": api.t("Files") });
    for (const [path, n] of [["mimetype", 38], ["manifest.json", null], ...info.files]) {
      const md = path.endsWith(".md");
      const label = [api.el("span", { class: "pkgpath", dir: "auto", text: path }), n != null ? api.el("small", { text: size(n) }) : null];
      files.append(md
        ? api.el("button", { class: "pkgfile link", role: "listitem", onclick: () => showFile(path) }, ...label)
        : api.el("div", { class: "pkgfile", role: "listitem" }, ...label));
    }
    const trashed = page ? 0 : api.pages().filter(p => p.trashed).length;
    m.body.replaceChildren(
      api.el("p", { class: "pkgwarn", role: "note", text: api.t("Anyone with this file can read these notes, because it is not encrypted. API keys, assistant chats and search indexes are never included.") }),
      api.el("p", { class: "pkgnote", text: fill(api.t("{pages} pages, {databases} databases, {files} files, {size}."), { pages: num(info.pages.length), databases: num(info.databases.length), files: num(info.files.length + 2), size: size(bytes.length) }) }),
      trashed ? api.el("p", { class: "pkgnote", text: fill(api.t("Pages in the trash are not included: {n}"), { n: num(trashed) }) }) : null,
      files);
  };
  const showFile = path => {
    let text = "";
    try { text = new TextDecoder().decode(package_entry(bytes, path)); } catch (err) { text = String(err?.message || err); }
    m.body.replaceChildren(
      api.el("button", { class: "textlink pkgback", text: api.t("Back to files"), onclick: listView }),
      api.el("p", { class: "pkgnote", dir: "auto", text: path }),
      api.el("pre", { tabindex: "0", dir: "auto", text }));
    api.$(".pkgback", m.body).focus();
  };
  listView();
  m.foot.replaceChildren(btn(api.t("Close"), m.close), btn(api.t("Save file"), () => {
    try { save(bytes, name, "application/vnd.casualnote.package+zip"); m.close(); toast(name); }
    catch (err) { console.warn("save", err); toast(String(err?.message || err)); }
  }, true));
  m.foot.lastChild.focus();
}

/* ---------- Import ---------- */
const peer = () => crypto.getRandomValues(new BigUint64Array(1))[0] >> 1n;

async function importFile(file) {
  const m = modal(api.t("Import a package"));
  m.body.append(api.el("p", { class: "pkgnote", role: "status", text: api.t("Reading the package.") }));
  m.foot.append(btn(api.t("Cancel"), m.close));
  if (file.size > MAX_FILE_BYTES) { m.body.replaceChildren(api.el("p", { class: "pkgerror", role: "alert", text: api.t("This file is larger than this browser can read.") })); m.foot.replaceChildren(btn(api.t("Close"), m.close, true)); return; }
  let bytes, plan;
  try {
    bytes = new Uint8Array(await file.arrayBuffer());
    await nextFrame();
    if (!m.box.isConnected) return;
    plan = JSON.parse(api.engine.package_plan(bytes, peer()));
  } catch (err) {
    console.warn("import", err);
    showRefusal(m, err, "This file could not be imported.");
    return;
  }
  const r = plan.report, lines = [];
  lines.push(api.el("p", { class: "pkgnote", text: `${api.t("Pages")}: ${num(r.pages)}. ${api.t("Database")}: ${num(r.databases)}. ${size(bytes.length)}.` }));
  lines.push(api.el("p", { class: r.copied ? "pkgwarn" : "pkgnote", text: api.t(r.copied ? "These pages already exist, so they will arrive as copies with new ids, and their links will follow." : "Ids will be kept.") }));
  const note = (n, text) => { if (n) lines.push(api.el("p", { class: "pkgnote", text: fill(api.t(text), { n: num(n) }) })); };
  note(r.dangling.length, "Links to pages that are not here: {n}. They are kept, and show as removed.");
  note(r.unknown_blocks.length, "Blocks of a type this version does not know: {n}. They are kept, with their text visible.");
  note(r.extra_fields, "Fields this version does not model and will not keep: {n}.");
  note(r.skipped_files.length, "Attachments and extensions not imported: {n}.");
  note(r.moved_to_top.length, "Pages moved to the top level, because their parent is missing: {n}.");
  m.body.replaceChildren(...lines);
  if (r.titles.length) {
    const titles = api.el("div", { class: "pkgfiles", role: "list", tabindex: "0" });
    for (const t of r.titles) titles.append(api.el("div", { class: "pkgfile", role: "listitem" }, api.el("span", { class: "pkgpath", dir: "auto", text: t || api.t("Untitled") })));
    m.body.append(titles);
  }
  m.foot.replaceChildren(btn(api.t("Cancel"), m.close), btn(api.t("Import"), () => {
    try {
      // Planned again in this same turn, so another tab's change cannot slip between proof and commit.
      const now = JSON.parse(api.engine.package_plan(bytes, peer()));
      commit(now.ops);
      m.close();
      if (now.report.first_page) api.openPage(now.report.first_page); else api.renderAll();
      toast(fill(api.t("Imported {n} pages."), { n: num(now.report.pages) }));
    } catch (err) {
      console.warn("import", err);
      showRefusal(m, err, "This file could not be imported.");
    }
  }, true));
  m.foot.lastChild.focus();
}
function commit(ops) {
  // One undo step through the one write path. A spread this long would overflow the stack.
  if (ops.length <= 50000) api.edit(...ops);
  else for (const op of ops) api.run(op);
}

/* ---------- Markdown ---------- */
function showMarkdown(meta) {
  const m = modal(meta.title || api.t("Untitled"));
  let text;
  try { text = api.engine.page_markdown(meta.id); }
  catch (err) { console.warn("markdown", err); m.body.append(api.el("p", { class: "pkgerror", role: "alert", text: String(err?.message || err) })); m.foot.append(btn(api.t("Close"), m.close, true)); return; }
  m.body.append(
    api.el("p", { class: "pkgnote", text: api.t("Markdown can lose canvas detail. Export a package to keep everything.") }),
    api.el("pre", { class: "pkgmd", tabindex: "0", dir: "auto", text }));
  m.foot.append(btn(api.t("Close"), m.close),
    btn(api.t("Copy"), async () => { try { await navigator.clipboard.writeText(text); toast(api.t("Copied.")); } catch { toast(api.t("This could not be copied here.")); } }),
    btn(api.t("Save as Markdown"), () => { save(new TextEncoder().encode(text), safeFile(meta.title) + ".md", "text/markdown;charset=utf-8"); m.close(); }, true));
  m.foot.lastChild.focus();
}

export function install(a) {
  api = a;
  api.ext.settings.push(box => {
    const input = api.el("input", { type: "file", id: "pkgFile", class: "pkghidden", accept: ".cnotepack,.zip,application/zip", "aria-label": api.t("Import a package"), tabindex: "-1",
      onchange: e => { const file = e.target.files?.[0]; e.target.value = ""; if (file) importFile(file); } });
    box.append(
      api.el("h2", { text: api.t("Export and import") }),
      api.el("p", { text: api.t("A package holds every page, database and canvas in one file. It also contains readable Markdown, so your notes never depend on this app.") }),
      api.el("div", { class: "pkgactions" },
        api.el("button", { class: "btn", id: "pkgExport", onclick: () => exportPackage(null) }, api.icon("download-simple"), api.el("span", { text: api.t("Export workspace") })),
        api.el("button", { class: "btn", id: "pkgImport", onclick: () => input.click() }, api.icon("stack"), api.el("span", { text: api.t("Import a package") })),
        input));
  });
  api.ext.pageMenu.push(meta => [
    { gl: "download-simple", label: api.t("Export this page"), run: () => exportPackage(meta) },
    { gl: "markdown-logo", label: api.t("View as Markdown"), run: () => showMarkdown(meta) },
  ]);
}
