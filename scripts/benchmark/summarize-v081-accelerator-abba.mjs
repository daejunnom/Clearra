// Read-only batch aggregation. Different binary, fixture, asset generation,
// worker count or result identity is rejected rather than averaged together.
import { mergeBatchReceipts } from './v081-accelerator-abba-core.mjs';
import { readBenchmarkSummary } from './v081-accelerator-receipts.mjs';

if (process.argv.length < 3) {
  throw new Error('usage: node summarize-v081-accelerator-abba.mjs SUMMARY.json [SUMMARY.json ...]');
}
const receipts = process.argv.slice(2).map(readBenchmarkSummary);
console.log(JSON.stringify(mergeBatchReceipts(receipts), null, 2));
