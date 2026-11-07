// Search through Ctrl K, the knowledge graph view, and the side panel's related pages and local
// graph. Everything is read from the engine, which derives it from the notes, so nothing here can
// disagree with them. Nothing is written except by an explicit Link. Document text only ever
// reaches the page through textContent or SVG text content.
import { I18N } from "../data.js";

let api;
const IN_WORKER = typeof document === "undefined";
const SVG = "http://www.w3.org/2000/svg";
const MAX_NODES = 2000, MAX_EDGES = 10000, SYNC_LAYOUT_NODES = 200, LOCAL_NODES = 14;
const REDUCED = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

// Drafts for the strings this feature adds, not yet native-reviewed (I18N-05).
const STRINGS = {
  "zh-Hans": { "Similar pages are found by shared words, not by meaning.": "相似页面按共同词语判断，而不是按含义。", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "还没有关联。用 [[ 链接页面、添加 #标签或数据库后，关联就会显示在这里。", "Everything is filtered out. Turn a filter back on.": "全部已被筛选隐藏。请重新打开一个筛选项。", "The graph could not be built.": "无法生成图谱。", "Try again": "重试", "Show as list": "以列表显示", "Show as graph": "以图谱显示", "Filters": "筛选", "Zoom in": "放大", "Zoom out": "缩小", "Fit": "适应", "Arranging {n} nodes": "正在排列 {n} 个节点", "Showing {n} of {m} nodes, the most connected.": "显示连接最多的 {n} 个节点，共 {m} 个。", "Select a node to see what it connects to.": "选择一个节点以查看它的关联。", "Similar, computed": "相似（计算得出）", "Open its page": "打开所在页面", "Clear selection": "取消选择", "and {n} more": "还有 {n} 个", "Page": "页面", "Tag": "标签", "Database row": "数据库行", "Person": "人物", "links to": "链接到", "linked from": "被链接自", "embeds": "嵌入", "embedded in": "被嵌入于", "has subpage": "子页面", "subpage of": "上级页面", "tagged": "标签", "tag on": "使用此标签的页面", "contains": "包含", "row in": "所在页面", "involves": "涉及", "owner or author of": "负责或撰写", "Links to removed pages: {n}": "指向已删除页面的链接：{n}", "Search failed.": "搜索失败。", "Pages and items connected to this page": "与此页面相关的页面和条目", "Knowledge graph of pages, tags, rows and people": "页面、标签、行和人物的知识图谱", "Link to {page}": "链接到 {page}" },
  ja: { "Similar pages are found by shared words, not by meaning.": "類似ページは意味ではなく、共通の語句で判断しています。", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "まだつながりがありません。[[ でページをリンクするか、#タグやデータベースを追加すると、ここに表示されます。", "Everything is filtered out. Turn a filter back on.": "すべて絞り込みで非表示です。フィルターを戻してください。", "The graph could not be built.": "グラフを作成できませんでした。", "Try again": "再試行", "Show as list": "リストで表示", "Show as graph": "グラフで表示", "Filters": "フィルター", "Zoom in": "拡大", "Zoom out": "縮小", "Fit": "全体表示", "Arranging {n} nodes": "{n} 個のノードを配置中", "Showing {n} of {m} nodes, the most connected.": "つながりの多い {n} 個を表示中（全 {m} 個）。", "Select a node to see what it connects to.": "ノードを選ぶと、つながりを表示します。", "Similar, computed": "類似（計算による）", "Open its page": "そのページを開く", "Clear selection": "選択を解除", "and {n} more": "他 {n} 件", "Page": "ページ", "Tag": "タグ", "Database row": "データベースの行", "Person": "人物", "links to": "リンク先", "linked from": "リンク元", "embeds": "埋め込み先", "embedded in": "埋め込み元", "has subpage": "サブページ", "subpage of": "親ページ", "tagged": "タグ", "tag on": "このタグのページ", "contains": "含む", "row in": "行のあるページ", "involves": "関係者", "owner or author of": "担当または作成", "Links to removed pages: {n}": "削除されたページへのリンク: {n}", "Search failed.": "検索に失敗しました。", "Pages and items connected to this page": "このページにつながるページと項目", "Knowledge graph of pages, tags, rows and people": "ページ、タグ、行、人物のナレッジグラフ", "Link to {page}": "{page} にリンク" },
  es: { "Similar pages are found by shared words, not by meaning.": "Las páginas similares se detectan por palabras compartidas, no por su significado.", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "Aún no hay conexiones. Enlaza páginas con [[, añade #etiquetas o una base de datos, y aparecerán aquí.", "Everything is filtered out. Turn a filter back on.": "Todo está filtrado. Vuelve a activar un filtro.", "The graph could not be built.": "No se pudo crear el grafo.", "Try again": "Reintentar", "Show as list": "Ver como lista", "Show as graph": "Ver como grafo", "Filters": "Filtros", "Zoom in": "Acercar", "Zoom out": "Alejar", "Fit": "Ajustar", "Arranging {n} nodes": "Colocando {n} nodos", "Showing {n} of {m} nodes, the most connected.": "Se muestran {n} de {m} nodos, los más conectados.", "Select a node to see what it connects to.": "Selecciona un nodo para ver sus conexiones.", "Similar, computed": "Similar, calculado", "Open its page": "Abrir su página", "Clear selection": "Quitar selección", "and {n} more": "y {n} más", "Page": "Página", "Tag": "Etiqueta", "Database row": "Fila de base de datos", "Person": "Persona", "links to": "enlaza a", "linked from": "enlazada desde", "embeds": "inserta", "embedded in": "insertada en", "has subpage": "tiene subpágina", "subpage of": "subpágina de", "tagged": "etiquetada", "tag on": "etiqueta en", "contains": "contiene", "row in": "fila en", "involves": "implica a", "owner or author of": "responsable o autor de", "Links to removed pages: {n}": "Enlaces a páginas eliminadas: {n}", "Search failed.": "La búsqueda falló.", "Pages and items connected to this page": "Páginas y elementos conectados a esta página", "Knowledge graph of pages, tags, rows and people": "Grafo de páginas, etiquetas, filas y personas", "Link to {page}": "Enlazar a {page}" },
  ar: { "Similar pages are found by shared words, not by meaning.": "تُحدَّد الصفحات المتشابهة بالكلمات المشتركة، لا بالمعنى.", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "لا توجد روابط بعد. اربط الصفحات باستخدام [[ أو أضف #وسومًا أو قاعدة بيانات، وستظهر الروابط هنا.", "Everything is filtered out. Turn a filter back on.": "كل شيء مُخفى بالتصفية. أعد تشغيل أحد عوامل التصفية.", "The graph could not be built.": "تعذر إنشاء الرسم البياني.", "Try again": "إعادة المحاولة", "Show as list": "عرض كقائمة", "Show as graph": "عرض كرسم بياني", "Filters": "التصفية", "Zoom in": "تكبير", "Zoom out": "تصغير", "Fit": "ملاءمة", "Arranging {n} nodes": "جارٍ ترتيب {n} عقدة", "Showing {n} of {m} nodes, the most connected.": "عرض {n} من {m} عقدة، الأكثر ارتباطًا.", "Select a node to see what it connects to.": "حدد عقدة لرؤية ما ترتبط به.", "Similar, computed": "مشابه، محسوب", "Open its page": "فتح صفحتها", "Clear selection": "إلغاء التحديد", "and {n} more": "و{n} أخرى", "Page": "صفحة", "Tag": "وسم", "Database row": "صف قاعدة بيانات", "Person": "شخص", "links to": "يرتبط بـ", "linked from": "مرتبط من", "embeds": "يضمّن", "embedded in": "مضمّن في", "has subpage": "له صفحة فرعية", "subpage of": "صفحة فرعية من", "tagged": "موسوم", "tag on": "وسم على", "contains": "يحتوي", "row in": "صف في", "involves": "يخص", "owner or author of": "مالك أو مؤلف", "Links to removed pages: {n}": "روابط إلى صفحات محذوفة: {n}", "Search failed.": "فشل البحث.", "Pages and items connected to this page": "الصفحات والعناصر المرتبطة بهذه الصفحة", "Knowledge graph of pages, tags, rows and people": "رسم بياني معرفي للصفحات والوسوم والصفوف والأشخاص", "Link to {page}": "ربط بـ {page}" },
  hi: { "Similar pages are found by shared words, not by meaning.": "समान पेज साझा शब्दों से पहचाने जाते हैं, अर्थ से नहीं।", "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.": "अभी कोई संबंध नहीं है। [[ से पेज जोड़ें, #टैग या डेटाबेस जोड़ें, और संबंध यहाँ दिखेंगे।", "Everything is filtered out. Turn a filter back on.": "सब कुछ फ़िल्टर से छिपा है। कोई फ़िल्टर फिर चालू करें।", "The graph could not be built.": "ग्राफ़ नहीं बन सका।", "Try again": "फिर कोशिश करें", "Show as list": "सूची में दिखाएँ", "Show as graph": "ग्राफ़ में दिखाएँ", "Filters": "फ़िल्टर", "Zoom in": "बड़ा करें", "Zoom out": "छोटा करें", "Fit": "फ़िट करें", "Arranging {n} nodes": "{n} नोड व्यवस्थित हो रहे हैं", "Showing {n} of {m} nodes, the most connected.": "{m} में से {n} नोड दिख रहे हैं, सबसे अधिक जुड़े हुए।", "Select a node to see what it connects to.": "संबंध देखने के लिए कोई नोड चुनें।", "Similar, computed": "समान, गणना से", "Open its page": "उसका पेज खोलें", "Clear selection": "चयन हटाएँ", "and {n} more": "और {n}", "Page": "पेज", "Tag": "टैग", "Database row": "डेटाबेस पंक्ति", "Person": "व्यक्ति", "links to": "लिंक करता है", "linked from": "से लिंक", "embeds": "एम्बेड करता है", "embedded in": "में एम्बेड", "has subpage": "उप-पेज", "subpage of": "का उप-पेज", "tagged": "टैग", "tag on": "टैग इन पर", "contains": "में शामिल", "row in": "पंक्ति इसमें", "involves": "संबंधित", "owner or author of": "स्वामी या लेखक", "Links to removed pages: {n}": "हटाए गए पेजों के लिंक: {n}", "Search failed.": "खोज विफल रही।", "Pages and items connected to this page": "इस पेज से जुड़े पेज और आइटम", "Knowledge graph of pages, tags, rows and people": "पेज, टैग, पंक्ति और लोगों का नॉलेज ग्राफ़", "Link to {page}": "{page} से लिंक करें" },
};
const MEANING_STRINGS = {
  "zh-Hans": {"Search by meaning": "按含义搜索", "Optional. Finds notes and related pages by what they mean, in any language, with a model that runs on this device. Your notes are not sent anywhere.": "可选功能。使用在本设备上运行的模型，按含义查找笔记和相关页面，支持任何语言。你的笔记不会发送到任何地方。", "Off. Related pages and search use shared words.": "已关闭。相关页面和搜索使用共同词语。", "Download model ({size})": "下载模型（{size}）", "Downloaded once from huggingface.co. {model}, licence {licence}.": "只从 huggingface.co 下载一次。{model}，许可证 {licence}。", "Downloading: {done} of {total}": "正在下载：{done} / {total}", "Cancel": "取消", "Checking the download": "正在校验下载内容", "Starting the model": "正在启动模型", "On. Reading your notes: {done} of {total} pages. Shared words are used for pages not read yet.": "已开启。正在读取笔记：{done} / {total} 页。尚未读取的页面使用共同词语。", "On. Related pages and search use meaning. {count} pieces of text are indexed on this device.": "已开启。相关页面和搜索使用含义。本设备已索引 {count} 段文字。", "Remove model": "移除模型", "Meaning search stopped. Related pages and search use shared words.": "按含义搜索已停止。相关页面和搜索使用共同词语。", "The download failed. Nothing was installed.": "下载失败，未安装任何内容。", "This build has no runtime for meaning search. Related pages and search use shared words.": "此版本没有按含义搜索的运行组件。相关页面和搜索使用共同词语。", "Found by meaning, on this device.": "按含义查找，在本设备上完成。", "Found by shared words.": "按共同词语查找。", "Found by shared words. This page has not been read for meaning yet.": "按共同词语查找。此页面尚未按含义读取。", "Matches by meaning": "按含义匹配", "No exact matches. Search by meaning is off; turn it on in Settings.": "没有完全匹配。按含义搜索已关闭，可在设置中开启。"},
  ja: {"Search by meaning": "意味で検索", "Optional. Finds notes and related pages by what they mean, in any language, with a model that runs on this device. Your notes are not sent anywhere.": "任意の機能です。この端末で動くモデルを使い、どの言語でも意味からノートや関連ページを探します。ノートはどこにも送信されません。", "Off. Related pages and search use shared words.": "オフ。関連ページと検索は共通の語句を使います。", "Download model ({size})": "モデルをダウンロード（{size}）", "Downloaded once from huggingface.co. {model}, licence {licence}.": "huggingface.co から一度だけダウンロードします。{model}、ライセンス {licence}。", "Downloading: {done} of {total}": "ダウンロード中：{done} / {total}", "Cancel": "キャンセル", "Checking the download": "ダウンロードを確認中", "Starting the model": "モデルを起動中", "On. Reading your notes: {done} of {total} pages. Shared words are used for pages not read yet.": "オン。ノートを読み取り中：{done} / {total} ページ。未読のページには共通の語句を使います。", "On. Related pages and search use meaning. {count} pieces of text are indexed on this device.": "オン。関連ページと検索は意味を使います。この端末で {count} 個の文章を索引しました。", "Remove model": "モデルを削除", "Meaning search stopped. Related pages and search use shared words.": "意味検索が停止しました。関連ページと検索は共通の語句を使います。", "The download failed. Nothing was installed.": "ダウンロードに失敗しました。何もインストールされていません。", "This build has no runtime for meaning search. Related pages and search use shared words.": "このビルドには意味検索の実行部品がありません。関連ページと検索は共通の語句を使います。", "Found by meaning, on this device.": "意味で検索（この端末上）。", "Found by shared words.": "共通の語句で検索。", "Found by shared words. This page has not been read for meaning yet.": "共通の語句で検索。このページはまだ意味を読み取っていません。", "Matches by meaning": "意味で一致", "No exact matches. Search by meaning is off; turn it on in Settings.": "完全な一致はありません。意味検索はオフです。設定でオンにできます。"},
  es: {"Search by meaning": "Buscar por significado", "Optional. Finds notes and related pages by what they mean, in any language, with a model that runs on this device. Your notes are not sent anywhere.": "Opcional. Encuentra notas y páginas relacionadas por su significado, en cualquier idioma, con un modelo que se ejecuta en este dispositivo. Tus notas no se envían a ningún sitio.", "Off. Related pages and search use shared words.": "Desactivado. Las páginas relacionadas y la búsqueda usan palabras compartidas.", "Download model ({size})": "Descargar modelo ({size})", "Downloaded once from huggingface.co. {model}, licence {licence}.": "Se descarga una vez desde huggingface.co. {model}, licencia {licence}.", "Downloading: {done} of {total}": "Descargando: {done} de {total}", "Cancel": "Cancelar", "Checking the download": "Comprobando la descarga", "Starting the model": "Iniciando el modelo", "On. Reading your notes: {done} of {total} pages. Shared words are used for pages not read yet.": "Activado. Leyendo tus notas: {done} de {total} páginas. Las páginas aún sin leer usan palabras compartidas.", "On. Related pages and search use meaning. {count} pieces of text are indexed on this device.": "Activado. Las páginas relacionadas y la búsqueda usan el significado. {count} fragmentos de texto indexados en este dispositivo.", "Remove model": "Quitar modelo", "Meaning search stopped. Related pages and search use shared words.": "La búsqueda por significado se detuvo. Las páginas relacionadas y la búsqueda usan palabras compartidas.", "The download failed. Nothing was installed.": "La descarga falló. No se instaló nada.", "This build has no runtime for meaning search. Related pages and search use shared words.": "Esta versión no incluye el componente de búsqueda por significado. Las páginas relacionadas y la búsqueda usan palabras compartidas.", "Found by meaning, on this device.": "Encontrado por significado, en este dispositivo.", "Found by shared words.": "Encontrado por palabras compartidas.", "Found by shared words. This page has not been read for meaning yet.": "Encontrado por palabras compartidas. Esta página aún no se ha leído por significado.", "Matches by meaning": "Coincidencias por significado", "No exact matches. Search by meaning is off; turn it on in Settings.": "Sin coincidencias exactas. La búsqueda por significado está desactivada; actívala en Ajustes."},
  ar: {"Search by meaning": "البحث بالمعنى", "Optional. Finds notes and related pages by what they mean, in any language, with a model that runs on this device. Your notes are not sent anywhere.": "اختياري. يعثر على الملاحظات والصفحات ذات الصلة حسب معناها، بأي لغة، باستخدام نموذج يعمل على هذا الجهاز. لا تُرسل ملاحظاتك إلى أي مكان.", "Off. Related pages and search use shared words.": "متوقف. تستخدم الصفحات ذات الصلة والبحث الكلمات المشتركة.", "Download model ({size})": "تنزيل النموذج ({size})", "Downloaded once from huggingface.co. {model}, licence {licence}.": "يُنزَّل مرة واحدة من huggingface.co. {model}، الترخيص {licence}.", "Downloading: {done} of {total}": "جارٍ التنزيل: {done} من {total}", "Cancel": "إلغاء", "Checking the download": "جارٍ التحقق من التنزيل", "Starting the model": "جارٍ تشغيل النموذج", "On. Reading your notes: {done} of {total} pages. Shared words are used for pages not read yet.": "يعمل. جارٍ قراءة ملاحظاتك: {done} من {total} صفحة. تُستخدم الكلمات المشتركة للصفحات التي لم تُقرأ بعد.", "On. Related pages and search use meaning. {count} pieces of text are indexed on this device.": "يعمل. تستخدم الصفحات ذات الصلة والبحث المعنى. فُهرس {count} مقطعًا نصيًا على هذا الجهاز.", "Remove model": "إزالة النموذج", "Meaning search stopped. Related pages and search use shared words.": "توقف البحث بالمعنى. تستخدم الصفحات ذات الصلة والبحث الكلمات المشتركة.", "The download failed. Nothing was installed.": "فشل التنزيل. لم يُثبَّت أي شيء.", "This build has no runtime for meaning search. Related pages and search use shared words.": "لا يتضمن هذا الإصدار مكوّن البحث بالمعنى. تستخدم الصفحات ذات الصلة والبحث الكلمات المشتركة.", "Found by meaning, on this device.": "عُثر عليها بالمعنى، على هذا الجهاز.", "Found by shared words.": "عُثر عليها بالكلمات المشتركة.", "Found by shared words. This page has not been read for meaning yet.": "عُثر عليها بالكلمات المشتركة. لم تُقرأ هذه الصفحة بالمعنى بعد.", "Matches by meaning": "نتائج بالمعنى", "No exact matches. Search by meaning is off; turn it on in Settings.": "لا توجد نتائج مطابقة. البحث بالمعنى متوقف؛ فعّله من الإعدادات."},
  hi: {"Search by meaning": "अर्थ से खोजें", "Optional. Finds notes and related pages by what they mean, in any language, with a model that runs on this device. Your notes are not sent anywhere.": "वैकल्पिक। इस डिवाइस पर चलने वाले मॉडल से किसी भी भाषा में अर्थ के आधार पर नोट और संबंधित पेज ढूँढता है। आपके नोट कहीं नहीं भेजे जाते।", "Off. Related pages and search use shared words.": "बंद। संबंधित पेज और खोज साझा शब्दों का उपयोग करते हैं।", "Download model ({size})": "मॉडल डाउनलोड करें ({size})", "Downloaded once from huggingface.co. {model}, licence {licence}.": "huggingface.co से एक बार डाउनलोड होता है। {model}, लाइसेंस {licence}।", "Downloading: {done} of {total}": "डाउनलोड हो रहा है: {total} में से {done}", "Cancel": "रद्द करें", "Checking the download": "डाउनलोड की जाँच हो रही है", "Starting the model": "मॉडल शुरू हो रहा है", "On. Reading your notes: {done} of {total} pages. Shared words are used for pages not read yet.": "चालू। नोट पढ़े जा रहे हैं: {total} में से {done} पेज। जो पेज अभी नहीं पढ़े गए उनके लिए साझा शब्द उपयोग होते हैं।", "On. Related pages and search use meaning. {count} pieces of text are indexed on this device.": "चालू। संबंधित पेज और खोज अर्थ का उपयोग करते हैं। इस डिवाइस पर पाठ के {count} अंश अनुक्रमित हैं।", "Remove model": "मॉडल हटाएँ", "Meaning search stopped. Related pages and search use shared words.": "अर्थ से खोज रुक गई। संबंधित पेज और खोज साझा शब्दों का उपयोग करते हैं।", "The download failed. Nothing was installed.": "डाउनलोड विफल रहा। कुछ भी इंस्टॉल नहीं हुआ।", "This build has no runtime for meaning search. Related pages and search use shared words.": "इस बिल्ड में अर्थ से खोज का घटक नहीं है। संबंधित पेज और खोज साझा शब्दों का उपयोग करते हैं।", "Found by meaning, on this device.": "अर्थ से मिला, इसी डिवाइस पर।", "Found by shared words.": "साझा शब्दों से मिला।", "Found by shared words. This page has not been read for meaning yet.": "साझा शब्दों से मिला। यह पेज अभी अर्थ के लिए नहीं पढ़ा गया है।", "Matches by meaning": "अर्थ से मिलान", "No exact matches. Search by meaning is off; turn it on in Settings.": "कोई सटीक मिलान नहीं। अर्थ से खोज बंद है; इसे सेटिंग्स में चालू करें।"},
};
for (const source of [STRINGS, MEANING_STRINGS]) for (const [lang, table] of Object.entries(source)) if (I18N[lang]) for (const [k, v] of Object.entries(table)) I18N[lang][k] ??= v;

