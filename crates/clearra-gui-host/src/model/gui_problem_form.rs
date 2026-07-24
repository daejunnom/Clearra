use clearra_app::GuiFormState;

use crate::model::GuiBackendForm;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiOpeningPcForm {
    lines: u8,
    rule: String,
    fixed_queue: Option<String>,
    queue_pattern: Option<String>,
    hold_enabled: bool,
    score_mode: String,
    score_profile: String,
    spin_profile: String,
    initial_b2b: u32,
    preserve_b2b: bool,
    solution_probabilities: bool,
}

impl GuiOpeningPcForm {
    pub fn new(lines: u8, rule: impl Into<String>) -> Self {
        Self {
            lines,
            rule: rule.into(),
            fixed_queue: None,
            queue_pattern: None,
            hold_enabled: true,
            score_mode: "off".to_owned(),
            score_profile: "tetrio".to_owned(),
            spin_profile: "t-spins".to_owned(),
            initial_b2b: 0,
            preserve_b2b: false,
            solution_probabilities: false,
        }
    }

    pub fn with_fixed_queue(mut self, queue: impl Into<String>, hold_enabled: bool) -> Self {
        self.fixed_queue = Some(queue.into());
        self.queue_pattern = None;
        self.hold_enabled = hold_enabled;
        self
    }

    pub fn with_queue_pattern(mut self, pattern: impl Into<String>, hold_enabled: bool) -> Self {
        self.fixed_queue = None;
        self.queue_pattern = Some(pattern.into());
        self.hold_enabled = hold_enabled;
        self
    }

    pub const fn with_hold_enabled(mut self, hold_enabled: bool) -> Self {
        self.hold_enabled = hold_enabled;
        self
    }
}
impl GuiOpeningPcForm {
    pub const fn lines(&self) -> u8 {
        self.lines
    }
}
impl GuiOpeningPcForm {
    pub fn rule(&self) -> &str {
        &self.rule
    }

    pub fn fixed_queue(&self) -> Option<&str> {
        self.fixed_queue.as_deref()
    }

    pub fn queue_pattern(&self) -> Option<&str> {
        self.queue_pattern.as_deref()
    }

    pub const fn hold_enabled(&self) -> bool {
        self.hold_enabled
    }

    pub fn score_mode(&self) -> &str {
        &self.score_mode
    }

    pub fn score_profile(&self) -> &str {
        &self.score_profile
    }

    pub fn spin_profile(&self) -> &str {
        &self.spin_profile
    }

    pub const fn initial_b2b(&self) -> u32 {
        self.initial_b2b
    }

    pub const fn preserve_b2b(&self) -> bool {
        self.preserve_b2b
    }

    pub fn with_score_input(mut self, mode: impl Into<String>, initial_b2b: u32) -> Self {
        self.score_mode = mode.into();
        self.initial_b2b = initial_b2b;
        self
    }

    pub fn with_score_profiles(
        mut self,
        score_profile: impl Into<String>,
        spin_profile: impl Into<String>,
    ) -> Self {
        self.score_profile = score_profile.into();
        self.spin_profile = spin_profile.into();
        self
    }

    pub const fn with_back_to_back_preservation(mut self, value: bool) -> Self {
        self.preserve_b2b = value;
        self
    }

    pub const fn solution_probabilities(&self) -> bool {
        self.solution_probabilities
    }

    pub fn with_solution_probabilities(mut self, value: bool) -> Self {
        self.solution_probabilities = value;
        self
    }
}

