import { parentPort, workerData } from "node:worker_threads";

import { renderDocumentGif } from "./gif.mjs";

if (!parentPort) throw new Error("The GIF renderer worker requires a parent port.");

try {
  const bytes = renderDocumentGif(workerData.document, workerData.options);
  parentPort.postMessage({ ok: true, bytes }, [bytes.buffer]);
} catch (error) {
  parentPort.postMessage({
    ok: false,
    name: error instanceof Error ? error.name : "Error",
    message:
      error instanceof Error && error.message
        ? error.message.slice(0, 512)
        : "The image preview could not be rendered.",
  });
}
