type ElapsedTiming = {
  now: () => number;
  every: (tick: () => void, milliseconds: number) => () => void;
};

const defaultTiming: ElapsedTiming = {
  now: () => performance.now(),
  every: (tick, milliseconds) => {
    const handle = setInterval(tick, milliseconds);
    return () => clearInterval(handle);
  }
};

type ElapsedRun = {
  startedAt: number;
  cancel?: () => void;
};

/** Own one run's ticks; the workspace still owns when a run starts or ends. */
export function createWorkspaceElapsedTimer(
  update: (elapsedMs: number, running: boolean) => void,
  timing: ElapsedTiming = defaultTiming
) {
  let activeRun: ElapsedRun | null = null;

  function release(): ElapsedRun | null {
    const run = activeRun;
    activeRun = null;
    run?.cancel?.();
    return run;
  }

  return {
    start() {
      release();
      const run: ElapsedRun = { startedAt: timing.now() };
      activeRun = run;
      run.cancel = timing.every(() => {
        if (activeRun === run) update(timing.now() - run.startedAt, true);
      }, 100);
      update(0, true);
    },
    stop() {
      const run = release();
      if (run) update(timing.now() - run.startedAt, false);
    },
    reset() {
      release();
      update(0, false);
    }
  };
}
