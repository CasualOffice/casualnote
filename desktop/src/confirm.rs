//! The native confirmation's rules (docs/44). Rust draws it, Decline is the default button, and a
//! script cannot keep asking until a tired person agrees. The words come from here, in the
//! interface language, and the only values in them are ones Rust parsed.
use crate::error::CnError;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub const COOL_DOWN: Duration = Duration::from_secs(60);
pub const MAX_DECLINES: u32 = 3;
pub const DECLINED: &str = "CN-DSK-0017";

#[derive(Debug, Default)]
pub struct Gate {
    open: bool,
    declines: u32,
    last: HashMap<String, Instant>,
}

impl Gate {
    /// May this request show a dialog now? One at a time, 60 s after a decline, and none after three.
    pub fn begin(&mut self, key: &str, now: Instant) -> Result<(), CnError> {
        if self.declines >= MAX_DECLINES {
            return Err(CnError::new(
                DECLINED,
                "Too many requests were declined. Restart the app to be asked again.",
            ));
        }
        if self.open {
            return Err(CnError::new(
                DECLINED,
                "Another confirmation is already open.",
            ));
        }
        if self
            .last
            .get(key)
            .is_some_and(|at| now.duration_since(*at) < COOL_DOWN)
        {
            return Err(CnError::new(
                DECLINED,
                "You declined this a moment ago. Wait a minute to be asked again.",
            ));
        }
        self.open = true;
        Ok(())
    }

    pub fn finish(&mut self, key: &str, allowed: bool, now: Instant) {
        self.open = false;
        if !allowed {
            self.declines += 1;
            self.last.insert(key.to_owned(), now);
        }
    }
}

/// The buttons, and what each may mean. On macOS the first button is what Return does, so Decline
/// goes first. Linux and Windows are unbuilt: rfd reports plain Ok and Cancel on Linux, so Allow
/// goes first there, and only Ok means Allow, which keeps "no answer" a decline on every system.
pub fn buttons(words: &Words) -> rfd::MessageButtons {
    if cfg!(target_os = "macos") {
        rfd::MessageButtons::OkCancelCustom(words.decline.to_owned(), words.allow.to_owned())
    } else {
        rfd::MessageButtons::OkCancelCustom(words.allow.to_owned(), words.decline.to_owned())
    }
}

