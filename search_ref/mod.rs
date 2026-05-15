pub mod tokenizer;
pub mod query;
pub mod index;
pub mod scorer;

use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use index::SearchIndex;
use query::parse_query;
use scorer::score_documents;

use crate::{SearchResult, MatchContext};

static INDEX: OnceLock<Mutex<Option<SearchIndex>>> = OnceLock::new();

fn index_cell() -> &'static Mutex<Option<SearchIndex>> {
    INDEX.get_or_init(|| Mutex::new(None))
}

pub fn ensure_index(note_dir: &Path) {
    let mut guard = index_cell().lock().unwrap();
    if guard.is_none() {
        *guard = Some(SearchIndex::load_or_build(note_dir));
    }
}

pub fn rebuild_index(note_dir: &Path) {
    let mut guard = index_cell().lock().unwrap();
    let mut idx = SearchIndex::load_or_build(note_dir);
    idx.full_rebuild(note_dir);
    *guard = Some(idx);
}

pub fn update_index_file(note_dir: &Path, rel_path: &str) {
    let mut guard = index_cell().lock().unwrap();
    match guard.as_mut() {
        Some(idx) => idx.update_single_file(note_dir, rel_path),
        None => {
            let mut idx = SearchIndex::load_or_build(note_dir);
            idx.update_single_file(note_dir, rel_path);
            *guard = Some(idx);
        }
    }
}

pub fn remove_index_file(rel_path: &str) {
    let mut guard = index_cell().lock().unwrap();
    if let Some(idx) = guard.as_mut() {
        idx.remove_single_file(rel_path);
    }
}

pub fn search(note_dir: &Path, query_str: &str) -> Vec<SearchResult> {
    ensure_index(note_dir);

    let guard = index_cell().lock().unwrap();
    let idx = match guard.as_ref() {
        Some(i) => i,
        None => return Vec::new(),
    };

    let parsed = parse_query(query_str);
    let scored = score_documents(idx, &parsed);

    let mut results = Vec::new();

    for scored_doc in scored {
        let doc_meta = match idx.documents.get(&scored_doc.doc_id) {
            Some(m) => m,
            None => continue,
        };

        let file_path = note_dir.join(&doc_meta.path);
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let matches = find_line_matches(&content, &parsed);
        if matches.is_empty() {
            continue;
        }

        results.push(SearchResult {
            file: doc_meta.path.clone(),
            score: scored_doc.score,
            title: doc_meta.title.clone(),
            matches,
            boost_reasons: scored_doc.boost_reasons,
        });
    }

    results
}

fn find_line_matches(content: &str, query: &query::ParsedQuery) -> Vec<MatchContext> {
    let lines: Vec<&str> = content.lines().collect();
    let mut matches = Vec::new();

    let all_terms: Vec<&str> = query.must_terms.iter()
        .map(|s| s.as_str())
        .collect();

    let phrase_strings: Vec<String> = query.must_phrases.iter()
        .map(|tokens| tokens.join(""))
        .collect();

    for (idx, line) in lines.iter().enumerate() {
        let line_lower = line.to_lowercase();
        let line_tokens = tokenizer::tokenize(&line_lower);

        let term_hit = all_terms.iter().any(|t| line_tokens.contains(&t.to_string()));
        let phrase_hit = phrase_strings.iter().any(|p| line_lower.contains(p));
        let raw_hit = {
            let raw_no_space = query.raw_query.to_lowercase().replace(' ', "");
            line_lower.contains(&raw_no_space)
        };

        if term_hit || phrase_hit || raw_hit {
            let line_number = idx + 1;
            let context_before = lines
                .iter()
                .take(idx)
                .rev()
                .take(2)
                .rev()
                .map(|s| s.to_string())
                .collect();
            let context_after = lines
                .iter()
                .skip(idx + 1)
                .take(2)
                .map(|s| s.to_string())
                .collect();

            matches.push(MatchContext {
                line_number,
                line_text: line.to_string(),
                context_before,
                context_after,
            });
        }
    }

    matches
}
