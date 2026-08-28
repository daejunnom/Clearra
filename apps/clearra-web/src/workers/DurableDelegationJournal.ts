// SRP rationale: this module has one change reason: crash-recoverable browser delegation authority.
// In-process CPU/memory leases remain in SharedExecutionResourceAuthority.

export const DELEGATION_JOURNAL_SCHEMA = 'clearra.delegation-journal.v1' as const;
export const DELEGATION_JOURNAL_HEADER_SCHEMA =
  'clearra.delegation-journal-header.v1' as const;
export const OFFER_ACCEPT_TIMEOUT_MS = 10_000;
export const WORKER_HEARTBEAT_INTERVAL_MS = 5_000;
export const PERSISTED_RENEWAL_INTERVAL_MS = 30_000;
export const ACTIVE_LEASE_EXPIRY_MS = 120_000;
export const TERMINAL_TOMBSTONE_RETENTION_MS = 24 * 60 * 60 * 1_000;

const ZERO_SHA256 = '0'.repeat(64);
const DATABASE_NAME = 'clearra-runtime-v1';
const DATABASE_VERSION = 1;
const EVENT_STORE = 'delegation-events-v1';
const META_STORE = 'delegation-meta-v1';
const HEAD_KEY = 'head';
const QUARANTINE_KEY = 'quarantine';
const HEADER_KEY = 'journal-header';
const MAX_U64 = (1n << 64n) - 1n;

export type DelegationPhase =
  | 'prepared'
  | 'offered'
  | 'accepted'
  | 'published'
  | 'running'
  | 'renewed'
  | 'result-sealed'
  | 'result-applied'
  | 'completed'
  | 'revoked'
  | 'expired'
  | 'cancelled'
  | 'failed-closed';

export type DelegationIdentity = Readonly<{
  jobId: string;
  taskId: string;
  coordinatorId: string;
  payloadSha256: string;
  requestSha256: string;
}>;

export type DelegationBudget = Readonly<{
  computeUnitsDecimal: string;
  memoryBytesDecimal: string;
}>;

export type DelegationToken = Readonly<{
  jobId: string;
  taskId: string;
  fencingTokenDecimal: string;
}>;

export type DelegationOffer = Readonly<{
  schemaVersion: 1;
  jobId: string;
  taskId: string;
  coordinatorId: string;
  payloadSha256: string;
  requestSha256: string;
  computeUnitsDecimal: string;
  memoryBytesDecimal: string;
  fencingTokenDecimal: string;
  acceptByUnixMsDecimal: string;
}>;

export type DelegationAcceptance = Readonly<{
  taskId: string;
  fencingTokenDecimal: string;
  workerId: string;
  reservationSha256: string;
}>;

export type ExecutableDelegationPermit = Readonly<{
  schemaVersion: 1;
  jobId: string;
  taskId: string;
  workerId: string;
  payloadSha256: string;
  requestSha256: string;
  fencingTokenDecimal: string;
  publicationSequenceDecimal: string;
  publicationSha256: string;
  expiresAtUnixMsDecimal: string;
}>;

export type DelegationEvent = Readonly<{
  schema: typeof DELEGATION_JOURNAL_SCHEMA;
  sequenceDecimal: string;
  jobId: string;
  taskId: string;
  coordinatorId: string;
  payloadSha256: string;
  requestSha256: string;
  computeUnitsDecimal: string;
  memoryBytesDecimal: string;
  phase: DelegationPhase;
  fencingTokenDecimal: string;
  workerId: string | null;
  reservationSha256: string | null;
  resultSha256: string | null;
  workerReplySha256: string | null;
  timestampUnixMsDecimal: string;
  reason: string | null;
  previousEventSha256: string;
  eventSha256: string;
}>;

type DelegationEventDraft = Omit<
  DelegationEvent,
  'schema' | 'sequenceDecimal' | 'previousEventSha256' | 'eventSha256'
>;

export interface DelegationJournal {
  load(): Promise<readonly DelegationEvent[]>;
  append(draft: DelegationEventDraft): Promise<DelegationEvent>;
  resetIfHead(expectedEventSha256: string): Promise<void>;
  quarantine(reason: string): Promise<void>;
  close(): void;
}

export type DelegationClock = () => number;

export class DurableDelegationError extends Error {
  constructor(
    readonly code:
      | 'invalid-identity'
      | 'identity-already-used'
      | 'unknown-delegation'
      | 'stale-fence'
      | 'invalid-transition'
      | 'offer-expired'
      | 'lease-expired'
      | 'journal-corrupt'
      | 'journal-head-changed'
      | 'journal-write-failed'
      | 'sequence-exhausted'
      | 'fencing-token-exhausted'
      | 'worker-rejected-offer',
    message: string,
    options?: ErrorOptions
  ) {
    super(message, options);
    this.name = 'DurableDelegationError';
  }
}

export class MemoryDelegationJournal implements DelegationJournal {
  private readonly events: DelegationEvent[];
  private appendTail: Promise<void> = Promise.resolve();
  private nextAppendFailure: Error | null = null;
  private nextResetFailure: Error | null = null;
  private quarantineReason: string | null = null;

  constructor(events: readonly DelegationEvent[] = []) {
    this.events = events.map((event) => Object.freeze({ ...event }));
  }

  failNextAppend(error = new Error('injected delegation journal failure')): void {
    this.nextAppendFailure = error;
  }

  failNextReset(error = new Error('injected delegation journal reset failure')): void {
    this.nextResetFailure = error;
  }

  async load(): Promise<readonly DelegationEvent[]> {
    if (this.quarantineReason) {
      throw new DurableDelegationError(
        'journal-corrupt',
        `delegation journal is quarantined: ${this.quarantineReason}`
      );
    }
    await validateEventChain(this.events);
    return this.events.map((event) => Object.freeze({ ...event }));
  }

  append(draft: DelegationEventDraft): Promise<DelegationEvent> {
    const operation = this.appendTail.then(async () => {
      if (this.quarantineReason) {
        throw new DurableDelegationError(
          'journal-corrupt',
          `delegation journal is quarantined: ${this.quarantineReason}`
        );
      }
      if (this.nextAppendFailure) {
        const failure = this.nextAppendFailure;
        this.nextAppendFailure = null;
        throw new DurableDelegationError(
          'journal-write-failed',
          'delegation journal durable append failed',
          { cause: failure }
        );
      }
      const event = await buildEvent(this.events, draft);
      this.events.push(event);
      return event;
    });
    this.appendTail = operation.then(
      () => undefined,
      () => undefined
    );
    return operation;
  }

