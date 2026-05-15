use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use scraper::{ElementRef, Html, Selector};
use std::path::Path;

pub struct ExtractedDoc {
    pub title: String,
    pub body: String,
}

pub fn extract(path: &Path, content: &str) -> ExtractedDoc {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "md" || ext == "markdown" {
        extract_markdown(content)
    } else {
        extract_html(content)
    }
}

pub fn extract_search_text(path: &Path, content: &str) -> String {
    extract(path, content).body
}

fn extract_html(content: &str) -> ExtractedDoc {
    let document = Html::parse_document(content);
    let mut title = select_text(&document, "title")
        .or_else(|| select_text(&document, "h1"))
        .unwrap_or_default();

    title = title.trim().to_string();
    let body = select_visible_text(&document, "body")
        .or_else(|| select_visible_text(&document, "html"))
        .unwrap_or_default();

    ExtractedDoc { title, body }
}

fn select_text(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .next()
        .map(|el| el.text().collect::<Vec<_>>().join(" "))
}

fn select_visible_text(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document.select(&selector).next().map(collect_visible_text)
}

fn collect_visible_text(root: ElementRef<'_>) -> String {
    let mut out = String::new();

    for node in root.descendants() {
        if let Some(text) = node.value().as_text() {
            if node
                .ancestors()
                .filter_map(ElementRef::wrap)
                .any(|el| is_non_content_element(el.value().name()))
            {
                continue;
            }

            let chunk = normalize_text_chunk(text);
            if !chunk.is_empty() {
                out.push_str(&chunk);
                out.push('\n');
            }
        }
    }

    out
}

fn normalize_text_chunk(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_non_content_element(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "head" | "meta" | "link" | "noscript" | "template" | "svg" | "canvas"
    )
}

fn extract_markdown(content: &str) -> ExtractedDoc {
    let parser = Parser::new(content);
    let mut title = String::new();
    let mut body = String::new();
    let mut in_heading = false;
    let mut heading_is_h1 = false;
    let mut heading_buf = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                heading_is_h1 = level == HeadingLevel::H1;
                heading_buf.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                if title.is_empty() && heading_is_h1 {
                    title = heading_buf.trim().to_string();
                }
                body.push_str(&heading_buf);
                body.push('\n');
                heading_buf.clear();
            }
            Event::Text(text) => {
                if in_heading {
                    heading_buf.push_str(&text);
                } else {
                    body.push_str(&text);
                    body.push(' ');
                }
            }
            Event::Code(code) => {
                if in_heading {
                    heading_buf.push_str(&code);
                } else {
                    body.push_str(&code);
                    body.push(' ');
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_heading {
                    heading_buf.push(' ');
                } else {
                    body.push('\n');
                }
            }
            _ => {}
        }
    }

    if title.is_empty() {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("# ") {
                title = rest.trim().to_string();
                break;
            }
        }
    }

    ExtractedDoc { title, body }
}

#[cfg(test)]
mod tests {
    use super::{extract, extract_search_text};
    use std::path::Path;

    #[test]
    fn html_search_text_ignores_non_content_nodes() {
        let html = r#"
            <!doctype html>
            <html>
              <head>
                <title>Sample</title>
                <style>.card { background: black; color: white; }</style>
                <script>const token = "hidden";</script>
              </head>
              <body>
                <h1>Visible Title</h1>
                <style>.inline { display: none; }</style>
                <p>Hello searchable text.</p>
              </body>
            </html>
        "#;

        let extracted = extract(Path::new("sample.html"), html);
        assert_eq!(extracted.title, "Sample");
        assert!(extracted.body.contains("Visible Title"));
        assert!(extracted.body.contains("Hello searchable text."));
        assert!(!extracted.body.contains("background"));
        assert!(!extracted.body.contains("display"));
        assert!(!extracted.body.contains("hidden"));
    }

    #[test]
    fn shared_search_text_uses_visible_html_body() {
        let html = r#"<html><body><style>.x{color:red}</style><p>Only this text</p></body></html>"#;
        let searchable = extract_search_text(Path::new("sample.html"), html);

        assert!(searchable.contains("Only this text"));
        assert!(!searchable.contains("color"));
    }
}
