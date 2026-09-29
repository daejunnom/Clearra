import { recoveryChainOrientationAllowed, recoveryChainSourcePages, validateRecoveryChainEvidence } from '../src/lib/workspace/recoveryChainEvidence';
import type { RecoveryChainEvidence } from '../src/lib/workspace/recoveryChainEvidence';
const candidate:unknown={};
if (validateRecoveryChainEvidence(candidate)) {
  const evidence:RecoveryChainEvidence=candidate;
  recoveryChainSourcePages(evidence,false);
  recoveryChainOrientationAllowed(evidence.initial,evidence.requestedTargets,evidence.targets,evidence.height);
}
