use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod search;

fn note_dir() -> PathBuf {
    let base_dir = dirs::document_dir()
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Documents")))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Documents")))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let dir = base_dir.join("znote");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn config_dir() -> PathBuf {
    let base_dir = dirs::config_dir()
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("AppData").join("Roaming")))
        .unwrap_or_else(|| note_dir());
    let dir = base_dir.join("znote");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn model_config_path() -> PathBuf {
    config_dir().join("model_config.json")
}

fn dev_styles_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("styles"))
        .unwrap_or_else(|| PathBuf::from("styles"))
}

fn style_file_path(file_name: &str) -> PathBuf {
    let dev_path = dev_styles_dir().join(file_name);
    if dev_path.exists() {
        return dev_path;
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let candidates = [
        exe_dir.join("_up_").join("styles").join(file_name),
        exe_dir.join("styles").join(file_name),
        exe_dir.join("resources").join("styles").join(file_name),
        exe_dir.join("..").join("resources").join("styles").join(file_name),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or(dev_path)
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

fn split_stem_and_ext(file_name: &str) -> (String, String) {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document")
        .to_string();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    (stem, ext)
}

fn unique_output_path(dir: &Path, preferred: &Path) -> PathBuf {
    if !preferred.exists() {
        return preferred.to_path_buf();
    }

    let parent = preferred.parent().unwrap_or(dir);
    let file_name = preferred
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document.html");
    let (stem, ext) = split_stem_and_ext(file_name);
    for index in 1.. {
        let candidate = parent.join(format!("{stem}-{index}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    preferred.to_path_buf()
}

fn normalize_api_url(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/').to_string();
    if trimmed.ends_with("/api") {
        return format!("{trimmed}/chat");
    }
    if trimmed.ends_with("/chat/completions") {
        trimmed
    } else if trimmed.ends_with("/chat") {
        trimmed
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn uses_ollama_chat_url(url: &str) -> bool {
    url.trim_end_matches('/').ends_with("/api/chat")
}

fn uses_deepseek_url(url: &str) -> bool {
    url.contains("api.deepseek.com")
}

fn strip_model_fence(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(start) = trimmed.find("```") {
        let after_start = &trimmed[start + 3..];
        let content_start = after_start
            .find('\n')
            .map(|index| start + 3 + index + 1)
            .unwrap_or(start + 3);
        if let Some(end_offset) = trimmed[content_start..].find("```") {
            return trimmed[content_start..content_start + end_offset]
                .trim()
                .to_string();
        }
    }

    trimmed.to_string()
}

fn extract_html_document(content: &str) -> String {
    let fenced = strip_model_fence(content);
    let lower = fenced.to_lowercase();
    if let Some(start) = lower.find("<!doctype html") {
        if let Some(end) = lower.rfind("</html>") {
            return fenced[start..end + "</html>".len()].trim().to_string();
        }
        return fenced[start..].trim().to_string();
    }
    if let Some(start) = lower.find("<html") {
        if let Some(end) = lower.rfind("</html>") {
            return fenced[start..end + "</html>".len()].trim().to_string();
        }
        return fenced[start..].trim().to_string();
    }
    fenced
}

fn extract_markdown_document(content: &str) -> String {
    strip_model_fence(content)
}

fn clean_model_output(content: &str, target_ext: &str) -> String {
    if target_ext == "html" {
        extract_html_document(content)
    } else {
        extract_markdown_document(content)
    }
}

fn classify_io_error(error: &std::io::Error, write: bool) -> String {
    if write && matches!(error.kind(), std::io::ErrorKind::PermissionDenied) {
        format!("输出文件目录无权限：{error}")
    } else if write {
        format!("输出文件异常：{error}")
    } else {
        format!("输入文件异常：{error}")
    }
}

fn read_model_config() -> Result<ModelConfig, String> {
    let path = model_config_path();
    let text = fs::read_to_string(&path).map_err(|_| "请配置模型。".to_string())?;
    let config: ModelConfig = serde_json::from_str(&text).map_err(|_| "请配置模型。".to_string())?;
    if !config.is_complete() {
        return Err("请配置模型。".to_string());
    }
    Ok(config)
}

fn save_model_config_file(config: &ModelConfig) -> Result<(), String> {
    let path = model_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

async fn request_model(config: &ModelConfig, prompt: String) -> Result<String, String> {
    let url = normalize_api_url(&config.api_url);
    let is_ollama_chat = uses_ollama_chat_url(&url);
    let is_deepseek = uses_deepseek_url(&url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("API 无法连接：{e}"))?;

    let request_body = if is_ollama_chat {
        json!({
            "model": config.model.trim(),
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "stream": false
        })
    } else if is_deepseek {
        json!({
            "model": config.model.trim(),
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "thinking": { "type": "disabled" },
            "reasoning_effort": "high",
            "stream": false
        })
    } else {
        json!({
            "model": config.model.trim(),
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "temperature": 0
        })
    };

    let response = client
        .post(url)
        .bearer_auth(config.api_key.trim())
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("API 无法连接：{e}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("模型返回异常：{e}"))?;

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(format!("API 鉴权失败：{text}"));
    }
    if !status.is_success() {
        return Err(format!("API 无法连接：HTTP {status} {text}"));
    }

    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("模型返回异常：{e}"))?;
    let content = if is_ollama_chat {
        value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .ok_or_else(|| "模型返回异常：未找到 message.content".to_string())?
    } else {
        value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .ok_or_else(|| "模型返回异常：未找到 choices[0].message.content".to_string())?
    };

    let cleaned = strip_model_fence(content);
    if cleaned.trim().is_empty() {
        return Err("模型返回异常：返回内容为空".to_string());
    }
    Ok(cleaned)
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
pub struct ModelConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

impl ModelConfig {
    fn is_complete(&self) -> bool {
        !self.api_url.trim().is_empty()
            && !self.api_key.trim().is_empty()
            && !self.model.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigView {
    pub api_url: String,
    pub model: String,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub css_file: String,
    pub html_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertResult {
    pub output_path: String,
    pub output_name: String,
}

fn style_templates() -> Vec<StyleTemplate> {
    vec![
        StyleTemplate { id: "github-dark".to_string(), name: "GitHub Dark 暗色科技".to_string(), description: "深色开发者文档风格".to_string(), css_file: "github-style.css".to_string(), html_file: "style-github-dark.html".to_string() },
        StyleTemplate { id: "apple-minimal".to_string(), name: "Apple 极简白".to_string(), description: "留白充足的极简阅读风格".to_string(), css_file: "apple-style.css".to_string(), html_file: "style-apple-minimal.html".to_string() },
        StyleTemplate { id: "google-material".to_string(), name: "Google Material Design".to_string(), description: "彩色强调与卡片式层级".to_string(), css_file: "material-style.css".to_string(), html_file: "style-google-material.html".to_string() },
        StyleTemplate { id: "stripe-elegant".to_string(), name: "Stripe 商务深蓝".to_string(), description: "商务化深蓝排版风格".to_string(), css_file: "stripe-style.css".to_string(), html_file: "style-stripe-elegant.html".to_string() },
        StyleTemplate { id: "notion-warm".to_string(), name: "Notion 温暖灰".to_string(), description: "温和、轻量的知识库风格".to_string(), css_file: "notion-style.css".to_string(), html_file: "style-notion-warm.html".to_string() },
        StyleTemplate { id: "tailwind-playful".to_string(), name: "Tailwind 现代多彩".to_string(), description: "明快多彩的现代文档风格".to_string(), css_file: "tailwind-style.css".to_string(), html_file: "style-tailwind-playful.html".to_string() },
        StyleTemplate { id: "vercel-cyber".to_string(), name: "Vercel 暗黑科技".to_string(), description: "高对比暗黑科技感风格".to_string(), css_file: "vercel-style.css".to_string(), html_file: "style-vercel-cyber.html".to_string() },
        StyleTemplate { id: "fluent-glass".to_string(), name: "Microsoft Fluent Design".to_string(), description: "轻玻璃与微软 Fluent 视觉".to_string(), css_file: "fluent-style.css".to_string(), html_file: "style-fluent-glass.html".to_string() },
        StyleTemplate { id: "japanese-cream".to_string(), name: "日系清新奶油".to_string(), description: "柔和奶油色阅读风格".to_string(), css_file: "japanese-style.css".to_string(), html_file: "style-japanese-cream.html".to_string() },
        StyleTemplate { id: "newspaper-classic".to_string(), name: "报刊经典衬线".to_string(), description: "适合长文的经典报刊风格".to_string(), css_file: "newspaper-style.css".to_string(), html_file: "style-newspaper-classic.html".to_string() },
    ]
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
fn get_note_directory() -> Result<String, String> {
    Ok(note_dir().to_string_lossy().to_string())
}

#[tauri::command]
fn get_model_config() -> Result<ModelConfigView, String> {
    let path = model_config_path();
    if !path.exists() {
        return Ok(ModelConfigView {
            api_url: String::new(),
            model: String::new(),
            has_api_key: false,
        });
    }

    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let config: ModelConfig = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(ModelConfigView {
        api_url: config.api_url,
        model: config.model,
        has_api_key: !config.api_key.trim().is_empty(),
    })
}

#[tauri::command]
fn save_model_config(api_url: String, api_key: String, model: String) -> Result<(), String> {
    let current_key = read_model_config().ok().map(|config| config.api_key);
    let next = ModelConfig {
        api_url: api_url.trim().to_string(),
        api_key: if api_key.trim().is_empty() {
            current_key.unwrap_or_default()
        } else {
            api_key.trim().to_string()
        },
        model: model.trim().to_string(),
    };

    if !next.is_complete() {
        return Err("请填写 API 请求地址、API 密钥和模型名称。".to_string());
    }

    save_model_config_file(&next)
}

#[tauri::command]
async fn test_model_config(api_url: String, api_key: String, model: String) -> Result<(), String> {
    let current_key = read_model_config().ok().map(|config| config.api_key);
    let config = ModelConfig {
        api_url: api_url.trim().to_string(),
        api_key: if api_key.trim().is_empty() {
            current_key.unwrap_or_default()
        } else {
            api_key.trim().to_string()
        },
        model: model.trim().to_string(),
    };

    if !config.is_complete() {
        return Err("请填写 API 请求地址、API 密钥和模型名称。".to_string());
    }

    request_model(&config, "请回复：OK".to_string())
        .await
        .map(|_| ())
}

#[tauri::command]
fn list_style_templates() -> Result<Vec<StyleTemplate>, String> {
    Ok(style_templates())
}

#[tauri::command]
async fn convert_note(
    relative_path: String,
    style_id: Option<String>,
    overwrite: bool,
) -> Result<ConvertResult, String> {
    let config = read_model_config()?;
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let source = dir.join(&rel);
    ensure_inside_note_dir(&source, &dir)?;
    if !source.is_file() {
        return Err("输入文件异常：文件不存在".to_string());
    }

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let source_content = fs::read_to_string(&source).map_err(|e| classify_io_error(&e, false))?;
    let parent = source.parent().ok_or_else(|| "输入文件异常：路径无效".to_string())?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");

    let (prompt, target_ext) = if ext == "html" || ext == "htm" {
        (
            format!(
                "请帮我基于以下html文档转换为Markdown文档，格式清晰，不要改变文本内容。\n只输出转换后的 Markdown 正文，不要输出解释、说明、寒暄、总结，不要使用代码围栏。\n\n{}",
                source_content
            ),
            "md",
        )
    } else if ext == "md" || ext == "markdown" {
        let selected_id = style_id.ok_or_else(|| "输入文件异常：未选择样式".to_string())?;
        let template = style_templates()
            .into_iter()
            .find(|item| item.id == selected_id)
            .ok_or_else(|| "输入文件异常：样式不存在".to_string())?;
        let css_path = style_file_path(&template.css_file);
        let html_path = style_file_path(&template.html_file);
        let css = fs::read_to_string(&css_path).map_err(|e| classify_io_error(&e, false))?;
        let html = fs::read_to_string(&html_path).map_err(|e| classify_io_error(&e, false))?;
        (
            format!(
                "请帮我基于以下Markdown文档转换为html文档，格式清晰，不要改变文本内容。样式请参考如下：\n只输出完整 HTML 文档源码，从 <!DOCTYPE html> 开始，到 </html> 结束；不要输出解释、说明、寒暄、总结，不要使用代码围栏。\nCSS样式为\n{}\n参考的html格式为：\n{}\nMarkdown文档内容为：\n{}",
                css, html, source_content
            ),
            "html",
        )
    } else {
        return Err("输入文件异常：仅支持 HTML 和 Markdown 文件".to_string());
    };

    let preferred = parent.join(format!("{stem}.{target_ext}"));
    let output = if overwrite {
        preferred
    } else {
        unique_output_path(&dir, &preferred)
    };
    ensure_target_inside_note_dir(&output, &dir)?;

    let converted = clean_model_output(&request_model(&config, prompt).await?, target_ext);
    fs::write(&output, converted).map_err(|e| classify_io_error(&e, true))?;

    let output_rel = output
        .strip_prefix(&dir)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let _ = search::update_index_file(&dir, &output_rel);

    Ok(ConvertResult {
        output_name: output
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&output_rel)
            .to_string(),
        output_path: output_rel,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            list_notes,
            read_note,
            save_note,
            delete_note,
            delete_folder,
            create_note,
            create_folder,
            rename_entry,
            import_note_content,
            search_notes,
            rebuild_search_index,
            get_note_directory,
            get_model_config,
            save_model_config,
            test_model_config,
            list_style_templates,
            convert_note,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{clean_model_output, strip_model_fence, uses_deepseek_url};

    #[test]
    fn strips_fenced_block_with_surrounding_text() {
        let raw = "我来帮你转换。\n```html\n<!DOCTYPE html><html><body>ok</body></html>\n```\n后续说明";
        assert_eq!(
            strip_model_fence(raw),
            "<!DOCTYPE html><html><body>ok</body></html>"
        );
    }

    #[test]
    fn html_output_keeps_only_complete_document() {
        let raw = "说明文字\n```html\n<!DOCTYPE html>\n<html><body>ok</body></html>\n```\n### 总结";
        assert_eq!(
            clean_model_output(raw, "html"),
            "<!DOCTYPE html>\n<html><body>ok</body></html>"
        );
    }

    #[test]
    fn markdown_output_removes_code_fence() {
        let raw = "```markdown\n# 标题\n\n正文\n```";
        assert_eq!(clean_model_output(raw, "md"), "# 标题\n\n正文");
    }

    #[test]
    fn detects_deepseek_chat_endpoint() {
        assert!(uses_deepseek_url("https://api.deepseek.com/chat/completions"));
    }
}
