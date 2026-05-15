import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { Mark, Node as TiptapNode, mergeAttributes } from "@tiptap/core";
import type { Editor as TiptapEditor, Extensions, JSONContent } from "@tiptap/core";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Bold, Code, Eye, FileCode, Heading1, Heading2, Italic, List, Type } from "lucide-react";
import type { Tab } from "@/types/note";

const MarkdownLink = Mark.create({
  name: "link",
  priority: 1000,
  inclusive: false,
  addAttributes() {
    return {
      href: {
        default: null,
        parseHTML: (element: HTMLElement) => element.getAttribute("href"),
        renderHTML: (attributes: Record<string, string | null>) => (attributes.href ? { href: attributes.href } : {}),
      },
      title: {
        default: null,
        parseHTML: (element: HTMLElement) => element.getAttribute("title"),
        renderHTML: (attributes: Record<string, string | null>) => (attributes.title ? { title: attributes.title } : {}),
      },
    };
  },
  parseHTML() {
    return [{ tag: "a[href]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["a", mergeAttributes({ rel: "noreferrer", target: "_blank" }, HTMLAttributes), 0];
  },
});

const MarkdownImage = TiptapNode.create({
  name: "image",
  inline: true,
  group: "inline",
  draggable: true,
  addAttributes() {
    return {
      src: {
        default: null,
        parseHTML: (element: HTMLElement) => element.getAttribute("src"),
      },
      alt: {
        default: null,
        parseHTML: (element: HTMLElement) => element.getAttribute("alt"),
      },
      title: {
        default: null,
        parseHTML: (element: HTMLElement) => element.getAttribute("title"),
      },
    };
  },
  parseHTML() {
    return [{ tag: "img[src]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["img", mergeAttributes(HTMLAttributes)];
  },
});

const MarkdownTable = TiptapNode.create({
  name: "table",
  group: "block",
  content: "tableRow+",
  isolating: true,
  parseHTML() {
    return [{ tag: "table" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["table", mergeAttributes(HTMLAttributes), ["tbody", 0]];
  },
});

const MarkdownTableRow = TiptapNode.create({
  name: "tableRow",
  content: "(tableHeader | tableCell)*",
  parseHTML() {
    return [{ tag: "tr" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["tr", mergeAttributes(HTMLAttributes), 0];
  },
});

const MarkdownTableCell = TiptapNode.create({
  name: "tableCell",
  content: "block+",
  parseHTML() {
    return [{ tag: "td" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["td", mergeAttributes(HTMLAttributes), 0];
  },
});

const MarkdownTableHeader = TiptapNode.create({
  name: "tableHeader",
  content: "block+",
  parseHTML() {
    return [{ tag: "th" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["th", mergeAttributes(HTMLAttributes), 0];
  },
});

const markdownRichExtensions: Extensions = [
  StarterKit.configure({
    heading: {
      levels: [1, 2, 3, 4, 5, 6],
    },
  }),
  MarkdownLink,
  MarkdownImage,
  MarkdownTable,
  MarkdownTableRow,
  MarkdownTableCell,
  MarkdownTableHeader,
];

function escapeHtml(content: string) {
  return content.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

async function renderMarkdownToHtml(content: string) {
  const [{ unified }, remarkParse, remarkGfm, remarkRehype, rehypeStringify, sanitizeHtml] = await Promise.all([
    import("unified"),
    import("remark-parse"),
    import("remark-gfm"),
    import("remark-rehype"),
    import("rehype-stringify"),
    import("sanitize-html"),
  ]);
  const processor = unified()
    .use(remarkParse.default)
    .use(remarkGfm.default)
    .use(remarkRehype.default)
    .use(rehypeStringify.default);
  const rendered = processor.processSync(content).toString();

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
    ]),
    allowedAttributes: {
      ...sanitizeHtml.default.defaults.allowedAttributes,
      a: ["href", "name", "target", "rel", "title"],
      img: ["src", "alt", "title", "width", "height"],
      code: ["class"],
      th: ["align"],
      td: ["align"],
    },
    allowedSchemes: ["http", "https", "mailto", "data"],
    transformTags: {
      a: sanitizeHtml.default.simpleTransform("a", { rel: "noreferrer", target: "_blank" }),
    },
  });
}

function MarkdownPreview({ content }: { content: string }) {
  const [html, setHtml] = useState("");

  useEffect(() => {
    let cancelled = false;

    renderMarkdownToHtml(content)
      .then((safeHtml) => {
        if (!cancelled) setHtml(safeHtml);
      })
      .catch(() => {
        if (!cancelled) setHtml(`<pre>${escapeHtml(content)}</pre>`);
      });

    return () => {
      cancelled = true;
    };
  }, [content]);

  return <article className="markdown-preview" dangerouslySetInnerHTML={{ __html: html }} />;
}

function escapeMarkdownText(text: string) {
  return text
    .replace(/\\/g, "\\\\")
    .replace(/`/g, "\\`")
    .replace(/\*/g, "\\*")
    .replace(/_/g, "\\_")
    .replace(/\[/g, "\\[")
    .replace(/\]/g, "\\]")
    .replace(/~/g, "\\~");
}

function textFromNode(node: JSONContent): string {
  if (node.text) return node.text;
  return (node.content ?? []).map(textFromNode).join("");
}

function serializeInline(content: JSONContent[] = []): string {
  return content
    .map((node) => {
      if (node.type === "hardBreak") return "  \n";
      if (node.type === "image" && node.attrs?.src) {
        const alt = escapeMarkdownText(String(node.attrs.alt ?? ""));
        const title = node.attrs.title ? ` "${String(node.attrs.title).replace(/"/g, '\\"')}"` : "";
        return `![${alt}](${node.attrs.src}${title})`;
      }
      if (node.type !== "text") return serializeBlock(node, 0, true);

      let text = escapeMarkdownText(node.text ?? "");
      for (const mark of node.marks ?? []) {
        if (mark.type === "code") {
          text = `\`${(node.text ?? "").replace(/`/g, "\\`")}\``;
        } else if (mark.type === "bold") {
          text = `**${text}**`;
        } else if (mark.type === "italic") {
          text = `*${text}*`;
        } else if (mark.type === "strike") {
          text = `~~${text}~~`;
        } else if (mark.type === "link" && mark.attrs?.href) {
          const title = mark.attrs.title ? ` "${String(mark.attrs.title).replace(/"/g, '\\"')}"` : "";
          text = `[${text}](${mark.attrs.href}${title})`;
        }
      }
      return text;
    })
    .join("");
}

function prefixLines(content: string, prefix: string) {
  return content
    .split("\n")
    .map((line) => `${prefix}${line}`)
    .join("\n");
}

function serializeList(node: JSONContent, depth: number, ordered: boolean) {
  let index = typeof node.attrs?.start === "number" ? node.attrs.start : 1;
  return (node.content ?? [])
    .map((item) => {
      const marker = ordered ? `${index++}.` : "-";
      return serializeListItem(item, depth, marker);
    })
    .join("\n");
}

function serializeListItem(node: JSONContent, depth: number, marker: string) {
  const indent = "  ".repeat(depth);
  const children = node.content ?? [];
  const [first, ...rest] = children;
  const firstText = first ? serializeBlock(first, depth + 1, true).trimEnd() : "";
  const lines = [`${indent}${marker} ${firstText}`.trimEnd()];

  for (const child of rest) {
    const childText = serializeBlock(child, depth + 1, true).trimEnd();
    if (!childText) continue;

    if (child.type === "bulletList" || child.type === "orderedList") {
      lines.push(childText);
    } else {
      lines.push(prefixLines(childText, `${indent}  `));
    }
  }

  return lines.join("\n");
}

function serializeTableCell(cell: JSONContent) {
  const content = serializeBlocks(cell.content ?? [])
    .replace(/\n{2,}/g, "<br>")
    .replace(/\n/g, "<br>")
    .replace(/\|/g, "\\|")
    .trim();
  return content || " ";
}

function serializeTable(node: JSONContent) {
  const rows = node.content ?? [];
  if (rows.length === 0) return "";

  const headerCells = rows[0].content ?? [];
  const header = `| ${headerCells.map(serializeTableCell).join(" | ")} |`;
  const separator = `| ${headerCells.map(() => "---").join(" | ")} |`;
  const body = rows
    .slice(1)
    .map((row) => `| ${(row.content ?? []).map(serializeTableCell).join(" | ")} |`)
    .join("\n");

  return body ? `${header}\n${separator}\n${body}` : `${header}\n${separator}`;
}

function serializeBlock(node: JSONContent, depth = 0, compact = false): string {
  switch (node.type) {
    case "doc":
      return serializeBlocks(node.content ?? [], depth);
    case "paragraph":
      return serializeInline(node.content ?? []);
    case "heading":
      return `${"#".repeat(node.attrs?.level ?? 1)} ${serializeInline(node.content ?? [])}`.trimEnd();
    case "blockquote":
      return prefixLines(serializeBlocks(node.content ?? [], depth).trimEnd(), "> ");
    case "codeBlock": {
      const language = node.attrs?.language ?? "";
      return `\`\`\`${language}\n${textFromNode(node).replace(/\n$/, "")}\n\`\`\``;
    }
    case "bulletList":
      return serializeList(node, depth, false);
    case "orderedList":
      return serializeList(node, depth, true);
    case "table":
      return serializeTable(node);
    case "horizontalRule":
      return "---";
    case "hardBreak":
      return "  \n";
    case "text":
      return serializeInline([node]);
    default:
      return compact ? serializeInline(node.content ?? []) : serializeBlocks(node.content ?? [], depth);
  }
}

function serializeBlocks(content: JSONContent[] = [], depth = 0): string {
  return content
    .map((node) => serializeBlock(node, depth))
    .filter((block) => block.length > 0)
    .join("\n\n");
}

function tiptapJsonToMarkdown(doc: JSONContent) {
  const markdown = serializeBlock(doc).replace(/[ \t]+\n/g, "\n").trimEnd();
  return markdown ? `${markdown}\n` : "";
}

function MarkdownRichEditor({
  content,
  onChange,
  onSave,
  onEditorReady,
}: {
  content: string;
  onChange: (content: string) => void;
  onSave: () => void;
  onEditorReady: (editor: TiptapEditor | null) => void;
}) {
  const [loadError, setLoadError] = useState(false);
  const applyingExternalContent = useRef(false);
  const lastMarkdown = useRef<string | null>(null);
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    onSaveRef.current = onSave;
  }, [onSave]);

  const editor = useEditor({
    extensions: markdownRichExtensions,
    content: "<p></p>",
    editorProps: {
      attributes: {
        class: "markdown-rich-editor",
      },
      handleKeyDown: (_, event) => {
        if (event.ctrlKey && event.key.toLowerCase() === "s") {
          event.preventDefault();
          onSaveRef.current();
          return true;
        }
        return false;
      },
    },
    onUpdate: ({ editor: currentEditor }) => {
      if (applyingExternalContent.current) return;
      const markdown = tiptapJsonToMarkdown(currentEditor.getJSON());
      lastMarkdown.current = markdown;
      onChangeRef.current(markdown);
    },
  });

  useEffect(() => {
    onEditorReady(editor);
    return () => onEditorReady(null);
  }, [editor, onEditorReady]);

  useEffect(() => {
    if (!editor || content === lastMarkdown.current) return;

    let cancelled = false;
    setLoadError(false);
    renderMarkdownToHtml(content)
      .then((html) => {
        if (cancelled) return;
        applyingExternalContent.current = true;
        editor.commands.setContent(html || "<p></p>", false);
        lastMarkdown.current = content;
        requestAnimationFrame(() => {
          applyingExternalContent.current = false;
        });
      })
      .catch(() => {
        if (!cancelled) setLoadError(true);
      });

    return () => {
      cancelled = true;
    };
  }, [content, editor]);

  if (loadError) {
    return (
      <textarea
        className="w-full h-full p-4 font-mono text-sm outline-none resize-none bg-white text-[#24292f]"
        value={content}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.ctrlKey && event.key.toLowerCase() === "s") {
            event.preventDefault();
            onSave();
          }
        }}
        spellCheck={false}
      />
    );
  }

  return (
    <div className="min-h-full bg-white">
      <EditorContent editor={editor} />
    </div>
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
  const [markdownEditor, setMarkdownEditor] = useState<TiptapEditor | null>(null);

  useEffect(() => {
    if (mode === "rich" && tab.format === "html" && ref.current && ref.current.innerHTML !== tab.content) {
      ref.current.innerHTML = tab.content || "<p><br></p>";
    }
  }, [tab.content, mode, tab.format]);

  const exec = (command: string, value?: string) => {
    document.execCommand(command, false, value);
    if (ref.current) onChange(ref.current.innerHTML);
  };

  const setMarkdownEditorRef = useCallback((editor: TiptapEditor | null) => {
    setMarkdownEditor(editor);
  }, []);

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

  const toolbarButton = (title: string, icon: ReactNode, onClick: () => void, active = false, disabled = false) => (
    <button
      onClick={onClick}
      className={`p-1 rounded text-[#24292f] hover:bg-[#eaeef2] ${active ? "bg-[#dbeafe]" : ""} ${disabled ? "opacity-50" : ""}`}
      title={title}
      disabled={disabled}
    >
      {icon}
    </button>
  );

  const markdownCommands = useMemo(
    () => ({
      bold: () => markdownEditor?.chain().focus().toggleBold().run(),
      italic: () => markdownEditor?.chain().focus().toggleItalic().run(),
      heading1: () => markdownEditor?.chain().focus().toggleHeading({ level: 1 }).run(),
      heading2: () => markdownEditor?.chain().focus().toggleHeading({ level: 2 }).run(),
      bulletList: () => markdownEditor?.chain().focus().toggleBulletList().run(),
      codeBlock: () => markdownEditor?.chain().focus().toggleCodeBlock().run(),
    }),
    [markdownEditor],
  );

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
        {mode === "rich" && tab.format === "markdown" && (
          <>
            <div className="w-px h-4 mx-1 bg-[#d8dee4]" />
            {toolbarButton("Bold", <Bold size={14} />, markdownCommands.bold, !!markdownEditor?.isActive("bold"), !markdownEditor)}
            {toolbarButton("Italic", <Italic size={14} />, markdownCommands.italic, !!markdownEditor?.isActive("italic"), !markdownEditor)}
            {toolbarButton("Heading 1", <Heading1 size={14} />, markdownCommands.heading1, !!markdownEditor?.isActive("heading", { level: 1 }), !markdownEditor)}
            {toolbarButton("Heading 2", <Heading2 size={14} />, markdownCommands.heading2, !!markdownEditor?.isActive("heading", { level: 2 }), !markdownEditor)}
            {toolbarButton("List", <List size={14} />, markdownCommands.bulletList, !!markdownEditor?.isActive("bulletList"), !markdownEditor)}
            {toolbarButton("Code block", <Code size={14} />, markdownCommands.codeBlock, !!markdownEditor?.isActive("codeBlock"), !markdownEditor)}
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
          <MarkdownRichEditor content={tab.content} onChange={onChange} onSave={onSave} onEditorReady={setMarkdownEditorRef} />
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
          <iframe key={tab.content} title={tab.title} srcDoc={tab.content} className="block w-full h-full border-0 bg-white" />
        )}
        {mode === "preview" && tab.format === "markdown" && <MarkdownPreview content={tab.content} />}
      </div>
    </div>
  );
}
