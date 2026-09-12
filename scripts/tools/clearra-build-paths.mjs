import { assertManagedBuildTransaction } from './clearra-build-policy.mjs';
let field;
let sourceRoot;
const arguments_ = process.argv.slice(2);
if (arguments_.length % 2 !== 0) throw new Error('Build path options require values');
for (let index = 0; index < arguments_.length; index += 2) {
  if (arguments_[index] === '--field') field = arguments_[index + 1];
  else if (arguments_[index] === '--source-root') sourceRoot = arguments_[index + 1];
  else throw new Error('Unknown build path option');
}
const owner = assertManagedBuildTransaction({ sourceRoot });
const values = { root: owner.root, transaction: owner.transaction, 'cargo-target': owner.cargoTarget, 'source-root': owner.source_root };
if (!Object.hasOwn(values, field)) throw new Error('Unknown managed build path field');
console.log(values[field]);
