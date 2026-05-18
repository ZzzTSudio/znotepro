import type { StyleTemplate } from "@/types/note";

export default function StyleSelectDialog({
  open,
  styles,
  selectedId,
  onSelect,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  styles: StyleTemplate[];
  selectedId: string;
  onSelect: (id: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/30">
      <div className="w-[680px] max-w-[calc(100vw-32px)] rounded-md border border-[#d8dee4] bg-white shadow-xl">
        <div className="border-b border-[#d8dee4] px-4 py-3">
          <h2 className="text-sm font-semibold text-[#24292f]">选择 HTML 样式</h2>
          <p className="mt-1 text-xs text-[#57606a]">Markdown 转 HTML 会参考所选样式模板生成页面。</p>
        </div>

        <div className="grid max-h-[420px] grid-cols-2 gap-2 overflow-auto p-4">
          {styles.map((style) => (
            <button
              key={style.id}
              className={`rounded-md border p-3 text-left transition ${
                selectedId === style.id
                  ? "border-[#0969da] bg-[#ddf4ff] ring-1 ring-[#0969da]/30"
                  : "border-[#d8dee4] hover:bg-[#f6f8fa]"
              }`}
              onClick={() => onSelect(style.id)}
            >
              <div className="text-sm font-medium text-[#24292f]">{style.name}</div>
              <div className="mt-1 text-xs text-[#57606a]">{style.description}</div>
            </button>
          ))}
        </div>

        <div className="flex justify-end gap-2 border-t border-[#d8dee4] px-4 py-3">
          <button className="rounded border border-[#d8dee4] px-3 py-1.5 text-sm hover:bg-[#f6f8fa]" onClick={onCancel}>
            取消
          </button>
          <button
            className="rounded bg-[#2da44e] px-3 py-1.5 text-sm text-white hover:bg-[#2c974b] disabled:opacity-60"
            disabled={!selectedId}
            onClick={onConfirm}
          >
            确定
          </button>
        </div>
      </div>
    </div>
  );
}
