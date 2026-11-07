//! Turns text into search terms. Pure: a function of the text and `TOKENISER_VERSION` alone, so it
//! ignores the interface language and gives the same terms on native and web. See docs/60.
//!
//! The stored text is never changed. Each grapheme is copied, normalised and folded for matching,
//! and every term remembers the bytes of the original text it came from, for highlighting.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;
use unicode_segmentation::UnicodeSegmentation;

/// Raised whenever terms could change, which forces an index rebuild.
pub const TOKENISER_VERSION: u32 = 1;
/// Text past this is not indexed, so one huge block cannot stall the worker.
pub const MAX_TEXT_BYTES: usize = 1 << 20;
const MAX_TERM_CHARS: usize = 64;
const MAX_TOKENS: usize = 200_000;
const MAX_QUERY_BYTES: usize = 1024;
const MAX_QUERY_TERMS: usize = 32;

/// One term, with the bytes of the original text it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub term: String,
    pub start: usize,
    pub end: usize,
    /// False for extra forms, such as an accent-free spelling. These rank below the real word.
    pub primary: bool,
}

/// Case folding for matching. Going through upper case gives full folding without a table:
/// `ß` and `SS` agree, as do `ς` and `σ`.
pub fn fold(text: &str) -> String {
    let once: String = text.nfc().collect::<String>().to_lowercase();
    if once.is_ascii() {
        return once;
    }
    once.to_uppercase().to_lowercase().nfc().collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// Han, Hiragana, Katakana, Thai, Lao, Khmer and Myanmar: bigrams of grapheme clusters.
    NoSpace,
    Hangul,
    /// Everything else that is written with letters or numbers: UAX #29 words.
    Word,
    Emoji,
    /// Punctuation. A word may run through it, as in `don't`, so UAX #29 decides.
    Neutral,
    Space,
}

struct Unit {
    text: String,
    start: usize,
    end: usize,
    class: Class,
}

fn is_ignorable(c: char) -> bool {
    matches!(c as u32,
        0xAD | 0x34F | 0x61C | 0x115F..=0x1160 | 0x17B4..=0x17B5 | 0x180B..=0x180F
        | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F
        | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFF8 | 0xE0000..=0xE0FFF)
}

/// Fullwidth Latin and digits, which Chinese and Japanese input methods produce.
fn unwiden(c: char) -> char {
    match c as u32 {
        0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        0x3000 => ' ',
        _ => c,
    }
}

fn is_no_space(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2FDF | 0x3005 | 0x3007 | 0x3021..=0x3029 | 0x3038..=0x303B | 0x3041..=0x309F
        | 0x30A1..=0x30FF | 0x31C0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0xFF66..=0xFF9F | 0x20000..=0x323AF
        | 0x0E00..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF | 0x19E0..=0x19FF
        | 0xA9E0..=0xA9FF | 0xAA60..=0xAA7F)
}

fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3131..=0x318E | 0xA960..=0xA97F | 0xAC00..=0xD7FF)
}

fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x2300..=0x23FF)
}

fn classify(folded: &str) -> Class {
    let Some(first) = folded.chars().next() else {
        return Class::Space;
    };
    if first.is_whitespace() {
        return Class::Space;
    }
    if is_emoji(first) {
        return Class::Emoji;
    }
    if is_no_space(first) {
        return Class::NoSpace;
    }
    if is_hangul(first) {
        return Class::Hangul;
    }
    if folded
        .chars()
        .any(|c| c.is_alphanumeric() || is_combining_mark(c))
    {
        Class::Word
    } else {
        Class::Neutral
    }
}

fn units(text: &str) -> Vec<Unit> {
    let mut out = Vec::new();
    for (start, grapheme) in text.grapheme_indices(true) {
        let cleaned: String = grapheme
            .chars()
            .filter(|c| !is_ignorable(*c))
            .map(unwiden)
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        let folded = fold(&cleaned);
        out.push(Unit {
            class: classify(&folded),
            text: folded,
            start,
            end: start + grapheme.len(),
        });
    }
    out
}

fn clip(term: &str) -> String {
    term.chars().take(MAX_TERM_CHARS).collect()
}

/// The form typed when marks are left out, as a secondary term. Hindi keeps its marks, because
/// its vowel signs are part of the word.
fn without_optional_marks(word: &str) -> Option<String> {
    let first = word.chars().next()? as u32;
    let bare: String = if first <= 0x052F {
        word.nfd()
            .filter(|c| !is_combining_mark(*c))
            .collect::<String>()
            .nfc()
            .collect()
    } else if (0x0600..=0x06FF).contains(&first) {
        word.chars()
            .filter(|c| !matches!(*c as u32, 0x64B..=0x652 | 0x640))
            .map(|c| match c {
                '\u{622}' | '\u{623}' | '\u{625}' => '\u{627}',
                other => other,
            })
            .collect()
    } else {
        return None;
    };
    (bare != word && !bare.is_empty()).then_some(bare)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Index,
    Query,
}

