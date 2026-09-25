// Path identity may be case-folded for comparison. Compiler module identifiers
// must retain the filesystem spelling (in particular on Windows). This resolver
// is read-only and never grants authority outside the already validated path.
import { lstatSync, realpathSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { assertBuildPathWithin, assertNoBuildLinks } from './clearra-build-policy.mjs';

export function resolveBuildFilesystemPath(value) {
  const selected = resolve(value);
  assertNoBuildLinks(selected);
  let ancestor = selected;
  const missing = [];
  for (;;) {
    try {
      const selectedStat = lstatSync(ancestor, { bigint: true });
      const physicalAncestor = realpathSync.native(ancestor);
      assertNoBuildLinks(physicalAncestor);
      const physicalStat = lstatSync(physicalAncestor, { bigint: true });
      // Windows TEMP can use an 8.3 alias such as RUNNER~1. Case folding cannot
      // equate that spelling with runneradmin. Prove the existing ancestor's
      // device/file identity instead; never make lexical root policy permissive.
      if (selectedStat.isSymbolicLink() || physicalStat.isSymbolicLink() ||
          selectedStat.ino === 0n || selectedStat.dev !== physicalStat.dev ||
          selectedStat.ino !== physicalStat.ino) {
        throw new Error('Clearra filesystem path changed its physical ancestor');
      }
      const physical = resolve(physicalAncestor, ...missing);
      assertBuildPathWithin(physical, physicalAncestor);
      assertNoBuildLinks(selected);
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
