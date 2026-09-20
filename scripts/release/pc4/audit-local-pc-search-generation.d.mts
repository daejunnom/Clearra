export const PC4_STRUCTURAL_KAT_AUDIT_RECEIPT_SCHEMA:
  'clearra.pc4.structural-kat-audit.v1';
export const PC4_STRUCTURAL_KAT_AUDIT_SCOPE:
  'complete-artifact-structure-and-bounded-observed-kat-only';
export const PC4_MISSING_PC_SEARCH_SEMANTIC_PROOF_IDENTITIES: Readonly<{
  outgoing_edge_completeness_identity: null;
  offline_exact_parity_identity: null;
}>;

export type Pc4StructuralKatAuditReceipt = Readonly<{
  schema: 'clearra.pc4.structural-kat-audit.v1';
  authority: 'non-target-qualification-evidence';
  target_qualification_schema: 'clearra.pc4.exact-target-qualification.v1';
  evidence_scope: 'complete-artifact-structure-and-bounded-observed-kat-only';
  repository: string;
  revision: string;
  profile: 'jstris-180';
  reader_contract: string;
  target_lines: 4;
  qualification_status: 'not-qualified';
  pc_search_target_receipt: null;
  missing_semantic_proof_identities: Readonly<{
    outgoing_edge_completeness_identity: null;
    offline_exact_parity_identity: null;
  }>;
  audit_identity: string;
}>;

export function auditPc4LocalPcSearchGeneration(options: {
  generation: unknown;
  profile?: 'jstris-180';
  rootKat: unknown;
  nonemptyKat: unknown;
  readArtifact: (
    artifact: { path: string; byte_length: number; content_identity: string },
    offset: number,
    length: number,
  ) => Promise<Uint8Array>;
  chunkRecords?: number;
  maxReadBytes?: number;
}): Promise<Pc4StructuralKatAuditReceipt>;
