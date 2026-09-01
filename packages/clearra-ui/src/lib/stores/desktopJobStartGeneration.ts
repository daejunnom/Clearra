/**
 * Owns only the asynchronous Desktop start boundary. Poll epochs belong to an
 * already accepted job and intentionally remain a separate concern.
 */
export class DesktopJobStartGeneration {
  private nextToken = 1;
  private pendingToken: number | null = null;

  begin(): number {
    if (this.pendingToken !== null) {
      throw new Error('a Desktop job start is already pending');
    }
    const token = this.nextToken++;
    this.pendingToken = token;
    return token;
  }

  complete(token: number): boolean {
    if (this.pendingToken !== token) return false;
    this.pendingToken = null;
    return true;
  }

  invalidatePending(): boolean {
    if (this.pendingToken === null) return false;
    this.pendingToken = null;
    this.nextToken += 1;
    return true;
  }

  hasPending(): boolean {
    return this.pendingToken !== null;
  }
}
