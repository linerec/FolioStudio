// Copy the bundled feature sample into a portable project in Documents.
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const NAME = 'Feature_Atlas';
const STATE = path.join('.folio', 'sample.json'); // the project's machine-local state folder

/** The sample shipped in the app (resources/samples/feature-atlas), or the repository's. */
function bundled(resources, folder, entry) {
  const dirs = [path.join(__dirname, '..', '..', '..', 'samples', folder)];
  if (resources) dirs.unshift(path.join(resources, 'samples', folder));
  return dirs.find((d) => fs.existsSync(path.join(d, entry))) ?? null;
}

const sha = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const stamp = (file) => { try { const s = fs.statSync(file); return `${s.size}:${Math.round(s.mtimeMs)}`; } catch { return '-'; } };

function copyIfChanged(from, to) {
  if (fs.existsSync(to) && fs.statSync(to).size === fs.statSync(from).size && sha(to) === sha(from)) return;
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.copyFileSync(from, to);
}

function readState(root) {
  try { return JSON.parse(fs.readFileSync(path.join(root, STATE), 'utf8')); } catch { return {}; }
}

const two = (n) => String(n).padStart(2, '0');

let pending = null;

function projectFiles(root, folder = '') {
  const files = [];
  for (const entry of fs.readdirSync(path.join(root, folder), { withFileTypes: true })) {
    const relative = path.join(folder, entry.name);
    if (entry.name === '.folio') continue;
    if (entry.isDirectory()) files.push(...projectFiles(root, relative));
    else if (entry.isFile()) files.push(relative);
  }
  return files.sort();
}

function prepareMagazine(resources, documents) {
  const folder = 'riverbend-letters';
  const issue = 'november.folio';
  const source = bundled(resources, folder, issue);
  if (!source || !fs.existsSync(path.join(source, 'folio.project'))) return { error: 'missing' };
  const files = projectFiles(source);
  const hash = crypto.createHash('sha256');
  for (const file of files) hash.update(file.split(path.sep).join('/') + '\0' + sha(path.join(source, file)));
  const version = hash.digest('hex');
  const parent = path.join(documents, 'Folio Studio');
  fs.mkdirSync(parent, { recursive: true });
  let kept = null;
  for (let n = 0; ; n++) {
    const suffix = n === 0 ? '' : '-' + version.slice(0, 8) + (n === 1 ? '' : '-' + n);
    const root = path.join(parent, folder + suffix);
    const document = path.join(root, issue);
    const reply = { root, project: true, source: document, document, import: false };
    if (fs.existsSync(root)) {
      if (readState(root)?.version === version && fs.existsSync(document) && fs.existsSync(path.join(root, 'folio.project'))) return reply;
      if (!kept && fs.existsSync(document)) kept = document;
      continue;
    }
    const staging = fs.mkdtempSync(path.join(parent, '.riverbend-'));
    try {
      for (const file of files) copyIfChanged(path.join(source, file), path.join(staging, file));
      fs.mkdirSync(path.join(staging, '.folio'));
      fs.writeFileSync(path.join(staging, STATE), JSON.stringify({ version }));
      try {
        fs.renameSync(staging, root);
      } catch (error) {
        if (fs.existsSync(root)) { n--; continue; }
        throw error;
      }
      return { ...reply, kept };
    } finally {
      fs.rmSync(staging, { recursive: true, force: true });
    }
  }
}

/**
 * Gets the sample project ready. Reply: { root, project (folio.project there), source (the IDML
 * in the project), document (.folio to open), import (import it first), kept (a changed .folio
 * left as it was) } or { error }.
 */
function prepare({ kind = 'atlas', resources, documents, engine }) {
  if (kind === 'magazine') return prepareMagazine(resources, documents);
  if (kind !== 'atlas') return { error: 'missing' };
  const from = bundled(resources, 'feature-atlas', NAME + '.idml');
  if (!from) return { error: 'missing' };
  const root = path.join(documents, 'Folio Studio', 'Feature Atlas');
  const source = path.join(root, `${NAME}.idml`);
  copyIfChanged(path.join(from, `${NAME}.idml`), source);
  const links = path.join(from, 'Links');
  if (fs.existsSync(links)) for (const f of fs.readdirSync(links)) copyIfChanged(path.join(links, f), path.join(root, 'Links', f));
  const fingerprint = [sha(source), stamp(engine)].join('|');
  const saved = readState(root);
  const state = /^Feature_Atlas(?: \d{4}-\d\d-\d\d \d{4}(?:-\d+)?)?\.folio$/.test(saved?.document) ? saved : {};
  let name = state.document && fs.existsSync(path.join(root, state.document)) ? state.document : `${NAME}.folio`;
  let document = path.join(root, name);
  const project = fs.existsSync(path.join(root, 'folio.project'));
  if (fs.existsSync(document) && state.fingerprint === fingerprint) {
    pending = null;
    return { root, project, source, document, import: false };
  }
  let kept = null;
  if (fs.existsSync(document)) {
    if (state.folio && sha(document) === state.folio) {
      fs.rmSync(document); // the last import, unchanged: the new one takes its place
      if (!fs.existsSync(path.join(root, `${NAME}.folio`))) document = path.join(root, `${NAME}.folio`);
    } else {
      kept = document;
      const d = new Date();
      name = `${NAME} ${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}${two(d.getMinutes())}.folio`;
      document = path.join(root, name);
      for (let n = 2; fs.existsSync(document); n++) {
        document = path.join(root, name.replace('.folio', '-' + n + '.folio'));
      }
    }
  }
  pending = { root, document, fingerprint };
  return { root, project, source, document, import: true, kept };
}

/** After the import: what it was made from, and the .folio as written (to see later changes). */
function imported(document) {
  if (!pending || pending.document !== document || !fs.existsSync(document)) return false;
  const state = { fingerprint: pending.fingerprint, document: path.basename(document), folio: sha(document) };
  fs.mkdirSync(path.join(pending.root, '.folio'), { recursive: true });
  fs.writeFileSync(path.join(pending.root, STATE), JSON.stringify(state, null, 1));
  pending = null;
  return true;
}

module.exports = { prepare, imported };
