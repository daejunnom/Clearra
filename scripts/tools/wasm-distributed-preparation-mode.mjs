const MODES = Object.freeze([
  Object.freeze({ label: 'serial', route: 'serial' }),
  Object.freeze({ label: 'cpu-multi', route: 'distributed' }),
  Object.freeze({ label: 'gpu-multi', route: 'distributed' }),
  Object.freeze({ label: 'ready', route: 'ready' }),
]);

export function decodeWasmDistributedPreparationMode(value) {
  const mode = Number.isInteger(value) ? MODES[value] : undefined;
  if (!mode) throw new Error(`invalid Clearra WASM distributed mode ${value}`);
  return mode;
}

export async function dispatchWasmDistributedPreparation(mode, routes) {
  if (mode.route === 'serial') return routes.serial();
  if (mode.route === 'ready') return routes.ready();
  return routes.distributed();
}
