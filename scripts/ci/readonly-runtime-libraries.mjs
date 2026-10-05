// SRP rationale: extract only validated host-library paths from ldd output for
// the read-only functional fixture; binary section headings are not libraries.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function collectReadonlyRuntimeLibraries(output) {
  const libraries = new Set();
  for (const original of output.split(/\r?\n/u)) {
    const line = original.trim();
    if (!line || /^\/[^\r\n]+:$/u.test(line)) continue;
    if (/^linux-(?:vdso|gate)\.so\.\d+\s+\(0x[a-f\d]+\)$/iu.test(line)) continue;
    const match = /^(?:\S+\s+=>\s+)?(\/\S+)\s+\(0x[a-f\d]+\)$/iu.exec(line);
    if (!match) throw new Error(`unsupported or unresolved ldd entry: ${line}`);
    const library = match[1];
    if (!/^\/(?:lib|lib64|usr\/lib|usr\/lib64)\//u.test(library) ||
        library.split('/').some(component => component === '.' || component === '..')) {
      throw new Error(`ldd library is outside approved host directories: ${library}`);
    }
    libraries.add(library);
  }
  if (libraries.size === 0) throw new Error('ldd emitted no host runtime libraries');
  return [...libraries].sort();
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) throw new Error('requires the prepared dynamic-libraries.txt path');
  const libraries = collectReadonlyRuntimeLibraries(readFileSync(process.argv[2], 'utf8'));
  process.stdout.write(`${libraries.join('\n')}\n`);
}
