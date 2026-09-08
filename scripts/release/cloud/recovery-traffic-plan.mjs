// Owns only the pure, traffic-only removal plan for one verified candidate.
// It cannot grant authority, authorize recovery debt, or select another runtime.
import { isDeepStrictEqual } from "node:util";

export const RECOVERY_SERVICE =
  "projects/clearra-cloud/locations/asia-northeast1/services/clearra-current-job";
const PROJECT_ALIASES = ["clearra-cloud", "50060711800"];
const REVISION_TYPE = "TRAFFIC_TARGET_ALLOCATION_TYPE_REVISION";
const REVISION = /^clearra-current-job-[a-z0-9][a-z0-9-]*$/u;
const TAG = /^[a-z][a-z0-9-]{0,62}$/u;

export function validateRecoveryTarget(target) {
  if (!target || target.project !== "clearra-cloud" || target.region !== "asia-northeast1" ||
      !REVISION.test(target.priorRevision) || !REVISION.test(target.candidateRevision) ||
      target.priorRevision.length > 63 || target.candidateRevision.length > 63 ||
      target.priorRevision === target.candidateRevision || !TAG.test(target.candidateTag) ||
      !/^asia-northeast1-docker\.pkg\.dev\/clearra-cloud\/clearra\/clearra-current-job@sha256:[0-9a-f]{64}$/u.test(target.image)) {
    throw new Error("Cloud traffic cleanup target differs from the bounded recovery scope");
  }
}

function shortRevision(value) {
  if (typeof value !== "string") throw new Error("Cloud revision is unavailable");
  for (const project of PROJECT_ALIASES) {
    const prefix = `projects/${project}/locations/asia-northeast1/services/clearra-current-job/revisions/`;
    if (value.startsWith(prefix)) value = value.slice(prefix.length);
  }
  if (!REVISION.test(value) || value.length > 63) throw new Error("Cloud revision is outside the exact service");
  return value;
}

function expectedServiceName(value) {
  return PROJECT_ALIASES.some((project) =>
    value === `projects/${project}/locations/asia-northeast1/services/clearra-current-job`);
}

// Desired traffic and status can merge an untagged allocation with a tag-only
// entry. Compare routing/tag semantics, never their incidental array grouping.
function trafficAuthority(entries, target, status = false) {
  if (!Array.isArray(entries) || entries.length === 0 || entries.length > 1000) {
    throw new Error("Cloud traffic is empty or malformed");
  }
  const allocations = new Map();
  const tags = new Map();
  const normalized = entries.map((entry) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry) ||
        Object.keys(entry).some((key) => !["type", "revision", "percent", "tag", ...(status ? ["uri"] : [])].includes(key)) ||
        (entry.type !== undefined && entry.type !== REVISION_TYPE)) {
      throw new Error("Cloud traffic contains unsupported or latest-relative authority");
    }
    const revision = shortRevision(entry.revision);
    const percent = entry.percent === undefined ? 0 : entry.percent;
    if (!Number.isInteger(percent) || percent < 0 || percent > 100) {
      throw new Error("Cloud traffic percent is invalid");
    }
    if (percent > 0) allocations.set(revision, (allocations.get(revision) ?? 0) + percent);
    if (entry.tag !== undefined) {
      if (typeof entry.tag !== "string" || !TAG.test(entry.tag) || tags.has(entry.tag)) {
        throw new Error("Cloud traffic tag is invalid or ambiguous");
      }
      tags.set(entry.tag, revision);
    }
    if (entry.tag === target.candidateTag && revision !== target.candidateRevision) {
      throw new Error("Cloud candidate tag points to another revision");
    }
    if (revision === target.candidateRevision && (percent !== 0 || entry.tag !== target.candidateTag)) {
      throw new Error("Cloud candidate has traffic or an unowned routing reference");
    }
    return { entry, revision, percent };
  });
  if (allocations.size !== 1 || allocations.get(target.priorRevision) !== 100) {
    throw new Error("Cloud traffic is not the exact prior revision at 100 percent");
  }
  return { normalized, tags: [...tags].sort(), allocations: [...allocations].sort() };
}

