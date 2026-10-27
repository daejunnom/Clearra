import { openFieldDocument, type FieldDocumentReader } from './fieldInterchange';

type Request = {
  id: number;
  type: 'open' | 'readPage' | 'readPages' | 'decodeAll';
  input?: string;
  index?: number;
  start?: number;
  count?: number;
};

let reader: FieldDocumentReader | null = null;
self.addEventListener('message', (event: MessageEvent<Request>) => {
  void handle(event.data);
});

async function handle(request: Request) {
  try {
    let value: unknown;
    if (request.type === 'open') {
      if (reader || typeof request.input !== 'string') throw new Error('invalid-fumen-input');
      reader = openFieldDocument(request.input);
      value = { width: reader.width, pageCount: reader.pageCount };
    } else {
      if (!reader) throw new Error('Document reader is closed.');
      switch (request.type) {
        case 'readPage': value = await reader.readPage(request.index!); break;
        case 'readPages':
          if (!reader.readPages) throw new Error('Document page ranges are unavailable.');
          value = await reader.readPages(request.start!, request.count!);
          break;
        case 'decodeAll': value = await reader.decodeAll(); break;
        default: throw new Error('Invalid document reader request.');
      }
    }
    self.postMessage({ id: request.id, ok: true, value });
  } catch (error) {
    self.postMessage({ id: request.id, ok: false, error: error instanceof Error ? error.message : 'fumen-worker-failed' });
  }
}
