// Lightweight TypeScript/TSX *syntax* guard before Vite's dev server starts.
// A JSX syntax error should be surfaced immediately with file/line/column,
// rather than appearing midway through a costly Tauri/Rust startup.
// Uses TypeScript already declared in devDependencies; never installs packages.
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const studioRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceRoot = path.join(studioRoot, 'src');
const skipped = new Set(['node_modules', 'target', 'dist', '.git']);

async function collect(dir) {
  const found = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.isSymbolicLink() || skipped.has(entry.name)) continue;
    const absolute = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      found.push(...await collect(absolute));
    } else if (entry.isFile() && /\.tsx?$/.test(entry.name)) {
      found.push(absolute);
    }
  }
  return found;
}

function parseErrors(filename, source) {
  const kind = filename.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const parsed = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true, kind);
  return parsed.parseDiagnostics.map((diagnostic) => {
    const loc = parsed.getLineAndCharacterOfPosition(diagnostic.start ?? 0);
    const text = ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ');
    return `${path.relative(studioRoot, filename)}:${loc.line + 1}:${loc.character + 1} - TS${diagnostic.code}: ${text}`;
  });
}

// Regression for the exact 2026-10-15 App.tsx bug: a JSX conditional
// <p> ended without the closing JavaScript '}' before the next <div>.
if (process.argv.includes('--self-test')) {
  const invalid = 'const ui = <section>{flag && <p>QA FAIL</p><div>stats</div></section>;';
  const valid = 'const ui = <section>{flag && <p>QA FAIL</p>}<div>stats</div></section>;';
  if (parseErrors(path.join(sourceRoot, 'App.tsx'), invalid).length < 1 ||
      parseErrors(path.join(sourceRoot, 'App.tsx'), valid).length !== 0) {
    throw new Error('JSX syntax regression self-test failed');
  }
  console.log('JSX missing-} regression self-test: PASS');
}

const all = [...await collect(sourceRoot), path.join(studioRoot, 'vite.config.ts')].sort();
const failures = [];
for (const filename of all) {
  failures.push(...parseErrors(filename, await readFile(filename, 'utf8')));
}
if (failures.length) {
  console.error('\nTS/TSX source syntax errors:\n' + failures.join('\n'));
  process.exitCode = 1;
} else {
  console.log('TS/TSX syntax PASS (' + all.length + ' files).');
}
