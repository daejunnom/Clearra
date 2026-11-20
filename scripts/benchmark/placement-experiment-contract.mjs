// Shared, closed experiment profiles for full P7P4 proof and measurement.
// Existing retained evidence without an explicit selector is movement-clear.
export const DEFAULT_PLACEMENT_EXPERIMENT = 'movement-clear';

const experiments = Object.freeze({
  [DEFAULT_PLACEMENT_EXPERIMENT]: Object.freeze({
    name: DEFAULT_PLACEMENT_EXPERIMENT,
    axis: 'movement_clear_certificates_mode',
    environmentKey: 'CLEARRA_MOVEMENT_CLEAR_CERTIFICATES',
    dependencyMode: '1',
    structuralMode: '1',
  }),
  'minimum-domain-cutoff': Object.freeze({
    name: 'minimum-domain-cutoff',
    axis: 'minimum_domain_cutoff_mode',
    environmentKey: 'CLEARRA_MINIMUM_DOMAIN_CUTOFF',
    dependencyMode: '0',
    structuralMode: '0',
  }),
});

export function placementExperiment(name = DEFAULT_PLACEMENT_EXPERIMENT) {
  if (typeof name !== 'string' || !Object.hasOwn(experiments, name)) {
    throw new Error('Unknown placement experiment');
  }
  return experiments[name];
}

// A flag set to 1 is not evidence it reached the intended compiler. Use
// only aggregate route counts here so the timed arms stay unprofiled.
export function unprofiledEmptyMinimumRoute(report) {
  const route = report?.geometry_domain;
  if (!route || typeof route !== 'object' || Array.isArray(route)) return false;
  const positive = ['completed_families', 'minimum_compiles', 'minimum_empty_origin'];
  const excluded = [
    'minimum_resource_authoritative', 'minimum_below_seven_pieces',
    'advanced_full_compiles', 'advanced_incremental_compiles',
    'minimum_profiled_compiles', 'minimum_incidence_rows_scanned',
    'minimum_incidence_rows_skipped', 'minimum_cells_cut_off',
  ];
  return positive.every(key => Number.isSafeInteger(route[key]) && route[key] > 0) &&
    route.minimum_empty_origin === route.minimum_compiles &&
    excluded.every(key => route[key] === 0);
}
