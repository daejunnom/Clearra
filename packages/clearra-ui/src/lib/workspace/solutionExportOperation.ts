type Success = 'copied' | 'saved';
type State<Done extends Success> = 'idle' | 'loading' | 'failed' | Done;
type Perform = (signal: AbortSignal, checkAbort: () => void) => void | Promise<void>;

type OperationClock = {
  nextPaint(): Promise<void>;
  scheduleReset(delay: number, reset: () => void): () => void;
};

const browserClock: OperationClock = {
  nextPaint() {
    return new Promise((resolve) => {
      if (typeof requestAnimationFrame === 'function') requestAnimationFrame(() => resolve());
      else setTimeout(resolve, 0);
    });
  },
  scheduleReset(delay, reset) {
    const timer = setTimeout(reset, delay);
    return () => clearTimeout(timer);
  }
};

/**
 * Owns only the button action's lifecycle. The caller selects and encodes the
 * whole solution set, and performs the clipboard or file operation itself.
 */
export function createSolutionExportOperation<Done extends Success>(
  options: {
    success: Done;
    abortMessage: string;
    onState(state: State<Done>): void;
    onFailure(error: unknown): void;
  },
  clock: OperationClock = browserClock
) {
  let active: AbortController | null = null;
  let destroyed = false;
  let cancelScheduledReset: (() => void) | null = null;
  let resetGeneration = 0;

  function abortError(): Error {
    const error = new Error(options.abortMessage);
    error.name = 'AbortError';
    return error;
  }

  function abort() {
    if (active && !active.signal.aborted) active.abort(abortError());
  }

  function cancelReset() {
    resetGeneration += 1;
    cancelScheduledReset?.();
    cancelScheduledReset = null;
  }

  function setTerminalState(state: Done | 'failed') {
    cancelReset();
    const generation = resetGeneration;
    options.onState(state);
    cancelScheduledReset = clock.scheduleReset(state === 'failed' ? 4000 : 1600, () => {
      if (destroyed || resetGeneration !== generation) return;
      cancelScheduledReset = null;
      options.onState('idle');
    });
  }

  async function run(available: boolean, perform: Perform): Promise<void> {
    if (destroyed || !available || active) return;
    cancelReset();
    const controller = new AbortController();
    const { signal } = controller;
    const checkAbort = () => {
      if (signal.aborted) throw signal.reason instanceof Error ? signal.reason : abortError();
    };
    active = controller;
    options.onState('loading');
    try {
      await clock.nextPaint();
      checkAbort();
      await perform(signal, checkAbort);
      checkAbort();
      if (!destroyed && active === controller) setTerminalState(options.success);
    } catch (error) {
      if (destroyed || active !== controller) return;
      if (signal.aborted || (error instanceof Error && error.name === 'AbortError')) {
        options.onState('idle');
      } else {
        options.onFailure(error);
        setTerminalState('failed');
      }
    } finally {
      if (active === controller) active = null;
    }
  }

  function bindPageHide(target: EventTarget): () => void {
    target.addEventListener('pagehide', abort);
    return () => target.removeEventListener('pagehide', abort);
  }

  function destroy() {
    destroyed = true;
    abort();
    active = null;
    cancelReset();
  }

  return { run, bindPageHide, destroy };
}
