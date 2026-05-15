import { X } from "lucide-react";
import type { Tab } from "@/types/note";

export default function TabBar({
  tabs,
  activeTabId,
  onSwitch,
  onClose,
}: {
  tabs: Tab[];
  activeTabId: string | null;
  onSwitch: (id: string) => void;
  onClose: (id: string) => void;
}) {
  if (tabs.length === 0) return null;

  return (
    <div className="h-10 flex items-stretch overflow-hidden border-b border-[#d8dee4] bg-[#eef3f6] px-1 pt-1">
      {tabs.map((tab) => {
        const active = tab.id === activeTabId;
        return (
          <div
            key={tab.id}
            onClick={() => onSwitch(tab.id)}
            className={`group relative flex h-full min-w-0 flex-1 cursor-pointer select-none items-center gap-1.5 rounded-t-md px-3 text-[13px] leading-5 transition ${
              active ? "bg-white text-[#24292f] shadow-[0_-1px_0_#d8dee4_inset]" : "text-[#57606a] hover:bg-[#f7fafc] hover:text-[#24292f]"
            }`}
            title={tab.title}
          >
            {!active && <span className="absolute right-0 top-2 bottom-2 w-px bg-[#d8dee4] group-last:hidden" />}
            <span className="min-w-0 flex-1 truncate whitespace-nowrap align-middle">
              {tab.title}
              {tab.dirty ? " *" : ""}
            </span>
            <button
              onClick={(e) => {
                e.stopPropagation();
                onClose(tab.id);
              }}
              className={`shrink-0 rounded p-0.5 text-[#57606a] hover:bg-[#d8dee4] hover:text-[#24292f] ${
                active ? "opacity-100" : "opacity-0 group-hover:opacity-100"
              }`}
              title="Close"
            >
              <X size={12} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
