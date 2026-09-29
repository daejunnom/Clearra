"""Apply a source-bound initial-boundary mirror repair on an isolated checkout.
No release rules, resource limits or runtime command guards are changed.
"""
from pathlib import Path
import subprocess

BASE = 'b781f6b6ef1c9ce27e0430f87663a92508e1cb7d'
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == BASE
root = Path('.')

def edit(path, before, after, count=1):
    p = root / path
    text = p.read_text()
    assert text.count(before) == count, (path, before[:100], text.count(before))
    p.write_text(text.replace(before, after))

def append(path, text):
    p = root / path
    p.write_text(p.read_text() + text)

core = 'crates/clearra-forward-search/src/recovery_build/'
(root / (core + 'mirror.rs')).write_text('''//! Reflect the complete remaining target suffix at a symmetric boundary.
//! Queues, hold tokens and kick rules remain unchanged and are verified again.
use super::{RecoveryBuildError as Error, RecoveryBuildFields};
use crate::board::{place_and_clear, ForwardBoard};
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
use clearra_problem::BuildProbabilityField;

fn reflected(mask: Mask, height: u8) -> Result<Mask, Error> {
    mask.mirrored_horizontally(10, u16::from(height))
        .map_err(|_| Error::BoardOutsideField)
}
fn applicable(height: u8, base: Mask, target: Mask) -> Result<bool, Error> {
    Ok(BuildProbabilityField::from_words_preserving_height(height, base.words(), target.words())
        .map_err(|_| Error::BoardOutsideField)?
        .with_horizontal_mirror_included(true)
        .includes_applicable_horizontal_mirror())
}
fn append_final(output: &mut Vec<RecoveryBuildFields>, fields: RecoveryBuildFields) -> Result<(), Error> {
    if !output.contains(&fields) { output.push(fields.clone()); }
    let (base, _, _) = place_and_clear(10, fields.height,
        ForwardBoard::from_mask(fields.initial.union(fields.middle)));
    if applicable(fields.height, Mask::from_words(base.words()), fields.result)? {
        let mut other = fields;
        other.result = reflected(other.result, other.height)?;
        if !output.contains(&other) { output.push(other); }
    }
    Ok(())
}
pub(super) fn orientations(fields: &RecoveryBuildFields) -> Result<Vec<RecoveryBuildFields>, Error> {
    fields.prepare()?;
    let mut result = Vec::with_capacity(4);
    // Retain the two pre-existing direction IDs before adding initial mirrors.
    append_final(&mut result, fields.clone())?;
    if applicable(fields.height, fields.initial, fields.middle)? {
        let mut other = fields.clone();
        other.middle = reflected(fields.middle, fields.height)?;
        other.result = reflected(fields.result, fields.height)?;
        append_final(&mut result, other)?;
    }
    Ok(result)
}
''')
edit(core + 'mod.rs', 'mod field;\n', 'mod field;\nmod mirror;\n')
p = root / (core + 'staged/geometry.rs')
s = p.read_text()
a = s.index('        let mut fields = vec![query.fields.clone()];')
b = s.index('        let stages = fields', a)
s = s[:a] + '        let fields = super::super::mirror::orientations(&query.fields)?;\n' + s[b:]
s = s.replace('fn as_mask(board: ForwardBoard) -> Mask {\n    Mask::from_words(board.words())\n}\n', '')
p.write_text(s)
edit(core + 'search.rs', '    pub terminal_board: [u64; 4],\n', '    pub terminal_board: [u64; 4],\n    pub middle_target: [u64; 4],\n')
edit(core + 'search.rs', 'result_target: self.fields.result.words(),', 'middle_target: self.fields.middle.words(),\n                    result_target: self.fields.result.words(),', 2)
edit(core + 'staged/solver.rs', '                        result_target: self.geometry.stages[usize::from(pos.stage)]', '                        middle_target: self.geometry.stages[usize::from(pos.stage)].fields.middle.words(),\n                        result_target: self.geometry.stages[usize::from(pos.stage)]')
for magic in ['RBIN', 'RBTK', 'RBRS']:
    edit(core + 'parallel/wire.rs', magic + r'\x04', magic + r'\x05')
edit(core + 'parallel/wire.rs', '    w.words(p.terminal_board);\n', '    w.words(p.terminal_board);\n    w.words(p.middle_target);\n')
edit(core + 'parallel/wire.rs', '    let terminal_board = r.words()?;\n', '    let terminal_board = r.words()?;\n    let middle_target = r.words()?;\n')
edit(core + 'parallel/wire.rs', '        terminal_board,\n', '        terminal_board,\n        middle_target,\n')
for magic in ['RCATK', 'RCATR']:
    edit(core + 'catalog/wire.rs', magic + r'\x02', magic + r'\x03')
