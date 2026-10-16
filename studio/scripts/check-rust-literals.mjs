// Fail fast on Rust literals like [.10,.20] that prevent desktop startup.
// Cargo parses cfg(test) functions in normal builds; this is a lightweight
// pre-Vite source check, NOT a replacement for cargo check/test.
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const studio = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const rustDir = path.join(studio, 'src-tauri', 'src');
const invalidLiteral = /(?<![\w.:\]\)\?])\.(?:\d+)(?!\w)/g;

function problems(source, filename) {
  const failures = [];
  for (const [index, line] of source.split('\n').entries()) {
    invalidLiteral.lastIndex = 0;
    for (const match of line.matchAll(invalidLiteral)) {
      failures.push(path.relative(studio, filename)+':' + (index+1)+':' +
        (match.index+1)+' - Rust fractional literal '+match[0]+
        ' requires a leading zero (e.g. 0'+match[0]+').');
    }
  }
  return failures;
}

if (process.argv.includes('--self-test')) {
  const examples = ['[.10,.20]', 'let x = .45;', 'end:[.80,.95]'];
  const valid = ['foo?.0', 'rgba.get_pixel(0,0).0', 'processed[3].1',
                 '{value:.4}', '0.15', 'tuple.0'];
  if (!examples.every(value => problems(value, 'test.rs').length > 0) ||
      !valid.every(value => problems(value, 'test.rs').length === 0)) {
    throw new Error('Rust leading-dot literal regression guard failed');
  }
  console.log('Rust decimal-literal regression self-test: PASS');
}
if (process.argv.includes('--self-test-only')) process.exit(0);

const files = (await readdir(rustDir)).filter(name => name.endsWith('.rs')).sort();
const failures = [];
for (const name of files) {
  const file = path.join(rustDir, name);
  failures.push(...problems(await readFile(file, 'utf8'), file));
}
if (failures.length) {
  console.error('Invalid Rust fractional literal(s):\n' + failures.join('\n'));
  process.exitCode = 1;
} else {
  console.log('Rust fractional-literal guard: PASS (' + files.length + ' modules).');
}
