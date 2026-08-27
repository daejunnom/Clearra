import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import test from 'node:test';

import {
  PRODUCT_CAPABILITY_REGISTRY_VERSION,
  discordGenericCompatibilityRouteProjection,
  discordLegacyRouteProjection,
  discordRuntimeProjection,
  findProductCapability,
  lowerCapabilityRouteRequest,
  productCapabilityRegistry as discordProductCapabilityRegistry,
} from '../../apps/clearra-discord-bot/src/discord/capability-registry.mjs';
import {
  findSlashCommand,
  findTextCommand,
  messageCommandCatalog,
} from '../../apps/clearra-discord-bot/src/discord/slash-command-catalog.mjs';

const registryUrl = new URL(
  '../fixtures/contracts/product_capability_registry.v1.json',
  import.meta.url,
);
const legacyAliasFixtureUrl = new URL(
  '../fixtures/contracts/legacy_alias_equivalence.v1.json',
  import.meta.url,
);
const readmeUrl = new URL('../../README.md', import.meta.url);

const SFINDER_MAN_COMMANDS = Object.freeze([
  'angery',
  'bestsave',
  'bestsetup',
  'blacklist',
  'calibrate',
  'cat_finder',
  'catboy',
  'catgirl',
  'catimage',
  'chance',
  'change_prefix',
  'command_blacklist',
  'congruent',
  'congruent_cover',
  'cover',
  'cover_percent',
  'currentcommands',
  'database',
  'deletefile',
  'dpcfinder',
  'getallsolutions',
  'getmyfolder',
  'getoutputfile',
  'hello',
  'help',
  'imageofacat',
  'jb',
  'killmyprocesses',
  'minimals',
  'parity',
  'pcnewsletterimage',
  'pcsetup',
  'purge_folders',
  'saves',
  'score',
  'score_minimals',
  'setupcover',
  'sfinder',
  'shutdown',
  'special_cover',
  'special_minimals',
  'spincover',
  'td',
  'tofumen',
  'togray',
  'uploadfile',
  'weightedfails',
]);

const SFINDERBOT_ONLY_COMMANDS = Object.freeze([
  'allspin_b2b_cover',
  'allspin_pres_chance',
  'allspin_sol_finder',
  'best_mn',
  'bot_admins',
  'boykisser',
  'change_game',
  'clean_comments',
  'cover_cutoff',
  'cover_minimals',
  'cover_score',
  'dependencies',
  'extra_minimals',
  'find_100ps',
  'find_100ps_with_dep',
  'get_page',
  'hot_dilf',
  'ilp_minimals',
  'kys',
  'mirror',
  'mn',
  'path_all_solutions',
  'place_piece',
  'score_allspin',
  'score_allspin_minimals',
  'setup_cover',
  'setup_cover_percent',
  'setup_cover_score',
  'setup_set_score',
  'special_score',
  'special_score_minimals',
  'sudo',
  'texttofumen',
  'view_database',
]);

function registry() {
  return JSON.parse(readFileSync(registryUrl, 'utf8'));
}

function legacyAliasFixture() {
  return JSON.parse(readFileSync(legacyAliasFixtureUrl, 'utf8'));
}

function byId(entries, id) {
  const entry = entries.find((candidate) => candidate.id === id);
  assert.ok(entry, `missing registry entry: ${id}`);
  return entry;
}

function assertUniqueIds(entries, label) {
  const ids = entries.map(({ id }) => id);
  assert.equal(new Set(ids).size, ids.length, `${label} IDs must be unique`);
  for (const id of ids) {
    assert.match(id, /^[A-Za-z0-9][A-Za-z0-9._/-]*$/u, `${label} ID syntax: ${id}`);
  }
}

