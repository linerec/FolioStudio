// Shared source inspection/runtime for the one-time migration and the ongoing audit.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), ts = require('typescript');
const root = path.resolve(__dirname, '../renderer/src');
function runtime(locale = 'ko-KR', platform = 'Win32') {
  const modules = new Map(), document = { documentElement: { lang: '' } };
  function load(p) {
    p = fs.existsSync(p + '.ts') ? p + '.ts' : fs.existsSync(p + '.tsx') ? p + '.tsx' : path.join(p, 'index.ts');
    if (modules.has(p)) return modules.get(p).exports;
    const module = { exports: {} }; modules.set(p, module);
    const js = ts.transpileModule(fs.readFileSync(p, 'utf8'), { compilerOptions: {
      module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2023,
    } }).outputText;
    vm.runInNewContext(js, { module, exports: module.exports, navigator: { language: locale, platform }, document,
      require: name => name === 'react' ? { useSyncExternalStore: (_, snapshot) => snapshot() } : load(path.resolve(path.dirname(p), name)),
    }, { filename: p });
    return module.exports;
  }
  return { load, document };
}
function files(dir = root) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(e => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? files(p) : /\.tsx?$/.test(p) ? [p] : [];
  });
}
// Line endings differ by checkout (core.autocrlf): JSX text must read the same either way.
function inspect(file, text = fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n')) {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true,
    file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const aliases = new Set();
  for (const node of source.statements) {
    if (!ts.isImportDeclaration(node) || !/(?:^|\/)(?:i18n|index)$/.test(node.moduleSpecifier.text)) continue;
    const bindings = node.importClause?.namedBindings;
    if (bindings && ts.isNamedImports(bindings)) {
      for (const b of bindings.elements) if (['t', 'tr'].includes((b.propertyName ?? b.name).text)) aliases.add(b.name.text);
    }
  }
  const calls = [], raw = [], translated = new Set();
  function visit(node) {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && aliases.has(node.expression.text)) {
      const arg = node.arguments[0];
      if (arg && ts.isStringLiteralLike(arg)) {
        translated.add(arg);
        calls.push({ key: arg.text, start: arg.getStart(source), end: arg.end,
          line: source.getLineAndCharacterOfPosition(arg.getStart(source)).line + 1 });
      }
    }
    if ((ts.isStringLiteralLike(node) || ts.isJsxText(node) ||
         ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)) &&
        /[\uAC00-\uD7A3]/.test(node.text) && !translated.has(node)) {
      raw.push({ text: node.text, kind: ts.SyntaxKind[node.kind],
        line: source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1 });
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  return { text, source, calls, raw };
}
module.exports = { root, runtime, files, inspect, ts };
