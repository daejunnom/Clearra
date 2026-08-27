import type {
  ExecutionAvailabilityReason,
  ExecutionAvailabilityReport,
  ExecutionAvailabilityState
} from '@clearra/ui/wasm';

const MAX_U64 = (1n << 64n) - 1n;
const MAX_U32 = 0xffff_ffff;
const MAX_TIMER_MS = 0x7fff_ffff;

export type SharedExecutionResourceCapacity = Readonly<{
  computeUnits: number;
  memoryBytes: bigint;
}>;

export type SharedExecutionResourceRequest = SharedExecutionResourceCapacity;

export type SharedExecutionResourceToken = Readonly<{
  authorityId: bigint;
  epoch: bigint;
  ownerId: string;
  parentEpoch: bigint | null;
  grant: SharedExecutionResourceRequest;
}>;

export type SharedExecutionResourceSnapshot = Readonly<{
  authorityId: bigint;
  capacity: SharedExecutionResourceCapacity;
  used: SharedExecutionResourceRequest;
  available: SharedExecutionResourceCapacity;
  activeLeaseCount: number;
}>;

export type BoundedAcquireOptions = Readonly<{
  timeoutMs: number;
  signal?: AbortSignal;
}>;

export type SharedExecutionResourceAuthorityOptions = Readonly<{
  authorityId?: bigint;
  initialEpoch?: bigint;
}>;

export type SharedExecutionResourceReleaseCode =
  | 'authority-mismatch'
  | 'owner-mismatch'
  | 'stale-epoch'
  | 'already-released'
  | 'children-active'
  | 'accounting-invariant-violated';

export class SharedExecutionAvailabilityError extends Error {
  constructor(
    readonly availability: ExecutionAvailabilityReport,
    readonly requested: SharedExecutionResourceRequest,
    readonly available: SharedExecutionResourceCapacity
  ) {
    super(
      `shared execution resource ${availability.state}: ` +
      `${availability.reason ?? 'no-reason'}`
    );
    this.name = 'SharedExecutionAvailabilityError';
  }
}

export class SharedExecutionResourceReleaseError extends Error {
  constructor(readonly code: SharedExecutionResourceReleaseCode) {
    super(`shared execution resource release failed: ${code}`);
    this.name = 'SharedExecutionResourceReleaseError';
  }
}

type Allocation = {
  token: SharedExecutionResourceToken;
  remaining: SharedExecutionResourceRequest;
  childCount: number;
};

let nextAuthorityId = 1n;

/**
 * Process-local authority for compute and memory that must be shared by every
 * owner of one browser verifier pool. Availability and result completeness are
 * intentionally independent: obtaining a lease only authorizes execution.
 */
export class SharedExecutionResourceAuthority {
  private readonly authorityId: bigint;
  private nextEpoch: bigint;
  private usedComputeUnits = 0;
  private usedMemoryBytes = 0n;
  private readonly allocations = new Map<bigint, Allocation>();
  private readonly capacityValue: SharedExecutionResourceCapacity;
  private readonly waiters = new Set<() => void>();

  constructor(
    capacity: SharedExecutionResourceCapacity,
    options: SharedExecutionResourceAuthorityOptions = {}
  ) {
    this.capacityValue = validateCapacity(capacity);
    const authorityId = options.authorityId ?? takeAuthorityId();
    if (authorityId <= 0n || authorityId > MAX_U64) {
      throw new RangeError('shared execution authority id must fit a nonzero u64');
    }
    const initialEpoch = options.initialEpoch ?? 1n;
    if (initialEpoch <= 0n || initialEpoch > MAX_U64 + 1n) {
      throw new RangeError('shared execution epoch must be within the checked u64 domain');
    }
    this.authorityId = authorityId;
    this.nextEpoch = initialEpoch;
  }

  capacity(): SharedExecutionResourceCapacity {
    return this.capacityValue;
  }

  snapshot(): SharedExecutionResourceSnapshot {
    const used = Object.freeze({
      computeUnits: this.usedComputeUnits,
      memoryBytes: this.usedMemoryBytes
    });
    return Object.freeze({
      authorityId: this.authorityId,
      capacity: this.capacityValue,
      used,
      available: Object.freeze({
        computeUnits: this.capacityValue.computeUnits - this.usedComputeUnits,
        memoryBytes: this.capacityValue.memoryBytes - this.usedMemoryBytes
      }),
      activeLeaseCount: this.allocations.size
    });
  }

