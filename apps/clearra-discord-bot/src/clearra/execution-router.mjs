// SRP: select the already-validated Discord execution authority. This module
// does not parse commands, perform tablebase lookup, or retry across
// authorities. An explicit tablebase request is local-only and therefore can
// never fall through to the heavy Cloud Run executor on a miss or failure.

export class ClearraExecutionRouter {
  constructor(primaryExecutor, tablebaseExecutor = null) {
    if (!primaryExecutor || typeof primaryExecutor.execute !== "function") {
      throw new Error("Clearra execution routing requires a primary executor.");
    }
    if (
      tablebaseExecutor !== null &&
      (!tablebaseExecutor || typeof tablebaseExecutor.execute !== "function")
    ) {
      throw new Error("Clearra tablebase execution authority is invalid.");
    }
    this.primaryExecutor = primaryExecutor;
    this.tablebaseExecutor = tablebaseExecutor;
  }

  get maxOutputBytes() {
    return this.primaryExecutor.maxOutputBytes;
  }

  execute(arguments_, options = {}) {
    if (!explicitTablebaseRequested(arguments_)) {
      return this.primaryExecutor.execute(arguments_, options);
    }
    if (!this.tablebaseExecutor) {
      throw new Error(
        "The qualified Discord tablebase service is unavailable; offline search was not started.",
      );
    }
    return this.tablebaseExecutor.execute(arguments_, options);
  }
}

export function explicitTablebaseRequested(arguments_) {
  if (!Array.isArray(arguments_)) return false;
  let requested = false;
  for (const argument of arguments_) {
    const token = String(argument).toLowerCase();
    if (token === "--tablebase" || token === "--tb") requested = true;
    if (token === "--no-tablebase" || token === "--no-tb") requested = false;
  }
  return requested;
}