impl Default for GuiOpeningPcForm {
    fn default() -> Self {
        Self::new(2, "srs-plus")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiScenarioPcForm {
    visible_height: u8,
    initial_board_mask: u64,
    remaining_queue: String,
    remaining_queue_is_pattern: bool,
    remaining_queue_is_standard_bag: bool,
    rule: String,
    piece_window: usize,
    hold_piece: Option<char>,
    allow_hold: bool,
    count_policy: String,
    score_mode: String,
    score_profile: String,
    spin_profile: String,
    initial_b2b: u32,
    preserve_b2b: bool,
    solution_probabilities: bool,
}

impl GuiScenarioPcForm {
    pub fn new(
        visible_height: u8,
        initial_board_mask: u64,
        remaining_queue: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        let remaining_queue = remaining_queue.into();
        let piece_window = remaining_queue
            .chars()
            .filter(|character| !character.is_whitespace() && *character != ',')
            .count();
        Self {
            visible_height,
            initial_board_mask,
            remaining_queue,
            remaining_queue_is_pattern: false,
            remaining_queue_is_standard_bag: false,
            rule: rule.into(),
            piece_window,
            hold_piece: None,
            allow_hold: true,
            count_policy: "all".to_owned(),
            score_mode: "off".to_owned(),
            score_profile: "tetrio".to_owned(),
            spin_profile: "t-spins".to_owned(),
            initial_b2b: 0,
            preserve_b2b: false,
            solution_probabilities: false,
        }
    }

    pub fn with_execution_input(
        mut self,
        piece_window: usize,
        hold_piece: Option<char>,
        allow_hold: bool,
        count_policy: impl Into<String>,
    ) -> Self {
        self.piece_window = piece_window;
        self.hold_piece = hold_piece;
        self.allow_hold = allow_hold;
        self.count_policy = count_policy.into();
        self
    }

    pub fn with_queue_pattern(mut self) -> Self {
        self.remaining_queue_is_pattern = true;
        self
    }

    pub fn with_standard_7_bag(mut self) -> Self {
        self.remaining_queue.clear();
        self.remaining_queue_is_pattern = false;
        self.remaining_queue_is_standard_bag = true;
        self
    }
}
impl GuiScenarioPcForm {
    pub const fn visible_height(&self) -> u8 {
        self.visible_height
    }
}
impl GuiScenarioPcForm {
    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }
}
impl GuiScenarioPcForm {
    pub fn remaining_queue(&self) -> &str {
        &self.remaining_queue
    }

    pub const fn remaining_queue_is_pattern(&self) -> bool {
        self.remaining_queue_is_pattern
    }

    pub const fn remaining_queue_is_standard_bag(&self) -> bool {
        self.remaining_queue_is_standard_bag
    }
}
impl GuiScenarioPcForm {
    pub fn rule(&self) -> &str {
        &self.rule
    }

    pub const fn piece_window(&self) -> usize {
        self.piece_window
    }

    pub const fn hold_piece(&self) -> Option<char> {
        self.hold_piece
    }

    pub const fn allow_hold(&self) -> bool {
        self.allow_hold
    }

    pub fn count_policy(&self) -> &str {
        &self.count_policy
    }

    pub fn score_mode(&self) -> &str {
        &self.score_mode
    }

    pub fn score_profile(&self) -> &str {
        &self.score_profile
    }

    pub fn spin_profile(&self) -> &str {
        &self.spin_profile
    }

    pub const fn initial_b2b(&self) -> u32 {
        self.initial_b2b
    }

    pub const fn preserve_b2b(&self) -> bool {
        self.preserve_b2b
    }

    pub fn with_score_input(mut self, mode: impl Into<String>, initial_b2b: u32) -> Self {
        self.score_mode = mode.into();
        self.initial_b2b = initial_b2b;
        self
    }

    pub fn with_score_profiles(
        mut self,
        score_profile: impl Into<String>,
        spin_profile: impl Into<String>,
    ) -> Self {
        self.score_profile = score_profile.into();
        self.spin_profile = spin_profile.into();
        self
    }

    pub const fn with_back_to_back_preservation(mut self, value: bool) -> Self {
        self.preserve_b2b = value;
        self
    }

    pub const fn solution_probabilities(&self) -> bool {
        self.solution_probabilities
    }

