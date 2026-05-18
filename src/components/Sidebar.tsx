import { useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent, PointerEvent as ReactPointerEvent } from "react";
import { ChevronDown, ChevronRight, FileText, FileUp, Folder, RefreshCw, Search, Trash2 } from "lucide-react";
import type { NoteInfo } from "@/types/note";

type MenuTarget =
  | { kind: "blank"; x: number; y: number }
  | { kind: "node"; node: NoteInfo; x: number; y: number };

type DropTarget = { kind: "root" } | { kind: "folder"; path: string };

type PointerDragState = {
  path: string;
  name: string;
  startX: number;
  startY: number;
  x: number;
  y: number;
  active: boolean;
};

type MenuItem = {
  label: string;
  danger?: boolean;
  onClick: () => void;
};

const DEFAULT_FILE_NAME = "未命名文件.md";
const DEFAULT_FOLDER_NAME = "未命名文件夹";
const DRAG_START_DISTANCE = 6;

function sortNodes(nodes: NoteInfo[]) {
  nodes.sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    return a.name.localeCompare(b.name, "zh-Hans-CN", { numeric: true, sensitivity: "base" });
  });
  nodes.forEach((node) => {
    if (node.children) sortNodes(node.children);
  });
  return nodes;
}

function buildTree(flat: NoteInfo[]): NoteInfo[] {
  const root: NoteInfo[] = [];
  const map = new Map<string, NoteInfo>();
  flat.forEach((n) => {
    map.set(n.path, { ...n, children: [] });
  });
  flat.forEach((n) => {
    const node = map.get(n.path)!;
    const parent = parentPath(n.path);
    if (parent && map.has(parent)) {
      map.get(parent)!.children!.push(node);
    } else {
      root.push(node);
    }
  });
  return sortNodes(root);
}

function parentPath(path: string) {
  return path.replace(/\\/g, "/").split("/").slice(0, -1).join("/");
}

function basename(path: string) {
  return path.replace(/\\/g, "/").split("/").pop() ?? path;
}

function joinPath(parent: string, name: string) {
  return parent ? `${parent}/${name}` : name;
}

function extensionOf(path: string) {
  return path.split(".").pop()?.toLowerCase() ?? "";
}

function hasSupportedExtension(name: string) {
  return /\.(html?|md|markdown)$/i.test(name);
}

function splitExtension(name: string) {
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return { stem: name, ext: "" };
  return { stem: name.slice(0, dot), ext: name.slice(dot) };
}

function uniquePath(existing: NoteInfo[], basePath: string, preferredName: string, ignorePath?: string) {
  const { stem, ext } = splitExtension(preferredName);
  const exists = (path: string) => existing.some((note) => note.path === path && note.path !== ignorePath);
  let candidate = preferredName;
  let index = 1;
  while (exists(joinPath(basePath, candidate))) {
    candidate = `${stem}-${index}${ext}`;
    index += 1;
  }
  return joinPath(basePath, candidate);
}

function findNote(notes: NoteInfo[], path: string) {
  return notes.find((note) => note.path === path);
}

function dropTargetMatches(target: DropTarget | null, node: NoteInfo) {
  return target?.kind === "folder" && target.path === node.path;
}

