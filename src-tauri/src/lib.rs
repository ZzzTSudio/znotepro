use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod search;

fn note_dir() -> PathBuf {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let dir = app_dir.join("Doc");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn is_supported_doc_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let e = ext.to_lowercase();
        return e == "html" || e == "htm" || e == "md" || e == "markdown";
    }
    false
}

fn sanitize_relative_path(input: &str) -> Result<String, String> {
    let trimmed = input.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("Empty path".to_string());
    }
    if trimmed.starts_with('/') || trimmed.contains("..") {
        return Err("Invalid path".to_string());
    }
    Ok(trimmed)
}

fn ensure_inside_note_dir(path: &Path, dir: &Path) -> Result<(), String> {
    let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let canonical_path = path.canonicalize().map_err(|e| e.to_string())?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err("Path traversal detected".to_string());
    }
    Ok(())
}

fn ensure_target_inside_note_dir(path: &Path, dir: &Path) -> Result<(), String> {
    let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let parent = path.parent().ok_or_else(|| "Invalid path".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let canonical_parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if !canonical_parent.starts_with(&canonical_dir) {
        return Err("Path traversal detected".to_string());
    }
    Ok(())
}

fn sanitize_import_file_name(input: &str) -> Result<String, String> {
    let name = Path::new(input)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid file name".to_string())?
        .trim()
        .to_string();

    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("Invalid file name".to_string());
    }

    if !is_supported_doc_file(Path::new(&name)) {
        return Err("Unsupported file type".to_string());
    }

    Ok(name)
}

fn unique_import_name(dir: &Path, file_name: &str) -> String {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    for index in 1.. {
        let candidate = if ext.is_empty() {
            format!("{stem}-{index}")
        } else {
            format!("{stem}-{index}.{ext}")
        };
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }

    file_name.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteInfo {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub children: Option<Vec<NoteInfo>>,
    pub mtime: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentContent {
    pub content: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub path: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file: String,
    pub score: f64,
    pub title: String,
    pub matches: Vec<MatchContext>,
    pub boost_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchContext {
    pub line_number: usize,
    pub line_text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

#[tauri::command]
fn list_notes() -> Result<Vec<NoteInfo>, String> {
    let dir = note_dir();
    let mut notes = Vec::new();
    for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p == dir {
            continue;
        }
        let rel = p.strip_prefix(&dir).map_err(|e| e.to_string())?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str == ".search_index" || rel_str.starts_with(".search_index/") {
            continue;
        }
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_str.clone());
        let is_dir = p.is_dir();
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            });
        if is_dir || is_supported_doc_file(p) {
            notes.push(NoteInfo {
                path: rel_str,
                name,
                is_dir,
                children: if is_dir { Some(Vec::new()) } else { None },
                mtime,
            });
        }
    }
    Ok(notes)
}

#[tauri::command]
fn read_note(relative_path: String) -> Result<DocumentContent, String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let full = dir.join(&rel);
    ensure_inside_note_dir(&full, &dir)?;
    let content = fs::read_to_string(&full).map_err(|e| e.to_string())?;
    let ext = full
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let format = if ext == "md" || ext == "markdown" {
        "markdown"
    } else {
        "html"
    };
    Ok(DocumentContent {
        content,
        format: format.to_string(),
    })
}

#[tauri::command]
fn save_note(relative_path: String, content: String) -> Result<(), String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let full = dir.join(&rel);
    ensure_target_inside_note_dir(&full, &dir)?;
    fs::write(&full, content).map_err(|e| e.to_string())?;
    let _ = search::update_index_file(&dir, &rel);
    Ok(())
}

#[tauri::command]
fn delete_note(relative_path: String) -> Result<(), String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let full = dir.join(&rel);
    ensure_inside_note_dir(&full, &dir)?;
    fs::remove_file(&full).map_err(|e| e.to_string())?;
    let _ = search::remove_index_file(&dir, &rel);
    Ok(())
}

#[tauri::command]
fn delete_folder(relative_path: String) -> Result<(), String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let full = dir.join(&rel);
    ensure_inside_note_dir(&full, &dir)?;
    if !full.is_dir() {
        return Err("Not a folder".to_string());
    }
    fs::remove_dir_all(&full).map_err(|e| e.to_string())?;
    let _ = search::rebuild_index(&dir);
    Ok(())
}