  resetIfHead(expectedEventSha256: string): Promise<void> {
    const operation = this.appendTail.then(async () => {
      if (this.quarantineReason) {
        throw new DurableDelegationError(
          'journal-corrupt',
          `delegation journal is quarantined: ${this.quarantineReason}`
        );
      }
      if (this.nextResetFailure) {
        const failure = this.nextResetFailure;
        this.nextResetFailure = null;
        throw new DurableDelegationError(
          'journal-write-failed',
          'delegation journal durable reset failed',
          { cause: failure }
        );
      }
      assertExpectedHead(this.events, expectedEventSha256);
      this.events.length = 0;
    });
    this.appendTail = operation.then(
      () => undefined,
      () => undefined
    );
    return operation;
  }

  async quarantine(reason: string): Promise<void> {
    this.quarantineReason = reason;
  }

  close(): void {}
}

type IndexedDbHead = Readonly<{
  key: typeof HEAD_KEY;
  sequenceDecimal: string;
  eventSha256: string;
}>;

type IndexedDbQuarantine = Readonly<{
  key: typeof QUARANTINE_KEY;
  reason: string;
  timestampUnixMsDecimal: string;
}>;

type IndexedDbJournalHeader = Readonly<{
  key: typeof HEADER_KEY;
  schema: typeof DELEGATION_JOURNAL_HEADER_SCHEMA;
  jobId: string;
}>;

export class IndexedDbDelegationJournal implements DelegationJournal {
  private appendTail: Promise<void> = Promise.resolve();
  private events: DelegationEvent[] | null = null;
  private quarantineReason: string | null = null;
  private jobId: string | null = null;

  private constructor(private readonly database: IDBDatabase) {}

  static async open(
    factory: IDBFactory = globalThis.indexedDB,
    databaseName = DATABASE_NAME
  ): Promise<IndexedDbDelegationJournal> {
    if (!factory) {
      throw new DurableDelegationError(
        'journal-write-failed',
        'IndexedDB is unavailable for durable delegation'
      );
    }
    const request = factory.open(databaseName, DATABASE_VERSION);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(EVENT_STORE)) {
        database.createObjectStore(EVENT_STORE, { keyPath: 'sequenceDecimal' });
      }
      if (!database.objectStoreNames.contains(META_STORE)) {
        database.createObjectStore(META_STORE, { keyPath: 'key' });
      }
    };
    const database = await requestResult(request, 'open delegation journal');
    return new IndexedDbDelegationJournal(database);
  }

  static async openForJob(
    factory: IDBFactory,
    databaseName: string,
    jobId: string
  ): Promise<IndexedDbDelegationJournal> {
    if (!isUuid(jobId)) {
      throw new DurableDelegationError(
        'invalid-identity',
        'durable delegation job ID must be a canonical lowercase UUID'
      );
    }
    const journal = await IndexedDbDelegationJournal.open(factory, databaseName);
    journal.jobId = jobId;
    await journal.ensureJobHeader();
    return journal;
  }

  async load(): Promise<readonly DelegationEvent[]> {
    await this.assertNotQuarantined();
    if (this.events) return this.events.map((event) => Object.freeze({ ...event }));
    const transaction = this.database.transaction([EVENT_STORE, META_STORE], 'readonly');
    const completed = transactionCompletion(transaction, 'load delegation journal');
    const eventRequest = transaction.objectStore(EVENT_STORE).getAll();
    const headRequest = transaction.objectStore(META_STORE).get(HEAD_KEY);
    const [raw, rawHead] = await Promise.all([
      requestResult(eventRequest, 'load delegation journal'),
      requestResult(headRequest, 'load delegation journal head')
    ]);
    await completed;
    if (!Array.isArray(raw)) {
      throw new DurableDelegationError('journal-corrupt', 'delegation event store is invalid');
    }
    const events = raw.map(parseStoredEvent).sort(compareEventSequence);
    if (this.jobId && events.some((event) => event.jobId !== this.jobId)) {
      throw new DurableDelegationError(
        'journal-corrupt',
        'delegation event job ID does not match the durable journal header'
      );
    }
    await validateEventChain(events);
    const storedHead = parseStoredHead(rawHead);
    const eventHead = events.at(-1);
    if (
      storedHead?.eventSha256 !== eventHead?.eventSha256 ||
      storedHead?.sequenceDecimal !== eventHead?.sequenceDecimal
    ) {
      throw new DurableDelegationError(
        'journal-corrupt',
        `delegation journal committed head does not match the event chain: ${
          storedHead?.eventSha256 ?? '<empty>'
        } != ${eventHead?.eventSha256 ?? '<empty>'}`
      );
    }
    this.events = events;
    return events.map((event) => Object.freeze({ ...event }));
  }

  append(draft: DelegationEventDraft): Promise<DelegationEvent> {
    let resolveResult!: (event: DelegationEvent) => void;
    let rejectResult!: (error: unknown) => void;
    const result = new Promise<DelegationEvent>((resolve, reject) => {
      resolveResult = resolve;
      rejectResult = reject;
    });
    this.appendTail = this.appendTail
      .then(async () => {
        await this.assertNotQuarantined();
        if (this.jobId && draft.jobId !== this.jobId) {
          throw new DurableDelegationError(
            'journal-corrupt',
            'delegation job ID does not match the durable journal header'
          );
        }
        const existing = await this.load();
        const event = await buildEvent(existing, draft);
        const transaction = this.database.transaction([EVENT_STORE, META_STORE], 'readwrite');
        const completed = transactionCompletion(transaction, 'append delegation journal');
        const headRequest = transaction.objectStore(META_STORE).get(HEAD_KEY);
        const storedHead = parseStoredHead(
          await requestResult(headRequest, 'read delegation journal head')
        );
        const expectedHead = existing.at(-1);
        if (
          storedHead?.eventSha256 !== expectedHead?.eventSha256 ||
          storedHead?.sequenceDecimal !== expectedHead?.sequenceDecimal
        ) {
          await completed;
          throw journalHeadChanged(expectedHead?.eventSha256, storedHead?.eventSha256);
        }
        transaction.objectStore(EVENT_STORE).add(event);
        const head: IndexedDbHead = {
          key: HEAD_KEY,
          sequenceDecimal: event.sequenceDecimal,
          eventSha256: event.eventSha256
        };
        transaction.objectStore(META_STORE).put(head);
        // Resolving only from transaction.oncomplete is the durable ACK boundary.
        await completed;
        this.events ??= [...existing];
        this.events.push(event);
        resolveResult(event);
      })
      .catch((error) => {
        const failure = asDelegationJournalFailure(error, 'append delegation journal');
        rejectResult(failure);
      });
    return result;
  }

  resetIfHead(expectedEventSha256: string): Promise<void> {
    let resolveResult!: () => void;
    let rejectResult!: (error: unknown) => void;
    const result = new Promise<void>((resolve, reject) => {
      resolveResult = resolve;
      rejectResult = reject;
    });
    this.appendTail = this.appendTail
      .then(async () => {
        await this.assertNotQuarantined();
        const existing = await this.load();
        assertExpectedHead(existing, expectedEventSha256);
        const transaction = this.database.transaction([EVENT_STORE, META_STORE], 'readwrite');
        const completed = transactionCompletion(transaction, 'reset delegation journal');
        const headRequest = transaction.objectStore(META_STORE).get(HEAD_KEY);
        const storedHead = parseStoredHead(
          await requestResult(headRequest, 'read delegation journal head for reset')
        );
        if (storedHead?.eventSha256 !== expectedEventSha256) {
          await completed;
          throw journalHeadChanged(expectedEventSha256, storedHead?.eventSha256);
        }
        transaction.objectStore(EVENT_STORE).clear();
        transaction.objectStore(META_STORE).delete(HEAD_KEY);
        // The tombstones become reusable only after both stores commit.
        await completed;
        this.events = [];
        resolveResult();
      })
      .catch((error) => {
        rejectResult(asDelegationJournalFailure(error, 'reset delegation journal'));
      });
    return result;
  }

  async quarantine(reason: string): Promise<void> {
    const transaction = this.database.transaction([META_STORE], 'readwrite');
    const completed = transactionCompletion(transaction, 'quarantine delegation journal');
    const marker: IndexedDbQuarantine = {
      key: QUARANTINE_KEY,
      reason,
      timestampUnixMsDecimal: String(Date.now())
    };
    transaction.objectStore(META_STORE).put(marker);
    await completed;
    this.quarantineReason = reason;
  }

  close(): void {
    this.database.close();
  }

  private async assertNotQuarantined(): Promise<void> {
    if (this.quarantineReason) {
      throw new DurableDelegationError(
        'journal-corrupt',
        `delegation journal is quarantined: ${this.quarantineReason}`
      );
    }
    const transaction = this.database.transaction([META_STORE], 'readonly');
    const completed = transactionCompletion(transaction, 'read delegation quarantine');
    const request = transaction.objectStore(META_STORE).get(QUARANTINE_KEY);
    const marker = (await requestResult(request, 'read delegation quarantine')) as
      | IndexedDbQuarantine
      | undefined;
    await completed;
    if (marker?.key === QUARANTINE_KEY) {
      this.quarantineReason = marker.reason;
      throw new DurableDelegationError(
        'journal-corrupt',
        `delegation journal is quarantined: ${marker.reason}`
      );
    }
  }

  private async ensureJobHeader(): Promise<void> {
    const jobId = this.jobId;
    if (!jobId) return;
    const transaction = this.database.transaction([META_STORE], 'readwrite');
    const completed = transactionCompletion(transaction, 'bind delegation journal header');
    const store = transaction.objectStore(META_STORE);
    const raw = await requestResult(store.get(HEADER_KEY), 'read delegation journal header');
    if (raw === undefined) {
      const header: IndexedDbJournalHeader = {
        key: HEADER_KEY,
        schema: DELEGATION_JOURNAL_HEADER_SCHEMA,
        jobId
      };
      store.put(header);
    } else {
      const header = raw as Partial<IndexedDbJournalHeader>;
      if (
        header.key !== HEADER_KEY ||
        header.schema !== DELEGATION_JOURNAL_HEADER_SCHEMA ||
        header.jobId !== jobId
      ) {
        transaction.abort();
        await completed.catch(() => undefined);
        throw new DurableDelegationError(
          'journal-corrupt',
          'durable delegation journal header is malformed or belongs to another job'
        );
      }
    }
    await completed;
  }
}

