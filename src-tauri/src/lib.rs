use pulldown_cmark::{html, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{
    image::Image,
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use walkdir::WalkDir;

mod search;

const CONVERT_FILE_LIMIT_BYTES: u64 = 5 * 1024 * 1024;
const EDIT_FILE_LIMIT_BYTES: u64 = 10 * 1024 * 1024;
const PREVIEW_IMAGE_LIMIT_BYTES: u64 = 20 * 1024 * 1024;
const MARKDOWN_NORMALIZE_CHUNK_TARGET_CHARS: usize = 18_000;
const MARKDOWN_NORMALIZE_CHUNK_HARD_CHARS: usize = 24_000;
const HTML_SUMMARY_LIMIT_CHARS: usize = 30;

fn note_dir() -> PathBuf {
    let base_dir = dirs::document_dir()
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Documents"))
        })
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Documents")))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let dir = base_dir.join("znote");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn config_dir() -> PathBuf {
    let base_dir = dirs::config_dir()
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|home| PathBuf::from(home).join("AppData").join("Roaming"))
        })
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

fn dev_markdown_ref_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("md").join("md_ref.md"))
        .unwrap_or_else(|| PathBuf::from("md").join("md_ref.md"))
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
        exe_dir
            .join("..")
            .join("resources")
            .join("styles")
            .join(file_name),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or(dev_path)
}

fn markdown_reference_path() -> PathBuf {
    let dev_path = dev_markdown_ref_path();
    if dev_path.exists() {
        return dev_path;
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let candidates = [
        exe_dir.join("_up_").join("md").join("md_ref.md"),
        exe_dir.join("md").join("md_ref.md"),
        exe_dir.join("resources").join("md").join("md_ref.md"),
        exe_dir
            .join("..")
            .join("resources")
            .join("md")
            .join("md_ref.md"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or(dev_path)
}

fn read_markdown_reference() -> Result<String, String> {
    fs::read_to_string(markdown_reference_path())
        .map_err(|e| format!("输入文件异常：无法读取 Markdown 参考文档 md_ref.md：{e}"))
}

fn is_supported_doc_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let e = ext.to_lowercase();
        return e == "html" || e == "htm" || e == "md" || e == "markdown";
    }
    false
}

fn image_mime_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("bmp") => Some("image/bmp"),
        Some("svg") => Some("image/svg+xml"),
        _ => None,
    }
}

fn is_absolute_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
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

#[cfg(test)]
fn markdown_normalize_prompt(chunk: &str, current: usize, total: usize) -> String {
    format!(
        "你正在为 znote Pro 做 Markdown 转 HTML 前的格式规范化。\n\
这是第 {current}/{total} 段，只整理当前片段，不添加上下文总结。\n\n\
请严格遵守：\n\
1. 文件使用 UTF-8，保留原始文本内容，不摘要、不扩写、不改写事实。\n\
2. 文档最多一个一级标题 #；章节用 ##，小节用 ###/####，标题前后保留空行。\n\
3. 普通段落之间保留空行，避免把标题、图片、公式、代码块粘在同一段。\n\
4. 图片使用标准格式 ![说明](相对路径或绝对路径)，不要修改图片文件名和路径。\n\
5. 代码块必须使用 fenced code block，尽量补充语言名，例如 ```python、```bash。\n\
6. 行内公式保留 $...$ 或 \\(...\\)，块级公式保留 $$...$$ 或 \\[...\\]，公式前后留空行。\n\
7. 表格、列表、引用按 CommonMark/GFM 规范补齐空格、分隔线和缩进。\n\
8. 只输出规范化 Markdown 正文，不要输出解释、说明、寒暄、总结，不要使用外层代码围栏。\n\n\
待整理 Markdown 片段：\n{chunk}"
    )
}

fn markdown_normalize_prompt_with_reference(
    reference_content: &str,
    chunk: &str,
    current: usize,
    total: usize,
) -> String {
    format!(
        "你是 znote Pro 的 Markdown 格式规范化器。任务不是写作，也不是总结，而是把原文片段整理成参考文档的 Markdown 格式。\n\n\
当前片段：第 {current}/{total} 段。\n\n\
必须严格遵守：\n\
1. 内容零改写：不得删除、摘要、扩写、翻译、润色、改变事实、改变术语含义或重排原文逻辑。\n\
2. 只允许做格式整理：标题层级与编号、空行、段落拆分、列表缩进、表格分隔线、引用格式、代码块围栏、代码语言名、公式分隔符、图片 alt 文本。\n\
3. 图片路径零改动：保留原始图片文件名、相对路径、绝对路径和大小写；不能把图片改成外链或占位描述。\n\
4. 代码内容零改动：代码块内部字符、缩进、注释、命令、路径、变量名必须原样保留；只允许补充 fenced code block 的语言名。\n\
5. 公式内容零改动：公式内部 LaTeX 字符必须原样保留；只允许规范为 $...$、\\(...\\)、$$...$$、\\[...\\] 之一。\n\
6. 标题格式参考 md_ref.md：全文最多一个 # 主标题；主章节用 ## 编号；小节用 ###/####；标题前后保留空行。\n\
7. 输出必须是纯 Markdown 正文：禁止寒暄、解释、差异说明、总结语、JSON、HTML、XML、外层 ```markdown 代码围栏。\n\
8. 如果原文片段中没有某类元素，不要为了贴近参考文档而新增该类内容。\n\n\
参考文档 md_ref.md 的格式规范如下。只学习格式，不复制内容：\n\
<<<REFERENCE_MARKDOWN\n\
{reference_content}\n\
REFERENCE_MARKDOWN>>>\n\n\
需要规范化的原文片段如下。请仅输出该片段规范化后的 Markdown：\n\
<<<SOURCE_MARKDOWN_CHUNK\n\
{chunk}\n\
SOURCE_MARKDOWN_CHUNK>>>"
    )
}

fn markdown_temp_path(source: &Path) -> PathBuf {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    parent.join(format!("{stem}_temp.md"))
}

fn is_markdown_fence_start(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn markdown_heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let count = trimmed.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&count)
        && trimmed
            .as_bytes()
            .get(count)
            .is_some_and(|value| *value == b' ')
    {
        Some(count)
    } else {
        None
    }
}