fn no_space_run(run: &[Unit], mode: Mode, out: &mut Vec<Token>) {
    if let [only] = run {
        out.push(Token {
            term: clip(&only.text),
            start: only.start,
            end: only.end,
            primary: true,
        });
        return;
    }
    for pair in run.windows(2) {
        if let [a, b] = pair {
            out.push(Token {
                term: clip(&format!("{}{}", a.text, b.text)),
                start: a.start,
                end: b.end,
                primary: true,
            });
        }
    }
    // The last character alone, so a one-character search can find it at the end of a run.
    if mode == Mode::Index
        && let Some(last) = run.last()
    {
        out.push(Token {
            term: clip(&last.text),
            start: last.start,
            end: last.end,
            primary: false,
        });
    }
}

fn hangul_run(run: &[Unit], mode: Mode, out: &mut Vec<Token>) {
    let (Some(first), Some(last)) = (run.first(), run.last()) else {
        return;
    };
    let word: String = run.iter().map(|u| u.text.as_str()).collect();
    if run.len() < 2 || mode == Mode::Index {
        out.push(Token {
            term: clip(&word),
            start: first.start,
            end: last.end,
            primary: true,
        });
    }
    // Particles attach to nouns, so `한국어를` must be found by `한국어`. Bigrams do that.
    if run.len() > 2 || (mode == Mode::Query && run.len() == 2) {
        for pair in run.windows(2) {
            if let [a, b] = pair {
                out.push(Token {
                    term: format!("{}{}", a.text, b.text),
                    start: a.start,
                    end: b.end,
                    primary: false,
                });
            }
        }
    }
}

fn word_run(run: &[Unit], mode: Mode, out: &mut Vec<Token>) {
    let mut joined = String::new();
    let mut map = Vec::with_capacity(run.len());
    for (i, unit) in run.iter().enumerate() {
        map.push((joined.len(), i));
        joined.push_str(&unit.text);
    }
    let unit_at = |offset: usize| match map.binary_search_by_key(&offset, |m| m.0) {
        Ok(i) => map.get(i).map(|m| m.1),
        Err(i) => i.checked_sub(1).and_then(|p| map.get(p)).map(|m| m.1),
    };
    for (offset, word) in joined.unicode_word_indices() {
        let from = unit_at(offset).and_then(|i| run.get(i));
        let to = unit_at(offset + word.len().saturating_sub(1)).and_then(|i| run.get(i));
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        out.push(Token {
            term: clip(word),
            start: from.start,
            end: to.end,
            primary: true,
        });
        if mode == Mode::Index
            && let Some(bare) = without_optional_marks(word)
        {
            out.push(Token {
                term: clip(&bare),
                start: from.start,
                end: to.end,
                primary: false,
            });
        }
    }
}

fn tokens(text: &str, mode: Mode) -> Vec<Token> {
    let units = units(text);
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(unit) = units.get(at) {
        if out.len() >= MAX_TOKENS {
            break;
        }
        let class = unit.class;
        if matches!(class, Class::Space | Class::Neutral) {
            at += 1;
            continue;
        }
        if class == Class::Emoji {
            out.push(Token {
                term: unit.text.clone(),
                start: unit.start,
                end: unit.end,
                primary: true,
            });
            at += 1;
            continue;
        }
        let mut end = at + 1;
        while let Some(next) = units.get(end) {
            let same =
                next.class == class || (class == Class::Word && next.class == Class::Neutral);
            if !same {
                break;
            }
            end += 1;
        }
        let run = units.get(at..end).unwrap_or(&[]);
        match class {
            Class::NoSpace => no_space_run(run, mode, &mut out),
            Class::Hangul => hangul_run(run, mode, &mut out),
            _ => word_run(run, mode, &mut out),
        }
        at = end;
    }
    out.truncate(MAX_TOKENS);
    out
}

fn head(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or("")
}

/// The terms a piece of stored text is indexed under.
pub fn tokenise(text: &str) -> Vec<Token> {
    tokens(head(text, MAX_TEXT_BYTES), Mode::Index)
}

/// The terms a search looks for, each once, in the order typed. Every one must match.
pub fn query_terms(query: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for token in tokens(head(query, MAX_QUERY_BYTES), Mode::Query) {
        if seen.insert(token.term.clone()) {
            out.push(token.term);
            if out.len() == MAX_QUERY_TERMS {
                break;
            }
        }
    }
    out
}

/// A natural logarithm from basic arithmetic only. Libraries differ in the last bit between
/// native and wasm, and scores must not.
pub fn stable_ln(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 || x.is_infinite() {
        return 0.0;
    }
    const LN2: f64 = std::f64::consts::LN_2;
    let (mut m, mut e) = (x, 0i32);
    let mut guard = 0;
    while m >= std::f64::consts::SQRT_2 && guard < 2100 {
        m /= 2.0;
        e += 1;
        guard += 1;
    }
    while m < std::f64::consts::FRAC_1_SQRT_2 && guard < 2100 {
        m *= 2.0;
        e -= 1;
        guard += 1;
    }
    let s = (m - 1.0) / (m + 1.0);
    let s2 = s * s;
    let (mut term, mut sum) = (s, 0.0);
    for k in [1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 13.0, 15.0, 17.0] {
        sum += term / k;
        term *= s2;
    }
    2.0 * sum + f64::from(e) * LN2
}
