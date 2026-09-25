// Path identity may be case-folded for comparison. Compiler module identifiers
// must retain the filesystem spelling (in particular on Windows). This resolver
// is read-only and never grants authority outside the already validated path.
import { realpathSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { assertBuildPathWithin, assertNoBuildLinks } from './clearra-build-policy.mjs';

export function resolveBuildFilesystemPath(value) {
  const selected = resolve(value);
  assertNoBuildLinks(selected);
  let ancestor = selected;
  const missing = [];
  for (;;) {
    try {
      const physical = resolve(realpathSync.native(ancestor), ...missing);
      // Case aliases are allowed; link escapes and another physical root are not.
      assertBuildPathWithin(physical, selected);
      assertBuildPathWithin(selected, physical);
      assertNoBuildLinks(physical);
      return physical;
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      const parent = dirname(ancestor);
      if (parent === ancestor) throw error;
      missing.unshift(basename(ancestor));
      ancestor = parent;
    }
  }
}
