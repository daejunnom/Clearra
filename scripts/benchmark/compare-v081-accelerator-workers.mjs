// Read-only 1/2/11-worker identity comparison; never averages wall times
// across different worker topologies.
import { compareWorkerReceipts } from './v081-accelerator-abba-core.mjs';
import { readBenchmarkSummary } from './v081-accelerator-receipts.mjs';

if (process.argv.length < 3) {
  throw new Error('usage: node compare-v081-accelerator-workers.mjs SUMMARY.json [SUMMARY.json ...]');
}
const receipts = process.argv.slice(2).map(readBenchmarkSummary);
console.log(JSON.stringify(compareWorkerReceipts(receipts), null, 2));