test('the registry pins the audited upstream sources and maps exactly 47 plus 34 commands', () => {
  const contract = registry();
  assert.equal(contract.schema_id, 'clearra.product-capability-registry.v1');
  assert.equal(contract.target_release, 'v0.8.0');

  const sfinderMan = byId(contract.upstream_sources, 'sfinder-man');
  assert.equal(sfinderMan.commit, '438187b6a0ce4bf543ffc9faae507fdc11970e13');
  assert.equal(sfinderMan.expected_active_command_count, 47);

  const sfinderbot = byId(contract.upstream_sources, 'sfinderbot');
  assert.equal(sfinderbot.commit, '0a539c7aa5835b210f8e7aa9248525ba8f3d95ef');
  assert.equal(sfinderbot.expected_sfinderbot_only_command_count, 34);

  for (const source of contract.upstream_sources) {
    assert.match(source.commit, /^[0-9a-f]{40}$/u, `source must pin one commit: ${source.id}`);
    assert.match(source.repository, /^https:\/\/github\.com\//u);
    assert.equal(typeof source.path, 'string');
    assert.notEqual(source.path.length, 0);
  }

  const names = (sourceId) => contract.upstream_command_inventory
    .filter(({ source_id: candidate }) => candidate === sourceId)
    .map(({ name }) => name)
    .sort();

  assert.deepEqual(names('sfinder-man'), SFINDER_MAN_COMMANDS);
  assert.deepEqual(names('sfinderbot'), SFINDERBOT_ONLY_COMMANDS);
  assert.equal(contract.upstream_command_inventory.length, 81);

  const upstreamScore = byId(contract.upstream_command_inventory, 'sfinder-man/score');
  assert.equal(upstreamScore.disposition, 'preset');
  assert.equal(upstreamScore.target, 'pc.score');
  assert.match(upstreamScore.preset, /score-profile=jstris-ultra/u);
});

test('every upstream command has one unique disposition and a live typed target or exclusion', () => {
  const contract = registry();
  assertUniqueIds(contract.upstream_sources, 'upstream source');
  assertUniqueIds(contract.capabilities, 'capability');
  assertUniqueIds(contract.exclusions, 'exclusion');
  assertUniqueIds(contract.upstream_command_inventory, 'upstream command');
  assertUniqueIds(contract.requirements, 'requirement');

  const capabilityIds = new Set(contract.capabilities.map(({ id }) => id));
  const exclusionIds = new Set(contract.exclusions.map(({ id }) => id));
  const sourceIds = new Set(contract.upstream_sources.map(({ id }) => id));
  const allowedDispositions = new Set(['absorbed', 'canonical', 'excluded', 'preset']);

  for (const command of contract.upstream_command_inventory) {
    assert.equal(command.id, `${command.source_id}/${command.name}`);
    assert.ok(sourceIds.has(command.source_id), `unversioned command source: ${command.id}`);
    assert.ok(allowedDispositions.has(command.disposition), `unknown disposition: ${command.id}`);
    if (command.disposition === 'excluded') {
      assert.ok(exclusionIds.has(command.target), `unmapped exclusion: ${command.id}`);
    } else {
      assert.ok(capabilityIds.has(command.target), `unmapped capability: ${command.id}`);
    }
    if (command.disposition === 'preset' || command.disposition === 'absorbed') {
      assert.equal(typeof command.preset, 'string', `missing preset lowering: ${command.id}`);
      assert.notEqual(command.preset.length, 0, `empty preset lowering: ${command.id}`);
    }
  }
});

test('image recognition aliases are excluded without removing typed Fumen output', () => {
  const contract = registry();
  const excluded = byId(contract.exclusions, 'exclusion.image-to-fumen-recognition');
  const tofumen = byId(contract.upstream_command_inventory, 'sfinder-man/tofumen');
  const calibrate = byId(contract.upstream_command_inventory, 'sfinder-man/calibrate');

  assert.equal(excluded.implementation_status, 'excluded');
  assert.deepEqual(
    [tofumen, calibrate].map(({ disposition, target }) => ({ disposition, target })),
    [
      { disposition: 'excluded', target: excluded.id },
      { disposition: 'excluded', target: excluded.id },
    ],
  );
  assert.equal(contract.capabilities.some(({ id }) => id === 'utility.to-fumen'), false);
  const artifact = byId(contract.capabilities, 'artifact.solution-set');
  assert.deepEqual(artifact.required_native_cli_formats, ['text', 'json', 'ctk3', 'fumen']);
  assert.equal(artifact.native_runtime_dependency_policy, 'no-javascript-or-network-codec');
});

test('portfolio alternatives are exact result-unit ties and score equality never uses attack', () => {
  const contract = registry();
  const policy = contract.portfolio_alternative_policy;
  const capabilityIds = new Set(contract.capabilities.map(({ id }) => id));
  const pairs = policy.applicability
    .map(({ capability_id: capabilityId, objective_form: objectiveForm }) => `${capabilityId}/${objectiveForm}`)
    .sort();

  assert.equal(policy.public_result_unit, 'portfolio');
  assert.equal(policy.enumeration, 'unbounded-lazy-exact-two-pass');
  assert.equal(policy.count_policy, 'progressive-known-count-and-null-total-until-sealed');
  assert.equal(policy.score_equality, 'exact-integer-score-only');
  assert.equal(policy.attack_affects_score_tie, false);
  assert.equal(policy.discord_policy, 'canonical-first-portfolio-only-without-tie-metadata');
  assert.deepEqual(pairs, [
    'build.congruent-cover/max-probability-minimum',
    'build.congruent-cover/min-cover',
    'build.cover/max-probability-minimum',
    'build.cover/min-cover',
    'build.evaluate.minimals/max-probability-minimum',
    'build.evaluate.minimals/min-cover',
    'build.evaluate.score/max-score-cover',
    'build.setup-cover-score/max-score-cover',
    'build.setup-cover/max-probability-minimum',
    'build.setup-cover/min-cover',
    'pc.minimals/min-cover',
    'pc.score-minimals/max-score-cover',
    'spin-structure.cover/min-cover',
  ]);
  assert.equal(new Set(pairs).size, pairs.length);
  for (const row of policy.applicability) {
    assert.ok(capabilityIds.has(row.capability_id), `unknown portfolio capability: ${row.capability_id}`);
    assert.equal(row.representative_metric, 'minimum-member-cardinality');
  }
  for (const id of policy.normal_family_not_tie) {
    assert.ok(capabilityIds.has(id), `unknown normal-family capability: ${id}`);
  }
  assert.ok(policy.normal_family_not_tie.includes('pc.allspin-sol'));
  assert.equal(byId(contract.requirements, 'REQ-V080-021').implementation_status, 'implemented');
});

test('every capability and exclusion has one explicit four-state implementation record', () => {
  const contract = registry();
  const allowedStatuses = new Set(contract.implementation_statuses);
  assert.deepEqual([...allowedStatuses].sort(), ['excluded', 'implemented', 'missing', 'partial']);

  const implementations = contract.capability_implementation;
  const implementationIds = implementations.map(({ capability_id: id }) => id);
  assert.equal(new Set(implementationIds).size, implementationIds.length);
  assert.deepEqual(
    implementationIds.slice().sort(),
    contract.capabilities.map(({ id }) => id).sort(),
    'every capability must have exactly one implementation record',
  );

  for (const entry of implementations) {
    assert.ok(allowedStatuses.has(entry.implementation_status), `invalid status: ${entry.capability_id}`);
    assert.notEqual(entry.implementation_status, 'excluded', `required capability cannot be excluded: ${entry.capability_id}`);
    assert.equal(typeof entry.current_surface, 'string');
    assert.ok(Array.isArray(entry.implementation_evidence));
    if (entry.implementation_status === 'partial' || entry.implementation_status === 'implemented') {
      assert.ok(entry.implementation_evidence.length > 0, `non-missing status needs evidence: ${entry.capability_id}`);
    }
    if (entry.implementation_status === 'missing') {
      assert.deepEqual(entry.implementation_evidence, [], `missing capability cannot cite implementation: ${entry.capability_id}`);
    }
  }

  for (const exclusion of contract.exclusions) {
    assert.equal(exclusion.implementation_status, 'excluded');
    assert.ok(exclusion.reason.length > 0);
  }
});

test('implementation evidence paths resolve on the current exact source', () => {
  const contract = registry();
  const evidenceOwners = [
    ...contract.capability_implementation.map((entry) => ({
      id: `capability:${entry.capability_id}`,
      evidence: entry.implementation_evidence,
    })),
    ...contract.result_affecting_option_exposure.map((entry) => ({
      id: `option:${entry.id}`,
      evidence: entry.implementation_evidence ?? [],
    })),
    ...contract.requirements.map((entry) => ({
      id: `requirement:${entry.id}`,
      evidence: entry.implementation_evidence,
    })),
  ];

  for (const owner of evidenceOwners) {
    for (const evidence of owner.evidence) {
      assert.match(evidence, /^(?![A-Za-z]:|\/|\\)(?!.*(?:^|\/)\.\.(?:\/|$)).+/u, owner.id);
      assert.equal(
        existsSync(new URL(`../../${evidence}`, import.meta.url)),
        true,
        `stale implementation evidence for ${owner.id}: ${evidence}`,
      );
    }
  }
});

test('capabilities keep algorithm authority, timeout policy, effects, and public paths typed separately', () => {
  const contract = registry();
  const algorithms = new Set(contract.algorithm_families);
  const timeoutClasses = new Set(Object.keys(contract.timeout_classes));
  const effects = new Set(contract.effect_classes);
  const surfaceProfiles = new Set(Object.keys(contract.surface_profiles));
  const publicPaths = [];

  for (const capability of contract.capabilities) {
    assert.ok(algorithms.has(capability.algorithm_family), `unknown algorithm family: ${capability.id}`);
    assert.ok(timeoutClasses.has(capability.timeout_class), `unknown timeout class: ${capability.id}`);
    assert.notEqual(capability.algorithm_family, capability.timeout_class, `conflated algorithm and timeout: ${capability.id}`);
    assert.ok(Array.isArray(capability.algorithm_phases) && capability.algorithm_phases.length > 0);
    assert.ok(Array.isArray(capability.effect_classes) && capability.effect_classes.length > 0);
    for (const effect of capability.effect_classes) {
      assert.ok(effects.has(effect), `unknown effect class ${effect}: ${capability.id}`);
    }
    assert.ok(surfaceProfiles.has(capability.surface_profile), `unknown surface profile: ${capability.id}`);
    assert.equal(typeof capability.input_schema_id, 'string');
    assert.equal(typeof capability.modal_schema_id, 'string');
    assert.equal(typeof capability.result_contract_id, 'string');

    if (capability.command_path !== null) {
      assert.ok(Array.isArray(capability.command_path));
      assert.ok(capability.command_path.length >= 1 && capability.command_path.length <= 3);
      publicPaths.push(capability.command_path.join(' '));
    } else {
      assert.notEqual(
        capability.surface_profile,
        'public_command',
        `non-command capability needs a dedicated non-slash surface: ${capability.id}`,
      );
      assert.ok(
        capability.text_command || capability.text_syntax || ['result_only'].includes(capability.surface_profile),
        `non-command capability must name its dedicated surface: ${capability.id}`,
      );
    }
  }

  assert.equal(new Set(publicPaths).size, publicPaths.length, 'public command paths must be unique');
  assert.ok(publicPaths.includes('pc path'));
  assert.ok(publicPaths.includes('build cover'));
  assert.ok(publicPaths.includes('build finesse-score'));
  assert.ok(publicPaths.includes('forward spin'));
  assert.ok(publicPaths.includes('spin-structure search'));
  assert.ok(!publicPaths.includes('search'));
  assert.ok(!publicPaths.some((path) => path === 'finesse' || path.startsWith('finesse ')));
  assert.ok(!publicPaths.some((path) => path === 'verify' || path.startsWith('verify ')));
});

test('every current Discord capability and legacy route is a fail-closed fieldwise runtime projection', () => {
  const contract = registry();
  const projection = contract.runtime_projection;
  assert.equal(projection.schema_id, 'clearra.discord-runtime-projection.v1');
  assert.match(projection.authority_note, /independent of target capability v2/u);

  const runtime = discordRuntimeProjection();
  const normalizeCapability = (entry) => ({
    id: entry.id,
    kind: entry.kind,
    status: entry.status,
    path: [entry.canonical.root, entry.canonical.subcommand].filter(Boolean),
    ingress: { slash: entry.canonical.slash, text: entry.canonical.text },
    problem_contract_id: entry.problemContractId,
    input_schema_id: entry.inputSchemaId,
    modal_schema_id: entry.modalSchemaId,
    result_contract_id: entry.resultContractId,
    algorithm_family: entry.algorithmFamily,
    timeout_class: entry.timeoutClass,
    effect_classes: entry.effectClasses,
    help_policy: entry.helpPolicy,
    i18n_policy: entry.i18nPolicy,
    result_allowlist: entry.resultAllowlist,
    telemetry_identity: entry.telemetryIdentity,
    lowering_authority: entry.loweringAuthority,
    engine: entry.engine,
  });
  const byStableId = (left, right) => left.id.localeCompare(right.id);
  const activeAndHidden = runtime
    .filter(({ status }) => status !== 'planned')
    .map(normalizeCapability)
    .sort(byStableId);
  assert.deepEqual(
    activeAndHidden,
    projection.current_capabilities.slice().sort(byStableId),
    'active/hidden runtime projection drifted fieldwise',
  );

  const planned = runtime
    .filter(({ status }) => status === 'planned')
    .map(normalizeCapability)
    .sort(byStableId);
  const plannedAuthority = projection.planned_capabilities.map((entry) => ({
    ...projection.planned_defaults,
    ...entry,
    telemetry_identity: entry.id,
  })).sort(byStableId);
  assert.deepEqual(planned, plannedAuthority, 'planned runtime projection drifted fieldwise');

  const routeRows = discordLegacyRouteProjection();
  assert.deepEqual(
    routeRows.slice().sort(byStableId),
    projection.legacy_routes.slice().sort(byStableId),
    'live legacy ingress escaped product authority',
  );

  const genericCompatibilityRows = discordGenericCompatibilityRouteProjection();
  assert.deepEqual(
    genericCompatibilityRows.slice().sort(byStableId),
    projection.generic_compatibility_routes.slice().sort(byStableId),
    'generic compatibility routes drifted from their independent product authority',
  );
  assert.deepEqual(
    genericCompatibilityRows.map(({ id }) => id).sort(),
    ['discord.compat.chance', 'discord.compat.percent', 'discord.compat.score'],
  );
  const typedChance = runtime.find(({ id }) => id === 'pc.chance');
  const typedScore = runtime.find(({ id }) => id === 'pc.score');
  const typedTiling = runtime.find(({ id }) => id === 'pc.tiling');
  assert.ok(typedChance);
  assert.ok(typedScore);
  assert.ok(typedTiling);
  assert.deepEqual(typedChance.resultAllowlist, ['pc-probability.v2']);
  assert.deepEqual(typedChance.engine.argvPrefix, ['pc', 'chance']);
  assert.equal(typedScore.problemContractId, 'pc-clear-to-empty.v2');
  assert.equal(typedScore.resultContractId, 'pc-score-summary.v2');
  assert.deepEqual(typedScore.resultAllowlist, ['pc-score-summary.v2']);
  assert.deepEqual(typedScore.engine.argvPrefix, ['pc', 'score']);
  assert.equal(typedTiling.problemContractId, 'pc-clear-to-empty.v2');
  assert.equal(typedTiling.inputSchemaId, 'pc-pattern.v2');
  assert.equal(typedTiling.modalSchemaId, 'pc-pattern.v2');
  assert.equal(typedTiling.resultContractId, 'pc-tiling-family.v1');
  assert.deepEqual(typedTiling.resultAllowlist, ['pc-tiling-family.v1']);
  assert.deepEqual(typedTiling.engine.argvPrefix, ['pc', 'tiling']);
  const genericTypedTarget = new Map([
    ['discord.compat.chance', typedChance],
    ['discord.compat.percent', typedChance],
    ['discord.compat.score', typedScore],
  ]);
  for (const generic of genericCompatibilityRows) {
    const typedTarget = genericTypedTarget.get(generic.id);
    assert.ok(typedTarget, `generic route lacks a typed migration target: ${generic.id}`);
    assert.equal(generic.classification, 'generic-compatibility');
    assert.equal(generic.problemContractId, 'pc-clear-to-empty');
    assert.equal(generic.resultContractId, 'pc-scenario');
    assert.deepEqual(generic.resultAllowlist, ['pc-scenario']);
    assert.notEqual(generic.problemContractId, typedTarget.problemContractId);
    assert.notEqual(generic.resultContractId, typedTarget.resultContractId);
    assert.notDeepEqual(generic.resultAllowlist, typedTarget.resultAllowlist);
  }

  assert.deepEqual(
    projection.non_capability_routes.map(({ surface, path, name }) => ({ surface, path, name })),
    [
      { surface: 'discord-slash', path: ['render-file'], name: undefined },
      { surface: 'discord-text', path: undefined, name: 'render-file' },
      { surface: 'discord-message', path: undefined, name: 'Get original GIF' },
    ],
  );
  assert.equal(findSlashCommand('render-file')?.kind, 'render-file');
  assert.equal(findTextCommand('render-file')?.kind, 'render-file');
  assert.deepEqual(messageCommandCatalog.map(({ registration }) => registration.name), ['Get original GIF']);

  const productTarget = byId(contract.capabilities, 'pc.path');
  const currentRuntime = byId(projection.current_capabilities, 'pc.path');
  assert.equal(currentRuntime.problem_contract_id, productTarget.problem_contract_id);
  assert.equal(currentRuntime.input_schema_id, productTarget.input_schema_id);
  assert.equal(currentRuntime.result_contract_id, productTarget.result_contract_id);
});

test('legacy route inventory retains lowering identity and immutable fixed presets', () => {
  const contract = registry();
  const authority = new Map(contract.runtime_projection.legacy_routes.map((route) => [route.id, route]));
  for (const capability of discordProductCapabilityRegistry) {
    for (const route of capability.aliases) {
      const id = [capability.id, route.slash ? 'slash' : 'text', route.root, route.subcommand]
        .filter(Boolean)
        .join('/');
      const declared = authority.get(id);
      assert.ok(declared, `missing alias authority: ${id}`);
      const projection = lowerCapabilityRouteRequest(capability, route);
      assert.deepEqual(
        {
          input: projection.route.input,
          input_schema_id: projection.route.inputSchemaId,
          modal_schema_id: projection.route.modalSchemaId,
          argv_prefix: projection.route.argvPrefix,
          public_result_kind: projection.route.publicResultKind,
        },
        {
          input: declared.input,
          input_schema_id: declared.input_schema_id,
          modal_schema_id: declared.modal_schema_id,
          argv_prefix: declared.argv_prefix,
          public_result_kind: declared.public_result_kind,
        },
        id,
      );
      assert.deepEqual(projection.parameters, route.preset ?? {}, id);
    }
  }

  const score = findProductCapability('pc.score');
  assert.deepEqual(score.aliases, [], 'generic score must not claim typed alias equivalence');
  const genericScore = discordGenericCompatibilityRouteProjection()
    .find(({ id }) => id === 'discord.compat.score');
  assert.ok(genericScore);
  assert.deepEqual(genericScore.argvPrefix, ['sfinder', 'score']);
  assert.equal(genericScore.problemContractId, 'pc-clear-to-empty');
  assert.equal(genericScore.resultContractId, 'pc-scenario');
  assert.notDeepEqual(
    {
      input: score.canonical.input,
      problemContractId: score.problemContractId,
      resultContractId: score.resultContractId,
      argvPrefix: score.canonical.argvPrefix,
    },
    {
      input: genericScore.input,
      problemContractId: genericScore.problemContractId,
      resultContractId: genericScore.resultContractId,
      argvPrefix: genericScore.argvPrefix,
    },
    'generic Jstris score must never be raw-equivalent to typed pc score',
  );
});

test('legacy parser fixture exhaustively binds slash and text routes to frozen argv', () => {
  const contract = registry();
  const fixture = legacyAliasFixture();
  assert.equal(fixture.schema_id, 'clearra.legacy-alias-equivalence.v1');
  assert.equal(fixture.registry_schema_id, contract.schema_id);
  assert.equal(fixture.target_release, contract.target_release);
  assert.equal(
    contract.runtime_projection.legacy_alias_equivalence_fixture,
    'tests/fixtures/contracts/legacy_alias_equivalence.v1.json',
  );

  const productRoutes = contract.runtime_projection.legacy_routes
    .filter(({ classification }) => classification !== 'generic-compatibility')
    .map((route) => ({
    id: route.id,
    capability_id: route.capability_id,
    surface: route.surface,
    path: route.path ?? route.name.split(' '),
    classification: route.classification,
    input: route.input,
    input_schema_id: route.input_schema_id,
    modal_schema_id: route.modal_schema_id,
    argv_prefix: route.argv_prefix,
    public_result_kind: route.public_result_kind,
    ...(route.preset ? { preset: route.preset } : {}),
    ...(route.remove_in ? { remove_in: route.remove_in } : {}),
    ...(route.lifetime ? { lifetime: route.lifetime } : {}),
    }));
  const fixtureRoutes = fixture.routes.map(({ case_id: _caseId, ...route }) => route);
  assert.deepEqual(fixtureRoutes, productRoutes);
  assert.equal(fixture.routes.length, 30);

  const cases = new Map(fixture.cases.map((entry) => [entry.id, entry]));
  assert.equal(cases.size, 15);
  const currentCapabilities = new Map(
    contract.runtime_projection.current_capabilities.map((entry) => [entry.id, entry]),
  );
  const legacyCaseAuthorityOverrides = contract.runtime_projection.legacy_case_authority_overrides;
  assert.deepEqual(
    Object.keys(legacyCaseAuthorityOverrides).sort(),
    [
      'build.cover/cover/',
      'build.cover/finesse/search',
      'pc.path/path/',
      'pc.score-finder/score-finder/',
      'setup.build/best-setup/',
      'setup.joint/pc-setup/',
      'setup.pc/dpc-finder/',
    ],
    'only frozen legacy v1 cases may override their current v2 capability authority',
  );
  for (const fixtureCase of fixture.cases) {
    const capability = currentCapabilities.get(fixtureCase.capability_id);
    assert.ok(capability, fixtureCase.id);
    const authority = legacyCaseAuthorityOverrides[fixtureCase.id] ?? capability;
    assert.equal(fixtureCase.problem_contract_id, authority.problem_contract_id, fixtureCase.id);
    assert.equal(fixtureCase.result_contract_id, authority.result_contract_id, fixtureCase.id);
    assert.ok(fixtureCase.canonical_argv.length > 0, fixtureCase.id);
    assert.ok(fixtureCase.alias_argv.length > 0, fixtureCase.id);
    assert.ok(fixtureCase.canonical_text_argv.length > 0, fixtureCase.id);
    assert.ok(fixtureCase.alias_text_argv.length > 0, fixtureCase.id);
    assert.equal(
      fixtureCase.canonical_text_web_command,
      ['clearra', ...fixtureCase.canonical_text_argv].join(' '),
      fixtureCase.id,
    );
    assert.equal(
      fixtureCase.alias_text_web_command,
      ['clearra', ...fixtureCase.alias_text_argv].join(' '),
      fixtureCase.id,
    );
    assert.equal(
      fixtureCase.canonical_web_command,
      ['clearra', ...fixtureCase.canonical_argv].join(' '),
      fixtureCase.id,
    );
    assert.equal(
      fixtureCase.alias_web_command,
      ['clearra', ...fixtureCase.alias_argv].join(' '),
      fixtureCase.id,
    );
    const routePair = fixture.routes.filter(({ case_id }) => case_id === fixtureCase.id);
    assert.deepEqual(
      routePair.map(({ surface }) => surface).sort(),
      ['discord-slash', 'discord-text'],
      fixtureCase.id,
    );
    if (fixtureCase.classification === 'fixed-preset') {
      assert.ok(fixtureCase.conflict, `fixed preset lacks conflict vector: ${fixtureCase.id}`);
      assert.ok(routePair.every(({ preset }) => preset), fixtureCase.id);
    } else {
      assert.equal(fixtureCase.classification, 'equivalence', fixtureCase.id);
      assert.equal(fixtureCase.conflict, undefined, fixtureCase.id);
    }
  }
});

test('implemented Discord runtime capabilities are a complete fieldwise product-authority projection', () => {
  const contract = registry();
  const productById = new Map(contract.capabilities.map((capability) => [capability.id, capability]));
  const implementationById = new Map(
    contract.capability_implementation.map((entry) => [entry.capability_id, entry]),
  );

  assert.equal(PRODUCT_CAPABILITY_REGISTRY_VERSION, contract.schema_id);
  for (const runtime of discordProductCapabilityRegistry) {
    assert.ok(productById.has(runtime.id), `runtime capability lacks product authority: ${runtime.id}`);
  }

  const runtimeBoundImplementedIds = contract.capabilities
    .filter(({ id, surface_profile: surfaceProfile }) =>
      implementationById.get(id)?.implementation_status === 'implemented' &&
      ['public_command', 'hidden_text_diagnostic'].includes(surfaceProfile))
    .map(({ id }) => id)
    .sort();
  assert.deepEqual(runtimeBoundImplementedIds, [
    'build.congruent',
    'build.congruent-cover',
    'build.cover',
    'build.evaluate.b2b-cover',
    'build.evaluate.cover',
    'build.evaluate.cover-percent',
    'build.evaluate.minimals',
    'build.evaluate.score',
    'build.finesse-score',
    'build.setup',
    'build.setup-cover',
    'build.setup-cover-percent',
    'build.setup-cover-score',
    'diagnostic.verify',
    'forward.damage',
    'forward.ren',
    'forward.spin',
    'meta.help',
    'pc.allspin-pres-chance',
    'pc.allspin-sol',
    'pc.best-save',
    'pc.chance',
    'pc.failed-queue',
    'pc.minimals',
    'pc.path',
    'pc.saves',
    'pc.score',
    'pc.score-finder',
    'pc.score-minimals',
    'pc.tiling',
    'setup.build',
    'setup.joint',
    'setup.pc',
    'setup.score',
    'spin-structure.cover',
    'spin-structure.guaranteed',
    'spin-structure.search',
    'utility.fumen',
    'utility.mirror',
    'utility.parity',
    'utility.render',
    'utility.sequence',
    'utility.sequence-dependencies',
    'utility.to-gray',
  ]);
  for (const id of runtimeBoundImplementedIds) {
    assert.ok(findProductCapability(id), `implemented Discord capability missing at runtime: ${id}`);
  }

  for (const id of [
    'pc.allspin-sol',
    'pc.allspin-pres-chance',
    'pc.best-save',
    'pc.chance',
    'pc.failed-queue',
    'pc.minimals',
    'pc.score',
    'pc.score-minimals',
    'pc.saves',
    'pc.tiling',
  ]) {
    const product = productById.get(id);
    const runtime = findProductCapability(id);
    assert.deepEqual(
      {
        id: runtime.id,
        problemContractId: runtime.problemContractId,
        algorithmFamily: runtime.algorithmFamily,
        timeoutClass: runtime.timeoutClass,
        inputSchemaId: runtime.inputSchemaId,
        modalSchemaId: runtime.modalSchemaId,
        resultContractId: runtime.resultContractId,
        canonicalPath: [runtime.canonical.root, runtime.canonical.subcommand]
          .filter(Boolean)
          .join(' '),
        engineArgvPrefix: runtime.engine.argvPrefix,
      },
      {
        id: product.id,
        problemContractId: product.problem_contract_id,
        algorithmFamily: product.algorithm_family,
        timeoutClass: product.timeout_class,
        inputSchemaId: product.input_schema_id,
        modalSchemaId: product.modal_schema_id,
        resultContractId: product.result_contract_id,
        canonicalPath: product.command_path.join(' '),
        engineArgvPrefix: product.command_path,
      },
      `runtime field drift: ${id}`,
    );
    assert.equal(runtime.status, 'active');
    if (
      [
        'pc.best-save',
        'pc.chance',
        'pc.failed-queue',
        'pc.saves',
        'pc.score',
        'pc.score-minimals',
        'pc.tiling',
      ].includes(id)
    ) {
      assert.deepEqual(runtime.effectClasses, product.effect_classes);
      assert.deepEqual(runtime.engineKinds, [product.result_contract_id]);
      if (id === 'pc.score-minimals') {
        assert.deepEqual(
          runtime.aliases.map((alias) => ({
            slash: alias.slash,
            text: alias.text,
            argvPrefix: alias.argvPrefix,
            publicResultKind: alias.publicResultKind,
            resultAuthorityId: alias.resultAuthorityId,
          })),
          [
            {
              slash: true,
              text: false,
              argvPrefix: ['pc', 'score-minimals'],
              publicResultKind: 'score-minimals',
              resultAuthorityId: 'score-minimals',
            },
            {
              slash: false,
              text: true,
              argvPrefix: ['pc', 'score-minimals'],
              publicResultKind: 'score-minimals',
              resultAuthorityId: 'score-minimals',
            },
          ],
        );
      } else {
        assert.deepEqual(runtime.aliases, []);
      }
      assert.equal(runtime.canonical.slash, true);
      assert.equal(runtime.canonical.text, true);
      assert.equal(
        runtime.publicResultKind,
        {
          'pc.chance': 'chance',
          'pc.failed-queue': 'failed-queue',
          'pc.score': 'score',
          'pc.score-minimals': 'score-minimals',
          'pc.saves': 'saves',
          'pc.best-save': 'best-save',
          'pc.tiling': 'tiling',
        }[id],
      );
      const expectedCurrentSurface = {
        'pc.best-save':
          'native-cli-wasm-web-gui-desktop-discord-grouped-slash-and-text-with-complete-ordinary-tie-family-and-canonical-smallest-id-discord-witness',
        'pc.saves':
          'native-cli-wasm-web-gui-desktop-discord-grouped-slash-and-text-with-full-finite-save-family-and-separate-unconditional-conditional-probabilities',
        'pc.score':
          'native-cli-wasm-web-gui-desktop-discord-complete-score-only-normal-family-with-informational-attack-only',
        'pc.score-minimals':
          'native-cli-wasm-web-gui-desktop-discord-grouped-slash-and-text-with-score-only-all-optimal-portfolio-paging-and-canonical-smallest-id-discord-witness',
        'pc.tiling': 'discord-grouped-slash-and-text-cli-web-gui-app-core',
      }[id] ?? 'discord-grouped-slash-and-text-cli-web-app';
      assert.equal(
        implementationById.get(id).current_surface,
        expectedCurrentSurface,
      );
    }
  }

  {
    const id = 'build.finesse-score';
    const product = productById.get(id);
    const runtime = findProductCapability(id);
    assert.deepEqual(
      {
        id: runtime.id,
        problemContractId: runtime.problemContractId,
        algorithmFamily: runtime.algorithmFamily,
        timeoutClass: runtime.timeoutClass,
        inputSchemaId: runtime.inputSchemaId,
        modalSchemaId: runtime.modalSchemaId,
        resultContractId: runtime.resultContractId,
        canonicalPath: [runtime.canonical.root, runtime.canonical.subcommand]
          .filter(Boolean)
          .join(' '),
        engineArgvPrefix: runtime.engine.argvPrefix,
      },
      {
        id: product.id,
        problemContractId: product.problem_contract_id,
        algorithmFamily: product.algorithm_family,
        timeoutClass: product.timeout_class,
        inputSchemaId: product.input_schema_id,
        modalSchemaId: product.modal_schema_id,
        resultContractId: product.result_contract_id,
        canonicalPath: product.command_path.join(' '),
        engineArgvPrefix: ['finesse', 'score'],
      },
      `runtime field drift: ${id}`,
    );
    assert.equal(runtime.status, 'active');
  }

  const verifyProduct = productById.get('diagnostic.verify');
  const verifyRuntime = findProductCapability('diagnostic.verify');
  assert.deepEqual(
    {
      id: verifyRuntime.id,
      algorithmFamily: verifyRuntime.algorithmFamily,
      timeoutClass: verifyRuntime.timeoutClass,
      status: verifyRuntime.status,
      helpPolicy: verifyRuntime.helpPolicy,
      textCommand: verifyRuntime.canonical.root,
      slash: verifyRuntime.canonical.slash,
      text: verifyRuntime.canonical.text,
    },
    {
      id: verifyProduct.id,
      algorithmFamily: verifyProduct.algorithm_family,
      timeoutClass: verifyProduct.timeout_class,
      status: 'hidden',
      helpPolicy: 'hidden',
      textCommand: verifyProduct.text_command,
      slash: false,
      text: true,
    },
  );

  // Advanced objective is an option on private text ingress, not a runtime
  // command capability. Its deliberate absence is part of the projection.
  assert.equal(findProductCapability('advanced.objective'), null);
});

test('objective is advanced text and CLI only while verify is undiscoverable text diagnostics', () => {
  const contract = registry();
  const objective = byId(contract.capabilities, 'advanced.objective');
  const verify = byId(contract.capabilities, 'diagnostic.verify');
  const advanced = contract.surface_profiles[objective.surface_profile];
  const diagnostic = contract.surface_profiles[verify.surface_profile];

  assert.equal(objective.command_path, null);
  assert.equal(objective.text_syntax, '--objective <registered-objective-id>');
  assert.equal(objective.cli_syntax, '--objective <registered-objective-id>');
  assert.deepEqual(objective.text_prefixes, ['$', '>']);
  assert.equal(objective.authorization, 'ordinary-user');
  assert.deepEqual(advanced, {
    discord_slash: 'none',
    discord_text: 'advanced',
    cli: 'advanced',
    gui: 'none',
    web: 'none',
    desktop: 'none',
    modal: 'none',
    autocomplete: 'none',
    help_visibility: 'advanced-objective-only',
  });

  assert.equal(verify.command_path, null);
  assert.equal(verify.text_command, 'verify');
  assert.deepEqual(verify.text_prefixes, ['$', '>']);
  assert.equal(verify.authorization, 'ordinary-user');
  assert.equal(verify.algorithm_family, 'verification');
  assert.equal(verify.timeout_class, 'diagnostic');
  assert.deepEqual(diagnostic, {
    discord_slash: 'none',
    discord_text: 'hidden',
    cli: 'internal',
    gui: 'none',
    web: 'none',
    desktop: 'none',
    modal: 'none',
    autocomplete: 'none',
    help_visibility: 'hidden',
  });

  const readme = readFileSync(readmeUrl, 'utf8');
  assert.doesNotMatch(readme, /`--objective(?:\s|`)/i);
  assert.doesNotMatch(readme, /advanced\.objective/i);
  assert.doesNotMatch(readme, /clearra\s+verify\b/i);
  assert.doesNotMatch(readme, /`--diagnostics`/i);
  assert.doesNotMatch(readme, /diagnostic\.verify/i);
});