type DelegationState = {
  identity: DelegationIdentity;
  budget: DelegationBudget;
  phase: DelegationPhase;
  fencingTokenDecimal: string;
  workerId: string | null;
  reservationSha256: string | null;
  resultSha256: string | null;
  workerReplySha256: string | null;
  offeredAtUnixMs: number | null;
  lastHeartbeatUnixMs: number;
  lastPersistedRenewalUnixMs: number;
  terminalAtUnixMs: number | null;
};

export class DurableDelegationAuthority {
  private readonly states = new Map<string, DelegationState>();
  private nextFencingToken = 0n;
  private mutationTail: Promise<void> = Promise.resolve();

  private constructor(
    private readonly journal: DelegationJournal,
    private readonly clock: DelegationClock
  ) {}

  static async recover(
    journal: DelegationJournal,
    clock: DelegationClock = () => Date.now()
  ): Promise<DurableDelegationAuthority> {
    const authority = new DurableDelegationAuthority(journal, clock);
    let events: readonly DelegationEvent[];
    try {
      events = await journal.load();
      for (const event of events) authority.replay(event);
      const unresolved = [...authority.states.values()].find(
        (state) => !isTerminal(state.phase)
      );
      if (unresolved) {
        throw new DurableDelegationError(
          'journal-corrupt',
          `process restart found unresolved delegation ${unresolved.identity.jobId}/${unresolved.identity.taskId} in phase ${unresolved.phase}; same-identity resume is forbidden`
        );
      }
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      try {
        await journal.quarantine(reason);
      } catch {
        // The original recovery failure remains authoritative. A failed
        // quarantine write must never make the journal usable.
      }
      throw error;
    }
    return authority;
  }

  async prepare(
    identity: DelegationIdentity,
    budget: DelegationBudget
  ): Promise<DelegationToken> {
    validateIdentity(identity);
    validateBudget(budget);
    const ownedIdentity = Object.freeze({ ...identity });
    const ownedBudget = Object.freeze({ ...budget });
    const operation = this.mutationTail.then(() => this.prepareNow(ownedIdentity, ownedBudget));
    this.mutationTail = operation.then(
      () => undefined,
      () => undefined
    );
    return operation;
  }