    pub fn with_solution_probabilities(mut self, value: bool) -> Self {
        self.solution_probabilities = value;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiSetupSearchForm {
    queue: String,
    fixed_queue: bool,
    rule: String,
}

impl GuiSetupSearchForm {
    pub fn new(queue: impl Into<String>, fixed_queue: bool, rule: impl Into<String>) -> Self {
        Self {
            queue: queue.into(),
            fixed_queue,
            rule: rule.into(),
        }
    }
}
impl GuiSetupSearchForm {
    pub fn queue(&self) -> &str {
        &self.queue
    }
}
impl GuiSetupSearchForm {
    pub const fn fixed_queue(&self) -> bool {
        self.fixed_queue
    }
}
impl GuiSetupSearchForm {
    pub fn rule(&self) -> &str {
        &self.rule
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiBuildCoverageForm {
    template_id: String,
    rule: String,
}

impl GuiBuildCoverageForm {
    pub fn new(template_id: impl Into<String>, rule: impl Into<String>) -> Self {
        Self {
            template_id: template_id.into(),
            rule: rule.into(),
        }
    }
}
impl GuiBuildCoverageForm {
    pub fn template_id(&self) -> &str {
        &self.template_id
    }
}
impl GuiBuildCoverageForm {
    pub fn rule(&self) -> &str {
        &self.rule
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GuiProblemForm {
    OpeningPc(GuiOpeningPcForm),
    ScenarioPc(GuiScenarioPcForm),
    SetupSearch(GuiSetupSearchForm),
    BuildCoverage(GuiBuildCoverageForm),
}

impl GuiProblemForm {
    pub fn with_score_input(self, mode: impl Into<String>, initial_b2b: u32) -> Self {
        let mode = mode.into();
        match self {
            Self::OpeningPc(form) => Self::OpeningPc(form.with_score_input(mode, initial_b2b)),
            Self::ScenarioPc(form) => Self::ScenarioPc(form.with_score_input(mode, initial_b2b)),
            other => other,
        }
    }
}
impl GuiProblemForm {
    pub fn with_score_profiles(
        self,
        score_profile: impl Into<String>,
        spin_profile: impl Into<String>,
    ) -> Self {
        let score_profile = score_profile.into();
        let spin_profile = spin_profile.into();
        match self {
            Self::OpeningPc(form) => {
                Self::OpeningPc(form.with_score_profiles(score_profile, spin_profile))
            }
            Self::ScenarioPc(form) => {
                Self::ScenarioPc(form.with_score_profiles(score_profile, spin_profile))
            }
            other => other,
        }
    }
}
impl GuiProblemForm {
    pub fn with_back_to_back_preservation(self, value: bool) -> Self {
        match self {
            Self::OpeningPc(form) => Self::OpeningPc(form.with_back_to_back_preservation(value)),
            Self::ScenarioPc(form) => Self::ScenarioPc(form.with_back_to_back_preservation(value)),
            other => other,
        }
    }
}
impl GuiProblemForm {
    pub fn with_solution_probabilities(self, value: bool) -> Self {
        match self {
            Self::OpeningPc(form) => Self::OpeningPc(form.with_solution_probabilities(value)),
            Self::ScenarioPc(form) => Self::ScenarioPc(form.with_solution_probabilities(value)),
            other => other,
        }
    }
}
impl GuiProblemForm {
    pub fn opening_pc(lines: u8, rule: impl Into<String>) -> Self {
        Self::OpeningPc(GuiOpeningPcForm::new(lines, rule))
    }

    pub fn opening_pc_fixed_queue(
        lines: u8,
        rule: impl Into<String>,
        queue: impl Into<String>,
        hold_enabled: bool,
    ) -> Self {
        Self::OpeningPc(GuiOpeningPcForm::new(lines, rule).with_fixed_queue(queue, hold_enabled))
    }

    pub fn opening_pc_queue_pattern(
        lines: u8,
        rule: impl Into<String>,
        pattern: impl Into<String>,
        hold_enabled: bool,
    ) -> Self {
        Self::OpeningPc(
            GuiOpeningPcForm::new(lines, rule).with_queue_pattern(pattern, hold_enabled),
        )
    }
}
impl GuiProblemForm {
    pub fn scenario_pc(
        visible_height: u8,
        initial_board_mask: u64,
        remaining_queue: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self::ScenarioPc(GuiScenarioPcForm::new(
            visible_height,
            initial_board_mask,
            remaining_queue,
            rule,
        ))
    }

    pub fn scenario_pc_with_execution_input(
        visible_height: u8,
        initial_board_mask: u64,
        remaining_queue: impl Into<String>,
        rule: impl Into<String>,
        piece_window: usize,
        hold_piece: Option<char>,
        allow_hold: bool,
        count_policy: impl Into<String>,
    ) -> Self {
        Self::ScenarioPc(
            GuiScenarioPcForm::new(visible_height, initial_board_mask, remaining_queue, rule)
                .with_execution_input(piece_window, hold_piece, allow_hold, count_policy),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scenario_pc_standard_bag_with_execution_input(
        visible_height: u8,
        initial_board_mask: u64,
        rule: impl Into<String>,
        piece_window: usize,
        hold_piece: Option<char>,
        allow_hold: bool,
        count_policy: impl Into<String>,
    ) -> Self {
        Self::ScenarioPc(
            GuiScenarioPcForm::new(visible_height, initial_board_mask, "", rule)
                .with_standard_7_bag()
                .with_execution_input(piece_window, hold_piece, allow_hold, count_policy),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scenario_pc_pattern_with_execution_input(
        visible_height: u8,
        initial_board_mask: u64,
        pattern: impl Into<String>,
        rule: impl Into<String>,
        piece_window: usize,
        hold_piece: Option<char>,
        allow_hold: bool,
        count_policy: impl Into<String>,
    ) -> Self {
        Self::ScenarioPc(
            GuiScenarioPcForm::new(visible_height, initial_board_mask, pattern, rule)
                .with_queue_pattern()
                .with_execution_input(piece_window, hold_piece, allow_hold, count_policy),
        )
    }
}
impl GuiProblemForm {
    pub fn setup_search(
        queue: impl Into<String>,
        fixed_queue: bool,
        rule: impl Into<String>,
    ) -> Self {
        Self::SetupSearch(GuiSetupSearchForm::new(queue, fixed_queue, rule))
    }
}
impl GuiProblemForm {
    pub fn build_coverage(template_id: impl Into<String>, rule: impl Into<String>) -> Self {
        Self::BuildCoverage(GuiBuildCoverageForm::new(template_id, rule))
    }
}
impl GuiProblemForm {
    pub const fn preset_id(&self) -> &'static str {
        match self {
            Self::OpeningPc(_) => "opening-pc",
            Self::ScenarioPc(_) => "scenario-pc",
            Self::SetupSearch(_) => "setup-search",
            Self::BuildCoverage(_) => "build-coverage",
        }
    }
}
impl GuiProblemForm {
    pub fn selected_lines(&self) -> u8 {
        match self {
            Self::OpeningPc(form) => form.lines(),
            Self::ScenarioPc(form) => form.visible_height(),
            Self::SetupSearch(_) | Self::BuildCoverage(_) => 2,
        }
    }
}
impl GuiProblemForm {
    pub fn selected_rule(&self) -> &str {
        match self {
            Self::OpeningPc(form) => form.rule(),
            Self::ScenarioPc(form) => form.rule(),
            Self::SetupSearch(form) => form.rule(),
            Self::BuildCoverage(form) => form.rule(),
        }
    }
}
impl GuiProblemForm {
    pub fn to_app_bridge_form(
        &self,
        selected_language: &str,
        backend_form: &GuiBackendForm,
    ) -> GuiFormState {
        GuiFormState::new(
            selected_language,
            backend_form.backend_id(),
            self.preset_id(),
            self.selected_lines(),
            self.selected_rule(),
        )
    }
}

impl Default for GuiProblemForm {
    fn default() -> Self {
        Self::OpeningPc(GuiOpeningPcForm::default())
    }
}
