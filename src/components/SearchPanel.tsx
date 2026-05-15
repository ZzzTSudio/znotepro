import { Loader2, Search, X } from "lucide-react";
import { useState } from "react";
import type { SearchResult } from "@/types/note";

export default function SearchPanel({
  results,
  query,
  isSearching,
  onSearch,
  onOpen,
  onClose,
}: {
  results: SearchResult[];
  query: string;
  isSearching: boolean;
  onSearch: (q: string) => void;
  onOpen: (path: string) => void;
  onClose: () => void;
}) {
  const [input, setInput] = useState(query);

  const escapeHtml = (str: string) => str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

  const highlight = (text: string, q: string) => {
    if (!q) return escapeHtml(text);
    const terms = q.split(/\s+/).filter(Boolean);
    let html = escapeHtml(text);
    terms.forEach((t) => {
      const re = new RegExp(`(${t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
      html = html.replace(re, '<mark class="bg-yellow-300 text-[#24292f]">$1</mark>');
    });
    return html;
  };

  return (
    <div className="w-96 flex flex-col border-l border-[#d8dee4] bg-[#f6f8fa] shrink-0">
      <div className="h-9 flex items-center justify-between px-3 border-b border-[#d8dee4]">
        <span className="text-xs font-medium text-[#57606a]">Search Results</span>
        <button onClick={onClose} className="p-1 rounded text-[#57606a] hover:bg-[#eaeef2]" title="Close">
          <X size={14} />
        </button>
      </div>
      <div className="p-2 border-b border-[#d8dee4]">
        <div className="flex items-center gap-1 px-2 py-1 rounded border border-[#d8dee4] bg-white">
          <Search size={12} className="text-[#57606a]" />
          <input
            className="bg-transparent text-xs outline-none w-full text-[#24292f] placeholder-[#6e7781]"
            placeholder="Enter keywords..."
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onSearch(input);
            }}
          />
        </div>
      </div>
      <div className="flex-1 overflow-y-auto">
        {isSearching ? (
          <div className="p-4 text-center text-[#57606a]">
            <Loader2 size={16} className="animate-spin mx-auto mb-2" />
            <p className="text-xs">Searching...</p>
          </div>
        ) : results.length === 0 ? (
          <div className="p-4 text-center text-xs text-[#57606a]">{query ? "No results" : "Enter keywords and press Enter"}</div>
        ) : (
          <div className="flex flex-col">
            {results.map((r) => (
              <div
                key={r.file}
                className="px-3 py-2 border-b border-[#d8dee4] cursor-pointer hover:bg-[#eaeef2]"
                onClick={() => onOpen(r.file)}
              >
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs font-medium text-[#2da44e] truncate">{r.title}</span>
                  <span className="text-[10px] shrink-0 ml-2 text-[#57606a]">{r.score.toFixed(2)}</span>
                </div>
                <div className="text-[10px] mb-1 truncate text-[#57606a]">{r.file}</div>
                {r.boost_reasons.length > 0 && (
                  <div className="flex gap-1 mb-1">
                    {r.boost_reasons.map((b) => (
                      <span key={b} className="text-[10px] px-1 rounded bg-[#eaeef2] text-[#24292f]">
                        {b}
                      </span>
                    ))}
                  </div>
                )}
                <div className="space-y-1">
                  {r.matches.slice(0, 3).map((m, idx) => (
                    <div key={idx} className="text-[11px] text-[#57606a]">
                      <span className="text-[#57606a] mr-1">{m.line_number}:</span>
                      <span dangerouslySetInnerHTML={{ __html: highlight(m.line_text, query) }} />
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