function TreeNode({
  node,
  depth,
  renamingPath,
  renameValue,
  setRenameValue,
  draggingPath,
  dropTarget,
  onOpen,
  onDelete,
  onDeleteFolder,
  onStartRename,
  onCommitRename,
  onCancelRename,
  onContextMenu,
  onPointerDownFile,
  onConsumeSuppressedClick,
}: {
  node: NoteInfo;
  depth: number;
  renamingPath: string | null;
  renameValue: string;
  setRenameValue: (value: string) => void;
  draggingPath: string | null;
  dropTarget: DropTarget | null;
  onOpen: (path: string) => void;
  onDelete: (path: string) => void;
  onDeleteFolder: (path: string) => void;
  onStartRename: (node: NoteInfo) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onContextMenu: (event: MouseEvent, node: NoteInfo) => void;
  onPointerDownFile: (event: ReactPointerEvent, node: NoteInfo) => void;
  onConsumeSuppressedClick: () => boolean;
}) {
  const [expanded, setExpanded] = useState(true);
  const clickTimer = useRef<number | null>(null);
  const isRenaming = renamingPath === node.path;
  const isDragged = draggingPath === node.path;
  const isFolderDropTarget = dropTargetMatches(dropTarget, node);
  const paddingLeft = `${depth * 12 + (node.is_dir ? 8 : 24)}px`;

  useEffect(() => {
    return () => {
      if (clickTimer.current !== null) {
        window.clearTimeout(clickTimer.current);
      }
    };
  }, []);

  const children = expanded
    ? node.children?.map((child) => (
        <TreeNode
          key={child.path}
          node={child}
          depth={depth + 1}
          renamingPath={renamingPath}
          renameValue={renameValue}
          setRenameValue={setRenameValue}
          draggingPath={draggingPath}
          dropTarget={dropTarget}
          onOpen={onOpen}
          onDelete={onDelete}
          onDeleteFolder={onDeleteFolder}
          onStartRename={onStartRename}
          onCommitRename={onCommitRename}
          onCancelRename={onCancelRename}
          onContextMenu={onContextMenu}
          onPointerDownFile={onPointerDownFile}
          onConsumeSuppressedClick={onConsumeSuppressedClick}
        />
      ))
    : null;

  return (
    <div>
      <div
        data-note-path={node.path}
        data-drop-folder={node.is_dir ? "true" : undefined}
        className={`relative flex items-center justify-between px-2 py-1 text-sm group hover:bg-[#eaeef2] ${
          node.is_dir ? "cursor-pointer" : "cursor-grab active:cursor-grabbing"
        } ${isFolderDropTarget ? "bg-[#ddf4ff] text-[#0969da] ring-1 ring-inset ring-[#0969da]/35" : ""} ${isDragged ? "opacity-45" : ""}`}
        style={{ paddingLeft, touchAction: "none" }}
        draggable={false}
        onDragStart={(event) => event.preventDefault()}
        onPointerDown={(event) => {
          if (!node.is_dir && !isRenaming) {
            onPointerDownFile(event, node);
          }
        }}
        onClick={() => {
          if (onConsumeSuppressedClick()) return;
          if (isRenaming) return;
          if (clickTimer.current !== null) {
            window.clearTimeout(clickTimer.current);
          }
          clickTimer.current = window.setTimeout(() => {
            clickTimer.current = null;
            if (node.is_dir) {
              setExpanded((e) => !e);
            } else {
              onOpen(node.path);
            }
          }, 180);
        }}
        onDoubleClick={(event) => {
          event.stopPropagation();
          if (clickTimer.current !== null) {
            window.clearTimeout(clickTimer.current);
            clickTimer.current = null;
          }
          onStartRename(node);
        }}
        onContextMenu={(event) => onContextMenu(event, node)}
      >
        <div className="flex items-center gap-1 min-w-0 flex-1">
          {node.is_dir && (expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />)}
          {node.is_dir ? <Folder size={14} className="text-[#2da44e]" /> : <FileText size={14} className="text-[#57606a] shrink-0" />}
          {isRenaming ? (
            <input
              autoFocus
              className="min-w-0 flex-1 rounded border border-[#0969da] bg-white px-1 text-sm outline-none"
              value={renameValue}
              onFocus={(event) => event.currentTarget.select()}
              onChange={(event) => setRenameValue(event.target.value)}
              onClick={(event) => event.stopPropagation()}
              onDoubleClick={(event) => event.stopPropagation()}
              onKeyDown={(event) => {
                if (event.key === "Enter") onCommitRename();
                if (event.key === "Escape") onCancelRename();
              }}
              onBlur={onCommitRename}
            />
          ) : (
            <span className="truncate">{node.name}</span>
          )}
        </div>
        {!node.is_dir && !isRenaming && (
          <button
            className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-red-500/15 text-red-500"
            title="Delete"
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              if (confirm(`删除 ${node.name}?`)) onDelete(node.path);
            }}
          >
            <Trash2 size={12} />
          </button>
        )}
        {node.is_dir && !isRenaming && (
          <button
            className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-red-500/15 text-red-500"
            title="Delete folder"
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              if (confirm(`删除文件夹 ${node.name}?`)) onDeleteFolder(node.path);
            }}
          >
            <Trash2 size={12} />
          </button>
        )}
      </div>
      {children}
    </div>
  );
}

