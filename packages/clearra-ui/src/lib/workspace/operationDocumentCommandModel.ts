import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from './cliCommandModel';

export type OperationDocumentCommandInput = {
  capability: 'sequence' | 'sequence-dependencies';
  document: string;
  ruleProfile: string;
  kickProfile: string;
  timeoutSeconds: number;
};

export function buildOperationDocumentCommandArguments(
  input: OperationDocumentCommandInput
): string[] {
  return [
    'clearra',
    'utility',
    input.capability,
    '--document',
    input.document,
    '--rule-profile',
    input.ruleProfile,
    '--kick-profile',
    input.kickProfile,
    '--timeout-seconds',
    String(input.timeoutSeconds)
  ];
}

export function buildOperationDocumentCommand(input: OperationDocumentCommandInput): string {
  return serializeCliCommandArguments(buildOperationDocumentCommandArguments(input));
}

export function operationDocumentRequestForDesktop(
  input: OperationDocumentCommandInput,
  language: 'en' | 'ko'
): ClearraDesktopCliCommandRequest {
  return cliCommandRequestForDesktop(buildOperationDocumentCommandArguments(input), language);
}
