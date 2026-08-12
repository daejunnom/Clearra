import { classifyClearraTextCommand } from "../clearra/text-command.mjs";
import { classifyTextManagementCommand } from "../discord/management-command.mjs";

const OUTCOME_REASONS = new Set([
  "guild-owner",
  "handler-delegated",
  "handler-failed",
  "handler-cancelled",
]);

export function delegatedOracleMessageOutcome() {
  return oracleMessageOutcome({
    handled: false,
    status: "delegated",
    owner: "sfinder-man",
    reason: "guild-owner",
  });
}

export function normalizeOracleMessageOutcome(value) {
  if (value === undefined || value === true) {
    return oracleMessageOutcome({ handled: true, status: "succeeded" });
  }
  if (value === false) {
    return oracleMessageOutcome({
      handled: false,
      status: "delegated",
      reason: "handler-delegated",
    });
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError("The Oracle message handler returned an invalid outcome.");
  }
  const keys = Object.keys(value);
  if (
    !keys.every((key) => key === "outcome" || key === "owner") ||
    (value.outcome !== "handled" && value.outcome !== "delegated")
  ) {
    throw new TypeError("The Oracle message handler returned an invalid outcome.");
  }
  if (
    value.owner !== undefined &&
    (value.outcome !== "delegated" || value.owner !== "sfinder-man")
  ) {
    throw new TypeError("The Oracle message handler returned an invalid owner.");
  }
  return oracleMessageOutcome({
    handled: value.outcome === "handled",
    status: value.outcome === "delegated" ? "delegated" : "succeeded",
    owner: value.owner,
    reason: value.outcome === "delegated" ? "handler-delegated" : undefined,
  });
}

export function exceptionOracleMessageOutcome(error) {
  return oracleMessageOutcome({
    handled: true,
    status: error?.name === "AbortError" ? "cancelled" : "failed",
    reason: error?.name === "AbortError" ? "handler-cancelled" : "handler-failed",
  });
}

export function publicOracleIngressOutcome(outcome) {
  return outcome.status === "delegated"
    ? {
        accepted: false,
        reason: "delegated",
        ...(outcome.owner ? { owner: outcome.owner } : {}),
      }
    : { accepted: true };
}

/**
 * Projects message text onto the allow-listed public command identity used by
 * operational logs. Argument values and malformed trailing input are never
 * retained.
 */
export function classifyOracleMessageCommand(content, prefixes) {
  for (const prefix of prefixes) {
    const management = classifyTextManagementCommand(content, prefix);
    if (management) return management;
    const command = classifyClearraTextCommand(content, prefix);
    if (command) return command;
  }
  return null;
}

export function classifyOracleMessageKind(content, prefixes) {
  const source = String(content ?? "").trimStart();
  return prefixes.some((prefix) =>
    Boolean(prefix) &&
    source.startsWith(prefix) &&
    source.slice(prefix.length).trim().length > 0
  )
    ? "text"
    : "render";
}

function oracleMessageOutcome(value) {
  const outcome = {
    handled: value.handled === true,
    status: value.status,
  };
  if (value.owner === "sfinder-man") outcome.owner = "sfinder-man";
  if (OUTCOME_REASONS.has(value.reason)) outcome.reason = value.reason;
  return Object.freeze(outcome);
}
