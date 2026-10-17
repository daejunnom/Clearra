// Development resolves dependencies from their workspace owner; production
// bundles SSR completely because its managed output lives outside node_modules.
/**
 * @param {'serve' | 'build'} command
 * @returns {{ optimizeDeps: { include: string[] }, ssr: { noExternal?: true } }}
 */
export function frontendDependencyPolicy(command) {
  if (!['serve', 'build'].includes(command)) throw new Error('Unknown frontend command');
  return {
    optimizeDeps: {
      include: [
        '@clearra/ui > @lucide/svelte',
        '@clearra/ui > tetris-fumen',
        '@clearra/ui > @tauri-apps/api/core',
      ],
    },
    // Dev must let Node load CommonJS dependencies. noExternal: true makes
    // the SSR module runner evaluate their `exports` as an ESM global instead.
    ssr: command === 'build' ? { noExternal: true } : {},
  };
}