export function planCandidateTagRemoval(service, revision, target) {
  validateRecoveryTarget(target);
  if (!service || !expectedServiceName(service.name) || typeof service.etag !== "string" ||
      service.etag.length === 0 || typeof service.uid !== "string" || service.uid.length === 0 ||
      service.deleteTime || service.reconciling === true ||
      !service.template || typeof service.template !== "object" ||
      shortRevision(service.latestCreatedRevision) !== target.candidateRevision) {
    throw new Error("Cloud service is absent, changing, or differs from latest candidate authority");
  }
  if (!revision || shortRevision(revision.name) !== target.candidateRevision || revision.deleteTime ||
      !Array.isArray(revision.containers) || revision.containers.length !== 1 ||
      revision.containers[0].image !== target.image) {
    throw new Error("Cloud candidate immutable image differs from the sealed intent");
  }
  const desired = trafficAuthority(service.traffic, target);
  const observed = trafficAuthority(service.trafficStatuses, target, true);
  if (!isDeepStrictEqual(desired.tags, observed.tags) ||
      !isDeepStrictEqual(desired.allocations, observed.allocations)) {
    throw new Error("Cloud desired and observed routing disagree");
  }
  const traffic = desired.normalized
    .filter(({ entry }) => entry.tag !== target.candidateTag)
    .map(({ entry }) => structuredClone(entry));
  const removed = traffic.length !== service.traffic.length;
  return {
    target: structuredClone(target),
    before: structuredClone(service),
    candidateBefore: structuredClone(revision),
    // This exact body is the entire PATCH. Never include template/serviceAccount.
    body: { name: service.name, etag: service.etag, traffic },
    removed,
  };
}

export function assertUnchangedBeforePatch(plan, service, revision) {
  const reread = planCandidateTagRemoval(service, revision, plan.target);
  if (!isDeepStrictEqual(reread, plan)) {
    throw new Error("Cloud recovery preimage changed after validateOnly; no mutation attempted");
  }
}

function unchangedServiceFields(service) {
  const result = structuredClone(service);
  for (const key of ["traffic", "trafficStatuses", "etag", "generation", "observedGeneration",
    "updateTime", "lastModifier", "reconciling", "terminalCondition", "conditions", "urls"]) {
    delete result[key];
  }
  return result;
}

export function verifyCandidateTagRemoval(plan, service, revision) {
  const after = planCandidateTagRemoval(service, revision, plan.target);
  const beforeFields = unchangedServiceFields(plan.before);
  const afterFields = unchangedServiceFields(service);
  if (after.removed || !isDeepStrictEqual(beforeFields, afterFields) ||
      !isDeepStrictEqual(plan.candidateBefore, revision)) {
    // Field names only: never include the service/template contents or raw API
    // values. Keep exact comparisons fail-closed, even for an unknown drift.
    const known = new Set(["name", "uid", "createTime", "deleteTime", "expireTime", "creator", "lastModifier",
      "client", "clientVersion", "description", "labels", "annotations", "ingress", "launchStage", "template",
      "scaling", "invokerIamDisabled", "defaultUriDisabled", "uri", "urls", "customAudiences", "binaryAuthorization",
      "satisfiesPzs", "satisfiesPzi", "latestReadyRevision", "latestCreatedRevision", "trafficStatuses", "reconciling",
      "containers", "serviceAccount", "conditions", "observedGeneration", "generation", "etag", "updateTime"]);
    const changed = (before, current) => [...new Set([...Object.keys(before), ...Object.keys(current)])]
      .filter(key => !isDeepStrictEqual(before[key], current[key]))
      .map(key => known.has(key) ? key : "unclassified-field").sort().slice(0, 12).join(",") || "none";
    throw new Error(`Cloud tag cleanup changed non-traffic authority or retained candidate routing; service_fields=${changed(beforeFields, afterFields)}; revision_fields=${changed(plan.candidateBefore, revision)}; candidate_routing=${after.removed ? "present" : "absent"}`);
  }
  const expected = trafficAuthority(plan.body.traffic, plan.target);
  const actual = trafficAuthority(service.traffic, plan.target);
  if (!isDeepStrictEqual(expected.tags, actual.tags) ||
      !isDeepStrictEqual(expected.allocations, actual.allocations)) {
    throw new Error("Cloud tag cleanup changed unrelated traffic or tags");
  }
}