  tryAcquire(
    ownerId: string,
    request: SharedExecutionResourceRequest
  ): SharedExecutionResourceLease {
    validateOwner(ownerId);
    const normalizedRequest = validateRequest(request);
    const available = this.snapshot().available;
    if (!fits(normalizedRequest, this.capacityValue)) {
      throw availabilityError(
        'exhausted',
        exceededReason(normalizedRequest, this.capacityValue),
        normalizedRequest,
        available
      );
    }
    if (!fits(normalizedRequest, available)) {
      throw availabilityError(
        'deferred',
        'shared-resource-contention',
        normalizedRequest,
        available
      );
    }

    const epoch = this.takeEpoch(normalizedRequest, available);
    this.usedComputeUnits = checkedComputeAdd(
      this.usedComputeUnits,
      normalizedRequest.computeUnits
    );
    this.usedMemoryBytes += normalizedRequest.memoryBytes;
    const token = freezeToken({
      authorityId: this.authorityId,
      epoch,
      ownerId,
      parentEpoch: null,
      grant: normalizedRequest
    });
    this.allocations.set(epoch, {
      token,
      remaining: normalizedRequest,
      childCount: 0
    });
    return new SharedExecutionResourceLease(this, token);
  }

  async acquireBounded(
    ownerId: string,
    request: SharedExecutionResourceRequest,
    options: BoundedAcquireOptions
  ): Promise<SharedExecutionResourceLease> {
    const timeoutMs = validateTimeout(options.timeoutMs);
    const deadline = Date.now() + timeoutMs;
    while (true) {
      if (options.signal?.aborted) {
        throw availabilityError(
          'cancelled',
          'cancelled-by-caller',
          validateRequest(request),
          this.snapshot().available
        );
      }
      try {
        return this.tryAcquire(ownerId, request);
      } catch (error) {
        if (!(error instanceof SharedExecutionAvailabilityError)) throw error;
        if (error.availability.state !== 'deferred') throw error;
      }

      const remainingMs = deadline - Date.now();
      if (remainingMs <= 0) {
        throw availabilityError(
          'deferred',
          'shared-resource-contention',
          validateRequest(request),
          this.snapshot().available
        );
      }
      await this.waitForRelease(remainingMs, options.signal, request);
    }
  }

  async withLease<T>(
    ownerId: string,
    request: SharedExecutionResourceRequest,
    options: BoundedAcquireOptions,
    operation: (lease: SharedExecutionResourceLease) => Promise<T> | T
  ): Promise<T> {
    const lease = await this.acquireBounded(ownerId, request, options);
    try {
      return await operation(lease);
    } finally {
      if (!lease.isReleased()) lease.release();
    }
  }

  acquireChild(
    parentToken: SharedExecutionResourceToken,
    ownerId: string,
    request: SharedExecutionResourceRequest
  ): SharedExecutionResourceLease {
    validateOwner(ownerId);
    const normalizedRequest = validateRequest(request);
    const parent = this.requireAllocation(parentToken);
    const parentCapacity = parent.token.grant;
    if (!fits(normalizedRequest, parentCapacity)) {
      throw availabilityError(
        'exhausted',
        exceededReason(normalizedRequest, parentCapacity),
        normalizedRequest,
        parent.remaining
      );
    }
    if (!fits(normalizedRequest, parent.remaining)) {
      throw availabilityError(
        'deferred',
        'shared-resource-contention',
        normalizedRequest,
        parent.remaining
      );
    }

    const epoch = this.takeEpoch(normalizedRequest, parent.remaining);
    parent.remaining = Object.freeze({
      computeUnits: parent.remaining.computeUnits - normalizedRequest.computeUnits,
      memoryBytes: parent.remaining.memoryBytes - normalizedRequest.memoryBytes
    });
    parent.childCount = checkedComputeAdd(parent.childCount, 1);
    const token = freezeToken({
      authorityId: this.authorityId,
      epoch,
      ownerId,
      parentEpoch: parentToken.epoch,
      grant: normalizedRequest
    });
    this.allocations.set(epoch, {
      token,
      remaining: normalizedRequest,
      childCount: 0
    });
    return new SharedExecutionResourceLease(this, token);
  }

