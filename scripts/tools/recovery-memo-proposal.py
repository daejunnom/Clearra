"""Apply the reviewed memo-only quotient to the pinned Recovery source.
No fixture, rule, probability measure, queue domain or deployment edits.
"""
from pathlib import Path
import hashlib

root = Path('crates/clearra-forward-search/src/recovery_build/staged')

def read_pinned(name, expected):
    data = (root / name).read_bytes()
    actual = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    if actual != expected:
        raise SystemExit(f'{name}: source moved ({actual}); refusing an unreviewed edit')
    return data.decode()

def once(source, old, new):
    if source.count(old) != 1:
        raise SystemExit(f'Expected a unique source anchor: {old[:100]}')
    return source.replace(old, new, 1)

s = read_pinned('solver.rs', '6353e74ed9e198ab9102ac253ae447bd7ec51d7c')
s = once(s, '    memo: HashMap<Key, Id>,\n', '    memo: HashMap<Key, Id>,\n    #[cfg(test)]\n    pub(super) literal_memo: bool,\n')
s = once(s, '            memo: HashMap::new(),\n', '            memo: HashMap::new(),\n            #[cfg(test)]\n            literal_memo: false,\n')
s = once(s, '            if let Some(&value) = self.memo.get(&key) {', '            if let Some(&value) = self.memo.get(&self.memo_key(key)) {')
s = once(s, '        self.memo.insert(key, value);', '        self.memo.insert(self.memo_key(key), value);')
s = once(s, '                        if let Some(&value) = self.memo.get(&action.child) {', '                        if let Some(&value) = self.memo.get(&self.memo_key(action.child)) {')
method = '''    /// Quotient continuation answers, never execution states or witnesses.
    /// The token's exact rank only labels output; future rules inspect its
    /// piece and which of the two original supplies it belongs to. Draw depth,
    /// unread/allowed languages, active-vs-hold, board, deletion history, B2B,
    /// first-used count and the exact early-placement limit remain in the key.
    fn memo_key(&self, mut key: Key) -> Key {
        #[cfg(test)]
        if self.literal_memo {
            return key;
        }
        for token in [&mut key.active, &mut key.hold].into_iter().flatten() {
            token.index = if token.index < self.source.first_len {
                0
            } else {
                self.source.first_len
            };
        }
        if self.query.allow_piece_exchange {
            // This balance is not an acceptance condition with repayment on.
            // Keep it in the actual Key for the final replay, not in the memo.
            key.exchange = [0; 7];
            let pos = self.geometry.position(key.geometry);
            let stage = &self.geometry.stages[usize::from(pos.stage)];
            if usize::from(self.source.end)
                == stage.prepared.middle_pieces + stage.prepared.result_pieces
                && self.source.first_counts.is_some()
                && self.source.second_counts.is_some()
            {
                // caps() reads only their sum in this mode. Both regions have
                // at most 60 tetrominoes, so the u8 sum is bounded by 120.
                for piece in 0..7 {
                    key.result_counts[piece] += key.middle_counts[piece];
                }
                key.middle_counts = [0; 7];
            } else {
                // caps() does not inspect either inventory in this mode.
                key.middle_counts = [0; 7];
                key.result_counts = [0; 7];
            }
        }
        key
    }
'''
s = once(s, '    fn remember(&mut self, key: Key, value: Id) -> Result<(), Error> {', method + '    fn remember(&mut self, key: Key, value: Id) -> Result<(), Error> {')
(root / 'solver.rs').write_text(s)
t = read_pinned('tests.rs', 'c1226110b27e32a49320e58b9b49f0affb5d9ea1')
t += r'''

fn memo_answers(q: &RecoveryBuildQuery, literal: bool, start: usize, rows: usize)
    -> (Vec<(bool, bool)>, [u128; 3], u128)
{
    let control = ExecutionControl::default();
    let p = PreparedPopulation::new(q.clone()).unwrap();
    let geometry = Geometry::new(q, &control).unwrap();
    let mut b = Block::new(&p, geometry, start, rows, 3, &control).unwrap();
    b.solver.literal_memo = literal;
    while !b.advance(&control).unwrap() {}
    let mut answers = Vec::new();
    for i in start..start + rows {
        let first = p.first.sequence_at(i);
        let normal = b.solver.source.follow_first(&b.solver.diagram, b.solver.normal, &first);
        let recovery = b.solver.source.follow_first(&b.solver.diagram, b.solver.recovery, &first);
        for j in 0..p.second.pattern_count() {
            let second = p.second.sequence_at(j);
            answers.push((
                b.solver.source.accepts_second(&b.solver.diagram, normal, &second),
                b.solver.source.accepts_second(&b.solver.diagram, recovery, &second),
            ));
        }
    }
    let states = b.solver.states;
    let (r, _) = b.finish(&p, &control).unwrap();
    for e in [&r.normal, &r.recovery].into_iter().flatten() {
        // The quotient may share answers across ranks, never replay indices.
        let supply = e.first_queue.iter().chain(&e.second_queue).collect::<Vec<_>>();
        let mut used = std::collections::HashSet::new();
        let mut exchange = [0_i16; 7];
        let mut active = None;
        let mut held = None;
        let mut cursor = 0;
        for s in &e.path.steps {
            if active.is_none() && cursor < supply.len() {
                active = Some(cursor);
                cursor += 1;
            }
            match s.hold_decision {
                "none" => {}
                "store" => {
                    assert!(q.hold_enabled && held.is_none());
                    held = active.take();
                    assert!(cursor < supply.len());
                    active = Some(cursor);
                    cursor += 1;
                }
                "swap" => {
                    assert!(q.hold_enabled && active.is_some() && held.is_some());
                    std::mem::swap(&mut active, &mut held);
                }
                "release-held-at-terminal" => {
                    assert!(q.hold_enabled && cursor == supply.len() && active.is_none());
                    active = held.take();
                }
                other => panic!("unknown hold event: {other}"),
            }
            assert_eq!(active.take(), Some(s.source_index));
            assert!(used.insert(s.source_index));
            assert_eq!(*supply[s.source_index], s.piece);
            exchange[crate::recovery_build::search::piece_index(s.piece)] +=
                i16::from(s.source_index < e.first_queue.len()) - i16::from(!s.result_target);
        }
        assert_eq!(exchange, e.path.exchange_balance);
    }
    (answers, r.counts, states)
}

#[test]
fn recovery_build_memo_quotient_preserves_each_pair_and_real_hold_indices() {
    for hold in [false, true] {
        for exchange in [false, true] {
            for limit in [CrossStageEarlyLimit::Auto, CrossStageEarlyLimit::AtMost(1)] {
                for extra in [false, true] {
                    let mut q = query();
                    q.hold_enabled = hold;
                    q.allow_piece_exchange = exchange;
                    q.early_limit = limit;
                    q.fields.middle = mask((0..4).fold(0, |m, y| m | (5 << (10*y))));
                    q.fields.result = mask((0..4).fold(0, |m, y| m | (0x300 << (10*y))));
                    let supply = if extra { "[IO][IO][IO]" } else { "[IO][IO]" };
                    q.first_supply = supply.into();
                    q.second_supply = supply.into();
                    // The independent fixed-pair engine validates every member
                    // and checks physical replay, not just the aggregate count.
                    compare(q.clone());
                    let rows = PreparedPopulation::new(q.clone()).unwrap().first.pattern_count();
                    let old = memo_answers(&q, true, 0, rows);
                    let new = memo_answers(&q, false, 0, rows);
                    assert_eq!(new.0, old.0, "{q:?}");
                    assert_eq!(new.1, old.1, "{q:?}");
                }
            }
        }
    }
}

#[test]
#[ignore = "finite managed A/B; same solver, source, rules, input ranks and resources"]
fn recovery_build_memo_quotient_benchmark() {
    let mut q = query();
    q.fields.height = 10;
    q.fields.initial = mask(0xc0383f3fc7);
    q.fields.middle = mask(0x3ff3fc7c0c038);
    // This is the engine's after-middle input. The browser test still paints
    // the original shared-coordinate fixture and calls the UI conversion.
    q.fields.result = mask(0x30483f07f3f8f);
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    q.preserve_b2b = true;
    for start in [0_usize, 704, 2496, 4992] {
        for exchange in [false, true] {
            q.allow_piece_exchange = exchange;
            let now = std::time::Instant::now();
            let old = memo_answers(&q, true, start, 32);
            let old_ms = now.elapsed().as_millis();
            let now = std::time::Instant::now();
            let new = memo_answers(&q, false, start, 32);
            let new_ms = now.elapsed().as_millis();
            assert_eq!(new.0, old.0, "all 161280 pairs, start={start}, exchange={exchange}");
            assert_eq!(new.1, old.1);
            eprintln!("memo_quotient_ab start={start} rows=32 exchange={exchange} counts={:?} literal_states={} quotient_states={} literal_ms={old_ms} quotient_ms={new_ms}", new.1, old.2, new.2);
        }
    }
}
'''
(root / 'tests.rs').write_text(t)
print('Applied only solver.rs and its regression tests; fixture and solver semantics unchanged.')
