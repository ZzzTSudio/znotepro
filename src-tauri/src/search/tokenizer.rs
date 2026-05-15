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
            if lower
                .chars()
                .all(|c| c.is_ascii_punctuation() || c.is_whitespace())
            {
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
        if lower
            .chars()
            .all(|c| c.is_ascii_punctuation() || c.is_whitespace())
        {
            continue;
        }

        result.push((lower, pos));
        pos += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{tokenize, tokenize_with_positions};

    #[test]
    fn token_positions_use_token_order() {
        let tokens = tokenize_with_positions("3a 算法处理");
        let positions: Vec<u32> = tokens.iter().map(|(_, pos)| *pos).collect();

        assert_eq!(positions, (0..positions.len() as u32).collect::<Vec<_>>());
    }

    #[test]
    fn chinese_and_ascii_tokens_are_kept() {
        let tokens = tokenize("3a 算法处理");

        assert!(tokens.iter().any(|token| token == "3a"));
        assert!(tokens.iter().any(|token| token.contains("算法")));
    }
}
