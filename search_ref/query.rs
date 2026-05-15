use super::tokenizer::tokenize;

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub must_terms: Vec<String>,
    pub must_phrases: Vec<Vec<String>>,
    pub exclude_terms: Vec<String>,
    pub raw_query: String,
}

pub fn parse_query(input: &str) -> ParsedQuery {
    let raw_query = input.to_string();
    let mut must_terms = Vec::new();
    let mut must_phrases = Vec::new();
    let mut exclude_terms = Vec::new();

    let mut chars = input.chars().peekable();
    let mut tokens: Vec<RawToken> = Vec::new();

    while let Some(&ch) = chars.peek() {
        match ch {
            '"' | '\u{201c}' | '\u{201d}' => {
                chars.next();
                let mut phrase = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '"' || c == '\u{201c}' || c == '\u{201d}' {
                        chars.next();
                        break;
                    }
                    phrase.push(c);
                    chars.next();
                }
                if !phrase.trim().is_empty() {
                    tokens.push(RawToken::Phrase(phrase));
                }
            }
            ' ' | '\t' => {
                chars.next();
            }
            '-' => {
                chars.next();
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' || c == '\t' {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                if !word.is_empty() {
                    tokens.push(RawToken::Exclude(word));
                }
            }
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' || c == '\t' || c == '"' || c == '\u{201c}' {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                if !word.is_empty() {
                    tokens.push(RawToken::Term(word));
                }
            }
        }
    }

    for token in tokens {
        match token {
            RawToken::Term(t) => {
                let tokenized = tokenize(&t);
                must_terms.extend(tokenized);
            }
            RawToken::Phrase(p) => {
                let tokenized = tokenize(&p);
                if !tokenized.is_empty() {
                    must_phrases.push(tokenized);
                }
            }
            RawToken::Exclude(e) => {
                let tokenized = tokenize(&e);
                exclude_terms.extend(tokenized);
            }
        }
    }

    ParsedQuery {
        must_terms,
        must_phrases,
        exclude_terms,
        raw_query,
    }
}

#[derive(Debug)]
enum RawToken {
    Term(String),
    Phrase(String),
    Exclude(String),
}
