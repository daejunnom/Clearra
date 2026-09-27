import assert from 'node:assert/strict';
import {createRecoveryBuildRequest,recoveryBuildDesktopRequest,recoveryBuildArguments,type RecoveryBuildRequest} from '../src/lib/workspace/recoveryBuildModel';
import {recoveryBuildMessage} from '../src/lib/workspace/recoveryBuildI18n';
const query:RecoveryBuildRequest={...createRecoveryBuildRequest(),firstSupply:'P7',secondSupply:'P7'};
assert.ok(recoveryBuildArguments(query).includes('--no-piece-exchange'));
assert.ok(recoveryBuildDesktopRequest(query,'ko'));
assert.equal(recoveryBuildMessage('ko','middle'),'중간');
