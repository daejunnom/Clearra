// The presence of navigator.gpu is not an adapter-availability promise.
// This owner performs a bounded, side-effect-free adapter probe before Rust
// borrows its runtime. An unavailable accelerator leaves the exact CPU usable.
export type BrowserGpuProbe = {
  requestAdapter(options: { powerPreference: 'high-performance' }): Promise<unknown>;
};
export type BrowserGpuQualification = Readonly<{
  available: boolean;
  reason: 'available' | 'api-unavailable' | 'adapter-unavailable' | 'adapter-rejected' | 'adapter-timeout';
}>;

export class BrowserGpuAvailability {
  private attempt: Promise<BrowserGpuQualification> | null = null;
  private result: BrowserGpuQualification = { available: false, reason: 'api-unavailable' };

  get available(): boolean { return this.result.available; }

  qualify(reported: boolean, gpu: BrowserGpuProbe | undefined, timeoutMs = 3000): Promise<BrowserGpuQualification> {
    if (!reported || !gpu || typeof gpu.requestAdapter !== 'function') {
      return Promise.resolve({ available: false, reason: 'api-unavailable' });
    }
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new RangeError('GPU probe deadline must be finite and positive');
    if (this.attempt) return this.attempt;
    this.attempt = new Promise<BrowserGpuQualification>((resolve) => {
      let settled = false;
      const finish = (result: BrowserGpuQualification) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.result = Object.freeze(result);
        resolve(this.result);
      };
      const timer = setTimeout(() => finish({ available: false, reason: 'adapter-timeout' }), timeoutMs);
      Promise.resolve().then(() => gpu.requestAdapter({ powerPreference: 'high-performance' }))
        .then(adapter => finish(adapter != null
          ? { available: true, reason: 'available' }
          : { available: false, reason: 'adapter-unavailable' }),
          () => finish({ available: false, reason: 'adapter-rejected' }));
    });
    return this.attempt;
  }
}
