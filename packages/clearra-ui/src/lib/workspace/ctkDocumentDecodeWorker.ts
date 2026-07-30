/// <reference lib="webworker" />

import {
  decodeCtk3Segment,
  encodeCtk3Compact,
  type Ctk3DecodeWorkerRequest,
  type Ctk3DecodeWorkerResponse
} from './ctk3Codec';

self.onmessage = (event: MessageEvent<Ctk3DecodeWorkerRequest>) => {
  const request = event.data;
  try {
    const response: Ctk3DecodeWorkerResponse =
      request.type === 'decode'
        ? {
            type: 'decoded',
            taskId: request.taskId,
            document: decodeCtk3Segment(request.segment)
          }
        : {
            type: 'encoded',
            taskId: request.taskId,
            encoded: encodeCtk3Compact(request.document)
          };
    self.postMessage(response);
  } catch (error) {
    const response: Ctk3DecodeWorkerResponse = {
      type: 'failed',
      taskId: request.taskId,
      message: error instanceof Error ? error.message : 'CTK3 decoding failed.'
    };
    self.postMessage(response);
  }
};

export {};
