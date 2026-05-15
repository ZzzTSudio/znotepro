use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

use super::extractor::extract;
use super::tokenizer::tokenize_with_positions;

const INDEX_DIR_NAME: &str = ".search_index";
const INDEX_FILE: &str = "index.json";
const META_FILE: &str = "meta.json";
const INDEX_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Posting {
    pub doc_id: u32,
    pub term_frequency: u32,
    pub positions: Vec<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocMeta {
    pub path: String,
    pub title: String,
    pub format: String,
    pub doc_length: u32,
    pub mtime: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IndexMeta {
    pub version: u32,
    pub documents: HashMap<u32, DocMeta>,
    pub total_docs: u32,
    pub avg_doc_length: f64,
    pub next_doc_id: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PersistedIndex {
    pub postings: HashMap<String, Vec<Posting>>,
}

pub struct SearchIndex {
    pub postings: HashMap<String, Vec<Posting>>,
    pub documents: HashMap<u32, DocMeta>,
    pub total_docs: u32,
    pub avg_doc_length: f64,
    next_doc_id: u32,
    index_dir: PathBuf,
}

impl SearchIndex {
    pub fn load_or_build(note_dir: &Path) -> Self {
        let index_dir = note_dir.join(INDEX_DIR_NAME);

        if let Some(mut idx) = Self::try_load(&index_dir) {
            idx.incremental_update(note_dir);
            idx
        } else {
            let mut idx = Self::new(index_dir);
            idx.full_rebuild(note_dir);
            idx
        }
    }

    fn new(index_dir: PathBuf) -> Self {
        SearchIndex {
            postings: HashMap::new(),
            documents: HashMap::new(),
            total_docs: 0,
            avg_doc_length: 0.0,
            next_doc_id: 1,
            index_dir,
        }
    }

    fn try_load(index_dir: &Path) -> Option<Self> {
        let meta_path = index_dir.join(META_FILE);
        let index_path = index_dir.join(INDEX_FILE);

        let meta_content = fs::read_to_string(&meta_path).ok()?;
        let meta: IndexMeta = serde_json::from_str(&meta_content).ok()?;

        if meta.version != INDEX_VERSION {
            return None;
        }

        let index_content = fs::read_to_string(&index_path).ok()?;
        let persisted: PersistedIndex = serde_json::from_str(&index_content).ok()?;

        Some(SearchIndex {
            postings: persisted.postings,
            documents: meta.documents,
            total_docs: meta.total_docs,
            avg_doc_length: meta.avg_doc_length,
            next_doc_id: meta.next_doc_id,
            index_dir: index_dir.to_path_buf(),
        })
    }

    pub fn full_rebuild(&mut self, note_dir: &Path) {
        self.postings.clear();
        self.documents.clear();
        self.total_docs = 0;
        self.avg_doc_length = 0.0;
        self.next_doc_id = 1;

        for entry in WalkDir::new(note_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() || !is_supported_doc_file(path) {
                continue;
            }

            let rel = path.strip_prefix(note_dir).unwrap_or(path);
            if rel.starts_with(INDEX_DIR_NAME) {
                continue;
            }

            self.index_file(note_dir, path);
        }

        self.recalculate_stats();
        self.persist();
    }

    fn incremental_update(&mut self, note_dir: &Path) {
        let mut current_files: HashMap<String, u64> = HashMap::new();

        for entry in WalkDir::new(note_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() || !is_supported_doc_file(path) {
                continue;
            }

            let rel = path.strip_prefix(note_dir).unwrap_or(path);
            if rel.starts_with(INDEX_DIR_NAME) {
                continue;
            }

            current_files.insert(normalize_rel_path(rel), file_mtime(path));
        }

        let indexed_paths: HashMap<String, (u32, u64)> = self
            .documents
            .iter()
            .map(|(&id, meta)| (meta.path.clone(), (id, meta.mtime)))
            .collect();

        let mut to_remove: Vec<u32> = Vec::new();
        let mut to_reindex: Vec<String> = Vec::new();

        for (path, &(doc_id, old_mtime)) in &indexed_paths {
            match current_files.get(path) {
                None => to_remove.push(doc_id),
                Some(&new_mtime) if new_mtime != old_mtime => {
                    to_remove.push(doc_id);
                    to_reindex.push(path.clone());
                }
                _ => {}
            }
        }

        for path in current_files.keys() {
            if !indexed_paths.contains_key(path) {
                to_reindex.push(path.clone());
            }
        }

        if to_remove.is_empty() && to_reindex.is_empty() {
            return;
        }

        for doc_id in &to_remove {
            self.remove_document(*doc_id);
        }

        for rel_path in &to_reindex {
            let full_path = note_dir.join(rel_path);
            if full_path.exists() {
                self.index_file(note_dir, &full_path);
            }
        }

        self.recalculate_stats();
        self.persist();
    }

    fn index_file(&mut self, note_dir: &Path, path: &Path) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let rel = path.strip_prefix(note_dir).unwrap_or(path);
        let rel_str = normalize_rel_path(rel);
        let ext = file_extension(path);
        let extracted = extract(path, &content);
        let tokens = tokenize_with_positions(&extracted.body);
        let doc_length = tokens.len() as u32;
        let doc_id = self.next_doc_id;
        self.next_doc_id += 1;

        self.documents.insert(
            doc_id,
            DocMeta {
                path: rel_str,
                title: extracted.title,
                format: if ext == "md" || ext == "markdown" {
                    "markdown".to_string()
                } else {
                    "html".to_string()
                },
                doc_length,
                mtime: file_mtime(path),
            },
        );

        let mut term_data: HashMap<String, (u32, Vec<u32>)> = HashMap::new();
        for (term, pos) in tokens {
            let entry = term_data.entry(term).or_insert((0, Vec::new()));
            entry.0 += 1;
            entry.1.push(pos);
        }

        for (term, (term_frequency, positions)) in term_data {
            self.postings.entry(term).or_default().push(Posting {
                doc_id,
                term_frequency,
                positions,
            });
        }
    }

    fn remove_document(&mut self, doc_id: u32) {
        self.documents.remove(&doc_id);
        for postings in self.postings.values_mut() {
            postings.retain(|p| p.doc_id != doc_id);
        }
        self.postings.retain(|_, postings| !postings.is_empty());
    }

    pub fn update_single_file(&mut self, note_dir: &Path, rel_path: &str) {
        let normalized = rel_path.replace('\\', "/");
        let doc_id = self
            .documents
            .iter()
            .find(|(_, meta)| meta.path == normalized)
            .map(|(&id, _)| id);

        if let Some(id) = doc_id {
            self.remove_document(id);
        }

        let full_path = note_dir.join(&normalized);
        if full_path.exists() && is_supported_doc_file(&full_path) {
            self.index_file(note_dir, &full_path);
        }

        self.recalculate_stats();
        self.persist();
    }

    pub fn remove_single_file(&mut self, rel_path: &str) {
        let normalized = rel_path.replace('\\', "/");
        let doc_id = self
            .documents
            .iter()
            .find(|(_, meta)| meta.path == normalized)
            .map(|(&id, _)| id);

        if let Some(id) = doc_id {
            self.remove_document(id);
            self.recalculate_stats();
            self.persist();
        }
    }

    fn recalculate_stats(&mut self) {
        self.total_docs = self.documents.len() as u32;
        if self.total_docs > 0 {
            let total_length: u64 = self.documents.values().map(|d| d.doc_length as u64).sum();
            self.avg_doc_length = total_length as f64 / self.total_docs as f64;
        } else {
            self.avg_doc_length = 0.0;
        }
    }

    fn persist(&self) {
        if fs::create_dir_all(&self.index_dir).is_err() {
            return;
        }

        let meta = IndexMeta {
            version: INDEX_VERSION,
            documents: self.documents.clone(),
            total_docs: self.total_docs,
            avg_doc_length: self.avg_doc_length,
            next_doc_id: self.next_doc_id,
        };

        let persisted = PersistedIndex {
            postings: self.postings.clone(),
        };

        let meta_path = self.index_dir.join(META_FILE);
        let index_path = self.index_dir.join(INDEX_FILE);
        let meta_tmp = self.index_dir.join("meta.tmp");
        let index_tmp = self.index_dir.join("index.tmp");

        if let Ok(json) = serde_json::to_string(&meta) {
            if fs::write(&meta_tmp, &json).is_ok() {
                let _ = fs::rename(&meta_tmp, &meta_path);
            }
        }

        if let Ok(json) = serde_json::to_string(&persisted) {
            if fs::write(&index_tmp, &json).is_ok() {
                let _ = fs::rename(&index_tmp, &index_path);
            }
        }
    }
}

fn is_supported_doc_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase()),
        Some(ext) if ext == "html" || ext == "htm" || ext == "md" || ext == "markdown"
    )
}

fn file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn file_mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::SearchIndex;
    use std::fs;

    #[test]
    fn html_index_uses_visible_text_only() {
        let note_dir =
            std::env::temp_dir().join(format!("znote_search_index_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&note_dir);
        fs::create_dir_all(&note_dir).unwrap();
        fs::write(
            note_dir.join("sample.html"),
            r#"
            <html>
              <head><style>.hidden-style { color: red; }</style></head>
              <body><h1>可见标题</h1><p>3a 算法处理</p></body>
            </html>
            "#,
        )
        .unwrap();

        let index = SearchIndex::load_or_build(&note_dir);

        assert!(index.postings.contains_key("3a"));
        assert!(index
            .postings
            .keys()
            .all(|term| !term.contains("hidden-style")));
        assert!(index.postings.keys().all(|term| !term.contains("color")));

        let _ = fs::remove_dir_all(&note_dir);
    }
}