test('problem families with different inputs or results cannot be declared aliases', () => {
  const contract = registry();
  const capability = (id) => byId(contract.capabilities, id);

  assert.notEqual(capability('pc.path').problem_contract_id, capability('build.cover').problem_contract_id);
  assert.notEqual(capability('pc.path').input_schema_id, capability('build.cover').input_schema_id);
  assert.notEqual(capability('forward.spin').problem_contract_id, capability('spin-structure.search').problem_contract_id);
  assert.notEqual(capability('forward.spin').algorithm_family, capability('spin-structure.search').algorithm_family);
  assert.notEqual(capability('pc.allspin-sol').input_schema_id, capability('pc.allspin-pres-chance').input_schema_id);
  assert.notEqual(capability('pc.allspin-sol').result_contract_id, capability('pc.allspin-pres-chance').result_contract_id);
  assert.notEqual(capability('pc.saves').result_contract_id, capability('pc.best-save').result_contract_id);
  assert.notEqual(capability('build.cover').problem_contract_id, capability('build.evaluate.cover').problem_contract_id);
  assert.notEqual(capability('build.finesse-score').input_schema_id, capability('build.cover').input_schema_id);

  const finesseAlias = capability('build.cover').compatibility_aliases
    .find(({ path }) => path?.join(' ') === 'finesse search');
  assert.equal(finesseAlias.preset, 'finesse=inputs mirror=exclude');
  assert.equal(finesseAlias.remove_in, 'v0.10.0');
});

