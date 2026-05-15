import { useCallback, useEffect } from "react";
import { PanelLeft, Search } from "lucide-react";
import { useNotes } from "@/hooks/useNotes";
import Sidebar from "@/components/Sidebar";
import TabBar from "@/components/TabBar";
import Editor from "@/components/Editor";
import SearchPanel from "@/components/SearchPanel";

const APP_NAME = "znote Pro";
const APP_VERSION = "v1.0.0";

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
  } = useNotes();

  const activeTab = tabs.find((t) => t.id === activeTabId) || null;

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
            onRefresh={loadNotes}
            onOpen={openNote}
            onImportFiles={handleImportFiles}
            onDelete={deleteNote}
            onDeleteFolder={deleteFolder}
            onCreateNote={createNote}
            onCreateFolder={createFolder}
            onRename={renameEntry}
            onSearch={handleSearch}
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
          <span>{activeTab ? activeTab.title : "Ready"}</span>
          {activeTab?.dirty && <span className="text-yellow-100">Unsaved</span>}
        </div>
        <div className="flex items-center gap-3">
          <span>{activeTab ? (activeTab.format === "html" ? "HTML" : "Markdown") : ""}</span>
          <span>{notes.length} files</span>
        </div>
      </div>
    </div>
  );
}
