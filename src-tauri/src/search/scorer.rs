use std::collections::{HashMap, HashSet};

use super::index::SearchIndex;
use super::query::ParsedQuery;
use super::tokenizer::tokenize;

const K1: f64 = 1.2;
const B: f64 = 0.75;
const TITLE_BOOST: f64 = 3.0;
const EXACT_MATCH_BOOST: f64 = 2.0;
const PHRASE_BOOST: f64 = 1.5;

#[derive(Debug, Clone)]
pub struct ScoredDoc {
    pub doc_id: u32,
    pub score: f64,
    pub boost_reasons: Vec<String>,
}

pub fn score_documents(index: &SearchIndex, query: &ParsedQuery) -> Vec<ScoredDoc> {
    if query.must_terms.is_empty() && query.must_phrases.is_empty() {
        return Vec::new();
    }

    let mut recall_terms: Vec<String> = query.must_terms.clone();
    for phrase in &query.must_phrases {
        recall_terms.extend(phrase.iter().cloned());
    }

    let mut doc_scores: HashMap<u32, (f64, Vec<String>)> = HashMap::new();

    for term in &recall_terms {
        if let Some(postings) = index.postings.get(term) {
            let df = postings.len() as f64;
            let idf = ((index.total_docs as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

            for posting in postings {
                let doc_meta = match index.documents.get(&posting.doc_id) {
                    Some(m) => m,
                    None => continue,
                };

                let tf = posting.term_frequency as f64;
                let dl = doc_meta.doc_length.max(1) as f64;
                let avgdl = index.avg_doc_length.max(1.0);
                let tf_norm = (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * dl / avgdl));
                let term_score = idf * tf_norm;

                doc_scores
                    .entry(posting.doc_id)
                    .or_insert((0.0, Vec::new()))
                    .0 += term_score;
            }
        }
    }

    if !query.must_terms.is_empty() {
        let must_doc_sets: Vec<HashSet<u32>> = query
            .must_terms
            .iter()
            .map(|term| {
                index
                    .postings
                    .get(term)
                    .map(|ps| ps.iter().map(|p| p.doc_id).collect())
                    .unwrap_or_default()
            })
            .collect();

        if let Some(first) = must_doc_sets.first() {
            let intersection: HashSet<u32> = first
                .iter()
                .filter(|id| must_doc_sets.iter().all(|set| set.contains(id)))
                .copied()
                .collect();
            doc_scores.retain(|id, _| intersection.contains(id));
        }
    }

    if !query.must_phrases.is_empty() {
        let phrases = &query.must_phrases;
        doc_scores.retain(|&doc_id, (_, boosts)| {
            let all_match = phrases
                .iter()
                .all(|tokens| tokens.len() < 2 || check_phrase_match(index, doc_id, tokens));
            if all_match && !boosts.iter().any(|b| b == "phrase_match") {
                boosts.push("phrase_match".to_string());
            }
            all_match
        });
    }

    if !query.exclude_terms.is_empty() {
        let exclude_docs: HashSet<u32> = query
            .exclude_terms
            .iter()
            .flat_map(|term| {
                index
                    .postings
                    .get(term)
                    .map(|ps| ps.iter().map(|p| p.doc_id).collect::<Vec<_>>())
                    .unwrap_or_default()
            })
            .collect();

        doc_scores.retain(|id, _| !exclude_docs.contains(id));
    }

    for (&doc_id, (score, boosts)) in doc_scores.iter_mut() {
        let doc_meta = match index.documents.get(&doc_id) {
            Some(m) => m,
            None => continue,
        };

        let title_tokens = tokenize(&doc_meta.title);
        if query.must_terms.iter().any(|t| title_tokens.contains(t)) {
            *score *= TITLE_BOOST;
            boosts.push("title_match".to_string());
        }

        if !query.raw_query.is_empty() {
            let raw_lower = query.raw_query.to_lowercase();
            let no_space = raw_lower.replace(' ', "");
            let title_lower = doc_meta.title.to_lowercase();
            let path_lower = doc_meta.path.to_lowercase();

            if title_lower.contains(&no_space) || path_lower.contains(&no_space) {
                *score *= EXACT_MATCH_BOOST;
                if !boosts.iter().any(|b| b == "exact_match") {
                    boosts.push("exact_match".to_string());
                }
            }
        }

        if boosts.iter().any(|b| b == "phrase_match") {
            *score *= PHRASE_BOOST;
        }
    }

    let mut results: Vec<ScoredDoc> = doc_scores
        .into_iter()
        .map(|(doc_id, (score, boost_reasons))| ScoredDoc {
            doc_id,
            score,
            boost_reasons,
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

fn check_phrase_match(index: &SearchIndex, doc_id: u32, phrase_tokens: &[String]) -> bool {
    if phrase_tokens.len() < 2 {
        return true;
    }

    let first_term = &phrase_tokens[0];
    let first_positions = match index.postings.get(first_term) {
        Some(postings) => match postings.iter().find(|p| p.doc_id == doc_id) {
            Some(p) => &p.positions,
            None => return false,
        },
        None => return false,
    };

    'outer: for &start_pos in first_positions {
        for (offset, term) in phrase_tokens.iter().enumerate().skip(1) {
            let expected_pos = start_pos + offset as u32;
            let found = index
                .postings
                .get(term)
                .and_then(|ps| ps.iter().find(|p| p.doc_id == doc_id))
                .map(|p| p.positions.contains(&expected_pos))
                .unwrap_or(false);

            if !found {
                continue 'outer;
            }
        }
        return true;
    }

    false
}