test('finesse search is implemented as a Build preset while finesse score keeps a distinct contract', () => {
  const contract = registry();
  const buildCover = byId(contract.capabilities, 'build.cover');
  const finesseScore = byId(contract.capabilities, 'build.finesse-score');
  const finesseScoreImplementation = contract.capability_implementation.find(
    ({ capability_id: capabilityId }) => capabilityId === 'build.finesse-score',
  );
  const requirement = byId(contract.requirements, 'REQ-V080-008');
  const slashAlias = buildCover.compatibility_aliases.find(
    ({ surface, path }) => surface === 'discord-slash' && path?.join(' ') === 'finesse search',
  );
  const textAlias = buildCover.compatibility_aliases.find(
    ({ surface, name }) => surface === 'discord-text' && name === 'finesse search',
  );

  assert.equal(slashAlias.preset, 'finesse=inputs mirror=exclude');
  assert.equal(textAlias.preset, 'finesse=inputs mirror=exclude');
  assert.notEqual(finesseScore.problem_contract_id, buildCover.problem_contract_id);
  assert.notEqual(finesseScore.algorithm_family, buildCover.algorithm_family);
  assert.notEqual(finesseScore.input_schema_id, buildCover.input_schema_id);
  assert.notEqual(finesseScore.modal_schema_id, buildCover.modal_schema_id);
  assert.notEqual(finesseScore.result_contract_id, buildCover.result_contract_id);
  assert.equal(finesseScoreImplementation?.implementation_status, 'implemented');
  assert.equal(requirement.implementation_status, 'implemented');
});