fn markdown_image_count(content: &str) -> usize {
    content.matches("![").count()
}

fn has_unclosed_markdown_fence(content: &str) -> bool {
    let mut fence_marker: Option<&str> = None;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if fence_marker == Some("```") {
                fence_marker = None;
            } else if fence_marker.is_none() {
                fence_marker = Some("```");
            }
        } else if trimmed.starts_with("~~~") {
            if fence_marker == Some("~~~") {
                fence_marker = None;
            } else if fence_marker.is_none() {
                fence_marker = Some("~~~");
            }
        }
    }
    fence_marker.is_some()
}

fn looks_like_model_explanation(content: &str) -> bool {
    let head = content
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    let lower = head.to_lowercase();
    lower.starts_with("我来")
        || lower.starts_with("下面")
        || lower.starts_with("以下")
        || lower.starts_with("here")
        || lower.starts_with("sure")
        || lower.contains("规范化后的")
        || lower.contains("整理后的")
        || lower.contains("markdown 正文如下")
}

fn validate_normalized_markdown_chunk(original: &str, normalized: &str) -> Result<(), String> {
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return Err("模型返回异常：规范化结果为空".to_string());
    }
    if normalized.starts_with("```") && normalized.ends_with("```") {
        return Err("模型返回异常：规范化结果包含外层代码围栏".to_string());
    }
    if looks_like_model_explanation(normalized) {
        return Err("模型返回异常：规范化结果包含说明文字".to_string());
    }
    if has_unclosed_markdown_fence(normalized) {
        return Err("模型返回异常：规范化结果存在未闭合代码块".to_string());
    }
    let original_images = markdown_image_count(original);
    let normalized_images = markdown_image_count(normalized);
    if normalized_images < original_images {
        return Err("模型返回异常：规范化结果丢失图片引用".to_string());
    }
    let original_len = original.trim().chars().count();
    let normalized_len = normalized.chars().count();
    if original_len > 800 && normalized_len * 2 < original_len {
        return Err("模型返回异常：规范化结果内容异常缩短".to_string());
    }
    Ok(())
}

