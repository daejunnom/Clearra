// Preserve Cargo's two POSIX jobserver pipe descriptors across the Node rustc
// guard. Inheriting stdin/out/err alone closes every descriptor above 2.
import { fstatSync } from 'node:fs';

export function cargoJobserverStdio(environment = process.env, platform = process.platform, inspect = fstatSync) {
  if (platform === 'win32') return 'inherit'; // Cargo uses a named semaphore.
  const flags = environment.CARGO_MAKEFLAGS ?? '';
  const tokens = [...flags.matchAll(/(?:^|\s)--jobserver-(auth|fds)=(\S+)/gu)];
  if (tokens.length === 0 || tokens.every(token => token[1] === 'auth' && token[2].startsWith('fifo:'))) {
    return 'inherit'; // No jobserver or GNU make FIFO form; no anonymous FDs.
  }
  const pairs = tokens.map(token => {
    if (!/^\d+,\d+$/u.test(token[2])) throw new Error('Invalid Cargo jobserver descriptor pair');
    return token[2].split(',').map(Number);
  });
  const [read, write] = pairs[0];
  if (read === write || pairs.some(pair => pair[0] !== read || pair[1] !== write)
      || [read, write].some(fd => !Number.isSafeInteger(fd) || fd < 3 || fd > 1024)) {
    throw new Error('Invalid Cargo jobserver descriptor pair');
  }
  // Never forward an arbitrary file/socket from a forged or stale FD number.
  let ends;
  try { ends = [inspect(read), inspect(write)]; }
  catch { throw new Error('Cargo jobserver pipe descriptors were not inherited by the compiler guard'); }
  if (ends.some(end => !end.isFIFO()) || ends[0].dev !== ends[1].dev || ends[0].ino !== ends[1].ino) {
    throw new Error('Cargo jobserver descriptors do not identify the same pipe');
  }
  const stdio = Array(Math.max(read, write) + 1).fill('ignore');
  stdio[0] = stdio[1] = stdio[2] = 'inherit';
  stdio[read] = read;
  stdio[write] = write;
  return stdio;
}