test('family topology and current GUI option exposure requirements are implemented independently of future Build semantics', () => {
  const contract = registry();
  const topology = byId(contract.requirements, 'REQ-V080-002');
  const optionExposure = byId(contract.requirements, 'REQ-V080-003');
  const futureBuildSemantics = byId(contract.requirements, 'REQ-V080-007');
  const publicPaths = contract.capabilities
    .filter(({ command_path: commandPath }) => commandPath !== null)
    .map(({ command_path: commandPath }) => commandPath.join(' '));

  assert.equal(topology.implementation_status, 'implemented');
  assert.equal(optionExposure.implementation_status, 'implemented');
  assert.equal(futureBuildSemantics.implementation_status, 'implemented');
  assert.equal(publicPaths.includes('search'), false);
  assert.ok(publicPaths.includes('pc path'));
  assert.ok(publicPaths.includes('build cover'));
  assert.ok(publicPaths.includes('setup joint'));
  assert.ok(publicPaths.includes('forward spin'));
  assert.ok(publicPaths.includes('spin-structure search'));
  assert.ok(
    contract.result_affecting_option_exposure.every(
      ({ target_discord_exposure: exposure }) =>
        ['advanced-objective', 'forbidden', 'named-option', 'semantic-subcommand'].includes(exposure),
    ),
  );
});

