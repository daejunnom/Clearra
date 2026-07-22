use clearra_core_domain::{board::standard_pc_board::Board256Mask, piece::piece_kind::PieceKind};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForwardPieceSource {
    FixedQueue(Vec<PieceKind>),
    Pattern(QueuePatternExpression),
}

impl ForwardPieceSource {
    pub fn fixed_queue(queue: Vec<PieceKind>) -> Self {
        Self::FixedQueue(queue)
    }

    pub fn pattern(expression: QueuePatternExpression) -> Self {
        Self::Pattern(expression)
    }

    pub const fn is_pattern(&self) -> bool {
        matches!(self, Self::Pattern(_))
    }

    pub fn sequence_len(&self) -> usize {
        match self {
            Self::FixedQueue(queue) => queue.len(),
            Self::Pattern(expression) => expression.sequence_len(),
        }
    }

    pub fn pattern_count(&self) -> usize {
        match self {
            Self::FixedQueue(_) => 1,
            Self::Pattern(expression) => expression.pattern_count(),
        }
    }

    pub fn sequence_at(&self, pattern_index: usize) -> Vec<PieceKind> {
        match self {
            Self::FixedQueue(queue) => {
                debug_assert_eq!(pattern_index, 0);
                queue.clone()
            }
            Self::Pattern(expression) => expression.sequence_at(pattern_index).into_owned(),
        }
    }

    pub fn fixed_sequence(&self) -> Option<&[PieceKind]> {
        match self {
            Self::FixedQueue(queue) => Some(queue),
            Self::Pattern(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardSpinCategory {
    Any,
    T,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForwardSpinTarget {
    lines: Option<u8>,
    category: ForwardSpinCategory,
}

impl ForwardSpinTarget {
    pub const fn new(lines: Option<u8>, category: ForwardSpinCategory) -> Self {
        Self { lines, category }
    }

    pub const fn lines(self) -> Option<u8> {
        self.lines
    }

    pub const fn category(self) -> ForwardSpinCategory {
        self.category
    }
}

impl Default for ForwardSpinTarget {
    fn default() -> Self {
        Self::new(None, ForwardSpinCategory::Any)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardSearchMode {
    MaximumDamage,
    DamageAtLeast(u32),
    SpinFinder(ForwardSpinTarget),
}

impl ForwardSearchMode {
    pub const fn is_damage(self) -> bool {
        matches!(self, Self::MaximumDamage | Self::DamageAtLeast(_))
    }

    pub const fn minimum_damage(self) -> Option<u32> {
        match self {
            Self::DamageAtLeast(damage) => Some(damage),
            Self::MaximumDamage | Self::SpinFinder(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForwardSearchQuery {
    board: Board256Mask,
    height: u8,
    piece_source: ForwardPieceSource,
    hold_enabled: bool,
    rule_profile: RuleProfileId,
    spin_profile: SpinProfileId,
    initial_combo: Option<u16>,
    initial_back_to_back: Option<u16>,
    mode: ForwardSearchMode,
}

impl ForwardSearchQuery {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        board: Board256Mask,
        height: u8,
        queue: Vec<PieceKind>,
        hold_enabled: bool,
        rule_profile: RuleProfileId,
        spin_profile: SpinProfileId,
        initial_combo: Option<u16>,
        initial_back_to_back: Option<u16>,
        mode: ForwardSearchMode,
    ) -> Self {
        Self::new_with_source(
            board,
            height,
            ForwardPieceSource::fixed_queue(queue),
            hold_enabled,
            rule_profile,
            spin_profile,
            initial_combo,
            initial_back_to_back,
            mode,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_source(
        board: Board256Mask,
        height: u8,
        piece_source: ForwardPieceSource,
        hold_enabled: bool,
        rule_profile: RuleProfileId,
        spin_profile: SpinProfileId,
        initial_combo: Option<u16>,
        initial_back_to_back: Option<u16>,
        mode: ForwardSearchMode,
    ) -> Self {
        Self {
            board,
            height,
            piece_source,
            hold_enabled,
            rule_profile,
            spin_profile,
            initial_combo,
            initial_back_to_back,
            mode,
        }
    }

    pub const fn board(&self) -> Board256Mask {
        self.board
    }

    pub const fn height(&self) -> u8 {
        self.height
    }

    pub const fn piece_source(&self) -> &ForwardPieceSource {
        &self.piece_source
    }

    pub const fn hold_enabled(&self) -> bool {
        self.hold_enabled
    }

    pub const fn rule_profile(&self) -> RuleProfileId {
        self.rule_profile
    }

    pub const fn spin_profile(&self) -> SpinProfileId {
        self.spin_profile
    }

    pub const fn initial_combo(&self) -> Option<u16> {
        self.initial_combo
    }

    pub const fn initial_back_to_back(&self) -> Option<u16> {
        self.initial_back_to_back
    }

    pub const fn mode(&self) -> ForwardSearchMode {
        self.mode
    }
}