/// Only the exact answer allows. Any other result is a decline, including whatever the toolkit
/// returns for a dialog that was closed, interrupted or never drawn.
pub fn is_allowed(answer: &rfd::MessageDialogResult, words: &Words) -> bool {
    match answer {
        rfd::MessageDialogResult::Custom(label) => label == words.allow,
        rfd::MessageDialogResult::Ok => !cfg!(target_os = "macos"),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bind,
    Run,
    NotesMove,
    ChooseProgram,
    AddModel,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Words {
    pub title: &'static str,
    pub body: &'static str,
    pub allow: &'static str,
    pub decline: &'static str,
}

// Drafts for languages other than English, not yet native-reviewed (I18N-05).
pub fn words(lang: &str, kind: Kind) -> Words {
    use Kind::*;
    let (title, body, allow, decline) = match (lang, kind) {
        ("zh-Hans", Bind) => (
            "更改助手的发送地址？",
            "你发送给助手的笔记和提示将发送到：\n\n{value}\n\n如果这不是你刚才的操作，请拒绝。",
            "允许",
            "拒绝",
        ),
        ("zh-Hans", Run) => (
            "运行本地模型？",
            "{value}\n\n此程序没有沙盒。它以你的权限运行，可以读取你的文件并使用网络。恶意的程序或模型文件也可以这样做。",
            "运行",
            "拒绝",
        ),
        ("zh-Hans", NotesMove) => (
            "把笔记保存到这里？",
            "你的笔记今后将写入：\n\n{value}",
            "允许",
            "拒绝",
        ),
        ("zh-Hans", ChooseProgram) => (
            "使用此程序？",
            "{value}\n\n此程序没有沙盒。启动时它以你的权限运行。只选择你信任的程序。",
            "使用",
            "拒绝",
        ),
        ("zh-Hans", AddModel) => ("添加此模型文件？", "{value}", "添加", "拒绝"),
        ("ja", Bind) => (
            "アシスタントの送信先を変更しますか？",
            "アシスタントに送るノートとプロンプトの送信先:\n\n{value}\n\n心当たりがない場合は拒否してください。",
            "許可",
            "拒否",
        ),
        ("ja", Run) => (
            "ローカルモデルを実行しますか？",
            "{value}\n\nこのプログラムはサンドボックス化されていません。あなたの権限で動作し、ファイルの読み取りとネットワークの利用ができます。悪意のあるプログラムやモデルファイルも同様です。",
            "実行",
            "拒否",
        ),
        ("ja", NotesMove) => (
            "ノートをここに保存しますか？",
            "今後ノートの書き込み先:\n\n{value}",
            "許可",
            "拒否",
        ),
        ("ja", ChooseProgram) => (
            "このプログラムを使いますか？",
            "{value}\n\nサンドボックス化されていません。起動するとあなたの権限で動作します。信頼できるプログラムだけを選んでください。",
            "使う",
            "拒否",
        ),
        ("ja", AddModel) => (
            "このモデルファイルを追加しますか？",
            "{value}",
            "追加",
            "拒否",
        ),
        ("es", Bind) => (
            "¿Cambiar el destino del asistente?",
            "Las notas y mensajes que envíes al asistente irán a:\n\n{value}\n\nSi no acabas de hacer esto, rechaza.",
            "Permitir",
            "Rechazar",
        ),
        ("es", Run) => (
            "¿Ejecutar el modelo local?",
            "{value}\n\nEste programa no está aislado. Se ejecuta con tus permisos: puede leer tus archivos y usar la red. Un programa o modelo malicioso también podría.",
            "Ejecutar",
            "Rechazar",
        ),
        ("es", NotesMove) => (
            "¿Guardar tus notas aquí?",
            "Tus notas se escribirán a partir de ahora en:\n\n{value}",
            "Permitir",
            "Rechazar",
        ),
        ("es", ChooseProgram) => (
            "¿Usar este programa?",
            "{value}\n\nNo está aislado. Al iniciarse se ejecuta con tus permisos. Elige solo un programa de confianza.",
            "Usar",
            "Rechazar",
        ),
        ("es", AddModel) => (
            "¿Añadir este archivo de modelo?",
            "{value}",
            "Añadir",
            "Rechazar",
        ),
        ("ar", Bind) => (
            "هل تغيّر وجهة المساعد؟",
            "ستُرسل الملاحظات والطلبات التي ترسلها إلى المساعد إلى:\n\n{value}\n\nإذا لم تفعل هذا للتو فارفض.",
            "سماح",
            "رفض",
        ),
        ("ar", Run) => (
            "هل تشغّل النموذج المحلي؟",
            "{value}\n\nهذا البرنامج غير معزول. يعمل بصلاحياتك: يمكنه قراءة ملفاتك واستخدام الشبكة. وكذلك يمكن لبرنامج أو ملف نموذج خبيث.",
            "تشغيل",
            "رفض",
        ),
        ("ar", NotesMove) => (
            "هل تحفظ ملاحظاتك هنا؟",
            "ستُكتب ملاحظاتك من الآن في:\n\n{value}",
            "سماح",
            "رفض",
        ),
        ("ar", ChooseProgram) => (
            "هل تستخدم هذا البرنامج؟",
            "{value}\n\nهذا البرنامج غير معزول، ويعمل بصلاحياتك عند بدئه. اختر برنامجًا تثق به فقط.",
            "استخدام",
            "رفض",
        ),
        ("ar", AddModel) => ("هل تضيف ملف النموذج هذا؟", "{value}", "إضافة", "رفض"),
        ("hi", Bind) => (
            "सहायक का गंतव्य बदलें?",
            "सहायक को भेजे जाने वाले नोट्स और प्रॉम्प्ट यहाँ जाएँगे:\n\n{value}\n\nअगर आपने अभी यह नहीं किया, तो अस्वीकार करें।",
            "अनुमति दें",
            "अस्वीकार",
        ),
        ("hi", Run) => (
            "स्थानीय मॉडल चलाएँ?",
            "{value}\n\nयह प्रोग्राम सैंडबॉक्स में नहीं है। यह आपके अधिकारों से चलता है: आपकी फ़ाइलें पढ़ सकता है और नेटवर्क उपयोग कर सकता है। दुर्भावनापूर्ण प्रोग्राम या मॉडल फ़ाइल भी ऐसा कर सकती है।",
            "चलाएँ",
            "अस्वीकार",
        ),
        ("hi", NotesMove) => (
            "नोट्स यहाँ सहेजें?",
            "अब से आपके नोट्स यहाँ लिखे जाएँगे:\n\n{value}",
            "अनुमति दें",
            "अस्वीकार",
        ),
        ("hi", ChooseProgram) => (
            "यह प्रोग्राम उपयोग करें?",
            "{value}\n\nयह सैंडबॉक्स में नहीं है। शुरू होने पर यह आपके अधिकारों से चलता है। केवल भरोसेमंद प्रोग्राम चुनें।",
            "उपयोग करें",
            "अस्वीकार",
        ),
        ("hi", AddModel) => ("यह मॉडल फ़ाइल जोड़ें?", "{value}", "जोड़ें", "अस्वीकार"),
        (_, Bind) => (
            "Change where the assistant sends?",
            "Notes and prompts you send to the assistant will go to:\n\n{value}\n\nIf you did not just do this, decline.",
            "Allow",
            "Decline",
        ),
        (_, Run) => (
            "Run the local model?",
            "{value}\n\nThis program is not sandboxed. It runs with your rights: it can read your files and use the network. A malicious program or model file could do the same.",
            "Run",
            "Decline",
        ),
        (_, NotesMove) => (
            "Save your notes here?",
            "Your notes will be written to this file from now on:\n\n{value}",
            "Allow",
            "Decline",
        ),
        (_, ChooseProgram) => (
            "Use this program?",
            "{value}\n\nIt is not sandboxed. When started, it runs with your rights. Choose only a program you trust.",
            "Use it",
            "Decline",
        ),
        (_, AddModel) => ("Add this model file?", "{value}", "Add", "Decline"),
    };
    Words {
        title,
        body,
        allow,
        decline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decline_refuses_the_same_request_for_a_minute_without_a_dialog() {
        let (mut gate, start) = (Gate::default(), Instant::now());
        gate.begin("https://evil.example", start).unwrap();
        gate.finish("https://evil.example", false, start);
        let soon = start + Duration::from_secs(59);
        assert_eq!(
            gate.begin("https://evil.example", soon).unwrap_err().code,
            DECLINED
        );
        gate.begin("https://evil.example", start + COOL_DOWN)
            .unwrap();
        gate.finish("https://evil.example", true, start + COOL_DOWN);
    }

    #[test]
    fn three_declines_stop_all_asking_until_restart() {
        let (mut gate, start) = (Gate::default(), Instant::now());
        for i in 0..MAX_DECLINES {
            let key = format!("k{i}");
            gate.begin(&key, start).unwrap();
            gate.finish(&key, false, start);
        }
        let much_later = start + Duration::from_secs(86_400);
        assert!(gate.begin("a new one", much_later).is_err());
    }

    #[test]
    fn only_one_dialog_at_a_time() {
        let (mut gate, start) = (Gate::default(), Instant::now());
        gate.begin("a", start).unwrap();
        assert!(gate.begin("b", start).is_err());
        gate.finish("a", true, start);
        gate.begin("b", start).unwrap();
    }

    #[test]
    fn no_answer_is_a_decline() {
        use rfd::MessageDialogResult as R;
        let w = words("en", Kind::Bind);
        assert!(is_allowed(&R::Custom("Allow".into()), &w));
        for other in [
            R::Cancel,
            R::No,
            R::Yes,
            R::Custom("Decline".into()),
            R::Custom(String::new()),
        ] {
            assert!(!is_allowed(&other, &w), "{other:?}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decline_is_the_first_button_on_macos() {
        let w = words("en", Kind::Run);
        let rfd::MessageButtons::OkCancelCustom(first, _) = buttons(&w) else {
            panic!()
        };
        assert_eq!(first, "Decline");
    }

    #[test]
    fn running_something_always_says_it_is_not_sandboxed() {
        assert!(words("en", Kind::Run).body.contains("not sandboxed"));
        assert!(
            words("en", Kind::ChooseProgram)
                .body
                .contains("not sandboxed")
        );
        for lang in ["zh-Hans", "ja", "es", "ar", "hi", "fr"] {
            assert!(words(lang, Kind::Bind).body.contains("{value}"), "{lang}");
            assert!(words(lang, Kind::Run).body.contains("{value}"), "{lang}");
        }
    }
}
