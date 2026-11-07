//! SPK-05: exact search at the Large workspace, against the real index. Pages go straight to the
//! index because the merge layer cannot reach 100,000 pages today (see `scale` in main.rs).

use crate::corpus::{self, LANGUAGES};
use crate::rng::Rng;
use casual_note_search::{Doc, Field, Index, Owner};

pub struct Corpus {
    pub index: Index,
    pub text_bytes: u64,
    pub docs: u64,
    pub rare: Vec<String>,
}

impl std::fmt::Debug for Corpus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Corpus({} docs)", self.docs)
    }
}

/// Fills an index with `pages` pages in mixed languages.
pub fn build(pages: usize, blocks_per_page: usize, seed: u64) -> Corpus {
    let mut rng = Rng::new(seed);
    let mut index = Index::new();
    let (mut text_bytes, mut docs) = (0u64, 0u64);
    let mut rare = Vec::new();
    for p in 0..pages {
        let lang = corpus::language_for(rng.next_u64());
        let id = format!("p{p}");
        let mut list = Vec::with_capacity(blocks_per_page + 1);
        let title = corpus::rare_word(lang, &mut rng);
        if rare.len() < 2000 && p % 37 == 0 {
            rare.push(title.clone());
        }
        text_bytes += title.len() as u64;
        list.push(Doc {
            item: String::new(),
            field: Field::Title,
            text: title,
        });
        for b in 0..rng.range(blocks_per_page / 2, blocks_per_page * 3 / 2) {
            let text = corpus::line(lang, &mut rng);
            text_bytes += text.len() as u64;
            list.push(Doc {
                item: format!("{id}b{b}"),
                field: Field::Body,
                text,
            });
        }
        docs += list.len() as u64;
        index.update(&Owner::Page(id), list);
    }
    Corpus {
        index,
        text_bytes,
        docs,
        rare,
    }
}

/// The queries, in every launch language: common words, phrases, prefixes, CJK runs and rare words.
pub fn queries(rare: &[String]) -> Vec<String> {
    let mut out: Vec<String> = [
        "plumber",
        "budget review",
        "passport",
        "the",
        "gard",
        "fontanero",
        "presupuesto",
        "plombier",
        "Klempner",
        "encanador",
        "水管工",
        "预算",
        "護照",
        "火车",
        "パスポート",
        "電車",
        "여권",
        "기차",
        "جواز السفر",
        "الميزانية",
        "पासपोर्ट",
        "बजट",
        "supplier decision",
        "tomates",
        "血压",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    out.extend(
        rare.iter()
            .step_by((rare.len() / 25).max(1))
            .take(25)
            .cloned(),
    );
    let _ = LANGUAGES;
    out
}
