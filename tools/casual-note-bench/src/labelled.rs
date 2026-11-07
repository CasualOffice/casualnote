//! SPK-06's labelled set, fixed before any model ran (docs/60). Twelve topics that the simulator's
//! corpus does not cover, each with a note and a differently worded question in ten languages.
//! The question avoids the note's main words where the language allows, which is the case word
//! overlap cannot answer and the reason meaning search exists.

/// One topic: `(language, note, question)`.
pub type Topic = &'static [(&'static str, &'static str, &'static str)];

pub const TOPICS: &[Topic] = &[
    &[
        (
            "en",
            "The vet gave Bruno his rabies shot and booked a booster for next year.",
            "When does my dog need his next vaccination?",
        ),
        (
            "zh",
            "兽医给布鲁诺打了狂犬疫苗，并预约了明年的加强针。",
            "我家的狗下一次什么时候该接种？",
        ),
        (
            "ja",
            "獣医がブルーノに狂犬病の注射をして、来年の追加接種を予約した。",
            "うちの犬の次のワクチンはいつ？",
        ),
        (
            "ko",
            "수의사가 브루노에게 광견병 주사를 놓고 내년 추가 접종을 예약했다.",
            "우리 강아지 다음 예방접종은 언제지?",
        ),
        (
            "es",
            "La veterinaria le puso a Bruno la inyección de la rabia y reservó un refuerzo para el año que viene.",
            "¿Cuándo le toca la próxima vacuna a mi perro?",
        ),
        (
            "fr",
            "La vétérinaire a fait le vaccin antirabique de Bruno et prévu un rappel l'an prochain.",
            "Quand mon chien doit-il repasser pour une piqûre ?",
        ),
        (
            "de",
            "Die Tierärztin hat Bruno gegen Tollwut gespritzt und die Auffrischung für nächstes Jahr eingetragen.",
            "Wann muss mein Hund wieder geimpft werden?",
        ),
        (
            "pt",
            "A veterinária deu ao Bruno a injeção antirrábica e marcou o reforço para o ano que vem.",
            "Quando meu cachorro precisa ser vacinado de novo?",
        ),
        (
            "ar",
            "أعطى الطبيب البيطري برونو حقنة داء الكلب وحجز جرعة منشطة للعام القادم.",
            "متى يحتاج كلبي إلى التطعيم التالي؟",
        ),
        (
            "hi",
            "पशु चिकित्सक ने ब्रूनो को रेबीज़ का इंजेक्शन दिया और अगले साल की बूस्टर खुराक तय की।",
            "मेरे कुत्ते को अगला टीका कब लगेगा?",
        ),
    ],
    &[
        (
            "en",
            "Picked up a nail on the motorway; the front left tyre went flat and the garage replaced it.",
            "Which wheel on the car had the puncture?",
        ),
        (
            "zh",
            "在高速公路上扎了钉子，左前轮胎没气了，修车厂换了一条新的。",
            "汽车哪个轱辘被扎破了？",
        ),
        (
            "ja",
            "高速道路で釘を踏んで左前のタイヤがパンクし、整備工場で交換してもらった。",
            "車のどの車輪に穴が空いたんだっけ？",
        ),
        (
            "ko",
            "고속도로에서 못을 밟아 왼쪽 앞 타이어가 터졌고 정비소에서 교체했다.",
            "차 어느 바퀴에 구멍이 났었지?",
        ),
        (
            "es",
            "Pisé un clavo en la autopista; se pinchó la rueda delantera izquierda y el taller la cambió.",
            "¿Qué neumático del coche se quedó sin aire?",
        ),
        (
            "fr",
            "Un clou sur l'autoroute : le pneu avant gauche s'est dégonflé et le garagiste l'a remplacé.",
            "Quelle roue de la voiture a crevé ?",
        ),
        (
            "de",
            "Auf der Autobahn einen Nagel eingefangen; der linke Vorderreifen war platt und die Werkstatt hat ihn getauscht.",
            "Welches Rad am Auto hatte die Panne?",
        ),
        (
            "pt",
            "Passei por um prego na estrada; o pneu dianteiro esquerdo esvaziou e a oficina trocou.",
            "Qual roda do carro furou?",
        ),
        (
            "ar",
            "دخل مسمار في الإطار على الطريق السريع، فهبط الإطار الأمامي الأيسر واستبدلته الورشة.",
            "أي عجلة في السيارة أصابها الثقب؟",
        ),
        (
            "hi",
            "राजमार्ग पर कील लग गई, आगे का बायाँ टायर पंचर हो गया और गैराज ने उसे बदल दिया।",
            "गाड़ी के किस पहिये में छेद हुआ था?",
        ),
    ],
    &[
        (
            "en",
            "Feed the sourdough starter twice a day with equal parts flour and water until it doubles.",
            "How do I keep my bread culture alive and bubbly?",
        ),
        (
            "zh",
            "每天给酸面团酵种喂两次等量的面粉和水，直到体积翻倍。",
            "怎样让做面包用的老面保持活力？",
        ),
        (
            "ja",
            "サワードウのスターターに一日二回、小麦粉と水を同量ずつ与え、倍に膨らむまで待つ。",
            "パン種を元気に保つにはどうすればいい？",
        ),
        (
            "ko",
            "사워도우 스타터에 하루 두 번 밀가루와 물을 같은 양으로 먹이고 두 배가 될 때까지 둔다.",
            "빵 효모종을 살아 있게 유지하려면 어떻게 하지?",
        ),
        (
            "es",
            "Alimenta la masa madre dos veces al día con partes iguales de harina y agua hasta que doble su tamaño.",
            "¿Cómo mantengo activo el fermento para hacer pan?",
        ),
        (
            "fr",
            "Nourrir le levain deux fois par jour avec autant de farine que d'eau jusqu'à ce qu'il double.",
            "Comment garder vivante ma culture pour le pain ?",
        ),
        (
            "de",
            "Den Sauerteig zweimal täglich mit gleichen Teilen Mehl und Wasser füttern, bis er sich verdoppelt.",
            "Wie halte ich mein Anstellgut für das Brot am Leben?",
        ),
        (
            "pt",
            "Alimente o fermento natural duas vezes por dia com partes iguais de farinha e água até dobrar.",
            "Como manter viva a cultura para fazer pão?",
        ),
        (
            "ar",
            "أطعم خميرة العجين مرتين في اليوم بكميتين متساويتين من الدقيق والماء حتى يتضاعف حجمها.",
            "كيف أبقي بادئ الخبز نشطا؟",
        ),
        (
            "hi",
            "खट्टे आटे के स्टार्टर को दिन में दो बार बराबर मात्रा में मैदा और पानी दें जब तक वह दोगुना न हो जाए।",
            "रोटी के ख़मीर को ज़िंदा कैसे रखूँ?",
        ),
    ],
    &[
        (
            "en",
            "Twenty minutes of scales on the piano each evening, then the slow movement with the left hand alone.",
            "What is my daily keyboard practice routine?",
        ),
        (
            "zh",
            "每天晚上练二十分钟钢琴音阶，然后单独用左手弹慢板乐章。",
            "我每天的琴键练习安排是什么？",
        ),
        (
            "ja",
            "毎晩ピアノの音階を二十分練習し、そのあと緩徐楽章を左手だけで弾く。",
            "毎日の鍵盤の練習メニューは？",
        ),
        (
            "ko",
            "매일 저녁 피아노 음계를 이십 분 연습하고 느린 악장을 왼손으로만 친다.",
            "내 건반 연습 일과가 뭐였지?",
        ),
        (
            "es",
            "Veinte minutos de escalas al piano cada tarde y luego el movimiento lento solo con la mano izquierda.",
            "¿Cuál es mi rutina diaria con el teclado?",
        ),
        (
            "fr",
            "Vingt minutes de gammes au piano chaque soir, puis le mouvement lent à la main gauche seule.",
            "Quel est mon programme quotidien au clavier ?",
        ),
        (
            "de",
            "Jeden Abend zwanzig Minuten Tonleitern am Klavier, danach den langsamen Satz nur mit der linken Hand.",
            "Wie sieht meine tägliche Übungsroutine an den Tasten aus?",
        ),
        (
            "pt",
            "Vinte minutos de escalas no piano toda noite, depois o movimento lento só com a mão esquerda.",
            "Qual é a minha rotina diária no teclado?",
        ),
        (
            "ar",
            "عشرون دقيقة من السلالم الموسيقية على البيانو كل مساء، ثم الحركة البطيئة باليد اليسرى وحدها.",
            "ما برنامج تدريبي اليومي على المفاتيح؟",
        ),
        (
            "hi",
            "हर शाम बीस मिनट पियानो पर सरगम, फिर धीमा भाग केवल बाएँ हाथ से।",
            "कीबोर्ड पर मेरा रोज़ का अभ्यास क्या है?",
        ),
    ],
    &[
        (
            "en",
            "Send the accountant the charity receipts and home office costs before the filing deadline in April.",
            "What paperwork do I owe for my annual return to the revenue service?",
        ),
        (
            "zh",
            "四月报税截止前把慈善捐款收据和居家办公费用发给会计。",
            "年度申报需要向税务局交哪些材料？",
        ),
        (
            "ja",
            "四月の申告期限までに寄付の領収書と在宅勤務の費用を税理士に送る。",
            "確定申告で税務署に出す書類は何だっけ？",
        ),
        (
            "ko",
            "사월 신고 마감 전에 기부 영수증과 재택근무 비용을 회계사에게 보낸다.",
            "연말 세금 신고에 어떤 서류가 필요하지?",
        ),
        (
            "es",
            "Enviar a la gestora los recibos de donaciones y los gastos de la oficina en casa antes del plazo de abril.",
            "¿Qué papeles debo entregar para la declaración anual a Hacienda?",
        ),
        (
            "fr",
            "Envoyer au comptable les reçus de dons et les frais de télétravail avant l'échéance d'avril.",
            "Quels documents fournir pour ma déclaration annuelle aux impôts ?",
        ),
        (
            "de",
            "Dem Steuerberater vor der Frist im April die Spendenbelege und die Kosten fürs Homeoffice schicken.",
            "Welche Unterlagen brauche ich für die jährliche Erklärung ans Finanzamt?",
        ),
        (
            "pt",
            "Enviar ao contador os recibos de doações e os gastos do escritório em casa antes do prazo de abril.",
            "Que documentos preciso para a declaração anual à Receita?",
        ),
        (
            "ar",
            "أرسل إلى المحاسب إيصالات التبرعات وتكاليف العمل من المنزل قبل الموعد النهائي في أبريل.",
            "ما الأوراق المطلوبة لإقراري السنوي لمصلحة الضرائب؟",
        ),
        (
            "hi",
            "अप्रैल की अंतिम तिथि से पहले लेखाकार को दान की रसीदें और घर से काम का ख़र्च भेजना है।",
            "सालाना आयकर विवरण के लिए कौन से काग़ज़ चाहिए?",
        ),
    ],
    &[
        (
            "en",
            "The removal van comes on Saturday; label the boxes by room and keep the kettle out until last.",
            "What needs doing before we relocate to the new flat?",
        ),
        (
            "zh",
            "搬家货车周六来，按房间给箱子贴标签，水壶留到最后再收。",
            "我们换新公寓之前要做些什么？",
        ),
        (
            "ja",
            "引っ越しのトラックは土曜日に来る。箱に部屋名を書き、やかんは最後まで出しておく。",
            "新しいアパートへ移る前にやることは？",
        ),
        (
            "ko",
            "이삿짐 트럭이 토요일에 온다. 상자에 방 이름을 적고 주전자는 마지막까지 빼 둔다.",
            "새 집으로 옮기기 전에 뭘 해야 하지?",
        ),
        (
            "es",
            "El camión de la mudanza viene el sábado; etiquetar las cajas por habitación y dejar la tetera para el final.",
            "¿Qué hay que hacer antes de cambiarnos al piso nuevo?",
        ),
        (
            "fr",
            "Le camion de déménagement vient samedi ; étiqueter les cartons par pièce et garder la bouilloire pour la fin.",
            "Que faut-il faire avant de s'installer dans le nouvel appartement ?",
        ),
        (
            "de",
            "Der Umzugswagen kommt am Samstag; Kartons nach Zimmer beschriften und den Wasserkocher zuletzt einpacken.",
            "Was ist zu erledigen, bevor wir in die neue Wohnung ziehen?",
        ),
        (
            "pt",
            "O caminhão da mudança vem no sábado; etiquetar as caixas por cômodo e deixar a chaleira por último.",
            "O que precisamos fazer antes de ir para o apartamento novo?",
        ),
        (
            "ar",
            "تأتي شاحنة النقل يوم السبت، ضع على الصناديق اسم كل غرفة واترك الغلاية إلى النهاية.",
            "ماذا يجب أن نفعل قبل الانتقال إلى الشقة الجديدة؟",
        ),
        (
            "hi",
            "सामान ढोने वाला ट्रक शनिवार को आएगा, डिब्बों पर कमरे का नाम लिखें और केतली सबसे आख़िर में रखें।",
            "नए घर में जाने से पहले क्या करना है?",
        ),
    ],
    &[
        (
            "en",
            "The laptop dies after an hour off the charger; the shop says the cell is worn and quotes eighty euros.",
            "Why does my notebook computer lose power so quickly?",
        ),
        (
            "zh",
            "笔记本离开充电器一小时就关机，店里说电池老化了，报价八十欧元。",
            "我的电脑为什么这么快就没电？",
        ),
        (
            "ja",
            "ノートパソコンは充電器を外すと一時間で切れる。店によるとバッテリーの劣化で、修理は八十ユーロ。",
            "パソコンの電源がすぐ落ちるのはなぜ？",
        ),
        (
            "ko",
            "노트북이 충전기를 빼면 한 시간 만에 꺼진다. 매장에서는 배터리 노후라며 팔십 유로를 불렀다.",
            "컴퓨터 전원이 왜 이렇게 빨리 나가지?",
        ),
        (
            "es",
            "El portátil se apaga tras una hora sin cargador; la tienda dice que la batería está gastada y pide ochenta euros.",
            "¿Por qué mi ordenador se queda sin energía tan rápido?",
        ),
        (
            "fr",
            "Le portable s'éteint au bout d'une heure sans chargeur ; la boutique dit que la batterie est usée, quatre-vingts euros.",
            "Pourquoi mon ordinateur n'a-t-il plus d'autonomie ?",
        ),
        (
            "de",
            "Der Laptop geht nach einer Stunde ohne Ladegerät aus; der Laden sagt, der Akku sei verschlissen, achtzig Euro.",
            "Warum ist mein Computer so schnell leer?",
        ),
        (
            "pt",
            "O notebook desliga depois de uma hora sem carregador; a loja diz que a bateria está gasta e cobra oitenta euros.",
            "Por que meu computador fica sem energia tão rápido?",
        ),
        (
            "ar",
            "ينطفئ الحاسوب المحمول بعد ساعة من فصل الشاحن، ويقول المتجر إن البطارية متهالكة والإصلاح بثمانين يورو.",
            "لماذا تنفد طاقة جهازي بهذه السرعة؟",
        ),
        (
            "hi",
            "चार्जर हटाने के एक घंटे बाद लैपटॉप बंद हो जाता है, दुकान कहती है बैटरी घिस गई है, अस्सी यूरो लगेंगे।",
            "मेरा कंप्यूटर इतनी जल्दी क्यों बंद होता है?",
        ),
    ],
    &[
        (
            "en",
            "Invitations for the wedding go out in May; ask both families for addresses and dietary needs.",
            "Who still has to be told about our marriage ceremony and when?",
        ),
        (
            "zh",
            "婚礼请柬五月寄出，向双方家人要地址和饮食禁忌。",
            "我们办喜事要在什么时候通知亲友？",
        ),
        (
            "ja",
            "結婚式の招待状は五月に送る。両家に住所と食事の制限を聞いておく。",
            "挙式の案内はいつ親族に知らせるの？",
        ),
        (
            "ko",
            "결혼식 청첩장은 오월에 보낸다. 양가에 주소와 식사 제한을 물어본다.",
            "우리 예식 소식은 언제 친척들에게 알리지?",
        ),
        (
            "es",
            "Las invitaciones de la boda salen en mayo; pedir a las dos familias direcciones y restricciones de comida.",
            "¿Cuándo avisamos a los parientes de nuestra ceremonia?",
        ),
        (
            "fr",
            "Les faire-part de mariage partent en mai ; demander aux deux familles les adresses et les régimes.",
            "Quand prévenir les proches de notre cérémonie ?",
        ),
        (
            "de",
            "Die Hochzeitseinladungen gehen im Mai raus; beide Familien nach Adressen und Essenswünschen fragen.",
            "Wann erfahren die Verwandten von unserer Trauung?",
        ),
        (
            "pt",
            "Os convites do casamento saem em maio; pedir às duas famílias endereços e restrições alimentares.",
            "Quando avisamos os parentes sobre a nossa cerimônia?",
        ),
        (
            "ar",
            "ترسل دعوات الزفاف في مايو، واسأل العائلتين عن العناوين والقيود الغذائية.",
            "متى نخبر الأقارب بموعد حفل زواجنا؟",
        ),
        (
            "hi",
            "शादी के निमंत्रण मई में जाएँगे, दोनों परिवारों से पते और खानपान की ज़रूरतें पूछनी हैं।",
            "रिश्तेदारों को हमारे विवाह समारोह की ख़बर कब देनी है?",
        ),
    ],
    &[
        (
            "en",
            "Sixteen-week marathon plan: long run on Sunday, build to thirty kilometres, taper in the last fortnight.",
            "How should I prepare my legs for the big race in autumn?",
        ),
        (
            "zh",
            "十六周马拉松计划：周日长跑，逐步加到三十公里，最后两周减量。",
            "秋天那场长距离比赛该怎么准备？",
        ),
        (
            "ja",
            "十六週間のマラソン計画：日曜に長距離走、三十キロまで伸ばし、最後の二週間は量を減らす。",
            "秋の大会に向けて足をどう鍛える？",
        ),
        (
            "ko",
            "십육 주 마라톤 계획: 일요일에 장거리를 뛰고 삼십 킬로까지 늘린 뒤 마지막 두 주는 줄인다.",
            "가을 대회를 위해 다리를 어떻게 준비하지?",
        ),
        (
            "es",
            "Plan de maratón de dieciséis semanas: tirada larga el domingo, subir a treinta kilómetros y reducir las dos últimas.",
            "¿Cómo preparo las piernas para la gran carrera de otoño?",
        ),
        (
            "fr",
            "Plan marathon de seize semaines : sortie longue le dimanche, monter à trente kilomètres, alléger les deux dernières.",
            "Comment préparer mes jambes pour la grande course d'automne ?",
        ),
        (
            "de",
            "Marathonplan über sechzehn Wochen: sonntags der lange Lauf, bis dreißig Kilometer steigern, die letzten zwei reduzieren.",
            "Wie bereite ich meine Beine auf das große Rennen im Herbst vor?",
        ),
        (
            "pt",
            "Plano de maratona de dezesseis semanas: longão no domingo, subir até trinta quilômetros e reduzir nas duas últimas.",
            "Como preparar as pernas para a grande prova de outono?",
        ),
        (
            "ar",
            "خطة ماراثون من ستة عشر أسبوعا: جري طويل يوم الأحد حتى ثلاثين كيلومترا، ثم تخفيف في الأسبوعين الأخيرين.",
            "كيف أجهز ساقي للسباق الكبير في الخريف؟",
        ),
        (
            "hi",
            "सोलह हफ़्ते की मैराथन योजना: रविवार को लंबी दौड़, तीस किलोमीटर तक बढ़ाना, आख़िरी दो हफ़्ते कम करना।",
            "शरद की बड़ी दौड़ के लिए पैरों को कैसे तैयार करूँ?",
        ),
    ],
    &[
        (
            "en",
            "Two hives survived the winter; add a super in May and check the frames for the queen.",
            "How are my bee colonies doing after the cold months?",
        ),
        (
            "zh",
            "两个蜂箱熬过了冬天，五月加一层继箱，检查巢框里的蜂王。",
            "天冷之后我的蜜蜂群情况怎样？",
        ),
        (
            "ja",
            "巣箱二つが冬を越した。五月に継箱を足し、巣枠で女王蜂を確認する。",
            "寒い時期のあと、うちのミツバチはどう？",
        ),
        (
            "ko",
            "벌통 두 개가 겨울을 넘겼다. 오월에 계상을 올리고 소비에서 여왕벌을 확인한다.",
            "추운 달이 지나고 꿀벌들은 어떤 상태지?",
        ),
        (
            "es",
            "Dos colmenas sobrevivieron al invierno; añadir un alza en mayo y buscar a la reina en los cuadros.",
            "¿Cómo están mis abejas después de los meses de frío?",
        ),
        (
            "fr",
            "Deux ruches ont passé l'hiver ; ajouter une hausse en mai et chercher la reine sur les cadres.",
            "Comment vont mes abeilles après les mois froids ?",
        ),
        (
            "de",
            "Zwei Völker haben den Winter überlebt; im Mai einen Honigraum aufsetzen und die Rähmchen nach der Königin absuchen.",
            "Wie geht es meinen Bienen nach der kalten Jahreszeit?",
        ),
        (
            "pt",
            "Duas colmeias sobreviveram ao inverno; colocar uma melgueira em maio e procurar a rainha nos quadros.",
            "Como estão minhas abelhas depois dos meses frios?",
        ),
        (
            "ar",
            "نجت خليتان من الشتاء، أضف عاسلة في مايو وابحث عن الملكة في الإطارات.",
            "كيف حال نحلي بعد أشهر البرد؟",
        ),
        (
            "hi",
            "दो छत्ते सर्दी झेल गए, मई में एक और खंड जोड़ें और चौखटों में रानी मक्खी देखें।",
            "ठंड के महीनों के बाद मेरी मधुमक्खियाँ कैसी हैं?",
        ),
    ],
    &[
        (
            "en",
            "The bank offered a fixed rate of 3.4 percent on the mortgage for ten years if we remortgage by June.",
            "What did the lender propose for our home loan repayment terms?",
        ),
        (
            "zh",
            "银行提出如果六月前转贷，房贷可按百分之三点四固定十年。",
            "贷款方给我们买房借款开的条件是什么？",
        ),
        (
            "ja",
            "六月までに借り換えれば、銀行は住宅ローンを十年固定三・四パーセントにすると提案した。",
            "家を買う借金の条件は貸し手から何と言われた？",
        ),
        (
            "ko",
            "유월까지 갈아타면 은행이 주택담보대출을 십 년 고정 삼점사 퍼센트로 해 준다고 했다.",
            "집 살 때 빌린 돈의 조건을 뭐라고 제안받았지?",
        ),
        (
            "es",
            "El banco ofreció un tipo fijo del 3,4 por ciento en la hipoteca durante diez años si cambiamos antes de junio.",
            "¿Qué condiciones nos propuso la entidad para el préstamo de la casa?",
        ),
        (
            "fr",
            "La banque propose un taux fixe de 3,4 pour cent sur le crédit immobilier pendant dix ans si on renégocie avant juin.",
            "Quelles conditions le prêteur a-t-il proposées pour l'emprunt de la maison ?",
        ),
        (
            "de",
            "Die Bank bietet für die Hypothek zehn Jahre feste 3,4 Prozent, wenn wir bis Juni umschulden.",
            "Welche Konditionen hat der Geldgeber für unser Hausdarlehen genannt?",
        ),
        (
            "pt",
            "O banco ofereceu taxa fixa de 3,4 por cento na hipoteca por dez anos se refinanciarmos até junho.",
            "Que condições a instituição propôs para o empréstimo da casa?",
        ),
        (
            "ar",
            "عرض البنك فائدة ثابتة 3.4 بالمئة على الرهن العقاري لعشر سنوات إذا أعدنا التمويل قبل يونيو.",
            "ما الشروط التي اقترحها المقرض لقرض بيتنا؟",
        ),
        (
            "hi",
            "बैंक ने जून तक पुनर्वित्त करने पर गृह ऋण पर दस साल के लिए 3.4 प्रतिशत की स्थिर दर बताई।",
            "घर के क़र्ज़ के लिए ऋणदाता ने क्या शर्तें रखीं?",
        ),
    ],
    &[
        (
            "en",
            "Still awake at three most nights; the sleep clinic suggests no screens after ten and a fixed wake time.",
            "What was the advice about my insomnia?",
        ),
        (
            "zh",
            "大多数晚上三点还醒着，睡眠门诊建议十点后不看屏幕，并固定起床时间。",
            "关于我失眠的问题，医生给了什么建议？",
        ),
        (
            "ja",
            "ほとんどの夜、三時まで眠れない。睡眠外来は十時以降の画面禁止と起床時刻の固定を勧めた。",
            "不眠について何を勧められたんだっけ？",
        ),
        (
            "ko",
            "거의 매일 밤 세 시까지 깨어 있다. 수면 클리닉은 열 시 이후 화면 금지와 기상 시간 고정을 권했다.",
            "불면증에 대해 어떤 조언을 받았지?",
        ),
        (
            "es",
            "Casi todas las noches sigo despierto a las tres; la clínica del sueño aconseja nada de pantallas después de las diez y hora fija al levantarse.",
            "¿Qué me recomendaron para el insomnio?",
        ),
        (
            "fr",
            "Encore éveillé à trois heures presque chaque nuit ; la clinique du sommeil conseille pas d'écrans après dix heures et un réveil fixe.",
            "Quel conseil m'a-t-on donné pour mon insomnie ?",
        ),
        (
            "de",
            "Fast jede Nacht um drei noch wach; die Schlafambulanz rät zu keinen Bildschirmen nach zehn und einer festen Aufstehzeit.",
            "Was wurde mir gegen die Schlaflosigkeit geraten?",
        ),
        (
            "pt",
            "Quase toda noite ainda acordado às três; a clínica do sono sugere nada de telas depois das dez e hora fixa para levantar.",
            "Qual foi o conselho sobre a minha insônia?",
        ),
        (
            "ar",
            "ما زلت مستيقظا عند الثالثة معظم الليالي، وتنصح عيادة النوم بعدم استخدام الشاشات بعد العاشرة وبوقت استيقاظ ثابت.",
            "ما النصيحة بشأن الأرق الذي أعاني منه؟",
        ),
        (
            "hi",
            "ज़्यादातर रातों में तीन बजे तक जागता हूँ, नींद क्लिनिक ने दस बजे के बाद स्क्रीन नहीं और उठने का तय समय बताया।",
            "मेरी अनिद्रा के बारे में क्या सलाह मिली थी?",
        ),
    ],
];