const svgEl = (tag, attrs = {}) => { const n = document.createElementNS(SVG, tag); for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v); return n; };
const num = n => new Intl.NumberFormat(api.lang).format(n);
const fill = (s, map) => s.replace(/\{(\w+)\}/g, (_, k) => map[k] ?? "");
const graphemes = text => { try { return [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(text)].map(x => x.segment); } catch { return [...text]; } };
const clip = (text, max) => { const g = graphemes(text); return g.length > max ? g.slice(0, max - 1).join("") + "…" : text; };
const match = (hay, q) => hay.normalize("NFC").toLocaleLowerCase().includes(q);
const labelOf = n => n.label || api.t("Untitled");

/* ---------- The derived graph, cached by the engine's revision ---------- */
let cache = { rev: -1, graph: null };
function currentGraph() {
  const rev = api.engine.revision();
  if (cache.rev !== rev || !cache.graph) {
    const g = JSON.parse(api.engine.graph());
    const nodes = new Map(g.nodes.map(n => [n.id, n]));
    const around = new Map(g.nodes.map(n => [n.id, []]));
    for (const e of g.edges) { around.get(e.from)?.push({ id: e.to, e, forward: true }); around.get(e.to)?.push({ id: e.from, e, forward: false }); }
    cache = { rev, graph: { nodes, edges: g.edges, around } };
  }
  return cache.graph;
}
const RELATIONS = { link: ["links to", "linked from"], embed: ["embeds", "embedded in"], child: ["has subpage", "subpage of"], tag: ["tagged", "tag on"], row: ["contains", "row in"], owner: ["involves", "owner or author of"] };
const KIND_LABEL = { page: "Page", tag: "Tag", row: "Database row", person: "Person" };
const percent = score => new Intl.NumberFormat(api.lang, { style: "percent" }).format(score / 1000);

