import {
  decodeCtk3Segment,
  encodeCtk3Compact,
  type Ctk3Document,
} from "./codec.js";
import type {
  Ctk3DecodeWorkerRequest,
  Ctk3DecodeWorkerResponse,
} from "./asyncCodec.js";

const scope = self as unknown as {
  onmessage: ((event: { data: Ctk3DecodeWorkerRequest }) => void) | null;
  postMessage(message: Ctk3DecodeWorkerResponse): void;
};

scope.onmessage = (event) => {
  const request = event.data;
  try {
    if (request.type === "decode") {
      const document: Ctk3Document = decodeCtk3Segment(request.segment);
      scope.postMessage({
        type: "decoded",
        taskId: request.taskId,
        document,
      });
    } else {
      scope.postMessage({
        type: "encoded",
        taskId: request.taskId,
        encoded: encodeCtk3Compact(request.document),
      });
    }
  } catch (error) {
    scope.postMessage({
      type: "failed",
      taskId: request.taskId,
      message: error instanceof Error ? error.message : "CTK3 decoding failed.",
    });
  }
};