export default function Sidebar({
  notes,
  noteDir,
  onRefresh,
  onOpen,
  onImportFiles,
  onDelete,
  onDeleteFolder,
  onCreateNote,
  onCreateFolder,
  onRename,
  onSearch,
  onConvert,
}: {
  notes: NoteInfo[];
  noteDir: string;
  onRefresh: () => void;
  onOpen: (path: string) => void;
  onImportFiles: (files: File[]) => void | Promise<void>;
  onDelete: (path: string) => void;
  onDeleteFolder: (path: string) => void;
  onCreateNote: (path: string) => void | Promise<void>;
  onCreateFolder: (path: string) => void | Promise<void>;
  onRename: (oldPath: string, newPath: string) => void | Promise<void>;
  onSearch: (q: string) => void;
  onConvert: (path: string) => void | Promise<void>;
}) {
  const [search, setSearch] = useState("");
  const [menu, setMenu] = useState<MenuTarget | null>(null);
  const [renamingPath, setRenamingPath] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [dragState, setDragState] = useState<PointerDragState | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const skipRenameCommitRef = useRef(false);
  const dragStateRef = useRef<PointerDragState | null>(null);
  const dropTargetRef = useRef<DropTarget | null>(null);
  const suppressNextClickRef = useRef(false);
  const tree = useMemo(() => buildTree(notes), [notes]);

  useEffect(() => {
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("blur", close);
    };
  }, []);

  useEffect(() => {
    return () => {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, []);

  const setCurrentDropTarget = (target: DropTarget | null) => {
    dropTargetRef.current = target;
    setDropTarget(target);
  };

  const createFile = async (basePath = "") => {
    const path = uniquePath(notes, basePath, DEFAULT_FILE_NAME);
    await onCreateNote(path);
    setRenamingPath(path);
    setRenameValue(basename(path));
  };

  const createFolder = async (basePath = "") => {
    const path = uniquePath(notes, basePath, DEFAULT_FOLDER_NAME);
    await onCreateFolder(path);
    setRenamingPath(path);
    setRenameValue(basename(path));
  };

  const startRename = (node: NoteInfo) => {
    setMenu(null);
    skipRenameCommitRef.current = false;
    setRenamingPath(node.path);
    setRenameValue(node.name);
  };

  const commitRename = () => {
    if (skipRenameCommitRef.current) {
      skipRenameCommitRef.current = false;
      return;
    }
    if (!renamingPath) return;
    const nextName = renameValue.trim();
    const oldPath = renamingPath;
    const node = findNote(notes, oldPath);
    setRenamingPath(null);
    setRenameValue("");
    if (!nextName) return;

    let finalName = nextName;
    if (node && !node.is_dir && !hasSupportedExtension(finalName)) {
      const oldExt = splitExtension(basename(oldPath)).ext;
      finalName = `${finalName}${oldExt}`;
    }

    const nextPath = joinPath(parentPath(oldPath), finalName);
    if (nextPath !== oldPath) {
      onRename(oldPath, nextPath);
    }
  };

  const cancelRename = () => {
    skipRenameCommitRef.current = true;
    setRenamingPath(null);
    setRenameValue("");
  };

  const moveFileToFolder = async (draggedPath: string, targetFolder: string) => {
    const currentParent = parentPath(draggedPath);
    if (currentParent === targetFolder) return;
    const nextPath = uniquePath(notes, targetFolder, basename(draggedPath), draggedPath);
    await onRename(draggedPath, nextPath);
  };

  const finishPointerDrag = () => {
    dragStateRef.current = null;
    setDragState(null);
    setCurrentDropTarget(null);
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  };

  const resolveDropTarget = (x: number, y: number, draggedPath: string): DropTarget | null => {
    const element = document.elementFromPoint(x, y);
    const rowElement = element?.closest<HTMLElement>("[data-note-path]");
    const rowPath = rowElement?.dataset.notePath;
    if (rowPath) {
      const rowNote = findNote(notes, rowPath);
      const targetFolder = rowNote?.is_dir ? rowPath : parentPath(rowPath);
      const currentParent = parentPath(draggedPath);
      if (targetFolder) {
        return targetFolder === currentParent ? null : { kind: "folder", path: targetFolder };
      }
      return currentParent ? { kind: "root" } : null;
    }

    const list = listRef.current;
    if (list) {
      const rect = list.getBoundingClientRect();
      const insideList = x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
      if (insideList && parentPath(draggedPath)) {
        return { kind: "root" };
      }
    }

    return null;
  };

  const beginPointerDrag = (event: ReactPointerEvent, node: NoteInfo) => {
    if (event.button !== 0 || node.is_dir || renamingPath) return;
    const target = event.target as HTMLElement;
    if (target.closest("button,input")) return;

    event.preventDefault();
    event.stopPropagation();
    const initialState: PointerDragState = {
      path: node.path,
      name: node.name,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      active: false,
    };
    dragStateRef.current = initialState;
    setCurrentDropTarget(null);

    const onPointerMove = (moveEvent: globalThis.PointerEvent) => {
      const current = dragStateRef.current;
      if (!current) return;
      const dx = moveEvent.clientX - current.startX;
      const dy = moveEvent.clientY - current.startY;
      const shouldActivate = current.active || Math.hypot(dx, dy) >= DRAG_START_DISTANCE;
      if (!shouldActivate) return;

      moveEvent.preventDefault();
      moveEvent.stopPropagation();
      suppressNextClickRef.current = true;
      document.body.style.cursor = "grabbing";
      document.body.style.userSelect = "none";

      const nextState = {
        ...current,
        x: moveEvent.clientX,
        y: moveEvent.clientY,
        active: true,
      };
      dragStateRef.current = nextState;
      setDragState(nextState);
      setCurrentDropTarget(resolveDropTarget(moveEvent.clientX, moveEvent.clientY, current.path));
    };

    const onPointerUp = (upEvent: globalThis.PointerEvent) => {
      const current = dragStateRef.current;
      window.removeEventListener("pointermove", onPointerMove, true);
      window.removeEventListener("pointerup", onPointerUp, true);
      window.removeEventListener("pointercancel", onPointerCancel, true);

      if (current?.active) {
        upEvent.preventDefault();
        upEvent.stopPropagation();
        const target = resolveDropTarget(upEvent.clientX, upEvent.clientY, current.path) ?? dropTargetRef.current;
        if (target) {
          const targetFolder = target.kind === "folder" ? target.path : "";
          void moveFileToFolder(current.path, targetFolder);
        }
      }
      finishPointerDrag();
      window.setTimeout(() => {
        suppressNextClickRef.current = false;
      }, 0);
    };

    const onPointerCancel = () => {
      window.removeEventListener("pointermove", onPointerMove, true);
      window.removeEventListener("pointerup", onPointerUp, true);
      window.removeEventListener("pointercancel", onPointerCancel, true);
      finishPointerDrag();
      suppressNextClickRef.current = false;
    };

    window.addEventListener("pointermove", onPointerMove, true);
    window.addEventListener("pointerup", onPointerUp, true);
    window.addEventListener("pointercancel", onPointerCancel, true);
  };

  const consumeSuppressedClick = () => {
    if (!suppressNextClickRef.current) return false;
    suppressNextClickRef.current = false;
    return true;
  };

  const menuItems = useMemo<MenuItem[]>(() => {
    if (!menu) return [];
    if (menu.kind === "blank") {
      return [
        { label: "新建文件夹", onClick: () => void createFolder() },
        { label: "新建文件", onClick: () => void createFile() },
      ];
    }

    const { node } = menu;
    if (node.is_dir) {
      return [
        { label: "新建文件", onClick: () => void createFile(node.path) },
        { label: "重命名", onClick: () => startRename(node) },
        { label: "删除文件夹", danger: true, onClick: () => confirm(`删除文件夹 ${node.name}?`) && onDeleteFolder(node.path) },
      ];
    }

    const ext = extensionOf(node.path);
    const convertItem =
      ext === "html" || ext === "htm"
        ? { label: "转 Markdown 格式", onClick: () => void onConvert(node.path) }
        : ext === "md" || ext === "markdown"
          ? { label: "转 html 格式", onClick: () => void onConvert(node.path) }
          : null;
    return [
      { label: "新建文件", onClick: () => void createFile(parentPath(node.path)) },
      { label: "重命名", onClick: () => startRename(node) },
      { label: "删除文件", danger: true, onClick: () => confirm(`删除 ${node.name}?`) && onDelete(node.path) },
      ...(convertItem ? [convertItem] : []),
    ];
  }, [menu, notes, onConvert]);

  return (
    <div
      className="znote-sidebar relative w-64 flex flex-col border-r border-[#d8dee4] bg-[#f6f8fa] shrink-0"
      onDragStartCapture={(event) => event.preventDefault()}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ kind: "blank", x: event.clientX, y: event.clientY });
      }}
    >
      <div className="h-9 flex items-center justify-between px-3 border-b border-[#d8dee4]">
        <span className="text-xs font-medium text-[#57606a]">Documents</span>
        <div className="flex gap-1">
          <button onClick={onRefresh} className="p-1 rounded text-[#57606a] hover:bg-[#eaeef2]" title="Refresh">
            <RefreshCw size={14} />
          </button>
          <button
            onClick={() => inputRef.current?.click()}
            className="p-1 rounded text-[#57606a] hover:bg-[#eaeef2]"
            title="Import text"
          >
            <FileUp size={14} />
          </button>
          <input
            ref={inputRef}
            type="file"
            multiple
            accept=".md,.markdown,.html,.htm"
            className="hidden"
            onChange={async (event) => {
              const files = Array.from(event.currentTarget.files ?? []);
              event.currentTarget.value = "";
              if (files.length > 0) {
                await onImportFiles(files);
              }
            }}
          />
        </div>
      </div>
      <div className="px-2 py-2">
        <div className="flex items-center gap-1 px-2 py-1 rounded border border-[#d8dee4] bg-white">
          <Search size={12} className="text-[#57606a]" />
          <input
            className="bg-transparent text-xs outline-none w-full text-[#24292f] placeholder-[#6e7781]"
            placeholder="Search..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onSearch(search);
            }}
          />
        </div>
      </div>
      <div ref={listRef} className="relative flex-1 overflow-y-auto text-[#24292f]">
        {tree.length === 0 ? (
          <div className="px-4 py-8 text-xs text-center text-[#57606a]">
            <p>No documents</p>
            <p className="mt-1 break-all">{noteDir}</p>
          </div>
        ) : (
          tree.map((node) => (
            <TreeNode
              key={node.path}
              node={node}
              depth={0}
              renamingPath={renamingPath}
              renameValue={renameValue}
              setRenameValue={setRenameValue}
              draggingPath={dragState?.active ? dragState.path : null}
              dropTarget={dropTarget}
              onOpen={onOpen}
              onDelete={onDelete}
              onDeleteFolder={onDeleteFolder}
              onStartRename={startRename}
              onCommitRename={commitRename}
              onCancelRename={cancelRename}
              onPointerDownFile={beginPointerDrag}
              onConsumeSuppressedClick={consumeSuppressedClick}
              onContextMenu={(event, targetNode) => {
                event.preventDefault();
                event.stopPropagation();
                setMenu({ kind: "node", node: targetNode, x: event.clientX, y: event.clientY });
              }}
            />
          ))
        )}
        {dropTarget?.kind === "root" && <div className="mx-3 my-1 rounded bg-[#ddf4ff] px-2 py-1 text-xs text-[#0969da]">移动到根目录</div>}
      </div>
      <div className="px-3 py-2 text-[10px] border-t border-[#d8dee4] truncate text-[#57606a]">{noteDir}</div>
      {dragState?.active && (
        <div
          className="fixed z-[60] pointer-events-none flex max-w-56 items-center gap-1 rounded-md border border-[#0969da] bg-white px-2 py-1 text-xs text-[#24292f] shadow-lg"
          style={{ left: dragState.x + 12, top: dragState.y + 10 }}
        >
          <FileText size={13} className="shrink-0 text-[#57606a]" />
          <span className="truncate">{dragState.name}</span>
        </div>
      )}
      {menu && (
        <div
          className="fixed z-50 min-w-36 rounded-md border border-[#d8dee4] bg-white py-1 text-sm shadow-lg"
          style={{ left: menu.x, top: menu.y }}
          onContextMenu={(event) => event.preventDefault()}
          onClick={(event) => event.stopPropagation()}
        >
          {menuItems.map((item) => (
            <button
              key={item.label}
              className={`block w-full px-3 py-1.5 text-left hover:bg-[#eaeef2] ${item.danger ? "text-red-600" : "text-[#24292f]"}`}
              onClick={() => {
                setMenu(null);
                item.onClick();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