edit(core + 'catalog/wire.rs', 'if orientation > 1 {', 'if orientation > 3 {')
edit(core + 'catalog.rs', '        let languages = self.diagram.import(&packet.languages, self.source.end)?;', '''        let variants = super::mirror::orientations(&self.prepared.query.fields)?;
        let target = variants.get(usize::from(packet.task.plan.orientation))
            .ok_or(ParallelError::InvalidWire("unknown catalog orientation"))?;
        let languages = self.diagram.import(&packet.languages, self.source.end)?;''')
edit(core + 'catalog.rs', '                    || e.path.status != status\n', '                    || e.path.status != status\n                    || e.path.middle_target != target.middle.words()\n                    || e.path.result_target != target.result.words()\n')
edit(core + 'parallel.rs', '        for (category, slot) in [(0, &mut batch.block.normal), (1, &mut batch.block.recovery)] {', '        let variants = super::mirror::orientations(&self.source.query.fields)?;\n        for (category, slot) in [(0, &mut batch.block.normal), (1, &mut batch.block.recovery)] {')
edit(core + 'parallel.rs', '                    || example.path.status != status\n', '''                    || example.path.status != status
                    || !variants.iter().any(|f| example.path.middle_target == f.middle.words()
                        && example.path.result_target == f.result.words())
''')
app = 'crates/clearra-app/src/commands/recovery_build_app_command.rs'
edit(app, 'recovery-build.v3:', 'recovery-build.v4:', 2)
edit(app, '        terminal_board_mask: mask(path.terminal_board),\n', '        terminal_board_mask: mask(path.terminal_board),\n        middle_target_mask: Some(mask(path.middle_target)),\n')
host = 'crates/clearra-host-contract/src/recovery_build_payload.rs'
edit(host, '    pub terminal_board_mask: String,\n', '    pub terminal_board_mask: String,\n    #[cfg_attr(feature = "serde", serde(default))]\n    pub middle_target_mask: Option<String>,\n')
edit(host, '        bytes = bytes.checked_add(self.terminal_board_mask.capacity() as u128)?;\n', '        bytes = bytes.checked_add(self.terminal_board_mask.capacity() as u128)?;\n        bytes = bytes.checked_add(self.middle_target_mask.as_ref().map_or(0, |v| v.capacity()) as u128)?;\n')
edit('crates/clearra-wasm/src/json_event_envelope/recovery_build_json.rs', '    object.string("terminal_board_mask", &source.terminal_board_mask);\n', '    object.string("terminal_board_mask", &source.terminal_board_mask);\n    object.optional_string("middle_target_mask", source.middle_target_mask.as_deref());\n')
edit('crates/clearra-wasm/src/wasm_command_runtime/recovery_build_projection.rs', '        terminal_board_mask: try_owned_string(&source.terminal_board_mask, ledger)?,\n', '        terminal_board_mask: try_owned_string(&source.terminal_board_mask, ledger)?,\n        middle_target_mask: try_optional_owned_string(source.middle_target_mask.as_deref(), ledger)?,\n')
ui = 'packages/clearra-ui/src/lib/workspace/'
edit(ui + 'recoveryBuildPayloadTypes.ts', '  terminal_board_mask: string;', '  middle_target_mask?: string;\n  terminal_board_mask: string;')
p = root / (ui + 'recoveryBuildPresentation.ts')
s = p.read_text()
needle = 'function require(ok: unknown)'
pos = s.index(needle)
s = s[:pos] + '''export function recoveryTargetOrientations(start: bigint, middle: bigint, result: bigint, height: number): Array<{middle: bigint; result: bigint}> {
  const pairs: Array<{middle: bigint; result: bigint}> = [];
  const add = (m: bigint, r: bigint) => {
    if (!pairs.some(p => p.middle === m && p.result === r)) pairs.push({middle:m,result:r});
  };
  const suffix = (m: bigint, r: bigint) => {
    add(m,r);
    const base = compactRecoveryBoard(start|m,height);
    if (base === mirror(base,height)) add(m,mirror(r,height));
  };
  suffix(middle,result);
  if (start === mirror(start,height)) suffix(mirror(middle,height),mirror(result,height));
  return pairs;
}
''' + s[pos:]
s = s.replace('    for (const e of [...p.examples, ...solutions.map(s=>s.example)]) {', '''    const orientations = recoveryTargetOrientations(start,middle,result,p.height);
    for (const e of [...p.examples, ...solutions.map(s=>s.example)]) {
      const middleHex = e.middle_target_mask ?? p.middle_target_mask;
      require(hex(middleHex) && BigInt(middleHex) < bound);
      const targetMiddle = BigInt(middleHex);''')
