import type { WorkspaceMode } from './workspaceMode';

export const PC_SOLVER_HREF_CONTEXT = 'clearra-pc-solver-href';
export const WORKSPACE_MODE_VISIBILITY_CONTEXT = 'clearra-workspace-mode-visibility';

// Pages keeps the intentionally small v0.7.5 navigation surface while the
// implementation and explicit query routes for advanced tools remain intact.
export const PAGES_ESSENTIAL_WORKSPACE_MODES = Object.freeze([
  'pc',
  'setup',
  'build-probability',
  'damage',
  'spin-finder',
  'ctk',
  'player'
] satisfies readonly WorkspaceMode[]);