  release(
    token: SharedExecutionResourceToken,
    releasingOwnerId: string
  ): void {
    if (token.authorityId !== this.authorityId) {
      throw new SharedExecutionResourceReleaseError('authority-mismatch');
    }
    if (token.ownerId !== releasingOwnerId) {
      throw new SharedExecutionResourceReleaseError('owner-mismatch');
    }
    const allocation = this.allocations.get(token.epoch);
    if (!allocation || !tokensEqual(allocation.token, token)) {
      throw new SharedExecutionResourceReleaseError('stale-epoch');
    }
    if (
      allocation.childCount !== 0 ||
      !resourcesEqual(allocation.remaining, allocation.token.grant)
    ) {
      throw new SharedExecutionResourceReleaseError('children-active');
    }

    const parent = token.parentEpoch === null
      ? null
      : this.allocations.get(token.parentEpoch);
    if (token.parentEpoch !== null && !parent) {
      throw new SharedExecutionResourceReleaseError('accounting-invariant-violated');
    }

    if (parent) {
      const computeUnits = checkedComputeAdd(
        parent.remaining.computeUnits,
        token.grant.computeUnits
      );
      const memoryBytes = parent.remaining.memoryBytes + token.grant.memoryBytes;
      if (
        computeUnits > parent.token.grant.computeUnits ||
        memoryBytes > parent.token.grant.memoryBytes ||
        parent.childCount <= 0
      ) {
        throw new SharedExecutionResourceReleaseError('accounting-invariant-violated');
      }
      parent.remaining = Object.freeze({ computeUnits, memoryBytes });
      parent.childCount -= 1;
    } else {
      if (
        token.grant.computeUnits > this.usedComputeUnits ||
        token.grant.memoryBytes > this.usedMemoryBytes
      ) {
        throw new SharedExecutionResourceReleaseError('accounting-invariant-violated');
      }
      this.usedComputeUnits -= token.grant.computeUnits;
      this.usedMemoryBytes -= token.grant.memoryBytes;
    }
    this.allocations.delete(token.epoch);
    this.notifyWaiters();
  }

  private requireAllocation(token: SharedExecutionResourceToken): Allocation {
    if (token.authorityId !== this.authorityId) {
      throw new SharedExecutionResourceReleaseError('authority-mismatch');
    }
    const allocation = this.allocations.get(token.epoch);
    if (!allocation || !tokensEqual(allocation.token, token)) {
      throw new SharedExecutionResourceReleaseError('stale-epoch');
    }
    return allocation;
  }

  private takeEpoch(
    requested: SharedExecutionResourceRequest,
    available: SharedExecutionResourceCapacity
  ): bigint {
    if (this.nextEpoch > MAX_U64) {
      throw availabilityError(
        'unavailable',
        'capability-unavailable',
        requested,
        available
      );
    }
    const epoch = this.nextEpoch;
    this.nextEpoch += 1n;
    return epoch;
  }

  private waitForRelease(
    timeoutMs: number,
    signal: AbortSignal | undefined,
    request: SharedExecutionResourceRequest
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      let settled = false;
      const finish = (operation: () => void) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.waiters.delete(wake);
        signal?.removeEventListener('abort', abort);
        operation();
      };
      const wake = () => finish(resolve);
      const abort = () => finish(() => reject(availabilityError(
        'cancelled',
        'cancelled-by-caller',
        request,
        this.snapshot().available
      )));
      const timer = setTimeout(() => finish(() => reject(availabilityError(
        'deferred',
        'shared-resource-contention',
        request,
        this.snapshot().available
      ))), timeoutMs);
      this.waiters.add(wake);
      signal?.addEventListener('abort', abort, { once: true });
      if (signal?.aborted) abort();
    });
  }

  private notifyWaiters() {
    const pending = [...this.waiters];
    for (const wake of pending) wake();
  }
}

export class SharedExecutionResourceLease {
  private released = false;

  constructor(
    private readonly authority: SharedExecutionResourceAuthority,
    readonly token: SharedExecutionResourceToken
  ) {}

  tryChild(
    ownerId: string,
    request: SharedExecutionResourceRequest
  ): SharedExecutionResourceLease {
    if (this.released) {
      throw new SharedExecutionResourceReleaseError('already-released');
    }
    return this.authority.acquireChild(this.token, ownerId, request);
  }

  release(): void {
    this.releaseAs(this.token.ownerId);
  }

  releaseAs(ownerId: string): void {
    if (this.released) {
      throw new SharedExecutionResourceReleaseError('already-released');
    }
    this.authority.release(this.token, ownerId);
    this.released = true;
  }

  isReleased(): boolean {
    return this.released;
  }
}

const authorityByPool = new WeakMap<object, SharedExecutionResourceAuthority>();

export function authorityForVerifierPool(
  pool: object,
  capacity: SharedExecutionResourceCapacity
): SharedExecutionResourceAuthority {
  const normalizedCapacity = validateCapacity(capacity);
  const existing = authorityByPool.get(pool);
  if (existing) {
    if (!resourcesEqual(existing.capacity(), normalizedCapacity)) {
      throw availabilityError(
        'unavailable',
        'capability-unavailable',
        normalizedCapacity,
        existing.snapshot().available
      );
    }
    return existing;
  }
  const created = new SharedExecutionResourceAuthority(normalizedCapacity);
  authorityByPool.set(pool, created);
  return created;
}

