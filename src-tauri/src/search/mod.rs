pub mod extractor;
pub mod index;
pub mod query;
pub mod scorer;
pub mod tokenizer;

use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::{MatchContext, SearchResult};
use extractor::extract_search_text;
use index::{DocMeta, SearchIndex, INDEX_FILE_LIMIT_BYTES};
use query::{parse_query, ParsedQuery};
use scorer::score_documents;

static INDEX: OnceLock<Mutex<Option<SearchIndex>>> = OnceLock::new();

fn index_cell() -> &'static Mutex<Option<SearchIndex>> {
    INDEX.get_or_init(|| Mutex::new(None))
}

pub fn ensure_index(note_dir: &Path) -> Result<(), String> {
    let mut guard = index_cell()
        .lock()
        .map_err(|_| "Search index lock poisoned".to_string())?;
    if guard.is_none() {
        *guard = Some(SearchIndex::load_or_build(note_dir));
    }
    Ok(())
}

pub fn rebuild_index(note_dir: &Path) -> Result<(), String> {
    let mut guard = index_cell()
        .lock()
        .map_err(|_| "Search index lock poisoned".to_string())?;
    let mut idx = SearchIndex::load_or_build(note_dir);
    idx.full_rebuild(note_dir);
    *guard = Some(idx);
    Ok(())
}

pub fn update_index_file(note_dir: &Path, rel_path: &str) -> Result<(), String> {
    let mut guard = index_cell()
        .lock()
        .map_err(|_| "Search index lock poisoned".to_string())?;
    match guard.as_mut() {
        Some(idx) => idx.update_single_file(note_dir, rel_path),
        None => {
            let mut idx = SearchIndex::load_or_build(note_dir);
            idx.update_single_file(note_dir, rel_path);
            *guard = Some(idx);
        }
    }
    Ok(())
}

pub fn remove_index_file(_note_dir: &Path, rel_path: &str) -> Result<(), String> {
    let mut guard = index_cell()
        .lock()
        .map_err(|_| "Search index lock poisoned".to_string())?;
    if let Some(idx) = guard.as_mut() {
        idx.remove_single_file(rel_path);
    }
    Ok(())
}

pub fn perform_search(note_dir: &Path, query_str: &str) -> Result<Vec<SearchResult>, String> {
    ensure_index(note_dir)?;

    let parsed = parse_query(query_str);
    let candidates = {
        let guard = index_cell()
            .lock()
            .map_err(|_| "Search index lock poisoned".to_string())?;
        let idx = match guard.as_ref() {
            Some(i) => i,
            None => return Ok(Vec::new()),
        };

        score_documents(idx, &parsed)
            .into_iter()
            .filter_map(|scored_doc| {
                idx.documents
                    .get(&scored_doc.doc_id)
                    .cloned()
                    .map(|doc_meta| SearchCandidate {
                        doc_meta,
                        score: scored_doc.score,
                        boost_reasons: scored_doc.boost_reasons,
                    })
            })
            .collect::<Vec<_>>()
    };

    let mut results = Vec::new();

    for candidate in candidates {
        let file_path = note_dir.join(&candidate.doc_meta.path);
        if is_too_large_for_context(&file_path) {
            continue;
        }
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let searchable_content = extract_search_text(&file_path, &content);
        let matches = find_line_matches(&searchable_content, &parsed);
        if matches.is_empty() {
            continue;
        }

        results.push(SearchResult {
            file: candidate.doc_meta.path,
            score: candidate.score,
            title: candidate.doc_meta.title,
            matches,
            boost_reasons: candidate.boost_reasons,
        });
    }

    Ok(results)
}

struct SearchCandidate {
    doc_meta: DocMeta,
    score: f64,
    boost_reasons: Vec<String>,
}

fn is_too_large_for_context(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.len() > INDEX_FILE_LIMIT_BYTES)
        .unwrap_or(true)
}

fn find_line_matches(content: &str, query: &ParsedQuery) -> Vec<MatchContext> {
    let lines: Vec<&str> = content.lines().collect();
    let mut matches = Vec::new();
    let all_terms: Vec<&str> = query.must_terms.iter().map(|s| s.as_str()).collect();
    let phrase_strings: Vec<String> = query
        .must_phrases
        .iter()
        .map(|tokens| tokens.join(""))
        .collect();
    let raw_no_space = query.raw_query.to_lowercase().replace(' ', "");

    for (idx, line) in lines.iter().enumerate() {
        let line_lower = line.to_lowercase();
        let line_no_space = line_lower.replace(' ', "");
        let line_tokens = tokenizer::tokenize(&line_lower);
        let term_hit = all_terms
            .iter()
            .any(|term| line_tokens.iter().any(|line_term| line_term == term));
        let phrase_hit = phrase_strings.iter().any(|phrase| {
            !phrase.is_empty() && (line_lower.contains(phrase) || line_no_space.contains(phrase))
        });
        let raw_hit = !raw_no_space.is_empty() && line_no_space.contains(&raw_no_space);

        if term_hit || phrase_hit || raw_hit {
            matches.push(MatchContext {
                line_number: idx + 1,
                line_text: line.to_string(),
                context_before: lines
                    .iter()
                    .take(idx)
                    .rev()
                    .take(2)
                    .rev()
                    .map(|s| s.to_string())
                    .collect(),
                context_after: lines
                    .iter()
                    .skip(idx + 1)
                    .take(2)
                    .map(|s| s.to_string())
                    .collect(),
            });
        }

        if matches.len() >= 5 {
            break;
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::perform_search;
    use crate::search::index::INDEX_FILE_LIMIT_BYTES;
    use std::fs;

    #[test]
    fn search_skips_oversized_context_files() {
        let note_dir = std::env::temp_dir().join(format!(
            "znote_search_context_limit_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&note_dir);
        fs::create_dir_all(&note_dir).unwrap();
        fs::write(note_dir.join("small.md"), "# title\n\nneedle\n").unwrap();
        fs::write(
            note_dir.join("large.md"),
            vec![b'a'; (INDEX_FILE_LIMIT_BYTES + 1) as usize],
        )
        .unwrap();

        let results = perform_search(&note_dir, "needle").unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file, "small.md");

        let _ = fs::remove_dir_all(&note_dir);
    }
}