/* ---------- Layout: deterministic, because start positions come from a spiral and never from chance ---------- */
const SPRINGS = { child: [110, .05], link: [170, .03], embed: [150, .03], tag: [90, .04], row: [64, .08], owner: [76, .05], similar: [240, .008] };
function settleStep(nodes, edges, alpha) {
  for (let i = 0; i < nodes.length; i++) {
    const a = nodes[i];
    for (let j = i + 1; j < nodes.length; j++) {
      const b = nodes[j]; let dx = a.x - b.x, dy = a.y - b.y; const d2 = dx * dx + dy * dy || 1;
      if (d2 > 500000) continue;
      const f = 3600 / d2 * alpha, d = Math.sqrt(d2); dx /= d; dy /= d;
      a.vx += dx * f; a.vy += dy * f; b.vx -= dx * f; b.vy -= dy * f;
    }
  }
  for (const e of edges) {
    const a = e.na, b = e.nb, [len, s] = SPRINGS[e.kind] || [90, .04];
    const dx = b.x - a.x, dy = b.y - a.y, d = Math.hypot(dx, dy) || 1, f = (d - len) * s * alpha;
    a.vx += dx / d * f; a.vy += dy / d * f; b.vx -= dx / d * f; b.vy -= dy / d * f;
  }
  for (const n of nodes) {
    n.vx -= n.x * .008 * alpha; n.vy -= n.y * .008 * alpha; n.vx *= .82; n.vy *= .82;
    if (!n.pin) { n.x += n.vx; n.y += n.vy; }
  }
}
const iterationsFor = n => Math.max(40, Math.min(420, Math.floor(4e7 / Math.max(1, n * n))));

/* ---------- The whole graph ---------- */
const view = {
  filter: { page: true, tag: true, row: true, person: true, similar: true },
  positions: new Map(), camera: null, selected: null, hover: null, query: "", list: false, filtersOpen: null, cancel: null,
};

function pickShown(g) {
  const wanted = [...g.nodes.values()].filter(n => view.filter[n.kind]);
  const made = new Map();
  for (const e of g.edges) if (e.kind !== "similar") { made.set(e.from, (made.get(e.from) || 0) + 1); made.set(e.to, (made.get(e.to) || 0) + 1); }
  let shown = wanted;
  if (wanted.length > MAX_NODES) shown = [...wanted].sort((a, b) => (made.get(b.id) || 0) - (made.get(a.id) || 0) || (a.id < b.id ? -1 : 1)).slice(0, MAX_NODES);
  const ids = new Set(shown.map(n => n.id));
  const edges = g.edges.filter(e => ids.has(e.from) && ids.has(e.to) && (e.kind !== "similar" || view.filter.similar)).slice(0, MAX_EDGES);
  return { shown, edges, total: wanted.length };
}

