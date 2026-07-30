/// <reference lib="webworker" />

import type { Ctk3Document } from './ctk3Codec';
import { encodeFieldDocument } from './fieldInterchange';

self.onmessage = (event: MessageEvent<Ctk3Document>) => {
  try {
    self.postMessage({
      type: 'encoded',
      encoded: encodeFieldDocument(event.data, 'fumen')
    });
  } catch (error) {
    self.postMessage({
      type: 'failed',
      message:
        error instanceof Error ? error.message : 'Fumen document export failed.'
    });
  }
};

export {};