fn split_markdown_by_paragraph(content: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if is_markdown_fence_start(trimmed) {
            in_fence = !in_fence;
        }

        let should_flush = !in_fence
            && line.trim().is_empty()
            && !current.trim().is_empty()
            && current.chars().count() >= max_chars;
        current.push_str(line);
        if should_flush {
            chunks.push(current.trim().to_string());
            current.clear();
        }
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

fn split_markdown_by_heading_raw(content: &str, heading_level: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if is_markdown_fence_start(trimmed) {
            in_fence = !in_fence;
            current.push_str(line);
            continue;
        }

        if !in_fence
            && markdown_heading_level(line).is_some_and(|level| level == heading_level)
            && !current.trim().is_empty()
        {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        current.push_str(line);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn split_markdown_chunk(content: &str) -> Vec<String> {
    if content.chars().count() <= MARKDOWN_NORMALIZE_CHUNK_HARD_CHARS {
        return vec![content.trim().to_string()];
    }

    for heading_level in [3usize, 4usize, 5usize, 6usize] {
        let chunks = split_markdown_by_heading_raw(content, heading_level);
        if chunks.len() > 1 {
            return chunks
                .into_iter()
                .flat_map(|chunk| split_markdown_chunk(&chunk))
                .collect();
        }
    }

    split_markdown_by_paragraph(content, MARKDOWN_NORMALIZE_CHUNK_TARGET_CHARS)
}

fn split_markdown_for_normalization(content: &str) -> Vec<String> {
    let chunks = split_markdown_by_heading_raw(content, 2);
    if chunks.is_empty() {
        split_markdown_chunk(content)
    } else {
        chunks
            .into_iter()
            .flat_map(|chunk| split_markdown_chunk(&chunk))
            .collect()
    }
}

fn is_protected_html_tag(tag: &str) -> bool {
    matches!(tag, "pre" | "code" | "script" | "style")
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_lowercase().find(&needle.to_lowercase())
}

fn normalize_math_delimiters_outside_protected_tags(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut rest = html;
    let mut protected_stack: Vec<String> = Vec::new();

    while let Some(tag_start) = rest.find('<') {
        let text = &rest[..tag_start];
        if protected_stack.is_empty() {
            output.push_str(
                &text
                    .replace(r"\\[", r"\[")
                    .replace(r"\\]", r"\]")
                    .replace(r"\\(", r"\(")
                    .replace(r"\\)", r"\)"),
            );
        } else {
            output.push_str(text);
        }

        if let Some(tag_end) = rest[tag_start..].find('>') {
            let tag = &rest[tag_start..tag_start + tag_end + 1];
            let tag_body = tag
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim()
                .to_lowercase();
            let is_closing = tag_body.starts_with('/');
            let tag_name = tag_body
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/');

            if is_protected_html_tag(tag_name) {
                if is_closing {
                    if let Some(position) =
                        protected_stack.iter().rposition(|item| item == tag_name)
                    {
                        protected_stack.truncate(position);
                    }
                } else if !tag_body.ends_with('/') {
                    protected_stack.push(tag_name.to_string());
                }
            }

            output.push_str(tag);
            rest = &rest[tag_start + tag_end + 1..];
        } else {
            output.push_str(&rest[tag_start..]);
            rest = "";
            break;
        }
    }

    if protected_stack.is_empty() {
        output.push_str(
            &rest
                .replace(r"\\[", r"\[")
                .replace(r"\\]", r"\]")
                .replace(r"\\(", r"\(")
                .replace(r"\\)", r"\)"),
        );
    } else {
        output.push_str(rest);
    }

    output
}

fn html_contains_math(html: &str) -> bool {
    html.contains(r"\[")
        || html.contains(r"\(")
        || html.contains("$$")
        || html.contains("$\\")
        || html.contains("$y")
        || html.contains("$s")
        || html.contains("$n")
        || html.contains("$x")
}

fn html_has_math_renderer(html: &str) -> bool {
    let lower = html.to_lowercase();
    lower.contains("mathjax") || lower.contains("katex")
}

fn mathjax_injection() -> &'static str {
    r#"<script>
window.MathJax = {
  tex: {
    inlineMath: [['\\(', '\\)'], ['$', '$']],
    displayMath: [['\\[', '\\]'], ['$$', '$$']],
    processEscapes: true
  },
  options: {
    skipHtmlTags: ['script', 'noscript', 'style', 'textarea', 'pre', 'code']
  }
};
</script>
<script defer src="https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-chtml.js"></script>"#
}

fn inject_mathjax(html: &str) -> String {
    if let Some(head_end) = find_case_insensitive(html, "</head>") {
        let mut output = String::with_capacity(html.len() + mathjax_injection().len() + 2);
        output.push_str(&html[..head_end]);
        output.push('\n');
        output.push_str(mathjax_injection());
        output.push('\n');
        output.push_str(&html[head_end..]);
        return output;
    }

    format!("{}\n{}", mathjax_injection(), html)
}

fn postprocess_html_math(content: &str) -> String {
    let normalized = normalize_math_delimiters_outside_protected_tags(content);
    if html_contains_math(&normalized) && !html_has_math_renderer(&normalized) {
        inject_mathjax(&normalized)
    } else {
        normalized
    }
}

fn is_markdown_heading(line: &str, level: usize) -> bool {
    let trimmed = line.trim_start();
    let prefix = "#".repeat(level);
    trimmed.starts_with(&prefix)
        && trimmed
            .as_bytes()
            .get(level)
            .is_some_and(|value| *value == b' ')
}

fn markdown_title(content: &str) -> String {
    content
        .lines()
        .find_map(|line| {
            if is_markdown_heading(line, 1) {
                Some(line.trim_start().trim_start_matches('#').trim().to_string())
            } else {
                None
            }
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn markdown_summary(content: &str) -> String {
    let text = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("```")
                && !trimmed.starts_with("!")
        })
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['*', '`', '_', '[', ']', '(', ')'], "");
    let summary = text.trim();
    if summary.chars().count() > 120 {
        format!("{}...", summary.chars().take(120).collect::<String>())
    } else if summary.is_empty() {
        "Markdown 转 HTML 文档".to_string()
    } else {
        summary.to_string()
    }
}

fn html_summary_prompt(content: &str) -> String {
    format!(
        "请通读下面完整 Markdown 文档，提炼一个 30 字以内的中文摘要，用于 HTML 标题下方简介。\n\
要求：只输出摘要正文；不要输出“摘要：”；不要换行；不要使用引号、项目符号或解释。\n\n\
Markdown 文档：\n{content}"
    )
}

fn clean_html_summary(content: &str) -> String {
    let cleaned = strip_model_fence(content)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut summary = cleaned
        .trim()
        .trim_start_matches(['-', '*', '•', '・', '·', '：', ':'])
        .trim()
        .trim_matches(['"', '\'', '“', '”', '‘', '’', '「', '」', '『', '』'])
        .trim()
        .to_string();

    for prefix in ["摘要：", "摘要:", "简介：", "简介:", "总结：", "总结:"] {
        if let Some(stripped) = summary.strip_prefix(prefix) {
            summary = stripped.trim().to_string();
            break;
        }
    }

    if summary.chars().count() > HTML_SUMMARY_LIMIT_CHARS {
        summary.chars().take(HTML_SUMMARY_LIMIT_CHARS).collect()
    } else {
        summary
    }
}

fn markdown_without_top_title(content: &str) -> String {
    let mut removed = false;
    let mut output = String::new();
    for line in content.split_inclusive('\n') {
        if !removed && is_markdown_heading(line, 1) {
            removed = true;
            continue;
        }
        output.push_str(line);
    }
    if !removed && !content.ends_with('\n') {
        content.to_string()
    } else {
        output
    }
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);
    options
}

fn normalize_markdown_math_for_parser(content: &str) -> String {
    let mut output = String::new();
    let mut in_fence = false;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            output.push_str(line);
            continue;
        }

        if in_fence {
            output.push_str(line);
        } else {
            output.push_str(
                &line
                    .replace(r"\[", "$$")
                    .replace(r"\]", "$$")
                    .replace(r"\(", "$")
                    .replace(r"\)", "$"),
            );
        }
    }

    output
}

fn normalize_code_language(info: &str) -> String {
    info.split_whitespace()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>()
        .to_lowercase()
}

fn code_block_open_html(language: &str) -> String {
    let escaped_label = if language.is_empty() {
        "text".to_string()
    } else {
        html_escape(language)
    };
    let class_attr = if language.is_empty() {
        String::new()
    } else {
        format!(" class=\"language-{}\"", html_escape(language))
    };

    format!(
        "<div class=\"code-block\"><div class=\"code-header\"><div class=\"code-dots\"><span class=\"code-dot red\"></span><span class=\"code-dot yellow\"></span><span class=\"code-dot green\"></span></div><span class=\"code-label\">{escaped_label}</span></div><pre><code{class_attr}>"
    )
}

#[cfg(test)]
fn render_markdown_locally(content: &str) -> String {
    let body = markdown_without_top_title(content);
    render_markdown_fragment(&body)
}

fn render_markdown_fragment(content: &str) -> String {
    let body = normalize_markdown_math_for_parser(content);
    let parser = Parser::new_ext(&body, markdown_options());
    let mut in_code_block: Option<String> = None;
    let mut events = Vec::new();

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => normalize_code_language(info.as_ref()),
                    CodeBlockKind::Indented => String::new(),
                };
                if language == "mermaid" {
                    events.push(Event::Html(CowStr::from(
                        "<div class=\"mermaid-wrap\"><div class=\"mermaid\">\n",
                    )));
                } else {
                    events.push(Event::Html(CowStr::from(code_block_open_html(&language))));
                }
                in_code_block = Some(language);
            }
            Event::End(TagEnd::CodeBlock) => {
                let language = in_code_block.take().unwrap_or_default();
                if language == "mermaid" {
                    events.push(Event::Html(CowStr::from("\n</div></div>")));
                } else {
                    events.push(Event::Html(CowStr::from("</code></pre></div>")));
                }
            }
            Event::InlineMath(math) => {
                events.push(Event::Html(CowStr::from(format!(
                    r"\({}\)",
                    html_escape(math.as_ref())
                ))));
            }
            Event::DisplayMath(math) => {
                events.push(Event::Html(CowStr::from(format!(
                    r"\[{}\]",
                    html_escape(math.as_ref())
                ))));
            }
            other => events.push(other),
        }
    }

    let mut output = String::new();
    html::push_html(&mut output, events.into_iter());
    output
}