test('unsafe and ambient-state upstream commands never map to a public capability', () => {
  const contract = registry();
  const commands = new Map(contract.upstream_command_inventory.map((entry) => [entry.id, entry]));
  const excludedNames = [
    'sfinder-man/sfinder',
    'sfinder-man/shutdown',
    'sfinder-man/purge_folders',
    'sfinder-man/uploadfile',
    'sfinder-man/deletefile',
    'sfinder-man/currentcommands',
    'sfinder-man/killmyprocesses',
    'sfinderbot/sudo',
    'sfinderbot/view_database',
  ];
  for (const id of excludedNames) {
    assert.equal(commands.get(id)?.disposition, 'excluded', `must remain excluded: ${id}`);
  }
});

test('result-affecting options have one explicit Discord exposure and implementation state', () => {
  const contract = registry();
  const options = contract.result_affecting_option_exposure;
  assertUniqueIds(options, 'result-affecting option');
  const allowedExposures = new Set([
    'advanced-objective',
    'forbidden',
    'named-option',
    'semantic-subcommand',
  ]);
  const allowedStatuses = new Set(['implemented', 'missing', 'partial']);
  const effects = new Set(contract.effect_classes);
  for (const option of options) {
    assert.ok(allowedExposures.has(option.target_discord_exposure), `unknown exposure: ${option.id}`);
    assert.ok(allowedStatuses.has(option.implementation_status), `unknown option status: ${option.id}`);
    assert.ok(effects.has(option.effect_class), `unknown option effect: ${option.id}`);
    if (option.implementation_status === 'implemented') {
      assert.ok(Array.isArray(option.implementation_evidence));
      assert.ok(option.implementation_evidence.length > 0, `implemented option needs evidence: ${option.id}`);
    }
  }

  assert.equal(byId(options, 'pc.target-field').target_discord_exposure, 'forbidden');
  assert.equal(byId(options, 'build.objective').target_discord_exposure, 'advanced-objective');
  assert.equal(byId(options, 'setup.priority').target_discord_exposure, 'semantic-subcommand');
  assert.equal(byId(options, 'spin-structure.minimality').target_discord_exposure, 'named-option');
  const buildSourcePieces = byId(options, 'build.source-pieces');
  assert.equal(buildSourcePieces.implementation_status, 'implemented');
  assert.ok(buildSourcePieces.implementation_evidence.length > 0);
  assert.deepEqual(
    options
      .filter(({ implementation_status: status }) => status !== 'implemented')
      .map(({ id }) => id)
      .sort(),
    [],
  );
});

