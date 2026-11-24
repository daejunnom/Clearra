use super::*;
use crate::{model::*, StructureBoard};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::{RotationRequest, ScoringLockEvidence};
use clearra_rules::profile::rule_profile::RuleProfileId;

pub(super) const INIT: u32 = u32::from_le_bytes(*b"SSIN");
const TASK: u32 = u32::from_le_bytes(*b"SSTK");
const MANIFEST: u32 = u32::from_le_bytes(*b"SSMF");
const RESULT: u32 = u32::from_le_bytes(*b"SSRS");
const ORDERS: u32 = u32::from_le_bytes(*b"SSOR");
const VERSION: u32 = 2;

pub(super) fn is_init(input: &[u8]) -> bool {
    input.get(..4) == Some(INIT.to_le_bytes().as_slice())
}
pub(super) fn initialization(query: &SpinStructureQuery) -> Vec<u8> {
    let mut out = header(INIT);
    put_query(&mut out, query);
    out
}
pub(super) fn read_initialization(bytes: &[u8]) -> Result<SpinStructureQuery, Error> {
    let mut read = Reader::new(bytes);
    read.header(INIT)?;
    let query = read.query()?;
    read.end()?;
    Ok(query)
}
pub(super) fn task(query: &SpinStructureQuery, task: Task) -> Vec<u8> {
    let mut out = header(TASK);
    put_query(&mut out, query);
    match task {
        Task::Bootstrap => out.push(0),
        Task::Target { ordinal, target } => {
            out.push(1);
            put_u64(&mut out, ordinal as u64);
            put_operation(&mut out, target);
        }
        Task::Orders(task) => {
            out.push(2);
            put_order_task(&mut out, &task);
        }
    }
    out
}
pub(super) fn read_task(bytes: &[u8], query: &SpinStructureQuery) -> Result<Task, Error> {
    let mut read = Reader::new(bytes);
    read.header(TASK)?;
    read.require_query(query)?;
    let task = match read.u8()? {
        0 => Task::Bootstrap,
        1 => Task::Target {
            ordinal: read.index()?,
            target: read.operation()?,
        },
        2 => Task::Orders(read.order_task(query)?),
        _ => return Err(wire_error("spin_task_kind_invalid")),
    };
    read.end()?;
    Ok(task)
}

pub(super) fn order_completion(query: &SpinStructureQuery, completion: OrderCompletion) -> Vec<u8> {
    let mut out = header(ORDERS);
    put_query(&mut out, query);
    put_order_task(&mut out, &completion.task);
    put_u64(&mut out, completion.covered);
    out
}
fn put_order_task(out: &mut Vec<u8>, task: &OrderTask) {
    match task.mode {
        SpinStructureParallelMode::Search => unreachable!("order task mode"),
        SpinStructureParallelMode::Cover { max_patterns } => {
            out.push(0);
            put_u64(out, max_patterns as u64);
        }
        SpinStructureParallelMode::Guaranteed {
            final_piece,
            max_patterns,
        } => {
            out.push(1);
            put_u64(out, max_patterns as u64);
            out.push(final_piece.as_ascii() as u8);
        }
    }
    put_u64(out, task.candidate as u64);
    put_u64(out, task.first as u64);
    out.push(task.count as u8);
    put_operation(out, task.target);
    put_u64(out, task.operations.len() as u64);
    for operation in &task.operations {
        put_operation(out, *operation);
    }
}

pub(super) struct ManifestEncoder {
    out: Vec<u8>,
    next: usize,
}
impl ManifestEncoder {
    pub(super) fn new(plan: &SpinStructureSourcePlan) -> Self {
        let mut out = header(MANIFEST);
        put_query(&mut out, plan.query());
        put_u64(&mut out, plan.task_count() as u64);
        Self { out, next: 0 }
    }
    pub(super) fn step(&mut self, plan: &SpinStructureSourcePlan) -> bool {
        if let Some(target) = plan.targets().get(self.next) {
            put_operation(&mut self.out, *target);
            self.next += 1;
            false
        } else {
            true
        }
    }
    pub(super) fn finish(self) -> Vec<u8> {
        self.out
    }
}

