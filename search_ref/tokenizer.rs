use jieba_rs::Jieba;
use std::sync::OnceLock;

static JIEBA: OnceLock<Jieba> = OnceLock::new();

fn jieba() -> &'static Jieba {
    JIEBA.get_or_init(Jieba::new)
}

pub fn tokenize(text: &str) -> Vec<String> {
    let words = jieba().cut(text, true);
    words
        .into_iter()
        .filter_map(|w| {
            let trimmed = w.trim();
            if trimmed.is_empty() {
                return None;
            }
            let lower = trimmed.to_lowercase();
            if lower.chars().all(|c| c.is_ascii_punctuation() || c.is_whitespace()) {
                return None;
            }
            Some(lower)
        })
        .collect()
}

pub fn tokenize_with_positions(text: &str) -> Vec<(String, u32)> {
    let words = jieba().cut(text, true);
    let mut result = Vec::new();
    let mut pos: u32 = 0;

    for w in words {
        let trimmed = w.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if lower.chars().all(|c| c.is_ascii_punctuation() || c.is_whitespace()) {
            continue;
        }
        result.push((lower, pos));
        pos += 1;
    }

    result
}