  private async prepareNow(
    identity: DelegationIdentity,
    budget: DelegationBudget
  ): Promise<DelegationToken> {
    const key = stateKey(identity.jobId, identity.taskId);
    if (this.states.has(key)) {
      throw new DurableDelegationError(
        'identity-already-used',
        `delegation identity ${identity.jobId}/${identity.taskId} was already used`
      );
    }
    const nextFencingToken = this.nextFencingToken + 1n;
    if (nextFencingToken > MAX_U64) {
      throw new DurableDelegationError(
        'fencing-token-exhausted',
        'delegation fencing token exceeds decimal u64'
      );
    }
    const fencingTokenDecimal = nextFencingToken.toString();
    const now = this.now();
    const event = await this.append({
      ...eventIdentity(identity, budget, fencingTokenDecimal),
      phase: 'prepared',
      workerId: null,
      reservationSha256: null,
      resultSha256: null,
      workerReplySha256: null,
      timestampUnixMsDecimal: String(now),
      reason: null
    });
    this.nextFencingToken = nextFencingToken;
    this.states.set(key, {
      identity: Object.freeze({ ...identity }),
      budget: Object.freeze({ ...budget }),
      phase: event.phase,
      fencingTokenDecimal,
      workerId: null,
      reservationSha256: null,
      resultSha256: null,
      workerReplySha256: null,
      offeredAtUnixMs: null,
      lastHeartbeatUnixMs: now,
      lastPersistedRenewalUnixMs: now,
      terminalAtUnixMs: null
    });
    return Object.freeze({
      jobId: identity.jobId,
      taskId: identity.taskId,
      fencingTokenDecimal
    });
  }

  async offered(token: DelegationToken): Promise<DelegationOffer> {
    const now = this.now();
    await this.transition(token, 'offered', now);
    const state = this.state(token);
    state.offeredAtUnixMs = now;
    return Object.freeze({
      schemaVersion: 1,
      jobId: state.identity.jobId,
      taskId: state.identity.taskId,
      coordinatorId: state.identity.coordinatorId,
      payloadSha256: state.identity.payloadSha256,
      requestSha256: state.identity.requestSha256,
      computeUnitsDecimal: state.budget.computeUnitsDecimal,
      memoryBytesDecimal: state.budget.memoryBytesDecimal,
      fencingTokenDecimal: state.fencingTokenDecimal,
      acceptByUnixMsDecimal: String(now + OFFER_ACCEPT_TIMEOUT_MS)
    });
  }

  async accepted(token: DelegationToken, acceptance: DelegationAcceptance): Promise<void> {
    const now = this.now();
    const state = this.state(token);
    if (
      acceptance.taskId !== token.taskId ||
      acceptance.fencingTokenDecimal !== token.fencingTokenDecimal ||
      !isPositiveU64Decimal(acceptance.workerId) ||
      !isSha256(acceptance.reservationSha256)
    ) {
      throw new DurableDelegationError(
        'worker-rejected-offer',
        'worker acceptance does not match the offered delegation'
      );
    }
    if (
      state.offeredAtUnixMs === null ||
      now - state.offeredAtUnixMs > OFFER_ACCEPT_TIMEOUT_MS
    ) {
      throw new DurableDelegationError('offer-expired', 'delegation offer acceptance expired');
    }
    await this.transition(
      token,
      'accepted',
      now,
      acceptance.workerId,
      acceptance.reservationSha256
    );
    state.workerId = acceptance.workerId;
    state.reservationSha256 = acceptance.reservationSha256;
  }

  async publish(token: DelegationToken): Promise<ExecutableDelegationPermit> {
    const now = this.now();
    const event = await this.transition(token, 'published', now);
    const state = this.state(token);
    state.lastHeartbeatUnixMs = now;
    state.lastPersistedRenewalUnixMs = now;
    if (!state.workerId) {
      throw new DurableDelegationError('invalid-transition', 'accepted worker identity is absent');
    }
    return Object.freeze({
      schemaVersion: 1,
      jobId: state.identity.jobId,
      taskId: state.identity.taskId,
      workerId: state.workerId,
      payloadSha256: state.identity.payloadSha256,
      requestSha256: state.identity.requestSha256,
      fencingTokenDecimal: state.fencingTokenDecimal,
      publicationSequenceDecimal: event.sequenceDecimal,
      publicationSha256: event.eventSha256,
      expiresAtUnixMsDecimal: String(now + ACTIVE_LEASE_EXPIRY_MS)
    });
  }

  async running(token: DelegationToken): Promise<void> {
    const now = this.now();
    this.ensureLive(token, now);
    await this.transition(token, 'running', now);
    this.state(token).lastHeartbeatUnixMs = now;
  }

  async heartbeat(token: DelegationToken): Promise<boolean> {
    const now = this.now();
    this.ensureLive(token, now);
    const state = this.state(token);
    state.lastHeartbeatUnixMs = now;
    if (now - state.lastPersistedRenewalUnixMs < PERSISTED_RENEWAL_INTERVAL_MS) return false;
    await this.transition(token, 'renewed', now);
    state.lastPersistedRenewalUnixMs = now;
    return true;
  }

  async resultSealed(
    token: DelegationToken,
    resultSha256: string,
    workerReplySha256: string
  ): Promise<void> {
    const now = this.now();
    this.ensureLive(token, now);
    if (!isSha256(resultSha256) || !isSha256(workerReplySha256)) {
      throw new DurableDelegationError('invalid-identity', 'delegation result digest is invalid');
    }
    const state = this.state(token);
    if (
      state.phase === 'result-sealed' ||
      state.phase === 'result-applied' ||
      state.phase === 'completed'
    ) {
      if (
        state.resultSha256 === resultSha256 &&
        state.workerReplySha256 === workerReplySha256
      ) {
        return;
      }
      throw new DurableDelegationError(
        'invalid-transition',
        'delegation result digest does not match the sealed result'
      );
    }
    await this.transition(
      token,
      'result-sealed',
      now,
      undefined,
      undefined,
      resultSha256,
      workerReplySha256
    );
    state.resultSha256 = resultSha256;
    state.workerReplySha256 = workerReplySha256;
  }