pub(super) struct CompletionEncoder {
    out: Vec<u8>,
    regular: std::vec::IntoIter<SpinStructureOutcome>,
    mini: std::vec::IntoIter<SpinStructureOutcome>,
}
impl CompletionEncoder {
    pub(super) fn new(completion: SpinStructureSourceCompletion) -> Self {
        let mut out = header(RESULT);
        let report = completion.report;
        put_query(
            &mut out,
            report
                .query
                .as_ref()
                .expect("private completed source query"),
        );
        put_u64(&mut out, completion.ordinal as u64);
        put_operation(&mut out, completion.target);
        put_stages(&mut out, report.stages);
        out.push(report.minimum_placements.unwrap_or(0));
        put_u32(&mut out, report.layers.len() as u32);
        for layer in report.layers {
            put_layer(&mut out, layer);
        }
        put_u64(&mut out, report.regular.len() as u64);
        put_u64(&mut out, report.mini.len() as u64);
        Self {
            out,
            regular: report.regular.into_iter(),
            mini: report.mini.into_iter(),
        }
    }
    pub(super) fn step(&mut self) -> bool {
        if let Some(outcome) = self.regular.next().or_else(|| self.mini.next()) {
            put_outcome(&mut self.out, &outcome);
            false
        } else {
            true
        }
    }
    pub(super) fn finish(self) -> Vec<u8> {
        self.out
    }
}

pub(super) enum Decoded {
    Manifest(Vec<StructureOperation>),
    Completion(SpinStructureSourceCompletion),
    Orders(OrderCompletion),
}
enum DecodeKind {
    Orders(OrderCompletion),
    Manifest {
        count: usize,
        targets: Vec<StructureOperation>,
    },
    Completion {
        ordinal: usize,
        target: StructureOperation,
        regular: usize,
        mini: usize,
        report: Box<SpinStructureReport>,
    },
}
pub(super) struct Decoder {
    bytes: Vec<u8>,
    cursor: usize,
    kind: DecodeKind,
}
impl Decoder {
    pub(super) fn new(bytes: &[u8], query: &SpinStructureQuery) -> Result<Self, Error> {
        let mut read = Reader::new(bytes);
        let magic = read.u32()?;
        if read.u32()? != VERSION {
            return Err(wire_error("spin_wire_version_unsupported"));
        }
        read.require_query(query)?;
        let kind = match magic {
            ORDERS => {
                let task = read.order_task(query)?;
                let covered = read.u64()?;
                DecodeKind::Orders(OrderCompletion { task, covered })
            }
            MANIFEST => {
                let count = read.count(40)?;
                DecodeKind::Manifest {
                    count,
                    targets: Vec::new(),
                }
            }
            RESULT => {
                let ordinal = read.index()?;
                let target = read.operation()?;
                let stages = read.stages()?;
                let minimum = read.u8()?;
                let layer_count = read.count32(65)?;
                if layer_count > usize::from(query.placement_limit()) {
                    return Err(wire_error("spin_layer_count_invalid"));
                }
                let mut layers = Vec::with_capacity(layer_count);
                for _ in 0..layer_count {
                    layers.push(read.layer()?);
                }
                let regular = read.index()?;
                let mini = read.index()?;
                let total = regular
                    .checked_add(mini)
                    .ok_or_else(|| wire_error("spin_outcome_count_overflow"))?;
                if total > (bytes.len() - read.cursor) / 150 {
                    return Err(wire_error("spin_outcome_count_exceeds_packet"));
                }
                DecodeKind::Completion {
                    ordinal,
                    target,
                    regular,
                    mini,
                    report: Box::new(SpinStructureReport {
                        query: Some(query.clone()),
                        stages,
                        layers,
                        minimum_placements: (minimum != 0).then_some(minimum),
                        complete: true,
                        workers_used: 1,
                        ..SpinStructureReport::default()
                    }),
                }
            }
            _ => return Err(wire_error("spin_result_kind_invalid")),
        };
        Ok(Self {
            cursor: read.cursor,
            bytes: bytes.to_vec(),
            kind,
        })
    }
    pub(super) fn step(&mut self, query: &SpinStructureQuery) -> Result<bool, Error> {
        let mut read = Reader {
            bytes: &self.bytes,
            cursor: self.cursor,
        };
        let done = match &mut self.kind {
            DecodeKind::Orders(_) => true,
            DecodeKind::Manifest { count, targets } => {
                if targets.len() == *count {
                    true
                } else {
                    let operation = read.operation()?;
                    validate_operation(operation, query)?;
                    if query.mode.t_only() && operation.piece() != PieceKind::T
                        || targets.last().is_some_and(|last| *last >= operation)
                    {
                        return Err(wire_error("spin_manifest_order_invalid"));
                    }
                    targets.push(operation);
                    false
                }
            }
            DecodeKind::Completion {
                target,
                regular,
                mini,
                report,
                ..
            } => {
                if report.regular.len() == *regular && report.mini.len() == *mini {
                    true
                } else {
                    let outcome = read.outcome(query, *target)?;
                    if report.regular.len() < *regular {
                        if outcome.mini {
                            return Err(wire_error("spin_regular_kind_mismatch"));
                        }
                        report.regular.push(outcome);
                    } else {
                        if !outcome.mini {
                            return Err(wire_error("spin_mini_kind_mismatch"));
                        }
                        report.mini.push(outcome);
                    }
                    false
                }
            }
        };
        if done {
            read.end()?;
        }
        self.cursor = read.cursor;
        Ok(done)
    }
    pub(super) fn finish(self) -> Decoded {
        match self.kind {
            DecodeKind::Orders(completion) => Decoded::Orders(completion),
            DecodeKind::Manifest { targets, .. } => Decoded::Manifest(targets),
            DecodeKind::Completion {
                ordinal,
                target,
                report,
                ..
            } => Decoded::Completion(SpinStructureSourceCompletion {
                ordinal,
                target,
                report: *report,
            }),
        }
    }
}