s = s.replace('      require(targetResult === result || (base === mirror(base,p.height) && targetResult === mirror(result,p.height)));', '''      require(orientations.some(o => o.middle === targetMiddle && o.result === targetResult));
      const targetBase = compactRecoveryBoard(start|targetMiddle,p.height);''')
s = s.replace('compactRecoveryBoard(base|targetResult,p.height)', 'compactRecoveryBoard(targetBase|targetResult,p.height)')
s = s.replace('(((start|middle)>>BigInt(y*10))&1023n)', '(((start|targetMiddle)>>BigInt(y*10))&1023n)')
s = s.replace('const target=step.result_target?liftedResult:middle,', 'const target=step.result_target?liftedResult:targetMiddle,')
s = s.replace('usedMiddle!==middle', 'usedMiddle!==targetMiddle').replace('usedMiddle===middle', 'usedMiddle===targetMiddle')
p.write_text(s)
# Independent explicit reflected-target oracle, not a call back into mirror.rs.
p = root / (core + 'staged/tests.rs'); s = p.read_text()
a = s.index('fn orientations(q: &RecoveryBuildQuery)'); b = s.index('fn oracle(', a)
s = s[:a] + '''fn orientations(q: &RecoveryBuildQuery) -> Vec<RecoveryBuildFields> {
    let mut candidates = vec![q.fields.clone()];
    if reflected(q.fields.initial, q.fields.height) == q.fields.initial {
        let mut f = q.fields.clone();
        f.middle = reflected(f.middle, f.height);
        f.result = reflected(f.result, f.height);
        candidates.push(f);
    }
    let mut result = Vec::new();
    for original in candidates {
        let (base, _, _) = place_and_clear(10, original.height,
            ForwardBoard::from_mask(original.initial.union(original.middle)));
        let base = Mask::from_words(base.words());
        if !result.contains(&original) { result.push(original.clone()); }
        if reflected(base, original.height) == base {
            let mut f = original;
            f.result = reflected(f.result, f.height);
            if !result.contains(&f) { result.push(f); }
        }
    }
    result
}

''' + s[b:]; p.write_text(s)
edit(core + 'catalog/tests.rs', '        2,\n        "4x2 has two horizontal I or two O tilings under these supplies"', '        4,\n        "two 4x2 tilings in each initial-symmetry orientation"')
append('crates/clearra-wasm/tests/recovery_build_wire.rs', '''
#[test]
fn recovery_build_initial_mirror_targets_survive_both_evidence_encodings() {
    let base = "clearra recovery build --start-mask 0 --middle-mask 0x1007 --result-mask 0x300c00 --height 8 --first-supply J --second-supply O --no-hold --no-piece-exchange --max-early 0";
    let runtime = WasmCommandRuntime::default().with_host_capabilities(WasmHostCapabilities::new(1, false, false));
    for suffix in ["", " --all-solutions --minimum-solutions"] {
        let result = runtime.run_command_text(&format!("{base}{suffix}")).unwrap();
        assert_eq!(result.app_response().status(), AppStatus::Success);
        let payload = result.app_response().product_result_payload().unwrap();
        let ProductResultPayloadContent::RecoveryBuild(report) = payload.content() else { panic!("recovery output") };
        assert_eq!(report.normal_count, "1");
        let example = &report.examples[0];
        assert_eq!(example.middle_target_mask, Some(format!("0x{:064x}", 0x20380_u64)));
        assert_eq!(example.result_target_mask, format!("0x{:064x}", 0x300c0000_u64));
        let events: serde_json::Value = serde_json::from_str(&serialize_distributed_final_events(19, &result).unwrap()).unwrap();
        let terminal = events.as_array().unwrap().iter().find(|e| e["event"] == "final_response").unwrap();
        assert_eq!(terminal["response"]["product_result_payload"], serde_json::to_value(payload).unwrap());
        let asymmetric = runtime.run_command_text(&format!("{}{}", base.replace("--start-mask 0 ", "--start-mask 512 "), suffix)).unwrap();
        let ProductResultPayloadContent::RecoveryBuild(report) = asymmetric.app_response().product_result_payload().unwrap().content() else { panic!("asymmetric output") };
        assert_eq!(report.normal_count, "0");
    }
}
''')
print(subprocess.check_output(['git', 'diff', '--stat'], text=True))
