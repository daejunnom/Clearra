// Shared by candidate admission and production observation. Cloud Run omits the
// default zero minimum; a non-default maximum must remain explicitly observable.
export function validateCloudScaleReadback(resource, label) {
  const annotations = resource?.metadata?.annotations ?? {};
  const scaling = resource?.spec?.scaling ?? {};
  const minimums = [
    annotations["autoscaling.knative.dev/minScale"],
    annotations["run.googleapis.com/minScale"],
    scaling.minInstanceCount,
  ].filter((value) => value !== undefined);
  const maximums = [
    annotations["autoscaling.knative.dev/maxScale"],
    annotations["run.googleapis.com/maxScale"],
    scaling.maxInstanceCount,
  ].filter((value) => value !== undefined);
  if (
    minimums.some((value) => !isExactScaleValue(value, 0)) ||
    maximums.length === 0 ||
    maximums.some((value) => !isExactScaleValue(value, 4))
  ) {
    throw new Error(`Cloud ${label} scale readback drifted`);
  }
}

function isExactScaleValue(value, expected) {
  return (
    (typeof value === "number" && Number.isSafeInteger(value) && value === expected) ||
    (typeof value === "string" && value === String(expected))
  );
}
