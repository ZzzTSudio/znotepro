export interface NoteInfo {
  path: string;
  name: string;
  is_dir: boolean;
  children?: NoteInfo[];
  mtime?: number;
}

export interface Tab {
  id: string;
  path: string;
  title: string;
  content: string;
  dirty: boolean;
  format: "html" | "markdown";
}

export interface MatchContext {
  line_number: number;
  line_text: string;
  context_before: string[];
  context_after: string[];
}

export interface SearchResult {
  file: string;
  score: number;
  title: string;
  matches: MatchContext[];
  boost_reasons: string[];
}
