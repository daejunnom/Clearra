import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

// Preserve the existing Discord public-document non-discovery policy exactly.
// This owner needs neither CTK3 nor any product module/build.
export const HIDDEN_COMMAND_DESCRIPTION =
  /--diagnostics|(?:^|[\s`])\/verify(?=$|[\s`])|\$verify|>verify|diagnostic\.verify|VerifyKicks|`Verify`|\b(?:clearra|sfinder)\s+verify\b|\bverify\s+kicks\b|\bhidden\s+verify\b|\bverification\s+(?:scope|commands)\b|\b(?:reserved|hidden|internal|non-search)\s+diagnostic(?:s|\s+probes?)?\b|\bdiagnostic\s+(?:root|route|modal|boundary|probes?|feature)\b|\bdiagnostics?\s+intentionally\b/iu;

export function findPublicCommandDocViolations(documents) {
  const violations = [];
  for (const { path, text } of documents) {
    for (const match of text.matchAll(new RegExp(HIDDEN_COMMAND_DESCRIPTION.source, 'giu'))) {
      const start = match.index + match[0].search(/\S/u);
      const line = text.slice(0, start).split('\n').length;
      // Do not attach RegExp match objects: their .input includes the entire
      // source document and previously inflated a single CI failure by >1 MB.
      violations.push({ path, line, matched: match[0].trim().slice(0, 120) });
    }
  }
  return violations;
}

export function readPublicCommandDocs(repositoryRoot) {
  const root = resolve(repositoryRoot);
  const paths = [join(root, 'README.md'),
    join(root, 'apps/clearra-discord-bot/README.md'), ...markdownPaths(join(root, 'docs'))];
  return paths.map((path) => ({
    path: relative(root, path).split('\\').join('/'), text: readFileSync(path, 'utf8'),
  }));
}

function markdownPaths(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name, 'en'))
    .flatMap((entry) => {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) return markdownPaths(path);
      return entry.isFile() && entry.name.endsWith('.md') ? [path] : [];
    });
}
