import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { PanelLeft, Search, Settings } from "lucide-react";
import { useNotes } from "@/hooks/useNotes";
import Sidebar from "@/components/Sidebar";
import TabBar from "@/components/TabBar";
import Editor from "@/components/Editor";
import SearchPanel from "@/components/SearchPanel";
import ModelSettingsDialog from "@/components/ModelSettingsDialog";
import StyleSelectDialog from "@/components/StyleSelectDialog";
import type { ConvertProgress, StyleTemplate } from "@/types/note";

const APP_NAME = "znote Pro";
const APP_VERSION = "v1.0.1";
const SIDEBAR_WIDTH_KEY = "znote.sidebarWidth";
const DEFAULT_SIDEBAR_WIDTH = 256;
const MIN_SIDEBAR_WIDTH = 220;
const MAX_SIDEBAR_WIDTH = 520;

function clampSidebarWidth(width: number) {
  return Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, width));
}

function getStoredSidebarWidth() {
  const stored = window.localStorage.getItem(SIDEBAR_WIDTH_KEY);
  const width = stored ? Number(stored) : DEFAULT_SIDEBAR_WIDTH;
  return Number.isFinite(width) ? clampSidebarWidth(width) : DEFAULT_SIDEBAR_WIDTH;
}

export default function Home() {
  const {
    notes,
    tabs,
    activeTabId,
    searchResults,
    searchQuery,
    isSearching,
    sidebarVisible,
    searchPanelVisible,
    noteDir,
    setActiveTabId,
    setSidebarVisible,
    setSearchPanelVisible,
    loadNotes,
    openNote,
    createNote,
    createFolder,
    importFiles,
    saveNote,
    deleteNote,
    deleteFolder,
    renameEntry,
    updateTabContent,
    closeTab,
    searchNotes,
    convertNote,
  } = useNotes();

  const activeTab = tabs.find((t) => t.id === activeTabId) || null;
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [statusMessage, setStatusMessage] = useState("Ready");
  const [styles, setStyles] = useState<StyleTemplate[]>([]);
  const [styleDialogOpen, setStyleDialogOpen] = useState(false);
  const [selectedStyleId, setSelectedStyleId] = useState("");
  const [pendingConvertPath, setPendingConvertPath] = useState<string | null>(null);
  const [sidebarWidth, setSidebarWidth] = useState(getStoredSidebarWidth);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (activeTab && activeTab.dirty) saveNote(activeTab);
      }
      if (e.ctrlKey && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setSidebarVisible((v) => !v);
      }
      if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setSearchPanelVisible((v) => !v);
      }
      if (e.ctrlKey && e.key.toLowerCase() === "n") {
        e.preventDefault();
        const name = prompt("New note name, for example folder/name.html:");
        if (name) createNote(name);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeTab, saveNote, setSidebarVisible, setSearchPanelVisible, createNote]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen<ConvertProgress>("convert_progress", (event) => {
      if (disposed) return;
      const { file_name, current, total, stage } = event.payload;
      if (stage === "normalizing") {
        setStatusMessage(`${file_name} 正在规范化：${current}/${total}`);
      }
    }).then((cleanup) => {
      if (disposed) {
        cleanup();
      } else {
        unlisten = cleanup;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const handleSearch = useCallback(
    (q: string) => {
      searchNotes(q);
      setSearchPanelVisible(true);
    },
    [searchNotes, setSearchPanelVisible],
  );

  const handleImportFiles = useCallback(
    async (files: File[]) => {
      await importFiles(files, (fileName) =>
        window.confirm(
          `Doc already contains ${fileName}.\n\nOK: overwrite the existing file\nCancel: import as a -1 copy`,
        ),
      );
    },
    [importFiles],
  );

  const extOf = (path: string) => path.split(".").pop()?.toLowerCase() ?? "";
  const baseName = (path: string) => path.replace(/\\/g, "/").split("/").pop() || path;
  const targetName = (path: string) => {
    const name = baseName(path);
    const dot = name.lastIndexOf(".");
    const stem = dot > 0 ? name.slice(0, dot) : name;
    const ext = extOf(path);
    return ext === "html" || ext === "htm" ? `${stem}.md` : `${stem}.html`;
  };

  const askOverwrite = useCallback(
    (path: string) => {
      const outputName = targetName(path);
      const parent = path.replace(/\\/g, "/").split("/").slice(0, -1).join("/");
      const outputPath = parent ? `${parent}/${outputName}` : outputName;
      const exists = notes.some((note) => !note.is_dir && note.path === outputPath);
      if (!exists) return false;
      return window.confirm(
        `${outputName} 已存在。\n\n确定：覆盖现有文件\n取消：生成 -1 副本`,
      );
    },
    [notes],
  );

  const runConvertWithStatus = useCallback(
    async (path: string, styleId: string | null) => {
      const expectedName = targetName(path);
      const overwrite = askOverwrite(path);
      setStatusMessage(`${expectedName} 正在转换...`);
      try {
        const result = await convertNote(path, styleId, overwrite);
        setStatusMessage(`${result.output_name} 转换成功`);
      } catch (error) {
        const message = String(error);
        if (message.includes("请配置模型") || message.includes("模型配置")) {
          window.alert("请配置模型。");
          setSettingsOpen(true);
          setStatusMessage(`${expectedName} 转换失败：请配置模型`);
          return;
        }
        setStatusMessage(`${expectedName} 转换失败：${message}`);
      }
    },
    [askOverwrite, convertNote],
  );

  const handleConvert = useCallback(
    async (path: string) => {
      const ext = extOf(path);
      if (ext === "html" || ext === "htm") {
        await runConvertWithStatus(path, null);
        return;
      }

      try {
        const list: StyleTemplate[] = await invoke("list_style_templates");
        setStyles(list);
        setSelectedStyleId(list[0]?.id ?? "");
        setPendingConvertPath(path);
        setStyleDialogOpen(true);
      } catch (error) {
        setStatusMessage(`${targetName(path)} 转换失败：${String(error)}`);
      }
    },
    [runConvertWithStatus],
  );

  const confirmStyleConvert = useCallback(() => {
    if (!pendingConvertPath || !selectedStyleId) return;
    const path = pendingConvertPath;
    const styleId = selectedStyleId;
    setStyleDialogOpen(false);
    setPendingConvertPath(null);
    void runConvertWithStatus(path, styleId);
  }, [pendingConvertPath, runConvertWithStatus, selectedStyleId]);
  const handleSidebarResize = useCallback((width: number) => {
    const nextWidth = clampSidebarWidth(width);
    setSidebarWidth(nextWidth);
    window.localStorage.setItem(SIDEBAR_WIDTH_KEY, String(nextWidth));
  }, []);

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden select-none bg-white text-[#24292f]">
      <div className="h-10 flex items-center justify-between px-4 border-b border-[#d8dee4] bg-[#f6f8fa]">
        <div className="flex items-center gap-2">
          <img src="/favicon.svg" className="h-5 w-5" alt="" />
          <span className="font-semibold text-sm tracking-wide">{APP_NAME}</span>
          <span className="text-xs text-[#57606a]">{APP_VERSION}</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => setSidebarVisible((v) => !v)}
            className="p-1.5 rounded text-[#24292f] transition hover:bg-[#eaeef2]"
            title="Toggle sidebar Ctrl+B"
          >
            <PanelLeft size={15} />
          </button>
          <button
            onClick={() => setSettingsOpen(true)}
            className="p-1.5 rounded text-[#24292f] transition hover:bg-[#eaeef2]"
            title="模型设置"
          >
            <Settings size={15} />
          </button>
          <button
            onClick={() => setSearchPanelVisible((v) => !v)}
            className="p-1.5 rounded text-[#24292f] transition hover:bg-[#eaeef2]"
            title="Search Ctrl+Shift+F"
          >
            <Search size={15} />
          </button>
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden">
        {sidebarVisible && (
          <Sidebar
            notes={notes}
            noteDir={noteDir}
            width={sidebarWidth}
            minWidth={MIN_SIDEBAR_WIDTH}
            maxWidth={MAX_SIDEBAR_WIDTH}
            onResize={handleSidebarResize}
            onRefresh={loadNotes}
            onOpen={openNote}
            onImportFiles={handleImportFiles}
            onDelete={deleteNote}
            onDeleteFolder={deleteFolder}
            onCreateNote={createNote}
            onCreateFolder={createFolder}
            onRename={renameEntry}
            onSearch={handleSearch}
            onConvert={handleConvert}
          />
        )}

        <div className="flex-1 flex flex-col min-w-0">
          <TabBar tabs={tabs} activeTabId={activeTabId} onSwitch={setActiveTabId} onClose={closeTab} />
          <div className="flex-1 overflow-auto">
            {activeTab ? (
              <Editor
                key={activeTab.id}
                tab={activeTab}
                onChange={(content) => updateTabContent(activeTab.id, content)}
                onSave={() => saveNote(activeTab)}
              />
            ) : (
              <div className="h-full flex flex-col items-center justify-center text-[#57606a]">
                <img src="/favicon.svg" className="h-12 w-12 mb-4 opacity-90" alt="" />
                <p className="text-lg mb-2">{APP_NAME}</p>
                <p className="text-sm">Import md/html documents from the left sidebar, or use Ctrl+N to create a note.</p>
              </div>
            )}
          </div>
        </div>

        {searchPanelVisible && (
          <SearchPanel
            results={searchResults}
            query={searchQuery}
            isSearching={isSearching}
            onSearch={handleSearch}
            onOpen={openNote}
            onClose={() => setSearchPanelVisible(false)}
          />
        )}
      </div>

      <div className="h-6 flex items-center justify-between px-3 text-[11px] bg-[#5f8f6b] text-white">
        <div className="flex items-center gap-3">
          <span className="truncate">{statusMessage || (activeTab ? activeTab.title : "Ready")}</span>
          {activeTab?.dirty && <span className="text-yellow-100">Unsaved</span>}
        </div>
        <div className="flex items-center gap-3">
          <span>{activeTab ? (activeTab.format === "html" ? "HTML" : "Markdown") : ""}</span>
          <span>{notes.length} files</span>
        </div>
      </div>
      <ModelSettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <StyleSelectDialog
        open={styleDialogOpen}
        styles={styles}
        selectedId={selectedStyleId}
        onSelect={setSelectedStyleId}
        onCancel={() => {
          setStyleDialogOpen(false);
          setPendingConvertPath(null);
        }}
        onConfirm={confirmStyleConvert}
      />
    </div>
  );
}


