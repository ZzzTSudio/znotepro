import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bold, Code, Eye, FileCode, Heading1, Heading2, Italic, List, Type } from "lucide-react";
import "katex/dist/katex.min.css";
import type { Tab } from "@/types/note";

function escapeHtml(content: string) {
  return content.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function isLocalImageSrc(src: string) {
  const value = src.trim();
  return !!value && !/^(data:|https?:|mailto:|#)/i.test(value);
}

async function resolveMarkdownImages(html: string, notePath?: string) {
  if (!notePath || !html.includes("<img")) return html;

  const wrapper = document.createElement("div");
  wrapper.innerHTML = html;
  const images = Array.from(wrapper.querySelectorAll("img[src]"));
  await Promise.all(
    images.map(async (image) => {
      const src = image.getAttribute("src") ?? "";
      if (!isLocalImageSrc(src)) return;

      try {
        const resolved = await invoke<string | null>("resolve_markdown_image", {
          relativePath: notePath,
          imageSrc: src,
        });
        if (resolved) {
          image.setAttribute("src", resolved);
        }
      } catch {
        image.setAttribute("data-missing-src", src);
      }
    }),
  );

  return wrapper.innerHTML;
}

async function resolveHtmlPreviewImages(html: string, notePath?: string) {
  if (!notePath || !html.includes("<img")) return html;

  const parser = new DOMParser();
  const document = parser.parseFromString(html, "text/html");
  const images = Array.from(document.querySelectorAll("img[src]"));
  await Promise.all(
    images.map(async (image) => {
      const src = image.getAttribute("src") ?? "";
      if (!isLocalImageSrc(src)) return;

      try {
        const resolved = await invoke<string | null>("resolve_markdown_image", {
          relativePath: notePath,
          imageSrc: src,
        });
        if (resolved) {
          image.setAttribute("src", resolved);
        }
      } catch {
        image.setAttribute("data-missing-src", src);
      }
    }),
  );

  const doctype = /^\s*<!doctype html>/i.test(html) ? "<!DOCTYPE html>\n" : "";
  return `${doctype}${document.documentElement.outerHTML}`;
}

async function renderMarkdownToHtml(content: string, notePath?: string) {
  const [{ unified }, remarkParse, remarkGfm, remarkMath, remarkRehype, rehypeKatex, rehypeHighlight, rehypeStringify, sanitizeHtml] =
    await Promise.all([
    import("unified"),
    import("remark-parse"),
    import("remark-gfm"),
    import("remark-math"),
    import("remark-rehype"),
    import("rehype-katex"),
    import("rehype-highlight"),
    import("rehype-stringify"),
    import("sanitize-html"),
  ]);
  const processor = unified()
    .use(remarkParse.default)
    .use(remarkGfm.default)
    .use(remarkMath.default)
    .use(remarkRehype.default)
    .use(rehypeKatex.default)
    .use(rehypeHighlight.default, { detect: true, ignoreMissing: true })
    .use(rehypeStringify.default);
  const rendered = await resolveMarkdownImages(processor.processSync(content).toString(), notePath);

  return sanitizeHtml.default(rendered, {
    allowedTags: sanitizeHtml.default.defaults.allowedTags.concat([
      "img",
      "h1",
      "h2",
      "h3",
      "h4",
      "h5",
      "h6",
      "table",
      "thead",
      "tbody",
      "tr",
      "th",
      "td",
      "pre",
      "code",
      "span",
      "math",
      "semantics",
      "mrow",
      "mi",
      "mn",
      "mo",
      "msup",
      "msub",
      "msubsup",
      "mfrac",
      "msqrt",
      "mroot",
      "mtext",
      "mtable",
      "mtr",
      "mtd",
      "annotation",
    ]),
    allowedAttributes: {
      ...sanitizeHtml.default.defaults.allowedAttributes,
      a: ["href", "name", "target", "rel", "title"],
      img: ["src", "alt", "title", "width", "height", "data-missing-src"],
      pre: ["class"],
      code: ["class"],
      span: ["class", "style"],
      math: ["xmlns", "display"],
      annotation: ["encoding"],
      th: ["align"],
      td: ["align"],
    },
    allowedSchemes: ["http", "https", "mailto", "data"],
    allowedSchemesByTag: {
      img: ["http", "https", "data"],
    },
    transformTags: {
      a: sanitizeHtml.default.simpleTransform("a", { rel: "noreferrer", target: "_blank" }),
    },
  });
}

function MarkdownRenderedView({
  content,
  notePath,
  className,
}: {
  content: string;
  notePath: string;
  className: string;
}) {
  const [html, setHtml] = useState("");

  useEffect(() => {
    let cancelled = false;

    renderMarkdownToHtml(content, notePath)
      .then((safeHtml) => {
        if (!cancelled) setHtml(safeHtml);
      })
      .catch(() => {
        if (!cancelled) setHtml(`<pre>${escapeHtml(content)}</pre>`);
      });

    return () => {
      cancelled = true;
    };
  }, [content, notePath]);

  return (
    <article
      className={className}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

function HtmlPreviewFrame({ content, notePath, title }: { content: string; notePath: string; title: string }) {
  const [html, setHtml] = useState(content);

  useEffect(() => {
    let cancelled = false;

    resolveHtmlPreviewImages(content, notePath)
      .then((resolvedHtml) => {
        if (!cancelled) setHtml(resolvedHtml);
      })
      .catch(() => {
        if (!cancelled) setHtml(content);
      });

    return () => {
      cancelled = true;
    };
  }, [content, notePath]);

  return <iframe key={html} title={title} srcDoc={html} className="block w-full h-full border-0 bg-white" />;
}

async function htmlToMarkdown(html: string) {
  const TurndownService = (await import("turndown")).default;
  const service = new TurndownService({
    bulletListMarker: "-",
    codeBlockStyle: "fenced",
    emDelimiter: "*",
    headingStyle: "atx",
  });

  service.addRule("katex", {
    filter: (node) => node instanceof HTMLElement && node.classList.contains("katex"),
    replacement: (_content, node) => {
      if (!(node instanceof HTMLElement)) return "";
      const annotation = node.querySelector('annotation[encoding="application/x-tex"]');
      const tex = annotation?.textContent?.trim();
      if (!tex) return node.textContent ?? "";
      const display = node.closest(".katex-display");
      return display ? `\n\n$$\n${tex}\n$$\n\n` : `$${tex}$`;
    },
  });

  service.addRule("katexDisplay", {
    filter: (node) => node instanceof HTMLElement && node.classList.contains("katex-display"),
    replacement: (content) => content,
  });

  service.addRule("highlightedCode", {
    filter: (node) => node.nodeName === "PRE" && node.firstChild?.nodeName === "CODE",
    replacement: (_content, node) => {
      if (!(node instanceof HTMLElement)) return "";
      const code = node.querySelector("code");
      if (!code) return "";
      const language = Array.from(code.classList)
        .find((name) => name.startsWith("language-"))
        ?.replace(/^language-/, "");
      const text = code.textContent?.replace(/\n$/, "") ?? "";
      return `\n\n\`\`\`${language ?? ""}\n${text}\n\`\`\`\n\n`;
    },
  });

  const markdown = service.turndown(html).replace(/[ \t]+\n/g, "\n").trimEnd();
  return markdown ? `${markdown}\n` : "";
}

function MarkdownRichEditor({
  content,
  notePath,
  onChange,
  onSave,
}: {
  content: string;
  notePath: string;
  onChange: (content: string) => void;
  onSave: () => void;
}) {
  const ref = useRef<HTMLElement>(null);
  const lastRenderedMarkdown = useRef<string | null>(null);
  const suppressInput = useRef(false);

  useEffect(() => {
    let cancelled = false;

    renderMarkdownToHtml(content, notePath)
      .then((safeHtml) => {
        if (cancelled || !ref.current || content === lastRenderedMarkdown.current) return;
        suppressInput.current = true;
        ref.current.innerHTML = safeHtml;
        lastRenderedMarkdown.current = content;
        requestAnimationFrame(() => {
          suppressInput.current = false;
        });
      })
      .catch(() => {
        if (cancelled || !ref.current) return;
        suppressInput.current = true;
        ref.current.innerHTML = `<pre>${escapeHtml(content)}</pre>`;
        lastRenderedMarkdown.current = content;
        requestAnimationFrame(() => {
          suppressInput.current = false;
        });
      });

    return () => {
      cancelled = true;
    };
  }, [content, notePath]);

  const emitChange = () => {
    if (suppressInput.current || !ref.current) return;
    const html = ref.current.innerHTML;
    htmlToMarkdown(html).then((markdown) => {
      lastRenderedMarkdown.current = markdown;
      onChange(markdown);
    });
  };

  return (
    <article
      ref={ref}
      className="markdown-rich-editor"
      contentEditable
      suppressContentEditableWarning
      onInput={emitChange}
      onBlur={emitChange}
      onKeyDown={(event) => {
        if (event.ctrlKey && event.key.toLowerCase() === "s") {
          event.preventDefault();
          emitChange();
          onSave();
        }
      }}
    />
  );
}

export default function Editor({
  tab,
  onChange,
  onSave,
}: {
  tab: Tab;
  onChange: (content: string) => void;
  onSave: () => void;
}) {
  const [mode, setMode] = useState<"rich" | "source" | "preview">(() => (tab.format === "html" || tab.format === "markdown" ? "preview" : "source"));
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (mode === "rich" && tab.format === "html" && ref.current && ref.current.innerHTML !== tab.content) {
      ref.current.innerHTML = tab.content || "<p><br></p>";
    }
  }, [tab.content, mode, tab.format]);

  const exec = (command: string, value?: string) => {
    document.execCommand(command, false, value);
    if (ref.current) onChange(ref.current.innerHTML);
  };

  const modeButton = (target: "rich" | "source" | "preview", title: string, icon: ReactNode) => (
    <button
      onClick={() => setMode(target)}
      className={`p-1 rounded text-[#24292f] hover:bg-[#eaeef2] ${mode === target ? "bg-[#dbeafe]" : ""}`}
      title={title}
    >
      {icon}
    </button>
  );

  const saveOnShortcut = (e: KeyboardEvent) => {
    if (e.ctrlKey && e.key.toLowerCase() === "s") {
      e.preventDefault();
      onSave();
    }
  };

  return (
    <div className="h-full flex flex-col">
      <div className="h-8 flex items-center gap-1 px-2 border-b border-[#d8dee4] bg-[#f6f8fa]">
        {tab.format === "html" && modeButton("preview", "Preview", <Eye size={14} />)}
        {tab.format === "markdown" && modeButton("preview", "Preview", <Eye size={14} />)}
        {modeButton("source", "Source", <FileCode size={14} />)}
        {(tab.format === "html" || tab.format === "markdown") && modeButton("rich", "Rich text", <Type size={14} />)}
        {mode === "rich" && tab.format === "html" && (
          <>
            <div className="w-px h-4 mx-1 bg-[#d8dee4]" />
            <button onClick={() => exec("bold")} className="p-1 rounded text-[#24292f] hover:bg-[#eaeef2]" title="Bold">
              <Bold size={14} />
            </button>
            <button onClick={() => exec("italic")} className="p-1 rounded text-[#24292f] hover:bg-[#eaeef2]" title="Italic">
              <Italic size={14} />
            </button>
            <button onClick={() => exec("formatBlock", "H1")} className="p-1 rounded text-[#24292f] hover:bg-[#eaeef2]" title="Heading 1">
              <Heading1 size={14} />
            </button>
            <button onClick={() => exec("formatBlock", "H2")} className="p-1 rounded text-[#24292f] hover:bg-[#eaeef2]" title="Heading 2">
              <Heading2 size={14} />
            </button>
            <button onClick={() => exec("insertUnorderedList")} className="p-1 rounded text-[#24292f] hover:bg-[#eaeef2]" title="List">
              <List size={14} />
            </button>
            <button onClick={() => exec("formatBlock", "PRE")} className="p-1 rounded text-[#24292f] hover:bg-[#eaeef2]" title="Code block">
              <Code size={14} />
            </button>
          </>
        )}
      </div>

      <div className="flex-1 overflow-auto bg-white text-[#24292f]">
        {mode === "rich" && tab.format === "html" && (
          <div
            ref={ref}
            contentEditable
            className="min-h-full outline-none p-4 leading-relaxed"
            onInput={() => ref.current && onChange(ref.current.innerHTML)}
            onKeyDown={saveOnShortcut}
          />
        )}
        {mode === "rich" && tab.format === "markdown" && (
          <MarkdownRichEditor content={tab.content} notePath={tab.path} onChange={onChange} onSave={onSave} />
        )}
        {mode === "source" && (
          <textarea
            className="w-full h-full p-4 font-mono text-sm outline-none resize-none bg-white text-[#24292f]"
            value={tab.content}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={saveOnShortcut}
            spellCheck={false}
          />
        )}
        {mode === "preview" && tab.format === "html" && (
          <HtmlPreviewFrame content={tab.content} notePath={tab.path} title={tab.title} />
        )}
        {mode === "preview" && tab.format === "markdown" && <MarkdownRenderedView content={tab.content} notePath={tab.path} className="markdown-preview" />}
      </div>
    </div>
  );
}
