import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

export function clearraManageExecutable(root, environment = process.env) {
  const executable = process.platform === 'win32' ? 'clearra-manage.exe' : 'clearra-manage';
  const candidates = [
    environment.CLEARRA_MANAGE_BIN,
    resolve(root, 'build', 'cargo', 'default', 'release', executable),
    resolve(root, 'build', 'cargo', 'default', 'debug', executable),
    resolve(root, 'build', 'tools', 'clearra-manage', 'host', 'release', executable),
  ].filter(Boolean);
  const selected = candidates.find(candidate => existsSync(candidate));
  if (selected) return selected;
  throw new Error(
    'The Rust Clearra manager is not built. Run cargo build --locked -p clearra-manage --release.'
  );
}