  resultApplicationDecision(
    token: DelegationToken,
    resultSha256: string
  ): 'apply-once' | 'already-applied' {
    if (!isSha256(resultSha256)) {
      throw new DurableDelegationError('invalid-identity', 'delegation result digest is invalid');
    }
    const state = this.state(token);
    if (state.resultSha256 !== resultSha256) {
      throw new DurableDelegationError(
        'invalid-transition',
        'delegation result digest does not match the sealed result'
      );
    }
    if (state.phase === 'result-sealed') return 'apply-once';
    if (state.phase === 'result-applied' || state.phase === 'completed') {
      return 'already-applied';
    }
    throw new DurableDelegationError(
      'invalid-transition',
      `delegation result cannot be applied from ${state.phase}`
    );
  }

  async resultApplied(token: DelegationToken): Promise<void> {
    const phase = this.state(token).phase;
    if (phase === 'result-applied' || phase === 'completed') return;
    await this.transition(token, 'result-applied', this.now());
  }

  async completed(token: DelegationToken): Promise<void> {
    if (this.state(token).phase === 'completed') return;
    await this.transition(token, 'completed', this.now());
  }

  async resultAppliedAndCompleted(token: DelegationToken): Promise<void> {
    await this.resultApplied(token);
    await this.completed(token);
  }

  async failedClosed(token: DelegationToken, reason: string): Promise<void> {
    const state = this.state(token);
    if (isTerminal(state.phase)) return;
    await this.transition(
      token,
      'failed-closed',
      this.now(),
      undefined,
      undefined,
      undefined,
      undefined,
      reason
    );
  }

  async revoked(token: DelegationToken, reason: string): Promise<void> {
    const state = this.state(token);
    if (isTerminal(state.phase)) return;
    await this.transition(
      token,
      'revoked',
      this.now(),
      undefined,
      undefined,
      undefined,
      undefined,
      reason
    );
  }

  async cancelled(token: DelegationToken): Promise<void> {
    const state = this.state(token);
    if (isTerminal(state.phase)) return;
    await this.transition(token, 'cancelled', this.now());
  }

  async expireStale(): Promise<number> {
    const now = this.now();
    const stale = [...this.states.values()].filter(
      (state) =>
        (state.phase === 'published' || state.phase === 'running' || state.phase === 'renewed') &&
        now - state.lastHeartbeatUnixMs > ACTIVE_LEASE_EXPIRY_MS
    );
    for (const state of stale) {
      const token: DelegationToken = {
        jobId: state.identity.jobId,
        taskId: state.identity.taskId,
        fencingTokenDecimal: state.fencingTokenDecimal
      };
      await this.transition(
        token,
        'expired',
        now,
        undefined,
        undefined,
        undefined,
        undefined,
        'heartbeat lease expired'
      );
    }
    return stale.length;
  }

  phase(token: DelegationToken): DelegationPhase {
    return this.state(token).phase;
  }

  retainedTerminalCount(nowUnixMs = this.now()): number {
    return [...this.states.values()].filter(
      (state) =>
        state.terminalAtUnixMs !== null &&
        nowUnixMs - state.terminalAtUnixMs <= TERMINAL_TOMBSTONE_RETENTION_MS
    ).length;
  }

  async compactExpiredTerminalTombstones(nowUnixMs = this.now()): Promise<boolean> {
    validateUnixMs(nowUnixMs);
    const operation = this.mutationTail.then(async () => {
      if (
        this.states.size === 0 ||
        [...this.states.values()].some(
          (state) =>
            !isTerminal(state.phase) ||
            state.terminalAtUnixMs === null ||
            nowUnixMs < state.terminalAtUnixMs ||
            nowUnixMs - state.terminalAtUnixMs <= TERMINAL_TOMBSTONE_RETENTION_MS
        )
      ) {
        return false;
      }
      const events = await this.journal.load();
      const expectedHead = events.at(-1)?.eventSha256;
      if (!expectedHead) {
        throw new DurableDelegationError(
          'journal-corrupt',
          'terminal delegation states have no journal head'
        );
      }
      await this.journal.resetIfHead(expectedHead);
      this.states.clear();
      this.nextFencingToken = 0n;
      return true;
    });
    this.mutationTail = operation.then(
      () => undefined,
      () => undefined
    );
    return operation;
  }

  close(): void {
    this.journal.close();
  }

  private async transition(
    token: DelegationToken,
    to: DelegationPhase,
    now: number,
    workerId?: string,
    reservationSha256?: string,
    resultSha256?: string,
    workerReplySha256?: string,
    reason?: string
  ): Promise<DelegationEvent> {
    const operation = this.mutationTail.then(() =>
      this.transitionNow(
        token,
        to,
        now,
        workerId,
        reservationSha256,
        resultSha256,
        workerReplySha256,
        reason
      )
    );
    this.mutationTail = operation.then(
      () => undefined,
      () => undefined
    );
    return operation;
  }

  private async transitionNow(
    token: DelegationToken,
    to: DelegationPhase,
    now: number,
    workerId?: string,
    reservationSha256?: string,
    resultSha256?: string,
    workerReplySha256?: string,
    reason?: string
  ): Promise<DelegationEvent> {
    const state = this.state(token);
    if (!validTransition(state.phase, to)) {
      throw new DurableDelegationError(
        'invalid-transition',
        `invalid delegation transition ${state.phase} -> ${to}`
      );
    }
    const event = await this.append({
      ...eventIdentity(state.identity, state.budget, state.fencingTokenDecimal),
      phase: to,
      workerId: workerId ?? state.workerId,
      reservationSha256: reservationSha256 ?? state.reservationSha256,
      resultSha256: resultSha256 ?? state.resultSha256,
      workerReplySha256: workerReplySha256 ?? state.workerReplySha256,
      timestampUnixMsDecimal: String(now),
      reason: reason ?? null
    });
    state.phase = to;
    if (isTerminal(to)) state.terminalAtUnixMs = now;
    return event;
  }

  private async append(draft: DelegationEventDraft): Promise<DelegationEvent> {
    try {
      return await this.journal.append(draft);
    } catch (error) {
      throw asDelegationJournalFailure(error, 'persist delegation transition');
    }
  }

  private state(token: DelegationToken): DelegationState {
    const state = this.states.get(stateKey(token.jobId, token.taskId));
    if (!state) {
      throw new DurableDelegationError('unknown-delegation', 'delegation identity is unknown');
    }
    if (state.fencingTokenDecimal !== token.fencingTokenDecimal) {
      throw new DurableDelegationError('stale-fence', 'delegation fencing token is stale');
    }
    return state;
  }

