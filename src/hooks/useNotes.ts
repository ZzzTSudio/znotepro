import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ConvertResult, NoteInfo, SearchResult, Tab } from "@/types/note";

export function useNotes() {
  const [notes, setNotes] = useState<NoteInfo[]>([]);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [isSearching, setIsSearching] = useState(false);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [searchPanelVisible, setSearchPanelVisible] = useState(false);
  const [noteDir, setNoteDir] = useState("");

  const loadNotes = useCallback(async () => {
    try {
      const list: NoteInfo[] = await invoke("list_notes");
      setNotes(list);
    } catch (e) {
      console.error("list_notes failed", e);
    }
  }, []);

  const loadNoteDir = useCallback(async () => {
    try {
      const dir: string = await invoke("get_note_directory");
      setNoteDir(dir);
    } catch (e) {
      console.error("get_note_directory failed", e);
    }
  }, []);

  useEffect(() => {
    loadNotes();
    loadNoteDir();
  }, [loadNotes, loadNoteDir]);

  const openNote = useCallback(async (relativePath: string) => {
    const existing = tabs.find((t) => t.path === relativePath);
    if (existing) {
      setActiveTabId(existing.id);
      return;
    }
    try {
      const result: { content: string; format: "html" | "markdown" } = await invoke("read_note", { relativePath });
      const id = `${relativePath}-${Date.now()}`;
      const title = relativePath.split(/[\\/]/).pop() || relativePath;
      const newTab: Tab = {
        id,
        path: relativePath,
        title,
        content: result.content,
        dirty: false,
        format: result.format,
      };
      setTabs((prev) => [...prev, newTab]);
      setActiveTabId(id);
    } catch (e) {
      console.error("read_note failed", e);
    }
  }, [tabs]);

  const createNote = useCallback(async (relativePath: string) => {
    try {
      await invoke("create_note", { relativePath });
      await loadNotes();
      await openNote(relativePath);
    } catch (e) {
      console.error("create_note failed", e);
    }
  }, [loadNotes, openNote]);

  const createFolder = useCallback(async (relativePath: string) => {
    try {
      await invoke("create_folder", { relativePath });
      await loadNotes();
    } catch (e) {
      console.error("create_folder failed", e);
    }
  }, [loadNotes]);

  const renameEntry = useCallback(async (oldPath: string, newPath: string) => {
    try {
      await invoke("rename_entry", { oldPath, newPath });
      setTabs((prev) =>
        prev.map((tab) => {
          if (tab.path === oldPath || tab.path.startsWith(`${oldPath}/`)) {
            const updatedPath = tab.path === oldPath ? newPath : `${newPath}/${tab.path.slice(oldPath.length + 1)}`;
            return {
              ...tab,
              path: updatedPath,
              title: updatedPath.split(/[\\/]/).pop() || updatedPath,
            };
          }
          return tab;
        })
      );
      await loadNotes();
    } catch (e) {
      console.error("rename_entry failed", e);
    }
  }, [loadNotes]);

  const importFiles = useCallback(
    async (files: File[], shouldOverwrite: (fileName: string) => boolean) => {
      let lastImportedPath: string | null = null;
      for (const file of files) {
        const lowerName = file.name.toLowerCase();
        const supported =
          lowerName.endsWith(".html") ||
          lowerName.endsWith(".htm") ||
          lowerName.endsWith(".md") ||
          lowerName.endsWith(".markdown");
        if (!supported) {
          continue;
        }

        const exists = notes.some((note) => !note.is_dir && note.path === file.name);
        const overwrite = exists ? shouldOverwrite(file.name) : false;
        const content = await file.text();
        const result: { path: string; success: boolean } = await invoke("import_note_content", {
          fileName: file.name,
          content,
          overwrite,
        });
        if (result.success) {
          lastImportedPath = result.path;
        }
      }

      await loadNotes();
      if (lastImportedPath) {
        await openNote(lastImportedPath);
      }
    },
    [loadNotes, notes, openNote]
  );

  const saveNote = useCallback(async (tab: Tab) => {
    try {
      await invoke("save_note", { relativePath: tab.path, content: tab.content });
      setTabs((prev) => prev.map((t) => (t.id === tab.id ? { ...t, dirty: false } : t)));
    } catch (e) {
      console.error("save_note failed", e);
    }
  }, []);

  const deleteNote = useCallback(async (relativePath: string) => {
    try {
      await invoke("delete_note", { relativePath });
      setTabs((prev) => prev.filter((t) => t.path !== relativePath));
      if (tabs.find((t) => t.path === relativePath)?.id === activeTabId) {
        setActiveTabId(null);
      }
      await loadNotes();
    } catch (e) {
      console.error("delete_note failed", e);
    }
  }, [tabs, activeTabId, loadNotes]);

  const deleteFolder = useCallback(async (relativePath: string) => {
    try {
      await invoke("delete_folder", { relativePath });
      setTabs((prev) => {
        const filtered = prev.filter((t) => !t.path.startsWith(`${relativePath}/`));
        if (activeTabId && !filtered.some((t) => t.id === activeTabId)) {
          setActiveTabId(filtered.length > 0 ? filtered[filtered.length - 1].id : null);
        }
        return filtered;
      });
      await loadNotes();
    } catch (e) {
      console.error("delete_folder failed", e);
    }
  }, [activeTabId, loadNotes]);

  const updateTabContent = useCallback((tabId: string, content: string) => {
    setTabs((prev) => prev.map((t) => (t.id === tabId ? { ...t, content, dirty: true } : t)));
  }, []);

  const closeTab = useCallback((tabId: string) => {
    setTabs((prev) => {
      const filtered = prev.filter((t) => t.id !== tabId);
      if (activeTabId === tabId && filtered.length > 0) {
        setActiveTabId(filtered[filtered.length - 1].id);
      } else if (filtered.length === 0) {
        setActiveTabId(null);
      }
      return filtered;
    });
  }, [activeTabId]);

  const searchNotes = useCallback(async (query: string) => {
    if (!query.trim()) {
      setSearchResults([]);
      setSearchQuery("");
      return;
    }
    setIsSearching(true);
    setSearchQuery(query);
    try {
      const results: SearchResult[] = await invoke("search_notes", { query });
      setSearchResults(results);
    } catch (e) {
      console.error("search_notes failed", e);
      setSearchResults([]);
    } finally {
      setIsSearching(false);
    }
  }, []);

  const rebuildIndex = useCallback(async () => {
    try {
      await invoke("rebuild_search_index");
    } catch (e) {
      console.error("rebuild_search_index failed", e);
    }
  }, []);

  const convertNote = useCallback(
    async (relativePath: string, styleId: string | null, overwrite: boolean) => {
      const result: ConvertResult = await invoke("convert_note", {
        relativePath,
        styleId,
        overwrite,
      });
      await loadNotes();
      return result;
    },
    [loadNotes],
  );

  return {
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
    loadNoteDir,
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
    rebuildIndex,
    convertNote,
  };
}
