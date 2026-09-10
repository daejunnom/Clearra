// Manual read-only A/B measurement; deliberately excluded from release gates.
import { writeFile } from "node:fs/promises";
import { parseArgs } from "node:util";
import { performance } from "node:perf_hooks";
import { runBoundedCommand } from "../release/bounded-command.mjs";
import { canonicalSha256 } from "../release/canonical-release-evidence.mjs";

const { values } = parseArgs({ options: {
  project: { type: "string" }, region: { type: "string" }, service: { type: "string" },
  revision: { type: "string" }, "stable-url": { type: "string" },
  "tagged-url": { type: "string" }, "gcloud-script": { type: "string" },
  output: { type: "string" }, pairs: { type: "string", default: "6" },
} });
const pairs = Number(values.pairs);
if (!Number.isSafeInteger(pairs) || pairs < 2 || pairs > 12) throw new Error("Use 2 through 12 pairs");
for (const field of ["project", "region", "service", "revision"]) {
  if (!/^[a-z][a-z0-9-]+$/u.test(values[field] ?? "")) throw new Error(`Invalid ${field}`);
}
const healthUrls = ["stable-url", "tagged-url"].map((key) => {
  const url = new URL(values[key]);
  if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/") {
    throw new Error("Health origins must be credential-free HTTPS origins");
  }
  return new URL("health", url);
});
const base = [`--project=${values.project}`, `--region=${values.region}`];
const command = process.platform === "win32" ? "pwsh.exe" : "gcloud";
const launcher = process.platform === "win32"
  ? ["-NoProfile", "-NonInteractive", "-File", values["gcloud-script"]] : [];
const control = async (kind) => {
  const args = kind === "service"
    ? ["run", "services", "describe", values.service, ...base,
      "--format=json(metadata.name,status.traffic,status.url)"]
    : ["run", "revisions", "describe", values.revision, ...base,
      "--format=json(metadata.name,status.conditions,status.imageDigest)"];
  const bytes = await runBoundedCommand(command, [...launcher, ...args], {
    timeoutMs: 45_000, maxBytes: 256 * 1024, label: `A/B ${kind} read`,
  });
  return JSON.parse(bytes.toString("utf8"));
};
const health = async (origin) => {
  const url = new URL(origin);
  url.searchParams.set("read_schedule_ab", String(performance.now()));
  const response = await fetch(url, {
    redirect: "error", cache: "no-store", signal: AbortSignal.timeout(30_000),
    headers: { "cache-control": "no-cache, no-store, max-age=0", pragma: "no-cache" },
  });
  if (!response.ok) throw new Error(`Health returned ${response.status}`);
  return response.json();
};
const run = async (mode) => {
  const start = performance.now();
  const controlStart = start;
  const controlValues = mode === "A"
    ? [await control("service"), await control("revision")]
    : await Promise.all([control("service"), control("revision")]);
  const controlMs = performance.now() - controlStart;
  const healthStart = performance.now();
  const healthValues = mode === "A"
    ? [await health(healthUrls[0]), await health(healthUrls[1])]
    : await Promise.all(healthUrls.map(health));
  return {
    mode, total_ms: performance.now() - start, control_ms: controlMs,
    health_ms: performance.now() - healthStart,
    control_sha256: canonicalSha256(controlValues),
    health_sha256: canonicalSha256(healthValues),
  };
};
await run("A"); // Warm authentication and connections before paired timing.
const trials = [];
for (let index = 0; index < pairs; index += 1) {
  for (const mode of index % 2 === 0 ? ["A", "B"] : ["B", "A"]) {
    const trial = { pair: index + 1, ...await run(mode) };
    trials.push(trial);
    process.stdout.write(`${JSON.stringify(trial)}\n`);
  }
}
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const summary = Object.fromEntries(["A", "B"].map((mode) => {
  const selected = trials.filter((trial) => trial.mode === mode);
  return [mode, Object.fromEntries(["total_ms", "control_ms", "health_ms"]
    .map((key) => [key, median(selected.map((trial) => trial[key]))]))];
}));
const report = {
  measured_at: new Date().toISOString(), platform: process.platform, pairs,
  project: values.project, service: values.service, revision: values.revision,
  authority_stable: new Set(trials.map((trial) => trial.control_sha256)).size === 1,
  health_stable: new Set(trials.map((trial) => trial.health_sha256)).size === 1,
  summary, improvement_percent: 100 * (1 - summary.B.total_ms / summary.A.total_ms), trials,
};
await writeFile(values.output, JSON.stringify(report, null, 2) + "\n", { flag: "wx" });
process.stdout.write(JSON.stringify({ summary, improvement_percent: report.improvement_percent }) + "\n");