  private ensureLive(token: DelegationToken, now: number): void {
    const state = this.state(token);
    if (now - state.lastHeartbeatUnixMs > ACTIVE_LEASE_EXPIRY_MS) {
      throw new DurableDelegationError('lease-expired', 'delegation heartbeat lease expired');
    }
  }

  private replay(event: DelegationEvent): void {
    validateReplayedEvent(event);
    const fence = parseDecimal(event.fencingTokenDecimal, 'fencing token');
    const key = stateKey(event.jobId, event.taskId);
    if (event.phase === 'prepared') {
      if (this.states.has(key)) {
        throw new DurableDelegationError(
          'journal-corrupt',
          'delegation journal reuses an identity'
        );
      }
      if (fence <= this.nextFencingToken) {
        throw new DurableDelegationError(
          'journal-corrupt',
          'prepared delegation fencing tokens are not strictly increasing'
        );
      }
      this.nextFencingToken = fence;
      const timestamp = decimalToSafeNumber(event.timestampUnixMsDecimal, 'timestamp');
      this.states.set(key, {
        identity: Object.freeze({
          jobId: event.jobId,
          taskId: event.taskId,
          coordinatorId: event.coordinatorId,
          payloadSha256: event.payloadSha256,
          requestSha256: event.requestSha256
        }),
        budget: Object.freeze({
          computeUnitsDecimal: event.computeUnitsDecimal,
          memoryBytesDecimal: event.memoryBytesDecimal
        }),
        phase: event.phase,
        fencingTokenDecimal: event.fencingTokenDecimal,
        workerId: null,
        reservationSha256: null,
        resultSha256: null,
        workerReplySha256: null,
        offeredAtUnixMs: null,
        lastHeartbeatUnixMs: timestamp,
        lastPersistedRenewalUnixMs: timestamp,
        terminalAtUnixMs: null
      });
      return;
    }
    const state = this.states.get(key);
    if (
      !state ||
      state.fencingTokenDecimal !== event.fencingTokenDecimal ||
      !sameIdentityAndBudget(state, event) ||
      !validTransition(state.phase, event.phase)
    ) {
      throw new DurableDelegationError(
        'journal-corrupt',
        `invalid recovered delegation transition for ${event.jobId}/${event.taskId}`
      );
    }
    const workerReservationUnchanged =
      state.workerId === null && state.reservationSha256 === null
        ? event.phase === 'accepted' ||
          (event.workerId === null && event.reservationSha256 === null)
        : event.workerId === state.workerId &&
          event.reservationSha256 === state.reservationSha256;
    if (!workerReservationUnchanged) {
      throw new DurableDelegationError(
        'journal-corrupt',
        'worker reservation changed outside the accepted transition'
      );
    }
    const resultIdentityUnchanged =
      state.resultSha256 === null && state.workerReplySha256 === null
        ? event.phase === 'result-sealed' ||
          (event.resultSha256 === null && event.workerReplySha256 === null)
        : event.resultSha256 === state.resultSha256 &&
          event.workerReplySha256 === state.workerReplySha256;
    if (!resultIdentityUnchanged) {
      throw new DurableDelegationError(
        'journal-corrupt',
        'sealed result identity changed outside the result-sealed transition'
      );
    }
    const timestamp = decimalToSafeNumber(event.timestampUnixMsDecimal, 'timestamp');
    state.phase = event.phase;
    state.workerId = event.workerId;
    state.reservationSha256 = event.reservationSha256;
    state.resultSha256 = event.resultSha256;
    state.workerReplySha256 = event.workerReplySha256;
    if (event.phase === 'offered') state.offeredAtUnixMs = timestamp;
    if (event.phase === 'published' || event.phase === 'running' || event.phase === 'renewed') {
      state.lastHeartbeatUnixMs = timestamp;
    }
    if (event.phase === 'published' || event.phase === 'renewed') {
      state.lastPersistedRenewalUnixMs = timestamp;
    }
    if (isTerminal(event.phase)) state.terminalAtUnixMs = timestamp;
  }

  private now(): number {
    const value = this.clock();
    validateUnixMs(value);
    return value;
  }
}

function validateReplayedEvent(event: DelegationEvent): void {
  try {
    validateIdentity(event);
    validateBudget(event);
  } catch (error) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'recovered delegation identity or budget is invalid',
      { cause: error }
    );
  }
  if (!isPositiveU64Decimal(event.fencingTokenDecimal)) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'recovered delegation fencing token must be positive'
    );
  }
  if ((event.workerId === null) !== (event.reservationSha256 === null)) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'worker identity and reservation digest must appear together'
    );
  }
  if (event.workerId !== null && !isPositiveU64Decimal(event.workerId)) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'worker identity must be a job-local positive integer'
    );
  }
  if ((event.resultSha256 === null) !== (event.workerReplySha256 === null)) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'normalized result and worker reply digests must appear together'
    );
  }
  if (
    (event.phase === 'prepared' || event.phase === 'offered') &&
    event.workerId !== null
  ) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'worker reservation is forbidden before acceptance'
    );
  }
  if (
    (event.phase === 'prepared' ||
      event.phase === 'offered' ||
      event.phase === 'accepted' ||
      event.phase === 'published' ||
      event.phase === 'running' ||
      event.phase === 'renewed') &&
    event.resultSha256 !== null
  ) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'result identity is forbidden before result sealing'
    );
  }
  if (
    (event.phase === 'accepted' ||
      event.phase === 'published' ||
      event.phase === 'running' ||
      event.phase === 'renewed' ||
      event.phase === 'result-sealed' ||
      event.phase === 'result-applied' ||
      event.phase === 'completed') &&
    event.workerId === null
  ) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'accepted delegation is missing its worker reservation'
    );
  }
  if (
    (event.phase === 'result-sealed' ||
      event.phase === 'result-applied' ||
      event.phase === 'completed') &&
    event.resultSha256 === null
  ) {
    throw new DurableDelegationError(
      'journal-corrupt',
      'sealed delegation is missing its result identity'
    );
  }
}

export async function createBrowserDelegationAuthority(
  clock: DelegationClock = () => Date.now(),
  jobId: string = browserJobUuid()
): Promise<DurableDelegationAuthority> {
  const journal =
    typeof globalThis.indexedDB === 'undefined'
      ? new MemoryDelegationJournal()
      : await IndexedDbDelegationJournal.openForJob(
          globalThis.indexedDB,
          `${DATABASE_NAME}-${jobId}`,
          jobId
        );
  return DurableDelegationAuthority.recover(journal, clock);
}