function renderGraph(content) {
  view.cancel?.(); view.cancel = null; view.hover = null;
  let g;
  try { g = currentGraph(); } catch (err) {
    console.warn("graph", err);
    content.replaceChildren(api.el("div", { class: "kg" }, api.el("div", { class: "kgempty", role: "alert" }, api.el("p", { text: api.t("The graph could not be built.") }), api.el("button", { class: "btn", text: api.t("Try again"), onclick: () => { cache.rev = -1; renderGraph(content); } }))));
    return;
  }
  if (view.selected && !g.nodes.has(view.selected)) view.selected = null;
  const { shown, edges: shownEdges, total } = pickShown(g);
  const nodes = shown.map((n, i) => {
    const p = view.positions.get(n.id), a = i * 2.399963, r = 40 * Math.sqrt(i + 1);
    return { ...n, x: p?.x ?? r * Math.cos(a), y: p?.y ?? r * Math.sin(a), vx: 0, vy: 0, pin: !!p?.pin };
  });
  const byId = new Map(nodes.map(n => [n.id, n]));
  const edges = shownEdges.map(e => ({ ...e, na: byId.get(e.from), nb: byId.get(e.to) }));
  const degree = new Map();
  for (const e of edges) { degree.set(e.from, (degree.get(e.from) || 0) + 1); degree.set(e.to, (degree.get(e.to) || 0) + 1); }

  const box = api.el("div", { class: "gv" });
  const side = api.el("aside", { class: "kgside", "aria-live": "polite", "aria-label": api.t("Details") });
  const root = api.el("div", { class: "kg" }, box, side);

  /* The controls */
  const count = k => [...g.nodes.values()].filter(n => n.kind === k).length;
  const similarCount = g.edges.filter(e => e.kind === "similar").length;
  const check = (key, label, n, cls = "") => api.el("label", { class: cls }, api.el("input", { type: "checkbox", id: "gf-" + key, ...(view.filter[key] ? { checked: true } : {}), onchange: e => { view.filter[key] = e.target.checked; renderGraph(content); } }), api.el("span", { text: label }), api.el("span", { class: "k", text: num(n) }));
  const missNote = api.el("p", { class: "kgmiss", role: "status", hidden: true, text: api.t("No matches") });
  const findBox = api.el("input", { type: "search", id: "gq", placeholder: api.t("Find in graph"), "aria-label": api.t("Find in graph"), value: view.query,
    oninput: e => { view.query = e.target.value.trim().normalize("NFC").toLocaleLowerCase(); paintFocus(); },
    onkeydown: e => {
      if (e.key !== "Enter") return;
      e.preventDefault();
      const hit = view.query && nodes.find(n => match(labelOf(n), view.query));
      if (hit) { select(hit.id); centerOn(hit); }
    } });
  const opened = view.filtersOpen ?? !matchMedia("(max-width: 760px)").matches;
  const body = api.el("div", { class: "gctlbody" },
    check("page", api.t("Pages"), count("page")), check("tag", api.t("Tags"), count("tag")), check("row", api.t("Database rows"), count("row")), check("person", api.t("People"), count("person")),
    check("similar", api.t("Similar pages"), similarCount, "gsimilar"),
    api.el("p", { text: api.t("Dashed lines are computed similarity. Solid lines come from your links, tags and data.") }),
    api.el("p", { text: api.t("Similar pages are found by shared words, not by meaning.") }),
    total > shown.length ? api.el("p", { class: "kgshowing", text: fill(api.t("Showing {n} of {m} nodes, the most connected."), { n: num(shown.length), m: num(total) }) }) : null,
    api.el("button", { class: "textlink kgaslist", text: api.t(view.list ? "Show as graph" : "Show as list"), onclick: () => { view.list = !view.list; renderGraph(content); } }));
  body.hidden = !opened;
  const toggle = api.el("button", { class: "gtoggle", "aria-expanded": String(opened), text: api.t("Filters"), onclick: () => { body.hidden = !body.hidden; view.filtersOpen = !body.hidden; toggle.setAttribute("aria-expanded", String(!body.hidden)); } });
  const ctl = api.el("div", { class: "gctl" }, findBox, missNote, toggle, body);

  /* The text alternative: every graph view has a list form (QR-5). */
  if (view.list) {
    const list = api.el("div", { class: "kglist", role: "list" });
    if (!nodes.length) list.append(api.el("p", { text: api.t("Everything is filtered out. Turn a filter back on.") }));
    for (const n of nodes) {
      const rels = (g.around.get(n.id) || []).filter(x => byId.has(x.id));
      list.append(api.el("button", { class: "kgrow" + (n.id === view.selected ? " on" : ""), role: "listitem", "data-id": n.id, onclick: () => { view.selected = n.id; renderGraph(content); } },
        api.el("span", { class: "kgrowt", dir: "auto", text: labelOf(n) }), api.el("small", { text: `${api.t(KIND_LABEL[n.kind])}, ${num(rels.length)}` })));
    }
    box.append(list, ctl);
    box.classList.add("listed");
    content.replaceChildren(root);
    paintDetail();
    return;
  }

  /* The drawing */
  const svg = svgEl("svg", { id: "gsvg", role: "group", "aria-label": api.t("Knowledge graph of pages, tags, rows and people") });
  const world = svgEl("g"); svg.append(world);
  const lines = edges.map(e => { const l = svgEl("line", { class: "e-" + e.kind }); if (e.kind === "similar") l.setAttribute("data-score", e.score); world.append(l); return l; });
  const shapes = nodes.map((n, i) => {
    const deg = degree.get(n.id) || 0;
    const r = n.kind === "page" ? 5 + Math.min(6, deg * .6) : n.kind === "row" ? 3.5 : 5;
    const gn = svgEl("g", { class: `n k-${n.kind}`, tabindex: i === 0 ? "0" : "-1", "data-id": n.id, role: "button", "aria-label": `${labelOf(n)}, ${api.t(KIND_LABEL[n.kind])}, ${num(deg)}` });
    if (n.kind === "tag") gn.append(svgEl("rect", { class: "sh", x: -r, y: -r, width: 2 * r, height: 2 * r }));
    else if (n.kind === "person") gn.append(svgEl("path", { class: "sh", d: `M0 ${-r - 1}L${r + 1} 0L0 ${r + 1}L${-r - 1} 0Z` }));
    else gn.append(svgEl("circle", { class: "sh", r }));
    const label = svgEl("text", { x: r + 4, y: 4 }); label.textContent = clip(labelOf(n), 28); gn.append(label);
    world.append(gn);
    return gn;
  });
  const empty = !nodes.length ? "Everything is filtered out. Turn a filter back on." : (g.edges.length ? null : "Nothing is connected yet. Link pages with [[, add #tags or a database, and the connections appear here.");
  const emptyNote = empty ? api.el("div", { class: "kgempty" }, api.el("p", { text: api.t(empty) })) : null;
  const status = api.el("div", { class: "kgstatus", role: "status", hidden: true });
  const zoomLabel = api.el("span");
  const zoom = api.el("div", { class: "zoom" },
    api.el("button", { "aria-label": api.t("Zoom out"), text: "−", onclick: () => zoomBy(.8) }), zoomLabel,
    api.el("button", { "aria-label": api.t("Zoom in"), text: "+", onclick: () => zoomBy(1.25) }),
    api.el("button", { class: "kgfit", text: api.t("Fit"), onclick: () => fit() }));
  box.append(svg, ctl, zoom, status);
  if (emptyNote) box.append(emptyNote);
  content.replaceChildren(root);

  const draw = () => {
    edges.forEach((e, i) => { const l = lines[i]; l.setAttribute("x1", e.na.x); l.setAttribute("y1", e.na.y); l.setAttribute("x2", e.nb.x); l.setAttribute("y2", e.nb.y); });
    nodes.forEach((n, i) => shapes[i].setAttribute("transform", `translate(${n.x.toFixed(1)} ${n.y.toFixed(1)})`));
  };
  const remember = () => { for (const n of nodes) view.positions.set(n.id, { x: n.x, y: n.y, pin: n.pin }); };
  const applyCamera = () => {
    const c = view.camera;
    world.setAttribute("transform", `translate(${c.x} ${c.y}) scale(${c.z})`);
    svg.classList.toggle("zoomed", c.z >= 1.25);
    zoomLabel.textContent = new Intl.NumberFormat(api.lang, { style: "percent" }).format(c.z);
  };
  const zoomAt = (x, y, z) => {
    const c = view.camera; z = Math.max(.15, Math.min(4, z));
    c.x = x - (x - c.x) * (z / c.z); c.y = y - (y - c.y) * (z / c.z); c.z = z; applyCamera();
  };
  const zoomBy = f => { const r = svg.getBoundingClientRect(); zoomAt(r.width / 2, r.height / 2, view.camera.z * f); };
  function fit() {
    const r = svg.getBoundingClientRect();
    if (!nodes.length || !r.width) { view.camera = { x: r.width / 2, y: r.height / 2, z: 1 }; applyCamera(); return; }
    const xs = nodes.map(n => n.x), ys = nodes.map(n => n.y), minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
    // Keep clear of the controls when there is room beside them. They sit on the inline start.
    const inset = r.width > 480 ? ctl.getBoundingClientRect().width + 24 : 0, rtl = document.documentElement.dir === "rtl", room = r.width - inset;
    const w = maxX - minX + 220, h = maxY - minY + 80, z = Math.max(.15, Math.min(1.4, Math.min(room / w, r.height / h)));
    view.camera = { z, x: (rtl ? 0 : inset) + room / 2 - z * (maxX + minX + 180) / 2 + z * 90, y: r.height / 2 - z * (maxY + minY) / 2 };
    applyCamera();
  }
  function centerOn(n) { const r = svg.getBoundingClientRect(), c = view.camera; c.x = r.width / 2 - n.x * c.z; c.y = r.height / 2 - n.y * c.z; applyCamera(); }
  function paintFocus() {
    const f = view.hover || view.selected;
    const near = f && byId.has(f) ? new Set([f, ...(g.around.get(f) || []).filter(x => x.e.kind !== "similar" || view.filter.similar).map(x => x.id)]) : null;
    svg.classList.toggle("focus", !!near);
    let hits = 0;
    nodes.forEach((n, i) => {
      const miss = !!view.query && !match(labelOf(n), view.query);
      if (view.query && !miss) hits++;
      const c = shapes[i].classList;
      c.toggle("hl", !!near && near.has(n.id)); c.toggle("sel", n.id === view.selected); c.toggle("miss", miss); c.toggle("found", !!view.query && !miss);
    });
    edges.forEach((e, i) => lines[i].classList.toggle("hl", !!near && (e.from === f || e.to === f)));
    missNote.hidden = !view.query || hits > 0;
  }
  function select(id) { view.selected = id; paintFocus(); paintDetail(); }
  view.select = select;
  view.reselect = id => { const n = byId.get(id); select(id); if (n) centerOn(n); };

  /* Pointers: drag a node, pan the background, pinch or wheel to zoom. */
  const pointers = new Map();
  let act = null, pinch = null;
  const toWorld = e => { const r = svg.getBoundingClientRect(), c = view.camera; return [(e.clientX - r.left - c.x) / c.z, (e.clientY - r.top - c.y) / c.z]; };
  svg.addEventListener("wheel", e => {
    e.preventDefault();
    const r = svg.getBoundingClientRect();
    if (e.ctrlKey || e.metaKey || e.deltaMode === 1) zoomAt(e.clientX - r.left, e.clientY - r.top, view.camera.z * Math.exp(-e.deltaY * (e.deltaMode === 1 ? .08 : .006)));
    else { view.camera.x -= e.deltaX; view.camera.y -= e.deltaY; applyCamera(); }
  }, { passive: false });
  svg.addEventListener("pointerdown", e => {
    pointers.set(e.pointerId, [e.clientX, e.clientY]);
    try { svg.setPointerCapture(e.pointerId); } catch { /* a synthetic pointer */ }
    if (pointers.size === 2) { const [a, b] = [...pointers.values()]; pinch = { d: Math.hypot(a[0] - b[0], a[1] - b[1]), z: view.camera.z }; act = null; return; }
    const gn = e.target.closest?.(".n");
    if (gn) act = { n: byId.get(gn.dataset.id), moved: false, sx: e.clientX, sy: e.clientY };
    else { svg.classList.add("grabbing"); act = { pan: true, x: e.clientX, y: e.clientY, vx: view.camera.x, vy: view.camera.y, moved: false }; }
  });
  svg.addEventListener("pointermove", e => {
    if (pointers.has(e.pointerId)) pointers.set(e.pointerId, [e.clientX, e.clientY]);
    if (pinch && pointers.size === 2) {
      const [a, b] = [...pointers.values()], r = svg.getBoundingClientRect();
      zoomAt((a[0] + b[0]) / 2 - r.left, (a[1] + b[1]) / 2 - r.top, pinch.z * Math.hypot(a[0] - b[0], a[1] - b[1]) / (pinch.d || 1));
      return;
    }
    if (!act) { const id = e.target.closest?.(".n")?.dataset.id || null; if (id !== view.hover) { view.hover = id; paintFocus(); } return; }
    if (act.pan) {
      if (Math.hypot(e.clientX - act.x, e.clientY - act.y) > 3) act.moved = true;
      view.camera.x = act.vx + e.clientX - act.x; view.camera.y = act.vy + e.clientY - act.y; applyCamera();
      return;
    }
    if (!act.moved && Math.hypot(e.clientX - act.sx, e.clientY - act.sy) < 3) return;
    act.moved = true;
    const [x, y] = toWorld(e); act.n.x = x; act.n.y = y; act.n.pin = true;
    if (nodes.length <= 600) for (let i = 0; i < 2; i++) settleStep(nodes, edges, .3);
    remember(); draw();
  });
  const release = e => {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinch = null;
    if (!act) return;
    svg.classList.remove("grabbing");
    if (act.n && !act.moved) select(act.n.id);
    else if (act.pan && !act.moved && view.selected) select(null);
    act = null;
  };
  svg.addEventListener("pointerup", release);
  svg.addEventListener("pointercancel", release);
  svg.addEventListener("pointerleave", () => { if (!act && view.hover) { view.hover = null; paintFocus(); } });

  /* Keyboard: one tab stop, then arrows walk to the nearest node that way. */
  const focusNode = n => { shapes.forEach(s => s.setAttribute("tabindex", "-1")); const s = shapes[nodes.indexOf(n)]; s.setAttribute("tabindex", "0"); s.focus(); view.hover = n.id; paintFocus(); };
  svg.addEventListener("focusin", e => { const id = e.target.closest?.(".n")?.dataset.id; if (id && id !== view.hover) { view.hover = id; paintFocus(); } });
  svg.addEventListener("focusout", () => { view.hover = null; paintFocus(); });
  svg.addEventListener("keydown", e => {
    const gn = e.target.closest?.(".n"), n = gn && byId.get(gn.dataset.id);
    if (e.key === "+" || e.key === "=") { e.preventDefault(); zoomBy(1.25); return; }
    if (e.key === "-") { e.preventDefault(); zoomBy(.8); return; }
    if (e.key === "0") { e.preventDefault(); fit(); return; }
    if (e.key === "Escape" && view.selected) { e.stopPropagation(); select(null); return; }
    if (!n) return;
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); select(n.id); return; }
    const dir = { ArrowRight: [1, 0], ArrowLeft: [-1, 0], ArrowDown: [0, 1], ArrowUp: [0, -1] }[e.key];
    if (!dir) return;
    e.preventDefault();
    let best = null, bestScore = Infinity;
    for (const o of nodes) {
      if (o === n) continue;
      const dx = o.x - n.x, dy = o.y - n.y, along = dx * dir[0] + dy * dir[1];
      if (along <= 0) continue;
      const score = Math.hypot(dx, dy) + 2 * Math.abs(dx * dir[1] - dy * dir[0]);
      if (score < bestScore) { bestScore = score; best = o; }
    }
    if (best) { focusNode(best); const r = svg.getBoundingClientRect(), c = view.camera, sx = best.x * c.z + c.x, sy = best.y * c.z + c.y; if (sx < 20 || sy < 20 || sx > r.width - 20 || sy > r.height - 20) centerOn(best); }
  });

  /* Settling. A warm start needs little. A big graph settles in slices, so the app stays responsive. */
  const warm = nodes.length > 0 && nodes.every(n => view.positions.has(n.id));
  const total_iters = warm ? 30 : iterationsFor(nodes.length);
  const alphaAt = it => Math.max(warm ? .03 : .06, (warm ? .25 : 1) * (1 - it / total_iters));
  const finish = () => { remember(); draw(); if (!warm || !view.camera) fit(); else applyCamera(); paintFocus(); paintDetail(); status.hidden = true; root.dataset.settled = "1"; };
  if (!view.camera) view.camera = { x: 0, y: 0, z: 1 };
  if (nodes.length <= SYNC_LAYOUT_NODES || REDUCED()) {
    for (let it = 0; it < total_iters; it++) settleStep(nodes, edges, alphaAt(it));
    finish();
  } else {
    status.hidden = false; status.textContent = fill(api.t("Arranging {n} nodes"), { n: num(nodes.length) });
    let it = 0, stop = false;
    view.cancel = () => { stop = true; };
    draw(); fit(); paintFocus(); paintDetail();
    const slice = () => {
      if (stop || !root.isConnected) return;
      const end = performance.now() + 12;
      while (it < total_iters && performance.now() < end) settleStep(nodes, edges, alphaAt(it++));
      draw();
      if (it < total_iters) requestAnimationFrame(slice); else finish();
    };
    requestAnimationFrame(slice);
  }

  function paintDetail() {
    side.replaceChildren();
    const n = view.selected && g.nodes.get(view.selected);
    side.classList.toggle("open", !!n);
    root.classList.toggle("withdetail", !!n);
    if (!n) return;
    const close = api.el("button", { class: "ibtn kgclose", "aria-label": api.t("Clear selection"), onclick: () => { view.selected = null; if (view.list) renderGraph(content); else select(null); } }, api.icon("x"));
    side.append(api.el("div", { class: "nodehead" }, close, api.el("small", { text: api.t(KIND_LABEL[n.kind]) + (n.status ? `, ${n.status}` : "") }), api.el("h3", { dir: "auto", text: labelOf(n) })));
    if (n.page) side.append(api.el("button", { class: "btn primary kgopen", text: api.t(n.kind === "page" ? "Open page" : "Open its page"), onclick: () => api.openPage(n.page) }));
    const groups = new Map(), computed = [];
    for (const x of g.around.get(n.id) || []) {
      if (x.e.kind === "similar") { computed.push(x); continue; }
      const label = RELATIONS[x.e.kind]?.[x.forward ? 0 : 1];
      if (!groups.has(label)) groups.set(label, []);
      groups.get(label).push(x);
    }
    const row = x => api.el("div", { class: "rel" },
      api.el("button", { dir: "auto", text: labelOf(g.nodes.get(x.id)), onclick: () => { if (view.list) { view.selected = x.id; renderGraph(content); } else if (byId.has(x.id)) view.reselect(x.id); else select(x.id); } }),
      x.e.kind === "similar" ? api.el("span", { text: percent(x.e.score) }) : x.e.count > 1 ? api.el("span", { text: num(x.e.count) }) : null);
    if (!groups.size && !computed.length) side.append(api.el("div", { class: "rp" }, api.el("p", { text: api.t("No connections yet.") })));
    for (const [label, list] of groups) side.append(api.el("div", { class: "rp" }, api.el("h4", { text: api.t(label) }), ...list.map(row)));
    if (computed.length) side.append(api.el("div", { class: "rp kgcomputed" }, api.el("h4", { text: api.t("Similar, computed") }), ...computed.map(row)));
    if (n.dangling) side.append(api.el("div", { class: "rp" }, api.el("p", { text: fill(api.t("Links to removed pages: {n}"), { n: num(n.dangling) }) })));
  }
}