fn split_markdown_sections_for_html(content: &str) -> Vec<String> {
    split_markdown_by_heading_raw(&markdown_without_top_title(content), 2)
}

fn render_markdown_fragments_locally(content: &str) -> Vec<String> {
    split_markdown_sections_for_html(content)
        .into_iter()
        .map(|section| render_markdown_fragment(&section))
        .filter(|fragment| !fragment.trim().is_empty())
        .collect()
}

fn convert_markdown_to_html_local(content: &str, template: &str) -> Result<String, String> {
    let summary = markdown_summary(content);
    convert_markdown_to_html_local_with_summary(content, template, &summary)
}

fn convert_markdown_to_html_local_with_summary(
    content: &str,
    template: &str,
    summary: &str,
) -> Result<String, String> {
    if content.trim().is_empty() {
        return Err("杈撳叆鏂囦欢寮傚父锛歁arkdown 鍐呭涓虹┖".to_string());
    }

    let title = markdown_title(content);
    let fragments = render_markdown_fragments_locally(content);
    let mut html = assemble_html_document(template, &title, &summary, &fragments)?;
    html = postprocess_html_math(&html);
    validate_complete_html(&html)?;
    Ok(html)
}

fn html_escape(content: &str) -> String {
    content
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn replace_between_case_insensitive(
    html: &str,
    start_marker: &str,
    end_marker: &str,
    replacement: &str,
) -> Option<String> {
    let lower = html.to_lowercase();
    let start_lower = start_marker.to_lowercase();
    let end_lower = end_marker.to_lowercase();
    let start = lower.find(&start_lower)? + start_marker.len();
    let end = lower[start..].find(&end_lower)? + start;
    Some(format!("{}{}{}", &html[..start], replacement, &html[end..]))
}

fn replace_first_tag_inner_after(
    html: &str,
    search_from: usize,
    tag_start: &str,
    tag_end: &str,
    replacement: &str,
) -> Option<String> {
    let lower = html.to_lowercase();
    let tag_position = lower[search_from..].find(&tag_start.to_lowercase())? + search_from;
    let open_end = html[tag_position..].find('>')? + tag_position + 1;
    let close_start = lower[open_end..].find(&tag_end.to_lowercase())? + open_end;
    Some(format!(
        "{}{}{}",
        &html[..open_end],
        replacement,
        &html[close_start..]
    ))
}

fn sync_template_metadata(template: &str, title: &str, summary: &str) -> String {
    let escaped_title = html_escape(title);
    let escaped_summary = html_escape(summary);
    let mut html =
        replace_between_case_insensitive(template, "<title>", "</title>", &escaped_title)
            .unwrap_or_else(|| template.to_string());

    if let Some(header_start) = html.to_lowercase().find("<header class=\"hero\"") {
        if let Some(updated) =
            replace_first_tag_inner_after(&html, header_start, "<h1", "</h1>", &escaped_title)
        {
            html = updated;
        }
        if let Some(updated) = replace_first_tag_inner_after(
            &html,
            header_start,
            "<p class=\"hero-desc\"",
            "</p>",
            &escaped_summary,
        ) {
            html = updated;
        }
    }

    html
}

fn find_content_div_bounds(html: &str) -> Result<(usize, usize), String> {
    let lower = html.to_lowercase();
    let content_start = lower
        .find("<div class=\"content\"")
        .or_else(|| lower.find("<div class='content'"))
        .ok_or_else(|| "样式模板异常：未找到 .content 容器".to_string())?;
    let content_open_end = html[content_start..]
        .find('>')
        .map(|offset| content_start + offset + 1)
        .ok_or_else(|| "样式模板异常：content 容器不完整".to_string())?;

    let mut cursor = content_open_end;
    let mut depth = 1usize;
    while depth > 0 {
        let rest = &lower[cursor..];
        let next_open = rest.find("<div");
        let next_close = rest.find("</div");
        match (next_open, next_close) {
            (_, Some(close)) if next_open.map(|open| close < open).unwrap_or(true) => {
                let close_start = cursor + close;
                depth -= 1;
                if depth == 0 {
                    return Ok((content_open_end, close_start));
                }
                let close_end = lower[close_start..]
                    .find('>')
                    .map(|offset| close_start + offset + 1)
                    .ok_or_else(|| "样式模板异常：div 标签不完整".to_string())?;
                cursor = close_end;
            }
            (Some(open), _) => {
                let open_start = cursor + open;
                let open_end = lower[open_start..]
                    .find('>')
                    .map(|offset| open_start + offset + 1)
                    .ok_or_else(|| "样式模板异常：div 标签不完整".to_string())?;
                depth += 1;
                cursor = open_end;
            }
            _ => return Err("样式模板异常：content 容器未闭合".to_string()),
        }
    }

    Err("样式模板异常：content 容器未闭合".to_string())
}

fn join_html_fragments(fragments: &[String]) -> String {
    fragments
        .iter()
        .filter_map(|fragment| {
            let trimmed = fragment.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n<div class=\"section-divider\"></div>\n")
}

fn generated_content_css() -> &'static str {
    r#"<style id="znote-generated-content-css">
.content img {
  display: block;
  max-width: 100%;
  height: auto;
  margin: 24px auto;
  box-sizing: border-box;
}
.content p > img:only-child {
  margin-top: 28px;
  margin-bottom: 28px;
}
</style>"#
}

fn inject_generated_content_css(html: &str) -> String {
    if html.contains("znote-generated-content-css") {
        return html.to_string();
    }

    if let Some(head_end) = find_case_insensitive(html, "</head>") {
        let mut output = String::with_capacity(html.len() + generated_content_css().len() + 2);
        output.push_str(&html[..head_end]);
        output.push('\n');
        output.push_str(generated_content_css());
        output.push('\n');
        output.push_str(&html[head_end..]);
        return output;
    }

    format!("{}\n{}", generated_content_css(), html)
}

fn assemble_html_document(
    template: &str,
    title: &str,
    summary: &str,
    fragments: &[String],
) -> Result<String, String> {
    let template = sync_template_metadata(template, title, summary);
    let (content_start, content_end) = find_content_div_bounds(&template)?;
    let content = join_html_fragments(fragments);
    let html = format!(
        "{}\n{}\n{}",
        &template[..content_start],
        content,
        &template[content_end..]
    );
    let html = inject_generated_content_css(&html);
    validate_complete_html(&html)?;
    Ok(html)
}

fn validate_complete_html(html: &str) -> Result<(), String> {
    let lower = html.to_lowercase();
    if !lower.contains("<!doctype html")
        || !lower.contains("<body")
        || !lower.contains("class=\"content\"")
        || !lower.contains("</html>")
    {
        return Err("HTML 输出不完整".to_string());
    }
    Ok(())
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

fn file_size(path: &Path) -> Result<u64, String> {
    fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

fn ensure_file_size(path: &Path, limit: u64, label: &str) -> Result<(), String> {
    let size = file_size(path)?;
    if size > limit {
        return Err(format!(
            "{label}瓒呰繃澶у皬闄愬埗锛氬綋鍓?{:.2} MB锛岄檺鍒?{} MB",
            size as f64 / 1024.0 / 1024.0,
            limit / 1024 / 1024
        ));
    }
    Ok(())
}

fn ensure_content_size(content: &str, limit: u64, label: &str) -> Result<(), String> {
    let size = content.len() as u64;
    if size > limit {
        return Err(format!(
            "{label}瓒呰繃澶у皬闄愬埗锛氬綋鍓?{:.2} MB锛岄檺鍒?{} MB",
            size as f64 / 1024.0 / 1024.0,
            limit / 1024 / 1024
        ));
    }
    Ok(())
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;

        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }

    out
}

fn read_model_config() -> Result<ModelConfig, String> {
    let path = model_config_path();
    let text = fs::read_to_string(&path).map_err(|_| "请配置模型。".to_string())?;
    let config: ModelConfig =
        serde_json::from_str(&text).map_err(|_| "请配置模型。".to_string())?;
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

    if content.trim().is_empty() {
        return Err("模型返回异常：返回内容为空".to_string());
    }
    Ok(content.to_string())
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

#[derive(Debug, Clone, Serialize)]
pub struct ConvertProgress {
    pub file_name: String,
    pub current: usize,
    pub total: usize,
    pub stage: String,
}

fn style_templates() -> Vec<StyleTemplate> {
    vec![
        StyleTemplate {
            id: "github-dark".to_string(),
            name: "GitHub Dark 暗色科技".to_string(),
            description: "深色开发者文档风格".to_string(),
            css_file: "github-style.css".to_string(),
            html_file: "style-github-dark.html".to_string(),
        },
        StyleTemplate {
            id: "apple-minimal".to_string(),
            name: "Apple 极简白".to_string(),
            description: "留白充足的极简阅读风格".to_string(),
            css_file: "apple-style.css".to_string(),
            html_file: "style-apple-minimal.html".to_string(),
        },
        StyleTemplate {
            id: "google-material".to_string(),
            name: "Google Material Design".to_string(),
            description: "彩色强调与卡片式层级".to_string(),
            css_file: "material-style.css".to_string(),
            html_file: "style-google-material.html".to_string(),
        },
        StyleTemplate {
            id: "stripe-elegant".to_string(),
            name: "Stripe 商务深蓝".to_string(),
            description: "商务化深蓝排版风格".to_string(),
            css_file: "stripe-style.css".to_string(),
            html_file: "style-stripe-elegant.html".to_string(),
        },
        StyleTemplate {
            id: "notion-warm".to_string(),
            name: "Notion 温暖灰".to_string(),
            description: "温和、轻量的知识库风格".to_string(),
            css_file: "notion-style.css".to_string(),
            html_file: "style-notion-warm.html".to_string(),
        },
        StyleTemplate {
            id: "tailwind-playful".to_string(),
            name: "Tailwind 现代多彩".to_string(),
            description: "明快多彩的现代文档风格".to_string(),
            css_file: "tailwind-style.css".to_string(),
            html_file: "style-tailwind-playful.html".to_string(),
        },
        StyleTemplate {
            id: "vercel-cyber".to_string(),
            name: "Vercel 暗黑科技".to_string(),
            description: "高对比暗黑科技感风格".to_string(),
            css_file: "vercel-style.css".to_string(),
            html_file: "style-vercel-cyber.html".to_string(),
        },
        StyleTemplate {
            id: "fluent-glass".to_string(),
            name: "Microsoft Fluent Design".to_string(),
            description: "轻玻璃与微软 Fluent 视觉".to_string(),
            css_file: "fluent-style.css".to_string(),
            html_file: "style-fluent-glass.html".to_string(),
        },
        StyleTemplate {
            id: "japanese-cream".to_string(),
            name: "日系清新奶油".to_string(),
            description: "柔和奶油色阅读风格".to_string(),
            css_file: "japanese-style.css".to_string(),
            html_file: "style-japanese-cream.html".to_string(),
        },
        StyleTemplate {
            id: "newspaper-classic".to_string(),
            name: "报刊经典衬线".to_string(),
            description: "适合长文的经典报刊风格".to_string(),
            css_file: "newspaper-style.css".to_string(),
            html_file: "style-newspaper-classic.html".to_string(),
        },
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
    ensure_file_size(&full, EDIT_FILE_LIMIT_BYTES, "鏂囨。")?;
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
    ensure_content_size(&content, EDIT_FILE_LIMIT_BYTES, "鏂囨。")?;
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
        "# 鏂板缓绗旇\n\n寮€濮嬬紪杈?..\n".to_string()
    } else {
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>鏂板缓绗旇</title></head><body><h1>鏂板缓绗旇</h1><p>寮€濮嬬紪杈?..</p></body></html>"#.to_string()
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
    ensure_content_size(&content, EDIT_FILE_LIMIT_BYTES, "鏂囨。")?;
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
fn resolve_markdown_image(
    relative_path: String,
    image_src: String,
) -> Result<Option<String>, String> {
    let src = image_src.trim();
    if src.is_empty()
        || src.starts_with("data:")
        || src.starts_with("http://")
        || src.starts_with("https://")
    {
        return Ok(None);
    }

    let dir = note_dir();
    let note_rel = sanitize_relative_path(&relative_path)?;
    let note_full = dir.join(&note_rel);
    ensure_inside_note_dir(&note_full, &dir)?;

    let candidate = if is_absolute_windows_path(src) || Path::new(src).is_absolute() {
        PathBuf::from(src)
    } else {
        let note_parent = note_full
            .parent()
            .ok_or_else(|| "鏂囨。璺緞鏃犳晥".to_string())?;
        note_parent.join(src.replace('\\', "/"))
    };

    let canonical = candidate.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.is_file() {
        return Ok(None);
    }

    let mime_type = match image_mime_type(&canonical) {
        Some(mime_type) => mime_type,
        None => return Ok(None),
    };

    ensure_file_size(&canonical, PREVIEW_IMAGE_LIMIT_BYTES, "鍥剧墖")?;
    let bytes = fs::read(&canonical).map_err(|e| e.to_string())?;
    Ok(Some(format!(
        "data:{mime_type};base64,{}",
        encode_base64(&bytes)
    )))
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

async fn normalize_markdown_with_model(
    app: &tauri::AppHandle,
    config: &ModelConfig,
    source_content: &str,
    temp_path: &Path,
    file_name: &str,
) -> Result<String, String> {
    let chunks = split_markdown_for_normalization(source_content);
    if chunks.is_empty() {
        return Err("输入文件异常：Markdown 内容为空".to_string());
    }

    let reference_content = read_markdown_reference()?;
    let total = chunks.len();
    let mut normalized_chunks = Vec::with_capacity(total);
    for (index, chunk) in chunks.iter().enumerate() {
        let current = index + 1;
        let _ = app.emit(
            "convert_progress",
            ConvertProgress {
                file_name: file_name.to_string(),
                current,
                total,
                stage: "normalizing".to_string(),
            },
        );
        let prompt =
            markdown_normalize_prompt_with_reference(&reference_content, chunk, current, total);
        let raw = request_model(config, prompt).await?;
        validate_normalized_markdown_chunk(chunk, &raw)?;
        normalized_chunks.push(raw.trim().to_string());
    }

    let normalized = normalized_chunks.join("\n\n");
    ensure_content_size(&normalized, EDIT_FILE_LIMIT_BYTES, "规范化结果")?;
    fs::write(temp_path, &normalized).map_err(|e| classify_io_error(&e, true))?;
    Ok(normalized)
}

async fn summarize_markdown_for_html(
    config: &ModelConfig,
    markdown_content: &str,
) -> Result<String, String> {
    let raw = request_model(config, html_summary_prompt(markdown_content)).await?;
    let summary = clean_html_summary(&raw);
    if summary.is_empty() {
        Err("模型返回的摘要为空。".to_string())
    } else {
        Ok(summary)
    }
}

#[tauri::command]
async fn convert_note(
    app: tauri::AppHandle,
    relative_path: String,
    style_id: Option<String>,
    overwrite: bool,
) -> Result<ConvertResult, String> {
    let rel = sanitize_relative_path(&relative_path)?;
    let dir = note_dir();
    let source = dir.join(&rel);
    ensure_inside_note_dir(&source, &dir)?;
    if !source.is_file() {
        return Err("输入文件异常：文件不存在".to_string());
    }
    ensure_file_size(&source, CONVERT_FILE_LIMIT_BYTES, "转换文件")?;

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let source_content = fs::read_to_string(&source).map_err(|e| classify_io_error(&e, false))?;
    let parent = source
        .parent()
        .ok_or_else(|| "输入文件异常：路径无效".to_string())?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");

    let target_ext = if ext == "html" || ext == "htm" {
        "md"
    } else if ext == "md" || ext == "markdown" {
        "html"
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

    let mut temp_path_to_cleanup: Option<PathBuf> = None;
    let converted = if target_ext == "md" {
        let config = read_model_config()?;
        let prompt = format!(
            "请帮我基于以下html文档转换为Markdown文档，格式清晰，不要改变文本内容。\n\
只输出转换后的 Markdown 正文，不要输出解释、说明、寒暄、总结，不要使用代码围栏。\n\n{}",
            source_content
        );
        clean_model_output(&request_model(&config, prompt).await?, target_ext)
    } else {
        let config = read_model_config()?;
        let selected_id = style_id.ok_or_else(|| "输入文件异常：未选择样式".to_string())?;
        let template = style_templates()
            .into_iter()
            .find(|item| item.id == selected_id)
            .ok_or_else(|| "输入文件异常：样式不存在".to_string())?;
        let html_path = style_file_path(&template.html_file);
        let html_template =
            fs::read_to_string(&html_path).map_err(|e| classify_io_error(&e, false))?;
        let temp_path = markdown_temp_path(&source);
        ensure_target_inside_note_dir(&temp_path, &dir)?;
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.md")
            .to_string();
        let normalized =
            normalize_markdown_with_model(&app, &config, &source_content, &temp_path, &file_name)
                .await?;
        let summary = summarize_markdown_for_html(&config, &normalized).await?;
        temp_path_to_cleanup = Some(temp_path);
        convert_markdown_to_html_local_with_summary(&normalized, &html_template, &summary)?
    };

    ensure_content_size(&converted, EDIT_FILE_LIMIT_BYTES, "转换结果")?;
    fs::write(&output, converted).map_err(|e| classify_io_error(&e, true))?;
    if let Some(temp_path) = temp_path_to_cleanup {
        let _ = fs::remove_file(temp_path);
    }

    let output_rel = output
        .strip_prefix(&dir)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let _ = search::update_index_file(&dir, &output_rel);

    Ok(ConvertResult {
        output_name: output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document")
            .to_string(),
        output_path: output_rel,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .setup(|app| {
            let menu = MenuBuilder::new(app)
                .text("show", "打开 znote Pro")
                .separator()
                .text("quit", "退出")
                .build()?;
            let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            TrayIconBuilder::new()
                .tooltip("znote Pro")
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::DoubleClick { .. }
                    | TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
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
            resolve_markdown_image,
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
    use super::{
        assemble_html_document, clean_html_summary, clean_model_output,
        convert_markdown_to_html_local, ensure_content_size, find_content_div_bounds,
        html_summary_prompt, markdown_normalize_prompt, markdown_normalize_prompt_with_reference,
        markdown_temp_path, postprocess_html_math, render_markdown_locally,
        split_markdown_for_normalization, strip_model_fence, uses_deepseek_url,
        validate_normalized_markdown_chunk, CONVERT_FILE_LIMIT_BYTES, EDIT_FILE_LIMIT_BYTES,
        HTML_SUMMARY_LIMIT_CHARS,
    };
    use std::path::Path;

    const TEMPLATE: &str = r#"<!DOCTYPE html><html><head><title>Old</title></head><body><header class="hero"><h1>Old</h1><p class="hero-desc">Old desc</p></header><div class="content"><p>sample</p><div><p>nested</p></div></div><footer>f</footer></body></html>"#;

    #[test]
    fn strips_fenced_block_with_surrounding_text() {
        let raw = "Intro\n```html\n<!DOCTYPE html><html><body>ok</body></html>\n```\nOutro";
        assert_eq!(
            strip_model_fence(raw),
            "<!DOCTYPE html><html><body>ok</body></html>"
        );
    }

    #[test]
    fn html_output_keeps_only_complete_document() {
        let raw = "Notes\n```html\n<!DOCTYPE html>\n<html><body>ok</body></html>\n```\n### Summary";
        assert_eq!(
            clean_model_output(raw, "html"),
            "<!DOCTYPE html>\n<html><body>ok</body></html>"
        );
    }

    #[test]
    fn markdown_output_removes_code_fence() {
        let raw = "```markdown\n# Title\n\nBody\n```";
        assert_eq!(clean_model_output(raw, "md"), "# Title\n\nBody");
    }

    #[test]
    fn html_summary_prompt_uses_complete_markdown() {
        let prompt = html_summary_prompt("# 标题\n\n第一段\n\n第二段");
        assert!(prompt.contains("30 字以内"));
        assert!(prompt.contains("# 标题"));
        assert!(prompt.contains("第二段"));
    }

    #[test]
    fn html_summary_cleanup_removes_prefix_and_limits_length() {
        let raw = "```text\n摘要：这是一段用于标题下方展示的精炼文档摘要内容\n```";
        let summary = clean_html_summary(raw);
        assert!(!summary.starts_with("摘要"));
        assert!(summary.chars().count() <= HTML_SUMMARY_LIMIT_CHARS);
        assert_eq!(summary, "这是一段用于标题下方展示的精炼文档摘要内容");
    }

    #[test]
    fn detects_deepseek_chat_endpoint() {
        assert!(uses_deepseek_url(
            "https://api.deepseek.com/chat/completions"
        ));
    }

    #[test]
    fn edit_size_limit_rejects_oversized_content() {
        let content = "a".repeat((EDIT_FILE_LIMIT_BYTES + 1) as usize);
        assert!(ensure_content_size(&content, EDIT_FILE_LIMIT_BYTES, "文档").is_err());
    }

    #[test]
    fn convert_size_limit_is_smaller_than_edit_size_limit() {
        assert_eq!(CONVERT_FILE_LIMIT_BYTES, 5 * 1024 * 1024);
        assert_eq!(EDIT_FILE_LIMIT_BYTES, 10 * 1024 * 1024);
        assert!(CONVERT_FILE_LIMIT_BYTES < EDIT_FILE_LIMIT_BYTES);
    }

    #[test]
    fn html_math_postprocess_injects_mathjax_for_display_formula() {
        let html = "<!DOCTYPE html><html><head><title>x</title></head><body><p>\\[ y(t)=s(t)+n(t) \\]</p></body></html>";
        let processed = postprocess_html_math(html);
        assert!(processed.contains("mathjax@3"));
        assert!(processed.contains("\\[ y(t)=s(t)+n(t) \\]"));
        assert!(processed.find("mathjax@3").unwrap() < processed.find("</head>").unwrap());
    }

    #[test]
    fn html_math_postprocess_injects_mathjax_for_inline_formula() {
        let html = "<html><head></head><body><p>其中 \\(y(t)\\) 是带噪语音</p></body></html>";
        let processed = postprocess_html_math(html);
        assert!(processed.contains("mathjax@3"));
        assert!(processed.contains("\\(y(t)\\)"));
    }

    #[test]
    fn html_math_postprocess_does_not_duplicate_existing_renderer() {
        let html =
            "<html><head><script src=\"https://cdn.example/mathjax.js\"></script></head><body><p>\\(x\\)</p></body></html>";
        let processed = postprocess_html_math(html);
        assert_eq!(processed.matches("mathjax").count(), 1);
    }

    #[test]
    fn html_math_postprocess_does_not_change_code_blocks() {
        let html = "<html><head></head><body><pre><code>\\\\[not math\\\\]</code></pre><p>\\\\[x\\\\]</p></body></html>";
        let processed = postprocess_html_math(html);
        assert!(processed.contains("<pre><code>\\\\[not math\\\\]</code></pre>"));
        assert!(processed.contains("<p>\\[x\\]</p>"));
    }

    #[test]
    fn html_math_postprocess_normalizes_overescaped_delimiters() {
        let html = "<html><body><p>\\\\[ y(t)=s(t)+n(t) \\\\]</p></body></html>";
        let processed = postprocess_html_math(html);
        assert!(processed.contains("<p>\\[ y(t)=s(t)+n(t) \\]</p>"));
        assert!(processed.contains("mathjax@3"));
    }

    #[test]
    fn local_markdown_renders_basic_blocks() {
        let markdown = "# Title\n\n## Heading\n\nParagraph with **bold**.\n\n- One\n- Two\n\n> Quote\n\n| A | B |\n| - | - |\n| 1 | 2 |";
        let html = render_markdown_locally(markdown);
        assert!(!html.contains("<h1>Title</h1>"));
        assert!(html.contains("<h2>Heading</h2>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<ul>"));
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("<table>"));
    }

    #[test]
    fn local_markdown_renders_code_block_with_template_wrapper() {
        let markdown = "```rust\nfn main() { println!(\"hi\"); }\n```";
        let html = render_markdown_locally(markdown);
        assert!(html.contains("class=\"code-block\""));
        assert!(html.contains("<span class=\"code-label\">rust</span>"));
        assert!(html.contains("<code class=\"language-rust\">"));
        assert!(html.contains("fn main()"));
    }

    #[test]
    fn local_markdown_renders_mermaid_block() {
        let markdown = "```mermaid\ngraph TD\nA-->B\n```";
        let html = render_markdown_locally(markdown);
        assert!(html.contains("class=\"mermaid-wrap\""));
        assert!(html.contains("class=\"mermaid\""));
        assert!(html.contains("A--&gt;B") || html.contains("A-->B"));
    }

    #[test]
    fn local_markdown_keeps_image_path() {
        let html = render_markdown_locally("![算法分类](algorithm_taxonomy.png)");
        assert!(html.contains("src=\"algorithm_taxonomy.png\""));
        assert!(html.contains("alt=\"算法分类\""));
    }

    #[test]
    fn local_markdown_math_triggers_mathjax_after_template_assembly() {
        let markdown = "# Math Guide\n\nInline \\(x(n)\\).\n\n\\[ y(t)=s(t)+n(t) \\]";
        let html = convert_markdown_to_html_local(markdown, TEMPLATE).unwrap();
        assert!(html.contains("<title>Math Guide</title>"));
        assert!(html.contains("mathjax@3"));
        assert!(html.contains("\\(x(n)\\)"));
        assert!(html.contains("\\[ y(t)=s(t)+n(t) \\]"));
    }

    #[test]
    fn markdown_normalize_prompt_contains_required_rules() {
        let prompt = markdown_normalize_prompt("![图](a.png)\n\n```python\nprint(1)\n```", 2, 5);
        assert!(prompt.contains("第 2/5 段"));
        assert!(prompt.contains("最多一个一级标题"));
        assert!(prompt.contains("不要修改图片文件名和路径"));
        assert!(prompt.contains("fenced code block"));
        assert!(prompt.contains("只输出规范化 Markdown 正文"));
    }

    #[test]
    fn markdown_normalize_prompt_with_reference_contains_strict_rules() {
        let prompt = markdown_normalize_prompt_with_reference(
            "# 参考格式\n\n## 1. 标题\n\n正文",
            "![图](a.png)\n\n```python\nprint(1)\n```",
            2,
            5,
        );
        assert!(prompt.contains("第 2/5 段"));
        assert!(prompt.contains("内容零改写"));
        assert!(prompt.contains("图片路径零改动"));
        assert!(prompt.contains("代码内容零改动"));
        assert!(prompt.contains("公式内容零改动"));
        assert!(prompt.contains("REFERENCE_MARKDOWN"));
        assert!(prompt.contains("SOURCE_MARKDOWN_CHUNK"));
    }

    #[test]
    fn markdown_normalize_split_keeps_fenced_code_together() {
        let mut markdown = String::from("# Guide\n\nIntro\n\n## A\n\n```python\n");
        for index in 0..900 {
            markdown.push_str(&format!("print({index})\n"));
        }
        markdown.push_str("```\n\n## B\n\nDone");

        let chunks = split_markdown_for_normalization(&markdown);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().any(|chunk| chunk.contains("print(899)")));
        for chunk in chunks {
            assert!(!super::has_unclosed_markdown_fence(&chunk));
        }
    }

    #[test]
    fn normalized_markdown_validation_rejects_bad_model_output() {
        let original = "## A\n\n![图](a.png)\n\n正文内容正文内容正文内容正文内容。";
        assert!(validate_normalized_markdown_chunk(original, "").is_err());
        assert!(validate_normalized_markdown_chunk(original, "我来帮你整理：\n\n## A").is_err());
        assert!(validate_normalized_markdown_chunk(original, "```markdown\n## A\n```").is_err());
        assert!(validate_normalized_markdown_chunk(original, "## A\n\n正文内容").is_err());
        assert!(
            validate_normalized_markdown_chunk(original, "## A\n\n```python\nprint(1)").is_err()
        );
    }

    #[test]
    fn normalized_markdown_validation_accepts_clean_output() {
        let original = "## A\n\n![图](a.png)\n\n正文内容。";
        let normalized = "## A\n\n![图](a.png)\n\n正文内容。";
        assert!(validate_normalized_markdown_chunk(original, normalized).is_ok());
    }

    #[test]
    fn markdown_temp_file_uses_source_stem() {
        let path = markdown_temp_path(Path::new(r"C:\Users\Administrator\Documents\znote\a\b.md"));
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("b_temp.md")
        );
    }

    #[test]
    fn html_fragments_are_injected_into_template_content() {
        let html = assemble_html_document(
            TEMPLATE,
            "New title",
            "New summary",
            &[
                "<section><h2>A</h2></section>".to_string(),
                "<section><h2>B</h2></section>".to_string(),
            ],
        )
        .unwrap();
        assert!(html.contains("<title>New title</title>"));
        assert!(html.contains("<h1>New title</h1>"));
        assert!(html.contains("<p class=\"hero-desc\">New summary</p>"));
        assert!(html.contains("znote-generated-content-css"));
        assert!(html.contains(".content img"));
        assert!(html.contains("<section><h2>A</h2></section>"));
        assert!(html.contains("<div class=\"section-divider\"></div>"));
        assert!(!html.contains("<p>sample</p>"));
    }

    #[test]
    fn missing_content_container_is_template_error() {
        let html = "<!DOCTYPE html><html><body><main></main></body></html>";
        assert!(find_content_div_bounds(html).is_err());
    }
}
