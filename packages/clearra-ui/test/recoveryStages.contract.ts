import {
  appendRecoveryMiddle, removeRecoveryMiddle, recoveryStageFrames,
  captureRecoveryEditor, restoreRecoveryEditor, recoveryMiddleStages,
  setRecoveryStageSupply, type RecoveryStageFrame
} from '../src/lib/workspace/recoveryStageModel';
import { createRecoveryBuildRequest, recoveryBuildInputKey } from '../src/lib/workspace/recoveryBuildModel';
import { recoveryFieldReferences } from '../src/lib/workspace/recoveryFieldReferences';
const original = createRecoveryBuildRequest();
const grown = appendRecoveryMiddle(original);
const modified = setRecoveryStageSupply(grown.request, grown.selection.middleId, 'P7');
const snapshot = captureRecoveryEditor(modified);
const restored = restoreRecoveryEditor(modified, snapshot);
const frames: RecoveryStageFrame[] = recoveryStageFrames(restored);
const identity: string = recoveryBuildInputKey(restored);
const fields = recoveryMiddleStages(restored);
const references = recoveryFieldReferences(restored,'middleMask',true,fields[0].id);
const direction: 'forward'|'backward'|undefined = references[0]?.hatch;
void removeRecoveryMiddle;void frames;void identity;void direction;