export async function sha256Hex(value: string | ArrayBuffer): Promise<string> {
  const bytes =
    typeof value === 'string' ? new TextEncoder().encode(value) : new Uint8Array(value);
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

async function buildEvent(
  records: readonly DelegationEvent[],
  draft: DelegationEventDraft
): Promise<DelegationEvent> {
  const sequence = BigInt(records.length) + 1n;
  if (sequence > MAX_U64) {
    throw new DurableDelegationError(
      'sequence-exhausted',
      'delegation journal sequence exceeds decimal u64'
    );
  }
  const sequenceDecimal = sequence.toString();
  const previousEventSha256 = records.at(-1)?.eventSha256 ?? ZERO_SHA256;
  const material = canonicalHashMaterial(sequenceDecimal, draft, previousEventSha256);
  const eventSha256 = await sha256Hex(material);
  return Object.freeze({
    schema: DELEGATION_JOURNAL_SCHEMA,
    sequenceDecimal,
    ...draft,
    previousEventSha256,
    eventSha256
  });
}

async function validateEventChain(events: readonly DelegationEvent[]): Promise<void> {
  let previous = ZERO_SHA256;
  for (let index = 0; index < events.length; index += 1) {
    const event = parseStoredEvent(events[index]);
    if (event.sequenceDecimal !== String(index + 1) || event.previousEventSha256 !== previous) {
      throw new DurableDelegationError(
        'journal-corrupt',
        `delegation journal chain mismatch at event ${index + 1}`
      );
    }
    const draft = eventDraft(event);
    const actual = await sha256Hex(
      canonicalHashMaterial(event.sequenceDecimal, draft, event.previousEventSha256)
    );
    if (actual !== event.eventSha256) {
      throw new DurableDelegationError(
        'journal-corrupt',
        `delegation journal digest mismatch at event ${index + 1}`
      );
    }
    previous = event.eventSha256;
  }
}

function canonicalHashMaterial(
  sequenceDecimal: string,
  draft: DelegationEventDraft,
  previousEventSha256: string
): string {
  return JSON.stringify({
    schema: DELEGATION_JOURNAL_SCHEMA,
    sequence: sequenceDecimal,
    job_id: draft.jobId,
    task_id: draft.taskId,
    coordinator_id: draft.coordinatorId,
    payload_sha256: draft.payloadSha256,
    request_sha256: draft.requestSha256,
    compute_units: draft.computeUnitsDecimal,
    memory_bytes: draft.memoryBytesDecimal,
    phase: draft.phase,
    fencing_token: draft.fencingTokenDecimal,
    worker_id: draft.workerId,
    reservation_sha256: draft.reservationSha256,
    result_sha256: draft.resultSha256,
    worker_reply_sha256: draft.workerReplySha256,
    timestamp_unix_ms: draft.timestampUnixMsDecimal,
    reason: draft.reason,
    previous_event_sha256: previousEventSha256
  });
}

function eventIdentity(
  identity: DelegationIdentity,
  budget: DelegationBudget,
  fencingTokenDecimal: string
) {
  return {
    jobId: identity.jobId,
    taskId: identity.taskId,
    coordinatorId: identity.coordinatorId,
    payloadSha256: identity.payloadSha256,
    requestSha256: identity.requestSha256,
    computeUnitsDecimal: budget.computeUnitsDecimal,
    memoryBytesDecimal: budget.memoryBytesDecimal,
    fencingTokenDecimal
  };
}

function eventDraft(event: DelegationEvent): DelegationEventDraft {
  return {
    jobId: event.jobId,
    taskId: event.taskId,
    coordinatorId: event.coordinatorId,
    payloadSha256: event.payloadSha256,
    requestSha256: event.requestSha256,
    computeUnitsDecimal: event.computeUnitsDecimal,
    memoryBytesDecimal: event.memoryBytesDecimal,
    phase: event.phase,
    fencingTokenDecimal: event.fencingTokenDecimal,
    workerId: event.workerId,
    reservationSha256: event.reservationSha256,
    resultSha256: event.resultSha256,
    workerReplySha256: event.workerReplySha256,
    timestampUnixMsDecimal: event.timestampUnixMsDecimal,
    reason: event.reason
  };
}

function parseStoredHead(value: unknown): IndexedDbHead | undefined {
  if (value === undefined) return undefined;
  if (!value || typeof value !== 'object') {
    throw new DurableDelegationError(
      'journal-corrupt',
      'delegation journal head is not an object'
    );
  }
  const head = value as Record<string, unknown>;
  if (
    head.key !== HEAD_KEY ||
    !isPositiveU64Decimal(head.sequenceDecimal) ||
    !isSha256(head.eventSha256)
  ) {
    throw new DurableDelegationError('journal-corrupt', 'delegation journal head is malformed');
  }
  return Object.freeze(head as IndexedDbHead);
}

function assertExpectedHead(
  events: readonly DelegationEvent[],
  expectedEventSha256: string
): void {
  if (!isSha256(expectedEventSha256)) {
    throw new DurableDelegationError(
      'journal-head-changed',
      'delegation journal expected head is malformed'
    );
  }
  const actual = events.at(-1)?.eventSha256;
  if (actual !== expectedEventSha256) {
    throw journalHeadChanged(expectedEventSha256, actual);
  }
}

function journalHeadChanged(
  expectedEventSha256: string | undefined,
  actualEventSha256: string | undefined
): DurableDelegationError {
  return new DurableDelegationError(
    'journal-head-changed',
    `delegation journal head changed: expected ${expectedEventSha256 ?? '<empty>'}, found ${
      actualEventSha256 ?? '<empty>'
    }`
  );
}

function parseStoredEvent(value: unknown): DelegationEvent {
  if (!value || typeof value !== 'object') {
    throw new DurableDelegationError('journal-corrupt', 'delegation journal event is not an object');
  }
  const event = value as Record<string, unknown>;
  if (
    event.schema !== DELEGATION_JOURNAL_SCHEMA ||
    !isPositiveU64Decimal(event.sequenceDecimal) ||
    !isNonemptyString(event.jobId) ||
    !isNonemptyString(event.taskId) ||
    !isNonemptyString(event.coordinatorId) ||
    !isSha256(event.payloadSha256) ||
    !isSha256(event.requestSha256) ||
    !isPositiveU64Decimal(event.computeUnitsDecimal) ||
    !isU64Decimal(event.memoryBytesDecimal) ||
    !isPhase(event.phase) ||
    !isPositiveU64Decimal(event.fencingTokenDecimal) ||
    !(event.workerId === null || isNonemptyString(event.workerId)) ||
    !(event.reservationSha256 === null || isSha256(event.reservationSha256)) ||
    !(event.resultSha256 === null || isSha256(event.resultSha256)) ||
    !(event.workerReplySha256 === null || isSha256(event.workerReplySha256)) ||
    !isU64Decimal(event.timestampUnixMsDecimal) ||
    !(event.reason === null || typeof event.reason === 'string') ||
    !isSha256(event.previousEventSha256) ||
    !isSha256(event.eventSha256)
  ) {
    throw new DurableDelegationError('journal-corrupt', 'delegation journal event is malformed');
  }
  return Object.freeze(event as DelegationEvent);
}

function compareEventSequence(left: DelegationEvent, right: DelegationEvent): number {
  const a = BigInt(left.sequenceDecimal);
  const b = BigInt(right.sequenceDecimal);
  return a < b ? -1 : a > b ? 1 : 0;
}

function validateIdentity(identity: DelegationIdentity): void {
  if (
    !isNonemptyString(identity.jobId) ||
    !isNonemptyString(identity.taskId) ||
    !isNonemptyString(identity.coordinatorId) ||
    !isSha256(identity.payloadSha256) ||
    !isSha256(identity.requestSha256)
  ) {
    throw new DurableDelegationError('invalid-identity', 'delegation identity is invalid');
  }
}

function validateBudget(budget: DelegationBudget): void {
  if (
    !isPositiveU64Decimal(budget.computeUnitsDecimal) ||
    !isU64Decimal(budget.memoryBytesDecimal)
  ) {
    throw new DurableDelegationError('invalid-identity', 'delegation budget is invalid');
  }
}

function sameIdentityAndBudget(state: DelegationState, event: DelegationEvent): boolean {
  return (
    state.identity.jobId === event.jobId &&
    state.identity.taskId === event.taskId &&
    state.identity.coordinatorId === event.coordinatorId &&
    state.identity.payloadSha256 === event.payloadSha256 &&
    state.identity.requestSha256 === event.requestSha256 &&
    state.budget.computeUnitsDecimal === event.computeUnitsDecimal &&
    state.budget.memoryBytesDecimal === event.memoryBytesDecimal
  );
}

function validTransition(from: DelegationPhase, to: DelegationPhase): boolean {
  if (isTerminal(from)) return false;
  if (to === 'revoked' || to === 'expired' || to === 'cancelled' || to === 'failed-closed') {
    return true;
  }
  return (
    (from === 'prepared' && to === 'offered') ||
    (from === 'offered' && to === 'accepted') ||
    (from === 'accepted' && to === 'published') ||
    (from === 'published' && to === 'running') ||
    (from === 'running' && to === 'renewed') ||
    (from === 'renewed' && to === 'renewed') ||
    ((from === 'running' || from === 'renewed') && to === 'result-sealed') ||
    (from === 'result-sealed' && to === 'result-applied') ||
    (from === 'result-applied' && to === 'completed')
  );
}

function isTerminal(phase: DelegationPhase): boolean {
  return (
    phase === 'completed' ||
    phase === 'revoked' ||
    phase === 'expired' ||
    phase === 'cancelled' ||
    phase === 'failed-closed'
  );
}

function stateKey(jobId: string, taskId: string): string {
  return `${jobId.length}:${jobId}${taskId.length}:${taskId}`;
}

function parseDecimal(value: string, label: string): bigint {
  if (!isDecimal(value)) {
    throw new DurableDelegationError('journal-corrupt', `invalid ${label} decimal`);
  }
  return BigInt(value);
}

function decimalToSafeNumber(value: string, label: string): number {
  const parsed = Number(parseDecimal(value, label));
  if (!Number.isSafeInteger(parsed)) {
    throw new DurableDelegationError('journal-corrupt', `${label} exceeds safe clock range`);
  }
  return parsed;
}

function validateUnixMs(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new DurableDelegationError('invalid-identity', 'delegation clock is invalid');
  }
}

