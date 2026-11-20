import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog.ts';
import type { OperationDocumentCommandInput } from './operationDocumentCommandModel.ts';
import type { WorkspaceLanguage, WorkspaceMessageKey } from './workspaceI18n.ts';

// The workspace mode is also the canonical capability sent to both hosts.
export type OperationDocumentMode = OperationDocumentCommandInput['capability'];

type OperationDocumentPresentation = {
  title: WorkspaceMessageKey;
  description: ComponentMessageKey;
  documentLabel: ComponentMessageKey;
  documentHelp: ComponentMessageKey;
  resultTitle: ComponentMessageKey;
  emptyResult: ComponentMessageKey;
  reportLabels: Partial<Record<string, ComponentMessageKey>>;
};

export const operationDocumentWorkspaceModes = {
  sequence: {
    title: 'operationSequence',
    description: 'surfaceLosslessOperationTraceNormalizationAndReplayValidation',
    documentLabel: 'operationTraceDocument',
    documentHelp: 'theRecordedOperationOrderAndCoordinatesAre',
    resultTitle: 'normalizedReplayReport',
    emptyResult: 'runValidationToSeeThePlacementCount',
    reportLabels: {
      operation_count: 'operations',
      cleared_line_count: 'clearedLines',
      rule_profile: 'ruleProfile',
      kick_profile: 'kickProfile'
    }
  },
  'sequence-dependencies': {
    title: 'sequenceDependencies',
    description: 'surfaceExactOperationOrderDependencyAnalysis',
    documentLabel: 'operationDocument',
    documentHelp: 'thisDocumentAloneOwnsTheInitialField',
    resultTitle: 'exactDependencyReport',
    emptyResult: 'runTheAnalysisToSeeTheExact',
    reportLabels: {
      operation_count: 'operations',
      exact_order_count: 'validOrders',
      universal_dependency_count: 'requiredPrecedenceRelations',
      transitive_reduction_count: 'essentialDependencyEdges',
      independent_pair_count: 'independentPairs'
    }
  }
} as const satisfies Record<OperationDocumentMode, OperationDocumentPresentation>;

export function publicOperationDocumentReportFields(
  mode: OperationDocumentMode,
  fields: Array<[string, string]>,
  language: WorkspaceLanguage
): Array<[string, string]> {
  const labels: OperationDocumentPresentation['reportLabels'] =
    operationDocumentWorkspaceModes[mode].reportLabels;
  return fields.flatMap(([key, value]) => {
    const label = Object.hasOwn(labels, key) ? labels[key] : undefined;
    return label ? [[componentMessage(language, label), value] as [string, string]] : [];
  });
}