fn header(magic: u32) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, magic);
    put_u32(&mut out, VERSION);
    out
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_board(out: &mut Vec<u8>, board: StructureBoard) {
    for word in board.words() {
        put_u64(out, word);
    }
}
fn put_query(out: &mut Vec<u8>, query: &SpinStructureQuery) {
    put_board(out, query.initial_board);
    out.push(query.height);
    out.extend(query.inventory.counts());
    out.push(
        SpinStructureMode::ALL
            .iter()
            .position(|mode| *mode == query.mode)
            .unwrap() as u8,
    );
    match query.line_requirement {
        SpinLineRequirement::Any => out.extend([0, 0]),
        SpinLineRequirement::Exact(n) => out.extend([1, n]),
        SpinLineRequirement::AtLeast(n) => out.extend([2, n]),
    }
    out.extend([
        query.fill_bottom,
        query.fill_top,
        match query.rule_profile {
            RuleProfileId::SrsPlus => 0,
            RuleProfileId::Srs => 1,
            RuleProfileId::SrsX => 2,
            RuleProfileId::Jstris180 => 3,
            RuleProfileId::NoKick => 4,
            _ => unreachable!("validated source rule"),
        },
        query.max_placements.unwrap_or(0),
        u8::from(query.minimality == MinimalityPolicy::MinimumPieceCount),
    ]);
}
fn put_operation(out: &mut Vec<u8>, op: StructureOperation) {
    out.extend([
        op.piece().as_ascii() as u8,
        op.rotation().quarter_turns(),
        op.x() as u8,
        op.y() as u8,
    ]);
    put_board(out, op.mask());
    put_u32(out, op.need_deleted_rows());
}
fn put_placement(out: &mut Vec<u8>, step: StructurePlacement) {
    out.extend([
        step.piece.as_ascii() as u8,
        step.rotation.quarter_turns(),
        step.x as u8,
        step.y as u8,
    ]);
    put_board(out, step.mask_before_clear);
    put_u32(out, step.cleared_rows);
    out.push(step.cleared_lines);
    let e = step.evidence;
    let (x, y) = e.predecessor();
    out.extend([
        u8::from(e.last_action_was_rotation()),
        e.from_rotation().quarter_turns(),
        match e.rotation_request() {
            RotationRequest::None => 0,
            RotationRequest::Clockwise => 1,
            RotationRequest::CounterClockwise => 2,
            RotationRequest::HalfTurn => 3,
        },
        e.kick_index(),
        e.kick_dx() as u8,
        e.kick_dy() as u8,
        x as u8,
        y as u8,
        u8::from(e.immobile_before_clear()),
    ]);
}
fn put_outcome(out: &mut Vec<u8>, outcome: &SpinStructureOutcome) {
    put_board(out, outcome.board_before_spin);
    put_board(out, outcome.final_board);
    put_placement(out, outcome.spin);
    out.push(u8::from(outcome.mini));
    put_operation(out, outcome.logical_spin);
    put_u32(out, outcome.logical_spin_cleared_rows);
    put_u32(out, outcome.logical_operations.len() as u32);
    for operation in &outcome.logical_operations {
        put_operation(out, *operation);
    }
    put_u32(out, outcome.build.len() as u32);
    for step in &outcome.build {
        put_placement(out, *step);
    }
}
fn put_stages(out: &mut Vec<u8>, s: SpinStructureStageMetrics) {
    for n in [
        s.build_states,
        s.fill_checks,
        s.support_locks,
        s.corner_checks,
        s.entry_states,
        s.verification_checks,
        s.exact_state_deduplications,
        s.exact_outcome_deduplications,
    ] {
        put_u64(out, n);
    }
}
fn put_layer(out: &mut Vec<u8>, s: LayerMetrics) {
    out.push(s.depth);
    for n in [
        s.input_states,
        s.piece_choices,
        s.reachable_locks,
        s.generated_states,
        s.exact_duplicates,
        s.terminal_candidates,
        s.accepted_regular,
        s.accepted_mini,
    ] {
        put_u64(out, n);
    }
}
fn validate_operation(op: StructureOperation, query: &SpinStructureQuery) -> Result<(), Error> {
    if query.inventory.count(op.piece()) == 0
        || op.mask().has_cells_at_or_above(query.height)
        || op.mask().intersects(query.initial_board)
        || op
            .mask()
            .words()
            .iter()
            .map(|word| word.count_ones())
            .sum::<u32>()
            != 4
        || op.need_deleted_rows() >> query.height != 0
    {
        Err(wire_error("spin_source_operation_invalid"))
    } else {
        Ok(())
    }
}
fn wire_error(reason: &'static str) -> Error {
    Error::InvalidWire(reason)
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let end = self
            .cursor
            .checked_add(N)
            .ok_or_else(|| wire_error("spin_wire_length_overflow"))?;
        let data = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| wire_error("spin_wire_truncated"))?;
        self.cursor = end;
        Ok(data.try_into().unwrap())
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take::<1>()?[0])
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take()?))
    }
    fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    fn index(&mut self) -> Result<usize, Error> {
        usize::try_from(self.u64()?).map_err(|_| wire_error("spin_wire_index_overflow"))
    }
    fn count(&mut self, min: usize) -> Result<usize, Error> {
        let n = self.index()?;
        if n > (self.bytes.len() - self.cursor) / min {
            Err(wire_error("spin_wire_count_exceeds_packet"))
        } else {
            Ok(n)
        }
    }
    fn count32(&mut self, min: usize) -> Result<usize, Error> {
        let n = self.u32()? as usize;
        if n > (self.bytes.len() - self.cursor) / min {
            Err(wire_error("spin_wire_count_exceeds_packet"))
        } else {
            Ok(n)
        }
    }
    fn boolean(&mut self) -> Result<bool, Error> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(wire_error("spin_wire_boolean_invalid")),
        }
    }
    fn rotation(&mut self) -> Result<RotationState, Error> {
        RotationState::from_quarter_turns(self.u8()?)
            .map_err(|_| wire_error("spin_wire_rotation_invalid"))
    }
    fn piece(&mut self) -> Result<PieceKind, Error> {
        PieceKind::from_ascii(char::from(self.u8()?))
            .map_err(|_| wire_error("spin_wire_piece_invalid"))
    }
    fn board(&mut self) -> Result<StructureBoard, Error> {
        Ok(StructureBoard::from_words([
            self.u64()?,
            self.u64()?,
            self.u64()?,
            self.u64()?,
        ]))
    }
    fn header(&mut self, magic: u32) -> Result<(), Error> {
        if self.u32()? != magic || self.u32()? != VERSION {
            Err(wire_error("spin_wire_header_invalid"))
        } else {
            Ok(())
        }
    }
    fn end(&self) -> Result<(), Error> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(wire_error("spin_wire_trailing_bytes"))
        }
    }
    fn require_query(&mut self, query: &SpinStructureQuery) -> Result<(), Error> {
        if &self.query()? != query {
            Err(wire_error("spin_source_query_mismatch"))
        } else {
            Ok(())
        }
    }
    fn order_task(&mut self, query: &SpinStructureQuery) -> Result<OrderTask, Error> {
        let mode = self.u8()?;
        let max_patterns = self.index()?;
        if max_patterns == 0 || max_patterns > u32::MAX as usize {
            return Err(wire_error("spin_order_pattern_limit_invalid"));
        }
        let mode = match mode {
            0 => SpinStructureParallelMode::Cover { max_patterns },
            1 => SpinStructureParallelMode::Guaranteed {
                final_piece: self.piece()?,
                max_patterns,
            },
            _ => return Err(wire_error("spin_order_mode_invalid")),
        };
        let candidate = self.index()?;
        let first = self.index()?;
        let count = usize::from(self.u8()?);
        if count == 0 || count > 64 {
            return Err(wire_error("spin_order_rank_count_invalid"));
        }
        let target = self.operation()?;
        validate_operation(target, query)?;
        let length = self.count(40)?;
        if length == 0 || length > usize::from(query.placement_limit()) {
            return Err(wire_error("spin_order_operation_count_invalid"));
        }
        let mut operations = Vec::with_capacity(length);
        for _ in 0..length {
            let operation = self.operation()?;
            validate_operation(operation, query)?;
            if operations.last().is_some_and(|last| *last >= operation) {
                return Err(wire_error("spin_order_operation_order_invalid"));
            }
            operations.push(operation);
        }
        if !operations.contains(&target) {
            return Err(wire_error("spin_order_target_missing"));
        }
        Ok(OrderTask {
            mode,
            candidate,
            first,
            count,
            target,
            operations,
        })
    }
    fn query(&mut self) -> Result<SpinStructureQuery, Error> {
        let initial_board = self.board()?;
        let height = self.u8()?;
        let inventory = PieceInventory::from_counts(self.take()?);
        let mode = *SpinStructureMode::ALL
            .get(self.u8()? as usize)
            .ok_or_else(|| wire_error("spin_mode_invalid"))?;
        let tag = self.u8()?;
        let n = self.u8()?;
        let line_requirement = match (tag, n) {
            (0, 0) => SpinLineRequirement::Any,
            (1, n) => SpinLineRequirement::Exact(n),
            (2, n) => SpinLineRequirement::AtLeast(n),
            _ => return Err(wire_error("spin_lines_invalid")),
        };
        let fill_bottom = self.u8()?;
        let fill_top = self.u8()?;
        let rule_profile = match self.u8()? {
            0 => RuleProfileId::SrsPlus,
            1 => RuleProfileId::Srs,
            2 => RuleProfileId::SrsX,
            3 => RuleProfileId::Jstris180,
            4 => RuleProfileId::NoKick,
            _ => return Err(wire_error("spin_rule_invalid")),
        };
        let maximum = self.u8()?;
        let minimality = if self.boolean()? {
            MinimalityPolicy::MinimumPieceCount
        } else {
            MinimalityPolicy::SubsetMinimal
        };
        let query = SpinStructureQuery {
            initial_board,
            height,
            inventory,
            mode,
            line_requirement,
            fill_bottom,
            fill_top,
            rule_profile,
            max_placements: (maximum != 0).then_some(maximum),
            minimality,
        };
        if SpinStructureSearcher::normalize_query(query.clone())? != query {
            return Err(wire_error("spin_source_not_normalized"));
        }
        Ok(query)
    }
    fn operation(&mut self) -> Result<StructureOperation, Error> {
        Ok(StructureOperation::new(
            self.piece()?,
            self.rotation()?,
            self.u8()? as i8,
            self.u8()? as i8,
            self.board()?,
            self.u32()?,
        ))
    }
    fn placement(&mut self) -> Result<StructurePlacement, Error> {
        let piece = self.piece()?;
        let rotation = self.rotation()?;
        let x = self.u8()? as i8;
        let y = self.u8()? as i8;
        let mask_before_clear = self.board()?;
        let cleared_rows = self.u32()?;
        let cleared_lines = self.u8()?;
        let rotated = self.boolean()?;
        let from = self.rotation()?;
        let request = match self.u8()? {
            0 => RotationRequest::None,
            1 => RotationRequest::Clockwise,
            2 => RotationRequest::CounterClockwise,
            3 => RotationRequest::HalfTurn,
            _ => return Err(wire_error("spin_rotation_request_invalid")),
        };
        let kick = self.u8()?;
        let dx = self.u8()? as i8;
        let dy = self.u8()? as i8;
        let px = self.u8()? as i8;
        let py = self.u8()? as i8;
        let immobile = self.boolean()?;
        let evidence = if rotated {
            if request == RotationRequest::None {
                return Err(wire_error("spin_rotation_evidence_invalid"));
            }
            ScoringLockEvidence::rotation(from, request, kick, dx, dy, px, py)
        } else {
            if request != RotationRequest::None
                || kick != 0
                || dx != 0
                || dy != 0
                || px != 0
                || py != 0
            {
                return Err(wire_error("spin_rotation_evidence_invalid"));
            }
            ScoringLockEvidence::no_rotation(from)
        }
        .with_immobile_before_clear(immobile);
        Ok(StructurePlacement {
            piece,
            rotation,
            x,
            y,
            mask_before_clear,
            cleared_rows,
            cleared_lines,
            evidence,
        })
    }
    fn outcome(
        &mut self,
        query: &SpinStructureQuery,
        target: StructureOperation,
    ) -> Result<SpinStructureOutcome, Error> {
        let board_before_spin = self.board()?;
        let final_board = self.board()?;
        let spin = self.placement()?;
        let mini = self.boolean()?;
        let logical_spin = self.operation()?;
        let logical_spin_cleared_rows = self.u32()?;
        if logical_spin != target
            || board_before_spin.has_cells_at_or_above(query.height)
            || final_board.has_cells_at_or_above(query.height)
        {
            return Err(wire_error("spin_outcome_source_mismatch"));
        }
        let count = self.count32(40)?;
        if count == 0 || count > usize::from(query.placement_limit()) {
            return Err(wire_error("spin_operation_count_invalid"));
        }
        let mut logical_operations = Vec::with_capacity(count);
        for _ in 0..count {
            let op = self.operation()?;
            validate_operation(op, query)?;
            logical_operations.push(op);
        }
        if !logical_operations.windows(2).all(|pair| pair[0] < pair[1])
            || logical_operations.binary_search(&target).is_err()
        {
            return Err(wire_error("spin_operation_order_invalid"));
        }
        let steps = self.count32(50)?;
        if steps != count {
            return Err(wire_error("spin_witness_length_mismatch"));
        }
        let mut build = Vec::with_capacity(steps);
        for _ in 0..steps {
            build.push(self.placement()?);
        }
        if build.last() != Some(&spin) {
            return Err(wire_error("spin_target_witness_mismatch"));
        }
        Ok(SpinStructureOutcome {
            board_before_spin,
            final_board,
            spin,
            build,
            mini,
            logical_operations,
            logical_spin,
            logical_spin_cleared_rows,
        })
    }
    fn stages(&mut self) -> Result<SpinStructureStageMetrics, Error> {
        Ok(SpinStructureStageMetrics {
            build_states: self.u64()?,
            fill_checks: self.u64()?,
            support_locks: self.u64()?,
            corner_checks: self.u64()?,
            entry_states: self.u64()?,
            verification_checks: self.u64()?,
            exact_state_deduplications: self.u64()?,
            exact_outcome_deduplications: self.u64()?,
        })
    }
    fn layer(&mut self) -> Result<LayerMetrics, Error> {
        Ok(LayerMetrics {
            depth: self.u8()?,
            input_states: self.u64()?,
            piece_choices: self.u64()?,
            reachable_locks: self.u64()?,
            generated_states: self.u64()?,
            exact_duplicates: self.u64()?,
            terminal_candidates: self.u64()?,
            accepted_regular: self.u64()?,
            accepted_mini: self.u64()?,
        })
    }
}