test('alias and non-alias decisions state their typed-contract rationale', () => {
  const contract = registry();
  const decisions = contract.alias_decisions;
  assertUniqueIds(decisions, 'alias decision');
  const capabilityIds = new Set(contract.capabilities.map(({ id }) => id));
  const classifications = new Set([
    'distinct',
    'equivalence',
    'fixed-preset',
    'generic-compatibility',
  ]);
  for (const decision of decisions) {
    assert.ok(classifications.has(decision.classification), `unknown alias classification: ${decision.id}`);
    assert.ok(Array.isArray(decision.names) && decision.names.length >= 2);
    assert.ok(Array.isArray(decision.targets) && decision.targets.length >= 1);
    assert.ok(decision.rationale.length >= 20, `alias rationale too weak: ${decision.id}`);
    for (const target of decision.targets) {
      assert.ok(capabilityIds.has(target), `unknown alias target ${target}: ${decision.id}`);
    }
    if (decision.classification === 'equivalence') {
      assert.equal(decision.targets.length, 1, `equivalent aliases need one typed target: ${decision.id}`);
    }
    if (decision.classification === 'distinct') {
      assert.ok(decision.targets.length >= 2, `distinct commands need distinct targets: ${decision.id}`);
      assert.equal(new Set(decision.targets).size, decision.targets.length);
    }
    if (decision.classification === 'generic-compatibility') {
      assert.equal(decision.targets.length, 1, `generic compatibility needs one migration target: ${decision.id}`);
      assert.match(decision.rationale, /generic.*contract/iu);
    }
  }
});

