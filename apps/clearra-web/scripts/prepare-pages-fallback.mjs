import { copyFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const appRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const buildRoot = join(appRoot, 'build');

await copyFile(join(buildRoot, 'index.html'), join(buildRoot, '404.html'));
