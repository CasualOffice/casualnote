//! Note-like text in the launch languages, written for this tool (Apache-2.0 with the repository),
//! so it can be kept as a fixture. Not lorem ipsum, and not a sample of real people's notes: real
//! notes remain the stronger test, and docs/24 asks for them once sources with licences exist.

use crate::rng::Rng;

/// How a language reaches the editor, which decides the shape of its edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// One keystroke, one splice.
    Keys,
    /// A composition: several characters arrive as one commit.
    Compose,
}

#[derive(Debug)]
pub struct Language {
    pub code: &'static str,
    pub input: Input,
    pub sentences: &'static [&'static str],
    /// Syllables a rare word (a name, a term) is made from.
    pub syllables: &'static [&'static str],
}

pub const LANGUAGES: &[Language] = &[
    Language {
        code: "en",
        input: Input::Keys,
        sentences: &[
            "Call the plumber about the leak under the kitchen sink before Friday.",
            "The budget review moved to next Tuesday because Maria is travelling.",
            "Ideas for the garden: raised beds along the south fence and more shade for the tomatoes.",
            "Remember to renew the passport at least three months before the trip.",
            "We agreed to ship the smaller version first and collect feedback from the pilot group.",
            "The train to the coast leaves at seven, so pack the night before.",
            "My notes from the meeting are rough, but the decision about the supplier is clear.",
            "Blood pressure was normal at the check-up; the doctor wants another test in spring.",
            "Book recommendation from Sam: a history of salt, surprisingly readable.",
            "Ask the landlord whether the heating contract covers the annual service.",
            "The recipe needs less sugar next time and at least ten more minutes in the oven.",
            "Children's school fees are due at the end of the month, pay from the joint account.",
            "Draft the letter to the insurance company with photos of the damaged roof.",
            "A slow morning, a long walk by the river, and finally some time to think.",
        ],
        syllables: &[
            "bar", "ten", "lo", "mar", "kel", "ston", "wick", "ash", "dor", "fen", "gra", "ham",
        ],
    },
    Language {
        code: "zh-Hans",
        input: Input::Compose,
        sentences: &[
            "周五之前给水管工打电话，厨房水槽下面还在漏水。",
            "预算评审改到下周二，因为玛丽亚要出差。",
            "花园的想法：沿南边的围栏做几个高床，给番茄多留些阴凉。",
            "记得在旅行前至少三个月更新护照。",
            "我们决定先发布小版本，再收集试点用户的反馈。",
            "去海边的火车七点开，前一天晚上就把行李收拾好。",
            "会议记录比较粗略，但关于供应商的决定很明确。",
            "体检时血压正常，医生希望春天再做一次检查。",
            "朋友推荐了一本关于盐的历史书，意外地好读。",
            "问问房东暖气合同是否包括每年的保养。",
            "下次做这道菜少放点糖，在烤箱里多烤十分钟。",
            "孩子的学费月底到期，从共同账户里支付。",
            "给保险公司写信，附上屋顶受损的照片。",
            "早上慢慢地过，沿着河边走了很久，终于有时间想一想。",
        ],
        syllables: &[
            "云", "海", "桥", "林", "安", "明", "川", "泽", "宁", "华", "庆", "远",
        ],
    },
    Language {
        code: "zh-Hant",
        input: Input::Compose,
        sentences: &[
            "週五之前打電話給水電工，廚房水槽下面還在漏水。",
            "預算審查改到下週二，因為瑪麗亞要出差。",
            "花園的想法：沿著南邊的圍籬做幾個高床，幫番茄多留些陰涼。",
            "記得在旅行前至少三個月更新護照。",
            "我們決定先發布小版本，再蒐集試用者的回饋。",
            "去海邊的火車七點開，前一天晚上就把行李整理好。",
            "會議筆記比較粗略，但關於供應商的決定很明確。",
            "體檢時血壓正常，醫生希望春天再做一次檢查。",
            "朋友推薦了一本關於鹽的歷史書，意外地好讀。",
            "問問房東暖氣合約是否包含每年的保養。",
        ],
        syllables: &[
            "雲", "灣", "橋", "嶺", "寧", "園", "澤", "鳳", "龍", "廣", "興", "遠",
        ],
    },
    Language {
        code: "ja",
        input: Input::Compose,
        sentences: &[
            "金曜日までに水道屋さんに電話する。台所の流しの下がまだ漏れている。",
            "マリアが出張のため、予算の見直しは来週の火曜日に変更になった。",
            "庭のアイデア：南側の柵に沿って花壇を作り、トマトに日陰を増やす。",
            "旅行の少なくとも三か月前にパスポートを更新すること。",
            "まず小さい版を出して、試験利用の方々から意見を集めることにした。",
            "海へ向かう電車は七時発なので、前の晩に荷造りをしておく。",
            "会議のメモは雑だが、仕入れ先についての決定ははっきりしている。",
            "健康診断の血圧は正常で、医師は春にもう一度検査をしたいとのこと。",
            "友人に塩の歴史の本を勧められた。意外と読みやすい。",
            "暖房の契約に年一回の点検が含まれるか大家さんに聞く。",
        ],
        syllables: &[
            "さく", "やま", "かわ", "とう", "みな", "はる", "たに", "もり", "ひか", "えん", "こう",
            "しま",
        ],
    },
    Language {
        code: "ko",
        input: Input::Compose,
        sentences: &[
            "금요일 전에 배관공에게 전화하기. 부엌 싱크대 밑이 아직 샌다.",
            "마리아가 출장을 가서 예산 검토는 다음 주 화요일로 옮겼다.",
            "정원 아이디어: 남쪽 울타리를 따라 화단을 만들고 토마토에 그늘을 더 준다.",
            "여행 최소 세 달 전에 여권을 갱신할 것.",
            "작은 버전을 먼저 내고 시범 사용자에게 의견을 받기로 했다.",
            "바다로 가는 기차는 일곱 시에 출발하니 전날 밤에 짐을 싼다.",
            "회의 메모는 거칠지만 공급업체에 대한 결정은 분명하다.",
            "건강검진에서 혈압은 정상이었고 의사는 봄에 한 번 더 검사하자고 했다.",
            "친구가 소금의 역사에 관한 책을 추천했는데 의외로 읽기 쉽다.",
            "난방 계약에 연간 점검이 포함되는지 집주인에게 묻기.",
        ],
        syllables: &[
            "하", "늘", "바", "람", "서", "윤", "민", "준", "강", "솔", "태", "경",
        ],
    },
    Language {
        code: "es",
        input: Input::Keys,
        sentences: &[
            "Llamar al fontanero por la fuga debajo del fregadero antes del viernes.",
            "La revisión del presupuesto pasó al martes porque María está de viaje.",
            "Ideas para el jardín: bancales junto a la valla sur y más sombra para los tomates.",
            "Recordar renovar el pasaporte al menos tres meses antes del viaje.",
            "Acordamos lanzar primero la versión pequeña y recoger opiniones del grupo piloto.",
            "El tren a la costa sale a las siete, así que hay que preparar la maleta la noche anterior.",
            "Mis notas de la reunión son escuetas, pero la decisión sobre el proveedor está clara.",
            "La tensión estaba bien en la revisión; la médica quiere otra prueba en primavera.",
            "Preguntar al casero si el contrato de la calefacción cubre el mantenimiento anual.",
            "La receta necesita menos azúcar y diez minutos más en el horno.",
        ],
        syllables: &[
            "mon", "ta", "ña", "ri", "ber", "nal", "ve", "ga", "cór", "do", "lu", "san",
        ],
    },
    Language {
        code: "fr",
        input: Input::Keys,
        sentences: &[
            "Appeler le plombier pour la fuite sous l'évier avant vendredi.",
            "La revue du budget est reportée à mardi car Maria est en déplacement.",
            "Idées pour le jardin : des bacs le long de la clôture sud et plus d'ombre pour les tomates.",
            "Penser à renouveler le passeport au moins trois mois avant le voyage.",
            "Nous avons décidé de livrer d'abord la petite version et de recueillir les avis du groupe pilote.",
            "Le train pour la côte part à sept heures, il faut préparer la valise la veille.",
            "Mes notes de réunion sont brouillonnes, mais la décision sur le fournisseur est claire.",
            "La tension était normale au bilan ; le médecin veut un autre examen au printemps.",
            "Demander au propriétaire si le contrat de chauffage couvre l'entretien annuel.",
            "La recette demande moins de sucre et dix minutes de plus au four.",
        ],
        syllables: &[
            "cha", "teau", "ver", "mont", "lys", "bel", "rou", "sel", "dé", "pin", "ga", "ré",
        ],
    },
    Language {
        code: "de",
        input: Input::Keys,
        sentences: &[
            "Vor Freitag den Klempner wegen des Lecks unter der Küchenspüle anrufen.",
            "Die Budgetprüfung wurde auf Dienstag verschoben, weil Maria auf Reisen ist.",
            "Ideen für den Garten: Hochbeete am Südzaun und mehr Schatten für die Tomaten.",
            "Den Reisepass mindestens drei Monate vor der Reise verlängern.",
            "Wir haben beschlossen, zuerst die kleine Version auszuliefern und Rückmeldungen der Pilotgruppe zu sammeln.",
            "Der Zug an die Küste fährt um sieben, also am Vorabend packen.",
            "Meine Besprechungsnotizen sind grob, aber die Entscheidung zum Lieferanten ist klar.",
            "Der Blutdruck war beim Check normal; die Ärztin möchte im Frühjahr einen weiteren Test.",
            "Den Vermieter fragen, ob der Heizungsvertrag die jährliche Wartung abdeckt.",
            "Das Rezept braucht weniger Zucker und zehn Minuten länger im Ofen.",
        ],
        syllables: &[
            "berg", "wald", "stein", "feld", "brück", "hau", "sen", "mül", "ler", "dorf", "kra",
            "nich",
        ],
    },
    Language {
        code: "pt",
        input: Input::Keys,
        sentences: &[
            "Ligar para o encanador por causa do vazamento debaixo da pia antes de sexta.",
            "A revisão do orçamento passou para terça porque a Maria está viajando.",
            "Ideias para o jardim: canteiros ao longo da cerca sul e mais sombra para os tomates.",
            "Lembrar de renovar o passaporte pelo menos três meses antes da viagem.",
            "Combinamos lançar primeiro a versão pequena e colher opiniões do grupo piloto.",
            "O trem para o litoral sai às sete, então é melhor arrumar a mala na noite anterior.",
            "Minhas anotações da reunião estão soltas, mas a decisão sobre o fornecedor está clara.",
            "A pressão estava normal no exame; a médica quer outro teste na primavera.",
            "Perguntar ao proprietário se o contrato do aquecimento cobre a manutenção anual.",
            "A receita precisa de menos açúcar e mais dez minutos no forno.",
        ],
        syllables: &[
            "ser", "ra", "bra", "vo", "lin", "ção", "cor", "ta", "pei", "xo", "ri", "mar",
        ],
    },
    Language {
        code: "ar",
        input: Input::Keys,
        sentences: &[
            "الاتصال بالسباك قبل يوم الجمعة بسبب التسرب تحت حوض المطبخ.",
            "تأجلت مراجعة الميزانية إلى الثلاثاء القادم لأن ماريا مسافرة.",
            "أفكار للحديقة: أحواض مرتفعة بجانب السور الجنوبي ومزيد من الظل للطماطم.",
            "تذكر تجديد جواز السفر قبل الرحلة بثلاثة أشهر على الأقل.",
            "اتفقنا على إطلاق النسخة الصغيرة أولا وجمع الآراء من المجموعة التجريبية.",
            "يغادر القطار إلى الساحل في السابعة، لذلك يجب تجهيز الحقيبة في الليلة السابقة.",
            "ملاحظاتي من الاجتماع مختصرة، لكن القرار بشأن المورد واضح.",
            "كان ضغط الدم طبيعيا في الفحص، وتريد الطبيبة فحصا آخر في الربيع.",
            "اسأل المالك هل يشمل عقد التدفئة الصيانة السنوية.",
            "تحتاج الوصفة إلى سكر أقل وعشر دقائق إضافية في الفرن.",
        ],
        syllables: &[
            "نور", "سلا", "مين", "كري", "هاد", "زين", "فار", "رو", "ليل", "جبل", "بحر", "واد",
        ],
    },
    Language {
        code: "hi",
        input: Input::Keys,
        sentences: &[
            "शुक्रवार से पहले प्लंबर को फ़ोन करना है, रसोई के सिंक के नीचे अभी भी पानी टपक रहा है।",
            "बजट की समीक्षा अगले मंगलवार को होगी क्योंकि मारिया यात्रा पर हैं।",
            "बगीचे के लिए विचार: दक्षिणी बाड़ के साथ ऊँची क्यारियाँ और टमाटरों के लिए ज़्यादा छाया।",
            "यात्रा से कम से कम तीन महीने पहले पासपोर्ट का नवीनीकरण कराना याद रखें।",
            "हमने तय किया कि पहले छोटा संस्करण जारी करेंगे और परीक्षण समूह से राय लेंगे।",
            "तट की ट्रेन सात बजे चलती है, इसलिए एक रात पहले सामान बाँध लें।",
            "बैठक के मेरे नोट्स अधूरे हैं, पर आपूर्तिकर्ता के बारे में निर्णय स्पष्ट है।",
            "जाँच में रक्तचाप सामान्य था; डॉक्टर वसंत में एक और जाँच चाहती हैं।",
            "मकान मालिक से पूछना है कि हीटिंग के अनुबंध में वार्षिक सर्विस शामिल है या नहीं।",
            "अगली बार इस व्यंजन में चीनी कम और ओवन में दस मिनट ज़्यादा रखें।",
        ],
        syllables: &[
            "गंगा",
            "मोह",
            "राज",
            "सिंह",
            "नील",
            "कम",
            "दीप",
            "वन",
            "पुर",
            "शिव",
            "अर",
            "तारा",
        ],
    },
];

