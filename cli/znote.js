const fs = require('fs');
const path = require('path');
const os = require('os');
const { execSync } = require('child_process');

const NOTE_DIR = path.join(os.homedir(), 'Documents', 'znote');
const INDEX_DIR = path.join(NOTE_DIR, '.search_index');
const META_PATH = path.join(INDEX_DIR, 'meta.json');
const IDX_PATH = path.join(INDEX_DIR, 'index.json');

function ensureDir(p) { if (!fs.existsSync(p)) fs.mkdirSync(p, { recursive: true }); }
function isDoc(f) { const e = path.extname(f).toLowerCase(); return e === '.html' || e === '.htm' || e === '.md' || e === '.markdown'; }
function relPath(full) { return path.relative(NOTE_DIR, full).replace(/\\/g, '/'); }

function tokenize(text) {
  const tokens = [];
  const re = /[\u4e00-\u9fa5]|[a-zA-Z0-9]+/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    tokens.push(m[0].toLowerCase());
  }
  return tokens;
}

function loadIndex() {
  if (!fs.existsSync(META_PATH) || !fs.existsSync(IDX_PATH)) return null;
  try {
    const meta = JSON.parse(fs.readFileSync(META_PATH, 'utf-8'));
    const postings = JSON.parse(fs.readFileSync(IDX_PATH, 'utf-8'));
    return { meta, postings };
  } catch { return null; }
}

function listNotes() {
  ensureDir(NOTE_DIR);
  const files = [];
  function walk(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (isDoc(full)) {
        const st = fs.statSync(full);
        files.push({ path: relPath(full), mtime: st.mtime });
      }
    }
  }
  walk(NOTE_DIR);
  files.sort((a, b) => b.mtime - a.mtime);
  for (const f of files) {
    console.log(`${f.path}  (${f.mtime.toISOString()})`);
  }
}

function saveNote(src) {
  ensureDir(NOTE_DIR);
  const srcPath = path.resolve(src);
  if (fs.statSync(srcPath).isDirectory()) {
    function walk(dir) {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(full);
        } else if (isDoc(full)) {
          const rel = relPath(full);
          const dest = path.join(NOTE_DIR, rel);
          ensureDir(path.dirname(dest));
          fs.copyFileSync(full, dest);
          console.log(`Imported: ${rel}`);
        }
      }
    }
    walk(srcPath);
  } else {
    const dest = path.join(NOTE_DIR, path.basename(srcPath));
    fs.copyFileSync(srcPath, dest);
    console.log(`Imported: ${path.basename(srcPath)}`);
  }
}

function bm25Search(query) {
  const idx = loadIndex();
  if (!idx) {
    console.log('索引不存在，执行全文扫描...');
    fullScanSearch(query);
    return;
  }
  const { meta, postings } = idx;
  const terms = tokenize(query);
  if (terms.length === 0) return;
  const docs = meta.documents;
  const totalDocs = meta.total_docs || 1;
  const avgLen = meta.avg_doc_length || 1;
  const scores = new Map();

  for (const term of terms) {
    const posts = postings[term];
    if (!posts) {
      const fallbackPosts = [];
      for (const [key, val] of Object.entries(postings)) {
        if (key.includes(term)) {
          fallbackPosts.push(...val);
        }
      }
      if (fallbackPosts.length === 0) continue;
      for (const p of fallbackPosts) {
        const doc = docs[String(p.doc_id)];
        if (!doc) continue;
        const tf = p.term_frequency;
        const idf = Math.log((totalDocs + 0.5) / (fallbackPosts.length + 0.5) + 1);
        const norm = (tf * 2.2) / (tf + 1.2 * (1 - 0.75 + 0.75 * (doc.doc_length / avgLen)));
        const prev = scores.get(p.doc_id) || 0;
        scores.set(p.doc_id, prev + idf * norm);
      }
      continue;
    }
    for (const p of posts) {
      const doc = docs[String(p.doc_id)];
      if (!doc) continue;
      const tf = p.term_frequency;
      const idf = Math.log((totalDocs + 0.5) / (posts.length + 0.5) + 1);
      const norm = (tf * 2.2) / (tf + 1.2 * (1 - 0.75 + 0.75 * (doc.doc_length / avgLen)));
      const prev = scores.get(p.doc_id) || 0;
      scores.set(p.doc_id, prev + idf * norm);
    }
  }

  const results = [];
  for (const [docId, score] of scores.entries()) {
    const doc = docs[String(docId)];
    if (!doc) continue;
    let finalScore = score;
    const reasons = [];
    const titleLower = doc.title.toLowerCase();
    const pathLower = doc.path.toLowerCase();
    const qLower = query.toLowerCase();
    if (terms.some(t => titleLower.includes(t) || pathLower.includes(t))) {
      finalScore *= 3; reasons.push('title_match');
    }
    if (titleLower.includes(qLower) || pathLower.includes(qLower)) {
      finalScore *= 2; reasons.push('exact_match');
    }
    results.push({ file: doc.path, score: finalScore, bm25: score, title: doc.title || doc.path, reasons });
  }
  results.sort((a, b) => b.score - a.score);
  for (const r of results.slice(0, 20)) {
    console.log(`[${r.score.toFixed(2)}] ${r.file}  boost: ${r.reasons.join(',')}`);
  }
}

function fullScanSearch(query) {
  const terms = tokenize(query);
  const results = [];
  function walk(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (isDoc(full)) {
        const content = fs.readFileSync(full, 'utf-8').toLowerCase();
        if (terms.every(t => content.includes(t))) {
          results.push(relPath(full));
        }
      }
    }
  }
  walk(NOTE_DIR);
  for (const r of results) console.log(r);
}

function showNote(name) {
  const full = path.join(NOTE_DIR, name);
  if (fs.existsSync(full)) {
    console.log(fs.readFileSync(full, 'utf-8'));
  } else {
    console.error('File not found:', name);
    process.exit(1);
  }
}

function printHelp() {
  console.log(`Usage: znote [options]
  -ls            List all notes
  -save <path>   Import file or directory
  -search <q>    Search notes
  -show <name>   Show note content
  -reindex       Rebuild search index (GUI only)
  -help          Show this help`);
}

function main() {
  const args = process.argv.slice(2);
  if (args.length === 0) {
    const gui = path.join(process.env.LOCALAPPDATA || '', 'Programs', 'znote', 'znote.exe');
    if (fs.existsSync(gui)) {
      execSync(`"${gui}"`, { stdio: 'inherit' });
    } else {
      printHelp();
    }
    return;
  }
  const cmd = args[0];
  if (cmd === '-ls') listNotes();
  else if (cmd === '-save') saveNote(args[1]);
  else if (cmd === '-search') bm25Search(args[1] || '');
  else if (cmd === '-show') showNote(args[1] || '');
  else if (cmd === '-help') printHelp();
  else printHelp();
}

main();
