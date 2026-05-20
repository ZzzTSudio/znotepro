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

export interface ModelConfigView {
  api_url: string;
  model: string;
  has_api_key: boolean;
}

export interface StyleTemplate {
  id: string;
  name: string;
  description: string;
  css_file: string;
  html_file: string;
}

export interface ConvertResult {
  output_path: string;
  output_name: string;
}

export interface ConvertProgress {
  file_name: string;
  current: number;
  total: number;
  stage: string;
}
