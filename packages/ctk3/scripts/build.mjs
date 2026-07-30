import { build } from "esbuild";

const shared = {
  entryPoints: ["src/index.ts"],
  bundle: true,
  platform: "neutral",
  target: "es2020",
  sourcemap: true,
  external: ["tetris-fumen"],
  logLevel: "info",
};

await Promise.all([
  build({
    ...shared,
    format: "esm",
    outfile: "dist/index.js",
  }),
  build({
    ...shared,
    format: "cjs",
    platform: "node",
    define: {
      "import.meta.url": "undefined",
    },
    outfile: "dist/index.cjs",
  }),
  build({
    entryPoints: ["src/decodeWorker.ts"],
    bundle: true,
    platform: "browser",
    target: "es2020",
    format: "esm",
    sourcemap: true,
    outfile: "dist/decodeWorker.js",
    logLevel: "info",
  }),
]);