/// A language for a page: most pages are one language, skewed the way one person's notes are.
pub fn language_for(page_hash: u64) -> &'static Language {
    LANGUAGES
        .get((page_hash % LANGUAGES.len() as u64) as usize)
        .unwrap_or(&LANGUAGES[0])
}

/// A rare word: a name or a term. The long tail of a vocabulary, which a fixed list lacks.
pub fn rare_word(lang: &Language, rng: &mut Rng) -> String {
    let n = rng.range(2, 4);
    (0..n)
        .map(|_| lang.syllables[rng.below(lang.syllables.len())])
        .collect()
}

/// A note line: a sentence, sometimes with a rare word or a second language mixed in.
pub fn line(lang: &Language, rng: &mut Rng) -> String {
    let mut s = lang.sentences[rng.below(lang.sentences.len())].to_string();
    if rng.chance(25.0) {
        s.push(' ');
        s.push_str(&rare_word(lang, rng));
    }
    if rng.chance(6.0) {
        let other = &LANGUAGES[rng.below(LANGUAGES.len())];
        s.push(' ');
        s.push_str(other.sentences[rng.below(other.sentences.len())]);
    }
    s
}

/// Cuts text into what arrives as one edit: a keystroke, or one composition commit.
pub fn edits(text: &str, input: Input, rng: &mut Rng) -> Vec<String> {
    match input {
        Input::Keys => text.chars().map(|c| c.to_string()).collect(),
        Input::Compose => {
            let chars: Vec<char> = text.chars().collect();
            let mut out = Vec::new();
            let mut at = 0;
            while at < chars.len() {
                let n = rng.range(1, 4).min(chars.len() - at);
                out.push(chars[at..at + n].iter().collect());
                at += n;
            }
            out
        }
    }
}