/* ---------- The side panel ---------- */
let panelTimer = null;
function ensureSection(id, title) {
  let box = api.$("#" + id);
  if (!box) {
    box = api.el("div", { class: "rp", id }, api.el("h4", { "data-t": title, text: api.t(title) }), api.el("div", { class: "rpbody" }));
    api.$("#rpDetails")?.append(box);
  }
  return api.$(".rpbody", box);
}
function paintPanels() {
  if (meaningOn() && api.engine.revision() !== meaning.seenRev) { markAll(); indexLoop(); }
  if (!api.$("#rpDetails") || api.state.view) return;
  const pageId = api.state.cur; if (!pageId) return;
  const related = ensureSection("rpRelated", "Related"), local = ensureSection("rpLocal", "Connections");
  let g;
  try { g = currentGraph(); } catch (err) { console.warn("graph", err); related.replaceChildren(api.el("p", { text: api.t("The graph could not be built.") })); local.replaceChildren(); return; }
  const me = "page:" + pageId, around = g.around.get(me) || [];

  paintRelated(related, pageId, g, around);

  local.replaceChildren();
  const near = [...around].sort((a, b) => (a.e.kind === "similar") - (b.e.kind === "similar") || (a.id < b.id ? -1 : 1));
  const seen = new Set(), unique = near.filter(x => !seen.has(x.id) && seen.add(x.id));
  if (!unique.length) { local.append(api.el("p", { text: api.t("No connections yet.") })); }
  else {
    const W = 270, H = 170, cx = W / 2, cy = H / 2, draw = unique.slice(0, LOCAL_NODES);
    const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}`, role: "group", "aria-label": api.t("Pages and items connected to this page") });
    draw.forEach((x, i) => {
      const a = i / draw.length * Math.PI * 2 - Math.PI / 2, r = draw.length > 8 ? 62 : 58, nx = cx + r * 1.45 * Math.cos(a), ny = cy + r * Math.sin(a);
      const line = svgEl("line", { x1: cx, y1: cy, x2: nx, y2: ny }); if (x.e.kind === "similar") line.setAttribute("class", "sim"); svg.append(line);
      const other = g.nodes.get(x.id), open = other.page || null;
      const dot = svgEl("circle", { cx: nx, cy: ny, r: 4.5, tabindex: open ? "0" : "-1", role: open ? "button" : "img", "aria-label": `${labelOf(other)}, ${api.t(KIND_LABEL[other.kind])}` });
      if (other.kind !== "page") dot.classList.add("k-other");
      const tip = svgEl("title"); tip.textContent = labelOf(other); dot.append(tip);
      if (open) { dot.addEventListener("click", () => api.openPage(open)); dot.addEventListener("keydown", e => { if (e.key === "Enter") { e.preventDefault(); api.openPage(open); } }); }
      svg.append(dot);
      if (draw.length <= 8) {
        const label = svgEl("text", { x: nx + (nx >= cx ? 8 : -8), y: ny + 3.5, "text-anchor": nx >= cx ? "start" : "end" });
        label.textContent = clip(labelOf(other), 12); svg.append(label);
      }
    });
    svg.append(svgEl("circle", { cx, cy, r: 6, class: "cur" }));
    local.append(api.el("div", { class: "localg" }, svg));
    const more = unique.length - draw.length;
    if (more > 0) local.append(api.el("p", { text: fill(api.t("and {n} more"), { n: num(more) }) }));
  }
  local.append(api.el("button", { class: "textlink", text: api.t("Full graph"), onclick: () => { view.selected = me; api.openView("graph"); } }));
}
/** Related pages: by meaning when this page has vectors, otherwise by shared words, and it says which. */
let relatedAsk = 0;
function relatedRows(box, pageId, rows, caption) {
  box.replaceChildren();
  if (!rows.length) box.append(api.el("p", { text: api.t("Nothing similar yet.") }));
  for (const { other, score } of rows) {
    box.append(api.el("div", { class: "rel" },
      api.el("button", { dir: "auto", text: labelOf(other), onclick: () => api.openPage(other.page) }),
      api.el("span", { text: percent(score) }),
      api.el("button", { class: "lk", text: api.t("Link"), "aria-label": fill(api.t("Link to {page}"), { page: labelOf(other) }), onclick: () => addLink(pageId, other) })));
  }
  box.append(api.el("p", { class: "kgmethod", "data-method": caption.method }, api.el("small", { text: api.t(caption.text) })));
}
function paintRelated(box, pageId, g, around) {
  const ask = ++relatedAsk;
  const words = around.filter(x => x.e.kind === "similar").sort((a, b) => b.e.score - a.e.score || (a.id < b.id ? -1 : 1)).slice(0, 5).map(x => ({ other: g.nodes.get(x.id), score: x.e.score }));
  const byWords = text => relatedRows(box, pageId, words, { method: "words", text });
  if (!meaningOn()) { byWords(meaning.state === "failed" ? "Meaning search stopped. Related pages and search use shared words." : "Found by shared words."); return; }
  embedder.call("related", { page: pageId, k: 12, threshold: MODEL.relatedThreshold }).then(({ known, near }) => {
    if (ask !== relatedAsk || !box.isConnected) return;
    if (!known) { byWords("Found by shared words. This page has not been read for meaning yet."); return; }
    const rows = near.map(n => ({ other: g.nodes.get("page:" + n.page), score: n.score / 1000 })).filter(r => r.other).slice(0, 5);
    relatedRows(box, pageId, rows, { method: "meaning", text: "Found by meaning, on this device." });
  }).catch(err => { stopMeaning(err); if (ask === relatedAsk && box.isConnected) byWords("Meaning search stopped. Related pages and search use shared words."); });
}
function addLink(pageId, other) {
  const page = api.pageData(pageId), last = page.blocks[page.blocks.length - 1];
  api.edit({ op: "insert_block", page: pageId, after: last ? last.id : null, block: { id: api.uid(), kind: "paragraph", checked: false, runs: [{ text: labelOf(other), marks: [], link: other.page }] } });
  if (api.state.cur === pageId && !api.state.view) api.renderPage();
  schedulePanels();
}
// Deriving the graph reads the whole workspace, so it never runs between keystrokes: typing pushes
// it back until the person pauses. SPK-02 measured it as the largest single cost near the typing path.
const TYPING_REST_MS = 1500;
let lastTyped = -Infinity;
function schedulePanels() {
  if (panelTimer) return;
  const rest = lastTyped + TYPING_REST_MS - performance.now();
  panelTimer = setTimeout(() => { panelTimer = null; if (performance.now() - lastTyped < TYPING_REST_MS) schedulePanels(); else paintPanels(); }, Math.max(300, rest));
}

/* ---------- Meaning search: optional, downloaded on request, never bundled. Docs/60. ---------- */
// One model, pinned to a revision and a checksum. Its file is never part of the app.
const HF = "https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2/resolve/e8f8c211226b894fcb81acc59f3b34ba3efd5f42";
const MODEL = {
  id: "paraphrase-multilingual-MiniLM-L12-v2", tag: "paraphrase-multilingual-MiniLM-L12-v2@e8f8c21#qint8#1", name: "paraphrase-multilingual-MiniLM-L12-v2", licence: "Apache-2.0",
  dims: 384, prefix: "none", maxTokens: 128, relatedThreshold: .4, queryThreshold: .3,
  files: [
    { key: "model", url: HF + "/onnx/model_qint8_arm64.onnx", sha256: "783fea82d71a58179b830a4dbd2d58447e640609e98eedf9ffa12622d375a672", bytes: 118412398 },
    { key: "tokenizer", url: HF + "/tokenizer.json", sha256: "2c3387be76557bd40970cec13153b3bbf80407865484b209e655e5e4729076b8", bytes: 9081518 },
  ],
};
const MEANING_KEY = "casualnote-meaning", MEANING_DB = "casualnote-meaning";
const RUNTIME = "../pkg/embed/casual_note_embed.js";

function meaningDb() {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(MEANING_DB, 1);
    open.onupgradeneeded = () => { open.result.createObjectStore("models"); open.result.createObjectStore("vectors"); };
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}
const dbDo = async (store, mode, work) => {
  const db = await meaningDb();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(store, mode), req = work(tx.objectStore(store));
      tx.oncomplete = () => resolve(req?.result);
      tx.onerror = tx.onabort = () => reject(tx.error);
    });
  } finally { db.close(); }
};
const hex = buffer => [...new Uint8Array(buffer)].map(b => b.toString(16).padStart(2, "0")).join("");

/* The worker half. This same module, started as a worker, holds the model and the vectors, so
   nothing here can touch the typing path. Between two pieces of text it yields, so a query waits
   for at most one. */
function workerMain() {
  let runtime = null, model = null, store = null, spec = null, abort = null;
  const tick = () => new Promise(r => setTimeout(r));
  const handlers = {
    async install({ spec: wanted }, progress) {
      abort = new AbortController();
      const total = wanted.files.reduce((s, f) => s + f.bytes, 0);
      let done = 0;
      const got = [];
      for (const f of wanted.files) {
        const res = await fetch(f.url, { signal: abort.signal, credentials: "omit", referrerPolicy: "no-referrer" });
        if (!res.ok) throw new Error("CN-EMB-0006: HTTP " + res.status);
        const parts = [];
        const reader = res.body.getReader();
        for (;;) {
          const { done: end, value } = await reader.read();
          if (end) break;
          parts.push(value); done += value.length;
          progress({ phase: "downloading", done, total });
        }
        got.push([f, new Blob(parts)]);
      }
      progress({ phase: "verifying" });
      for (const [f, blob] of got) {
        const sum = hex(await crypto.subtle.digest("SHA-256", await blob.arrayBuffer()));
        if (sum !== f.sha256) throw new Error("CN-EMB-0006: the download did not match its checksum");
      }
      await dbDo("models", "readwrite", s => { for (const [f, blob] of got) s.put(blob, wanted.id + ":" + f.key); });
      return { ok: true };
    },
    cancel() { abort?.abort(); return { ok: true }; },
    async start({ spec: wanted }) {
      spec = wanted;
      const [m, t] = [await dbDo("models", "readonly", s => s.get(spec.id + ":model")), await dbDo("models", "readonly", s => s.get(spec.id + ":tokenizer"))];
      if (!m || !t) throw new Error("CN-EMB-0001: the model is not installed");
      runtime = await import(RUNTIME);
      await runtime.default();
      model = new runtime.Model(new Uint8Array(await m.arrayBuffer()), new Uint8Array(await t.arrayBuffer()), spec.dims, spec.prefix, spec.maxTokens);
      const saved = await dbDo("vectors", "readonly", s => s.get(spec.tag));
      if (saved) { try { store = runtime.Store.load(spec.tag, spec.dims, new Uint8Array(saved)); } catch (err) { console.warn("vectors rebuilt", err); } }
      store ??= new runtime.Store(spec.tag, spec.dims);
      return { count: store.len(), pages: JSON.parse(store.page_ids()) };
    },
    async plan({ page, chunks }) {
      const todo = JSON.parse(store.plan(page, chunks));
      for (const chunk of todo) {
        await tick();
        store.submit(page, JSON.stringify({ key: chunk.key, hash: chunk.hash, weight: chunk.weight }), model.embed(chunk.input, false));
      }
      return { embedded: todo.length, count: store.len() };
    },
    remove({ page }) { store.remove_page(page); return { count: store.len() }; },
    pages() { return { pages: JSON.parse(store.page_ids()) }; },
    query({ text, k }) { return { near: JSON.parse(store.nearest(model.embed(text, true), k)) }; },
    related({ page, k, threshold }) {
      return { known: JSON.parse(store.page_ids()).includes(page), near: JSON.parse(store.related(page, k, threshold)) };
    },
    async save() { const bytes = store.dump(); await dbDo("vectors", "readwrite", s => s.put(bytes.buffer, spec.tag)); return { count: store.len() }; },
    async uninstall({ spec: wanted }) {
      await dbDo("models", "readwrite", s => { for (const f of wanted.files) s.delete(wanted.id + ":" + f.key); });
      await dbDo("vectors", "readwrite", s => s.delete(wanted.tag));
      return { ok: true };
    },
  };
  self.onmessage = async ({ data: { id, type, payload } }) => {
    try {
      const value = await handlers[type](payload ?? {}, progress => self.postMessage({ id, progress }));
      self.postMessage({ id, value });
    } catch (err) { self.postMessage({ id, error: String(err?.message || err) }); }
  };
}

/* The seam. Anything with `call(type, payload, onProgress)` and `close()` will do: this worker, a
   desktop shell's process, or a test's stand-in. */
function workerEmbedder() {
  const worker = new Worker(import.meta.url, { type: "module" });
  let next = 0;
  const waiting = new Map();
  worker.onmessage = ({ data }) => {
    const w = waiting.get(data.id); if (!w) return;
    if (data.progress) { w.progress?.(data.progress); return; }
    waiting.delete(data.id);
    if (data.error) w.reject(new Error(data.error)); else w.resolve(data.value);
  };
  worker.onerror = e => { for (const w of waiting.values()) w.reject(new Error("CN-EMB-0006: " + (e.message || "the worker stopped"))); waiting.clear(); };
  return {
    call(type, payload, progress) { return new Promise((resolve, reject) => { const id = ++next; waiting.set(id, { resolve, reject, progress }); worker.postMessage({ id, type, payload }); }); },
    close() { worker.terminate(); },
  };
}
let embedder = null, pauseAfterEdit = 30000, pauseInBuild = 2000;
const meaning = { state: "off", done: 0, total: 0, count: 0, error: null, dirty: new Set(), running: false, lastEdit: 0, seenRev: -1, building: false, section: null };
const meaningOn = () => meaning.state === "ready" || meaning.state === "indexing";
const remembered = () => { try { return localStorage.getItem(MEANING_KEY); } catch { return null; } };
const remember = value => { try { if (value) localStorage.setItem(MEANING_KEY, value); else localStorage.removeItem(MEANING_KEY); } catch { /* optional */ } };
const size = bytes => new Intl.NumberFormat(api.lang, { style: "unit", unit: "megabyte", maximumFractionDigits: 0 }).format(bytes / 1e6);
function setMeaning(state, extra = {}) {
  Object.assign(meaning, { state, ...extra });
  document.documentElement.dataset.meaning = state;
  paintMeaning();
}
function stopMeaning(err) {
  console.warn("meaning search stopped", err);
  embedder?.close?.(); embedder = null;
  meaning.dirty.clear();
  setMeaning("failed", { error: String(err?.message || err).slice(0, 200) });
  schedulePanels();
}
function meaningHits(near) {
  const out = [];
  for (const n of near) {
    if (n.score < MODEL.queryThreshold * 1e6) continue;
    const item = n.key.split("#")[0];
    let text = "";
    try { text = api.pageData(n.page).blocks.find(b => b.id === item)?.runs.map(r => r.text).join("") ?? ""; } catch { continue; }
    out.push({ page: n.page, item, text: clip(text, 120), meaning: true, score: n.score });
  }
  return out;
}
const idle = () => new Promise(r => ("requestIdleCallback" in self ? requestIdleCallback(r, { timeout: 1000 }) : setTimeout(r, 50)));
const sleep = ms => new Promise(r => setTimeout(r, ms));
function markAll() { for (const p of api.pages()) meaning.dirty.add(p.id); meaning.seenRev = api.engine.revision(); }
/** Reads pages one at a time, only when nobody is typing. All of it is off the typing path except
    `embed_chunks`, one page's read, which waits for an idle moment. */
async function indexLoop() {
  if (meaning.running || !embedder) return;
  meaning.running = true;
  try {
    let sinceSave = 0;
    while (embedder && meaning.dirty.size) {
      const pause = meaning.building ? pauseInBuild : pauseAfterEdit;
      const quiet = performance.now() - meaning.lastEdit;
      if (quiet < pause) { await sleep(Math.min(pause - quiet, 1000)); continue; }
      await idle();
      if (!embedder) break; // It stopped while this waited.
      const id = meaning.dirty.values().next().value;
      meaning.dirty.delete(id);
      const meta = api.metaOf(id);
      const answer = meta && !meta.trashed ? await embedder.call("plan", { page: id, chunks: api.engine.embed_chunks(id) }) : await embedder.call("remove", { page: id });
      meaning.count = answer.count; sinceSave += answer.embedded ?? 1;
      meaning.done = meaning.total - meaning.dirty.size;
      if (meaning.building) paintMeaning();
      if (sinceSave >= 200) { await embedder.call("save"); sinceSave = 0; }
    }
    if (embedder) {
      if (sinceSave) await embedder.call("save");
      if (meaning.state !== "ready") { meaning.building = false; setMeaning("ready"); schedulePanels(); }
      else paintMeaning();
    }
  } catch (err) { stopMeaning(err); }
  meaning.running = false;
}
async function startMeaning() {
  try {
    setMeaning("loading");
    embedder ??= globalThis.casualNoteHost?.embedder ?? workerEmbedder();
    const { count, pages } = await embedder.call("start", { spec: MODEL });
    for (const id of pages) meaning.dirty.add(id);
    markAll();
    meaning.building = true;
    setMeaning("indexing", { count, done: 0, total: meaning.dirty.size, error: null });
    remember(MODEL.id);
    schedulePanels();
    indexLoop();
  } catch (err) { stopMeaning(err); }
}
async function installMeaning() {
  try {
    setMeaning("downloading", { done: 0, total: MODEL.files.reduce((s, f) => s + f.bytes, 0), error: null });
    if (!embedder && !globalThis.casualNoteHost?.embedder) {
      const here = await fetch(new URL(RUNTIME, import.meta.url), { method: "HEAD" }).catch(() => null);
      if (!here?.ok || !crypto?.subtle) { setMeaning("unavailable"); return; }
    }
    embedder ??= globalThis.casualNoteHost?.embedder ?? workerEmbedder();
    await embedder.call("install", { spec: MODEL }, p => {
      if (p.phase === "verifying") setMeaning("verifying");
      else setMeaning("downloading", { done: p.done, total: p.total });
    });
    await startMeaning();
  } catch (err) {
    embedder?.close?.(); embedder = null;
    if (err?.name === "AbortError" || /abort/i.test(String(err))) { setMeaning("off"); return; }
    console.warn("model download", err);
    setMeaning("downloadfailed", { error: String(err?.message || err).slice(0, 200) });
  }
}
async function removeMeaning() {
  const host = embedder ?? (globalThis.casualNoteHost?.embedder ?? workerEmbedder());
  try { await host.call("uninstall", { spec: MODEL }); } catch (err) { console.warn("model removal", err); }
  host.close?.(); embedder = null;
  remember(null); meaning.dirty.clear();
  setMeaning("off", { count: 0, error: null });
  schedulePanels();
}
function paintMeaning() {
  const sec = meaning.section; if (!sec) return;
  const { status, bar, actions, detail } = sec._parts;
  const m = meaning, btn = (text, id, onclick, cls = "btn") => api.el("button", { class: cls, id, text: api.t(text), onclick });
  sec.dataset.state = m.state;
  actions.replaceChildren(); detail.textContent = ""; bar.hidden = true;
  const fileNote = fill(api.t("Downloaded once from huggingface.co. {model}, licence {licence}."), { model: MODEL.name, licence: MODEL.licence });
  const total = MODEL.files.reduce((s, f) => s + f.bytes, 0);
  const download = () => api.el("button", { class: "btn primary", id: "meaningGet", text: fill(api.t("Download model ({size})"), { size: size(total) }), onclick: () => installMeaning() });
  status.classList.toggle("bad", ["failed", "downloadfailed", "unavailable"].includes(m.state));
  if (m.state === "off") { status.textContent = api.t("Off. Related pages and search use shared words."); actions.append(download()); detail.textContent = fileNote; }
  else if (m.state === "downloading") {
    status.textContent = fill(api.t("Downloading: {done} of {total}"), { done: size(m.done), total: size(m.total) });
    bar.hidden = false; bar.max = m.total || 1; bar.value = m.done;
    actions.append(btn("Cancel", "meaningCancel", () => embedder?.call("cancel")));
    detail.textContent = fileNote;
  }
  else if (m.state === "verifying") status.textContent = api.t("Checking the download");
  else if (m.state === "loading") status.textContent = api.t("Starting the model");
  else if (m.state === "indexing") {
    status.textContent = fill(api.t("On. Reading your notes: {done} of {total} pages. Shared words are used for pages not read yet."), { done: num(m.done), total: num(m.total) });
    bar.hidden = false; bar.max = m.total || 1; bar.value = m.done;
    actions.append(btn("Remove model", "meaningRemove", removeMeaning));
    detail.textContent = fileNote;
  }
  else if (m.state === "ready") {
    status.textContent = fill(api.t("On. Related pages and search use meaning. {count} pieces of text are indexed on this device."), { count: num(m.count) });
    actions.append(btn("Remove model", "meaningRemove", removeMeaning));
    detail.textContent = fileNote;
  }
  else if (m.state === "failed") {
    status.textContent = api.t("Meaning search stopped. Related pages and search use shared words.");
    actions.append(btn("Try again", "meaningRetry", startMeaning), btn("Remove model", "meaningRemove", removeMeaning));
    detail.textContent = m.error || "";
  }
  else if (m.state === "downloadfailed") {
    status.textContent = api.t("The download failed. Nothing was installed.");
    actions.append(api.el("button", { class: "btn primary", id: "meaningGet", text: api.t("Try again"), onclick: () => installMeaning() }));
    detail.textContent = m.error || "";
  }
  else if (m.state === "unavailable") status.textContent = api.t("This build has no runtime for meaning search. Related pages and search use shared words.");
}
function meaningSection(box) {
  const status = api.el("p", { class: "asst-status", id: "meaningStatus", role: "status" });
  const bar = api.el("progress", { id: "meaningBar", hidden: true });
  const actions = api.el("div", { class: "row2", id: "meaningActions" });
  const detail = api.el("small", { class: "asst-hint", id: "meaningDetail", dir: "auto" });
  const sec = api.el("div", { id: "meaningSet" }, api.el("h2", { text: api.t("Search by meaning") }),
    api.el("p", { text: api.t("Optional. Finds notes and related pages by what they mean, in any language, with a model that runs on this device. Your notes are not sent anywhere.") }), status, bar, actions, detail);
  sec._parts = { status, bar, actions, detail };
  meaning.section = sec;
  box.append(sec);
  paintMeaning();
}
function meaningChanged(op) {
  if (!meaningOn()) return;
  const page = op.page ?? (["create_page", "set_title", "set_trashed", "move_page"].includes(op.op) ? op.id : null);
  if (page) meaning.dirty.add(page);
  meaning.lastEdit = performance.now();
  meaning.seenRev = api.engine.revision();
  indexLoop();
}
/* The palette is app.js's, and it repaints on each input. Exact matches stay first and apart, so a
   person can always tell whether their words are in the note. This adds a labelled group after them. */
let paletteTimer = null, paletteAsk = 0;
function closePalette(input) { input.closest(".modalbg")?.remove(); }
function meaningRows(input, list, near) {
  const titles = new Map(api.live().map(p => [p.id, p.title]));
  const exact = new Set(api.$$(".mi", list).map(n => n.textContent));
  const rows = meaningHits(near).filter(h => titles.has(h.page));
  if (!rows.length) return;
  const none = api.$(".hdr.none", list), create = api.$(".mirow", list);
  none?.remove();
  const anchor = create || null;
  list.insertBefore(api.el("div", { class: "hdr", id: "paletteMeaning", role: "presentation", text: api.t("Matches by meaning") }), anchor);
  rows.forEach((h, i) => {
    const row = api.el("button", { class: "mi", role: "option", id: "palm" + i, tabindex: "-1", "data-meaning": "", "aria-selected": "false", onmousedown: e => e.preventDefault(), onclick: () => {
      closePalette(input); api.openPage(h.page);
      if (h.item) api.$(`.b[data-id="${CSS.escape(h.item)}"]`)?.scrollIntoView({ block: "center" });
    } }, api.el("span", { class: "gl" }, api.icon("file-text")), api.el("span", { class: "palb" }, api.el("span", { class: "nm palt", dir: "auto", text: titles.get(h.page) || api.t("Untitled") }), api.el("small", { dir: "auto", text: h.text })));
    if (exact.has(row.textContent)) return;
    list.insertBefore(row, anchor);
  });
  // With no exact match the app's selection is on its "create" row. The first match is the better default.
  if (create?.classList.contains("on")) {
    create.classList.remove("on"); create.setAttribute("aria-selected", "false");
    const first = api.$("#palm0", list);
    if (first) { first.classList.add("on"); first.setAttribute("aria-selected", "true"); input.setAttribute("aria-activedescendant", first.id); }
  }
}
function meaningInPalette(input) {
  clearTimeout(paletteTimer);
  const ask = ++paletteAsk, q = input.value.trim(), list = api.$("#paletteList");
  if (!list || !q) return;
  if (meaning.state === "off") {
    if (api.$(".hdr.none", list)) list.append(api.el("div", { class: "hdr", id: "paletteHint", role: "presentation", text: api.t("No exact matches. Search by meaning is off; turn it on in Settings.") }));
    return;
  }
  if (!meaningOn() || q.length < 2) return;
  paletteTimer = setTimeout(async () => {
    let near;
    try { near = (await embedder.call("query", { text: q, k: 10 })).near; } catch (err) { stopMeaning(err); return; }
    if (ask === paletteAsk && input.isConnected && input.value.trim() === q) meaningRows(input, list, near);
  }, 160);
}
/** For tests and a desktop shell: put an embedder behind the seam. */
export const meaningSeam = {
  async use(host, { afterEdit = 30000, inBuild = 2000 } = {}) { embedder = host; pauseAfterEdit = afterEdit; pauseInBuild = inBuild; await startMeaning(); },
  state: () => ({ ...meaning, dirty: meaning.dirty.size, section: undefined }),
  clear() { embedder = null; meaning.dirty.clear(); setMeaning("off", { count: 0, error: null }); pauseAfterEdit = 30000; pauseInBuild = 2000; remember(null); },
  settled: async () => { while (meaning.running || meaning.dirty.size) await sleep(10); },
  spec: MODEL,
};

export function install(a) {
  api = a;
  const ext = api.ext;
  ext.views.graph = { label: "Knowledge graph", render: content => renderGraph(content) };
  ext.nav.push({ id: "graphBtn", icon: "graph", label: "Knowledge graph", run: () => api.openView("graph") });
  ext.panels.push(schedulePanels);
  ext.settings.push(meaningSection);
  ext.onChange.push(meaningChanged);
  ext.onChange.push(op => { if (op.op === "splice_text" && (op.insert?.length ?? 0) <= 8 && op.page === api.state.cur && api.$(".blocks .t:focus")) lastTyped = performance.now(); });
  // Nothing about meaning search runs, loads or connects unless it was installed here before.
  if (remembered() === MODEL.id) setTimeout(startMeaning, 1500);
  document.addEventListener("input", e => { if (e.target.id === "palette") meaningInPalette(e.target); });
  document.addEventListener("keydown", e => {
    if (e.target.id !== "palette" || e.key !== "Enter" || e.isComposing) return;
    const row = api.$("#paletteList .mi.on[data-meaning]");
    if (row) { e.preventDefault(); e.stopPropagation(); row.click(); }
  }, true);
}

if (IN_WORKER) workerMain();
