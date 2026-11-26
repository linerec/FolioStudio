// The feature sample, InDesign_Feature_Atlas.indd (51 pages, 181 samples in 29 groups, made and
// saved by InDesign 20.5; fixtures/indesign-feature-atlas-20261018): it ships with the app so any
// computer opens it with one button while Folio is being built. It is copied into a project under
// Documents (Folio Studio/Feature Atlas). A new engine, reader or sample imports it again so the
// latest reading shows; a .folio the person changed is kept and the new import gets its own name.
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const NAME = 'InDesign_Feature_Atlas';
const STATE = path.join('.folio', 'sample.json'); // the project's machine-local state folder

/** The sample shipped in the app (resources/samples/feature-atlas), or the repository's. */
function bundled(resources) {
  const dirs = [path.join(resources || '', 'samples', 'feature-atlas'), path.join(__dirname, '..', '..', '..', 'fixtures', 'indesign-feature-atlas-20261018')];
  return dirs.find((d) => fs.existsSync(path.join(d, `${NAME}.indd`))) ?? null;
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

/**
 * Gets the sample project ready. Reply: { root, project (folio.project there), source (the INDD
 * in the project), document (.folio to open), import (import it first), kept (a changed .folio
 * left as it was) } or { error }.
 */
function prepare({ resources, documents, engine }) {
  const from = bundled(resources);
  if (!from) return { error: 'missing' };
  const root = path.join(documents, 'Folio Studio', 'Feature Atlas');
  const source = path.join(root, `${NAME}.indd`);
  copyIfChanged(path.join(from, `${NAME}.indd`), source);
  const links = path.join(from, 'Links');
  if (fs.existsSync(links)) for (const f of fs.readdirSync(links)) copyIfChanged(path.join(links, f), path.join(root, 'Links', f));
  const reader = engine ? path.join(path.dirname(engine), process.platform === 'win32' ? 'folio-indd-reader.exe' : 'folio-indd-reader') : '';
  const fingerprint = [sha(source), stamp(engine), stamp(reader)].join('|');
  const state = readState(root);
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