test('requirement completion claims are evidence-backed', () => {
  const contract = registry();
  const allowedStatuses = new Set(contract.implementation_statuses);
  for (const requirement of contract.requirements) {
    assert.equal(requirement.contract_status, 'accepted');
    assert.ok(allowedStatuses.has(requirement.implementation_status));
    assert.ok(Array.isArray(requirement.implementation_evidence));
    assert.notEqual(requirement.implementation_status, 'excluded');
    if (requirement.implementation_status === 'implemented') {
      assert.ok(requirement.implementation_evidence.length > 0, `completion needs evidence: ${requirement.id}`);
    }
  }

  for (const id of [
    'REQ-V080-004',
    'REQ-V080-005',
    'REQ-V080-006',
    'REQ-V080-009',
    'REQ-V080-010',
    'REQ-V080-011',
    'REQ-V080-012',
    'REQ-V080-016',
    'REQ-V080-017',
    'REQ-V080-018',
    'REQ-V080-020',
    'REQ-V080-021',
  ]) {
    assert.equal(byId(contract.requirements, id).implementation_status, 'implemented', id);
  }
  assert.equal(byId(contract.requirements, 'REQ-V080-019').implementation_status, 'implemented');
  assert.equal(byId(contract.requirements, 'REQ-V080-014').implementation_status, 'implemented');
  assert.equal(byId(contract.requirements, 'REQ-V080-015').implementation_status, 'implemented');
});

test('[release gate] expired slash aliases are absent at v0.10.0 and later', () => {
  const contract = registry();
  const release = parseRelease(contract.target_release);
  const expired = contract.runtime_projection.legacy_routes.filter((route) =>
    route.surface === 'discord-slash' &&
    route.remove_in &&
    compareRelease(release, parseRelease(route.remove_in)) >= 0
  );
  assert.deepEqual(
    expired,
    [],
    `release ${contract.target_release} retains expired slash aliases: ${expired
      .map(({ id }) => id)
      .join(', ')}`,
  );
});

test('[release gate] v0.8.0 readiness closes', () => {
  const contract = registry();
  const terminal = new Set(contract.release_readiness.allowed_terminal_implementation_statuses);
  const blockers = [
    ...contract.capability_implementation.map((entry) => ({
      id: `capability:${entry.capability_id}`,
      status: entry.implementation_status,
    })),
    ...contract.result_affecting_option_exposure.map((entry) => ({
      id: `option:${entry.id}`,
      status: entry.implementation_status,
    })),
    ...contract.requirements.map((entry) => ({
      id: `requirement:${entry.id}`,
      status: entry.implementation_status,
    })),
  ].filter(({ status }) => !terminal.has(status));

  assert.equal(
    blockers.length,
    0,
    `v0.8.0 is not ready; ${blockers.length} open ledger entries: ${blockers
      .slice(0, 20)
      .map(({ id, status }) => `${id}=${status}`)
      .join(', ')}`,
  );
  assert.equal(contract.release_readiness.status, 'ready');
});

function parseRelease(value) {
  const match = /^v(\d+)\.(\d+)\.(\d+)$/u.exec(value);
  assert.ok(match, `invalid release: ${value}`);
  return match.slice(1).map(Number);
}

function compareRelease(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}