function isDecimal(value: unknown): value is string {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value);
}

function isPositiveDecimal(value: unknown): value is string {
  return typeof value === 'string' && /^[1-9][0-9]*$/.test(value);
}

function isU64Decimal(value: unknown): value is string {
  return isDecimal(value) && BigInt(value) <= MAX_U64;
}

function isPositiveU64Decimal(value: unknown): value is string {
  return isPositiveDecimal(value) && BigInt(value) <= MAX_U64;
}

function isSha256(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
}

function isUuid(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value)
  );
}

function browserJobUuid(): string {
  const value = globalThis.crypto?.randomUUID?.();
  if (!value || !isUuid(value)) {
    throw new DurableDelegationError(
      'invalid-identity',
      'crypto.randomUUID is required for durable browser delegation'
    );
  }
  return value;
}

function isNonemptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

function isPhase(value: unknown): value is DelegationPhase {
  return (
    value === 'prepared' ||
    value === 'offered' ||
    value === 'accepted' ||
    value === 'published' ||
    value === 'running' ||
    value === 'renewed' ||
    value === 'result-sealed' ||
    value === 'result-applied' ||
    value === 'completed' ||
    value === 'revoked' ||
    value === 'expired' ||
    value === 'cancelled' ||
    value === 'failed-closed'
  );
}

function requestResult<T>(request: IDBRequest<T>, label: string): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(
        new DurableDelegationError('journal-write-failed', `${label} request failed`, {
          cause: request.error
        })
      );
  });
}

function transactionCompletion(transaction: IDBTransaction, label: string): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onabort = () =>
      reject(
        new DurableDelegationError('journal-write-failed', `${label} transaction aborted`, {
          cause: transaction.error
        })
      );
    transaction.onerror = () => {
      // onabort is the single rejection/ACK boundary for transaction errors.
    };
  });
}

function asDelegationJournalFailure(error: unknown, label: string): DurableDelegationError {
  if (error instanceof DurableDelegationError) return error;
  return new DurableDelegationError('journal-write-failed', `${label} failed`, { cause: error });
}