export function browserExecutionResourceCapacity(
  logicalProcessorCount: number,
  transferByteCap: number
): SharedExecutionResourceCapacity {
  const computeUnits = Math.max(1, Math.floor(logicalProcessorCount));
  if (!Number.isSafeInteger(computeUnits) || computeUnits > MAX_U32) {
    throw new RangeError('logical processor count must fit a positive u32');
  }
  if (!Number.isSafeInteger(transferByteCap) || transferByteCap < 0) {
    throw new RangeError('transfer byte cap must be a nonnegative safe integer');
  }
  const memoryBytes = BigInt(transferByteCap) * BigInt(computeUnits);
  if (memoryBytes > MAX_U64) {
    throw new RangeError('browser aggregate memory authority must fit a u64');
  }
  return Object.freeze({ computeUnits, memoryBytes });
}

function availabilityError(
  state: Exclude<ExecutionAvailabilityState, 'available' | 'incomplete'>,
  reason: ExecutionAvailabilityReason,
  requested: SharedExecutionResourceRequest,
  available: SharedExecutionResourceCapacity
): SharedExecutionAvailabilityError {
  return new SharedExecutionAvailabilityError(
    Object.freeze({
      state,
      reason,
      surface: 'browser-wasm32',
      descriptor_pattern_count: null,
      dense_pattern_count: null,
      required_dense_bytes: null,
      required_memory_bytes: null
    }),
    requested,
    available
  );
}

function exceededReason(
  request: SharedExecutionResourceRequest,
  capacity: SharedExecutionResourceCapacity
): ExecutionAvailabilityReason {
  return request.computeUnits > capacity.computeUnits
    ? 'compute-budget-exceeded'
    : 'memory-budget-exceeded';
}

function validateCapacity(
  capacity: SharedExecutionResourceCapacity
): SharedExecutionResourceCapacity {
  if (
    !Number.isSafeInteger(capacity.computeUnits) ||
    capacity.computeUnits <= 0 ||
    capacity.computeUnits > MAX_U32
  ) {
    throw new RangeError('shared compute capacity must fit a positive u32');
  }
  if (
    typeof capacity.memoryBytes !== 'bigint' ||
    capacity.memoryBytes < 0n ||
    capacity.memoryBytes > MAX_U64
  ) {
    throw new RangeError('shared memory capacity must fit a nonnegative u64');
  }
  return Object.freeze({ ...capacity });
}

function validateRequest(
  request: SharedExecutionResourceRequest
): SharedExecutionResourceRequest {
  if (
    !Number.isSafeInteger(request.computeUnits) ||
    request.computeUnits <= 0 ||
    request.computeUnits > MAX_U32
  ) {
    throw new RangeError('shared compute request must fit a positive u32');
  }
  if (
    typeof request.memoryBytes !== 'bigint' ||
    request.memoryBytes < 0n ||
    request.memoryBytes > MAX_U64
  ) {
    throw new RangeError('shared memory request must fit a nonnegative u64');
  }
  return Object.freeze({ ...request });
}

function validateOwner(ownerId: string) {
  if (typeof ownerId !== 'string' || ownerId.length === 0) {
    throw new RangeError('shared execution resource owner id must not be empty');
  }
}

function validateTimeout(timeoutMs: number): number {
  if (!Number.isFinite(timeoutMs) || timeoutMs < 0 || timeoutMs > MAX_TIMER_MS) {
    throw new RangeError('shared execution wait timeout must fit a bounded host timer');
  }
  return Math.floor(timeoutMs);
}

function checkedComputeAdd(left: number, right: number): number {
  const value = left + right;
  if (!Number.isSafeInteger(value) || value < 0 || value > MAX_U32) {
    throw new SharedExecutionResourceReleaseError('accounting-invariant-violated');
  }
  return value;
}

function fits(
  request: SharedExecutionResourceRequest,
  capacity: SharedExecutionResourceCapacity
): boolean {
  return request.computeUnits <= capacity.computeUnits &&
    request.memoryBytes <= capacity.memoryBytes;
}

function resourcesEqual(
  left: SharedExecutionResourceCapacity,
  right: SharedExecutionResourceCapacity
): boolean {
  return left.computeUnits === right.computeUnits && left.memoryBytes === right.memoryBytes;
}

function tokensEqual(
  left: SharedExecutionResourceToken,
  right: SharedExecutionResourceToken
): boolean {
  return left.authorityId === right.authorityId &&
    left.epoch === right.epoch &&
    left.ownerId === right.ownerId &&
    left.parentEpoch === right.parentEpoch &&
    resourcesEqual(left.grant, right.grant);
}

function freezeToken(
  token: SharedExecutionResourceToken
): SharedExecutionResourceToken {
  return Object.freeze({ ...token, grant: Object.freeze({ ...token.grant }) });
}

function takeAuthorityId(): bigint {
  if (nextAuthorityId > MAX_U64) {
    throw new RangeError('shared execution authority id space is exhausted');
  }
  const value = nextAuthorityId;
  nextAuthorityId += 1n;
  return value;
}
