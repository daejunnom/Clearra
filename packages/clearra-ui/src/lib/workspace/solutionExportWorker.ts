/// <reference lib="webworker" />

import { FastColoredFumenEncoder } from './fastFumenSolutionEncoder';
import {
  encodeCtkSolutionKeySegment,
  parseSolutionKey,
  type SolutionExportPage
} from './solutionExport';

type ExportWorkerRequest =
  | { type: 'ctk-segment'; taskId: number; keys: string[] }
  | { type: 'fumen-start'; jobId: number }
  | { type: 'fumen-chunk'; jobId: number; keys: string[] }
  | { type: 'fumen-pages-chunk'; jobId: number; pages: SolutionExportPage[] }
  | { type: 'fumen-finish'; jobId: number };

type ExportWorkerResponse =
  | { type: 'ctk-segment'; taskId: number; encoded: string }
  | { type: 'fumen-ready'; jobId: number }
  | { type: 'fumen-chunk'; jobId: number }
  | { type: 'fumen-finished'; jobId: number; encoded: string }
  | { type: 'failed'; taskId?: number; jobId?: number; code: string };

let fumenJobId = -1;
let fumenEncoder: FastColoredFumenEncoder | null = null;

self.onmessage = (event: MessageEvent<ExportWorkerRequest>) => {
  const request = event.data;
  try {
    if (request.type === 'ctk-segment') {
      post({
        type: 'ctk-segment',
        taskId: request.taskId,
        encoded: encodeCtkSolutionKeySegment(request.keys)
      });
      return;
    }
    if (request.type === 'fumen-start') {
      fumenJobId = request.jobId;
      fumenEncoder = new FastColoredFumenEncoder();
      post({ type: 'fumen-ready', jobId: request.jobId });
      return;
    }
    if (!fumenEncoder || request.jobId !== fumenJobId) {
      throw new Error('invalid-fumen-export-job');
    }
    if (request.type === 'fumen-chunk') {
      for (const key of request.keys) {
        const page = parseSolutionKey(key);
        if (!page) throw new Error('invalid-solution-key');
        fumenEncoder.append(page);
      }
      post({ type: 'fumen-chunk', jobId: request.jobId });
      return;
    }
    if (request.type === 'fumen-pages-chunk') {
      for (const page of request.pages) fumenEncoder.append(page);
      post({ type: 'fumen-chunk', jobId: request.jobId });
      return;
    }
    const encoded = fumenEncoder.finish();
    fumenEncoder = null;
    fumenJobId = -1;
    post({ type: 'fumen-finished', jobId: request.jobId, encoded });
  } catch (error) {
    fumenEncoder = null;
    fumenJobId = -1;
    post({
      type: 'failed',
      ...(request.type === 'ctk-segment'
        ? { taskId: request.taskId }
        : { jobId: request.jobId }),
      code: error instanceof Error ? error.message : 'solution-export-failed'
    });
  }
};

function post(message: ExportWorkerResponse) {
  self.postMessage(message);
}

export {};
