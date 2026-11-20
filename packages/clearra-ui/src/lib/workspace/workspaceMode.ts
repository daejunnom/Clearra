// Route admission and renderer selection use the same registry. A host's
// visible menu is deliberately independent of its accepted deep links.
const workspaceToolRoutes = {
  pc: { kind: 'pc' },
  setup: { kind: 'setup' },
  'setup-score': { kind: 'setup-score' },
  'spin-structure': { kind: 'spin-structure' },
  build: { kind: 'build' },
  'build-probability': { kind: 'build-probability' },
  sequence: { kind: 'sequence' },
  'sequence-dependencies': { kind: 'sequence-dependencies' },
  parity: { kind: 'document-utility', tool: 'parity' },
  fumen: { kind: 'document-utility', tool: 'fumen' },
  render: { kind: 'document-utility', tool: 'render' },
  'to-gray': { kind: 'document-utility', tool: 'to-gray' },
  mirror: { kind: 'document-utility', tool: 'mirror' },
  damage: { kind: 'forward-search', tool: 'damage' },
  'spin-finder': { kind: 'forward-search', tool: 'spin-finder' },
  ren: { kind: 'forward-search', tool: 'ren' },
  recovery: { kind: 'recovery' },
  ctk: { kind: 'ctk' },
  player: { kind: 'player' },
} as const;

export type WorkspaceMode = keyof typeof workspaceToolRoutes;

export function isWorkspaceMode(value: string | null): value is WorkspaceMode {
  return value !== null && Object.hasOwn(workspaceToolRoutes, value);
}

export function workspaceToolRoute(mode: WorkspaceMode | null) {
  return workspaceToolRoutes[mode ?? 'pc'];
}