#[tauri::command]
fn create_note(relative_path: String) -> Result<(), String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let full = dir.join(&rel);
    if !is_supported_doc_file(&full) {
        return Err("Unsupported file type".to_string());
    }
    ensure_target_inside_note_dir(&full, &dir)?;
    if full.exists() {
        return Err("Path already exists".to_string());
    }

    let ext = full
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let default_content = if ext == "md" || ext == "markdown" {
        "# 新建笔记\n\n开始编辑...\n".to_string()
    } else {
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>新建笔记</title></head><body><h1>新建笔记</h1><p>开始编辑...</p></body></html>"#.to_string()
    };
    fs::write(&full, default_content).map_err(|e| e.to_string())?;
    let _ = search::update_index_file(&dir, &rel);
    Ok(())
}

#[tauri::command]
fn create_folder(relative_path: String) -> Result<(), String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let full = dir.join(&rel);
    ensure_target_inside_note_dir(&full, &dir)?;
    if full.exists() {
        return Err("Path already exists".to_string());
    }
    fs::create_dir_all(&full).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_entry(old_path: String, new_path: String) -> Result<(), String> {
    let old_rel = sanitize_relative_path(&old_path)?;
    let new_rel = sanitize_relative_path(&new_path)?;
    let dir = note_dir();
    let old_full = dir.join(&old_rel);
    let new_full = dir.join(&new_rel);
    ensure_inside_note_dir(&old_full, &dir)?;
    ensure_target_inside_note_dir(&new_full, &dir)?;
    if new_full.exists() {
        return Err("Target already exists".to_string());
    }

    let old_is_file = old_full.is_file();
    fs::rename(&old_full, &new_full).map_err(|e| e.to_string())?;

    if old_is_file {
        let _ = search::remove_index_file(&dir, &old_rel);
        let _ = search::update_index_file(&dir, &new_rel);
    } else {
        let _ = search::rebuild_index(&dir);
    }

    Ok(())
}

#[tauri::command]
fn import_note(source_path: String) -> Result<Vec<ImportResult>, String> {
    let src = PathBuf::from(source_path);
    let dir = note_dir();
    let mut results = Vec::new();
    if src.is_file() {
        if is_supported_doc_file(&src) {
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let dest = dir.join(&name);
            match fs::copy(&src, &dest) {
                Ok(_) => {
                    let rel = name;
                    let _ = search::update_index_file(&dir, &rel);
                    results.push(ImportResult {
                        path: rel,
                        success: true,
                    });
                }
                Err(_) => results.push(ImportResult {
                    path: name,
                    success: false,
                }),
            }
        }
    } else if src.is_dir() {
        for entry in WalkDir::new(&src).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() && is_supported_doc_file(p) {
                let rel = p.strip_prefix(&src).map_err(|e| e.to_string())?;
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let dest = dir.join(&rel_str);
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::copy(p, &dest).is_ok() {
                    let _ = search::update_index_file(&dir, &rel_str);
                    results.push(ImportResult {
                        path: rel_str,
                        success: true,
                    });
                } else {
                    results.push(ImportResult {
                        path: rel_str,
                        success: false,
                    });
                }
            }
        }
    }
    Ok(results)
}

#[tauri::command]
fn import_note_content(
    file_name: String,
    content: String,
    overwrite: bool,
) -> Result<ImportResult, String> {
    let dir = note_dir();
    let clean_name = sanitize_import_file_name(&file_name)?;
    let final_name = if !overwrite && dir.join(&clean_name).exists() {
        unique_import_name(&dir, &clean_name)
    } else {
        clean_name
    };
    let dest = dir.join(&final_name);
    fs::write(&dest, content).map_err(|e| e.to_string())?;
    let _ = search::update_index_file(&dir, &final_name);
    Ok(ImportResult {
        path: final_name,
        success: true,
    })
}

#[tauri::command]
fn search_notes(query: String) -> Result<Vec<SearchResult>, String> {
    let dir = note_dir();
    search::perform_search(&dir, &query)
}

#[tauri::command]
fn rebuild_search_index() -> Result<(), String> {
    let dir = note_dir();
    search::rebuild_index(&dir).map_err(|e| e)
}

#[tauri::command]
fn show_note(relative_path: String) -> Result<String, String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let full = dir.join(&rel);
    ensure_inside_note_dir(&full, &dir)?;
    fs::read_to_string(&full).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_note_directory() -> Result<String, String> {
    Ok(note_dir().to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_cli::init())
        .invoke_handler(tauri::generate_handler![
            list_notes,
            read_note,
            save_note,
            delete_note,
            delete_folder,
            create_note,
            create_folder,
            rename_entry,
            import_note,
            import_note_content,
            search_notes,
            rebuild_search_index,
            show_note,
            get_note_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
