use clearra_app::{PcChanceIngressOrigin, PcSaveIngressOrigin, PcScoreIngressOrigin};
use clearra_fumen::{SourceFumenBoard, SourceFumenColoredFieldSet};
use clearra_pc_graph::request::PcScenarioBoard;

use crate::{
    ctk3_mask_input::{parse_ctk3_board_mask, parse_ctk3_field_mask, Ctk3FieldMask},
    WebCommandError, WebCommandErrorCode, WebCompatibilityAuthority,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PcPreset {
    Path,
    Chance,
    Minimals,
    Score,
    ScoreMinimals,
    Saves,
    BestSave,
}

#[derive(Debug)]
enum CompatibilityFieldInput {
    Fumen(String),
    OccupiedMask(String),
    BoardMask(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompatibilityRule {
    id: &'static str,
    requested: bool,
}

impl CompatibilityRule {
    const DEFAULT_ID: &'static str = "srs-plus";

    fn extract(args: &[String]) -> Result<(Vec<String>, Self), WebCommandError> {
        let mut remaining = Vec::with_capacity(args.len());
        let mut selected = None;
        let mut cursor = 0usize;
        while cursor < args.len() {
            if args[cursor] != "--rule" {
                remaining.push(args[cursor].clone());
                cursor += 1;
                continue;
            }
            if selected.is_some() {
                return Err(invalid("--rule may be specified only once"));
            }
            let value = next(args, &mut cursor, "--rule")?;
            selected = Some(Self::parse(value)?);
        }
        Ok((
            remaining,
            Self {
                id: selected.unwrap_or(Self::DEFAULT_ID),
                requested: selected.is_some(),
            },
        ))
    }

    fn parse(value: &str) -> Result<&'static str, WebCommandError> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "srs-plus" => Ok("srs-plus"),
            "srs" => Ok("srs"),
            "srs-x" => Ok("srs-x"),
            "jstris-180" => Ok("jstris-180"),
            _ => Err(invalid(format!(
                "unsupported Sfinder compatibility rule '{value}'; expected srs-plus, srs, srs-x, or jstris-180"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompatibilityBoard {
    occupied_mask: u64,
    grey_mask: u64,
    colored_mask: u64,
    visible_height: u8,
}

impl CompatibilityBoard {
    fn from_fumen(source: &str) -> Result<Self, WebCommandError> {
        let board = SourceFumenBoard::decode(source).map_err(|error| {
            invalid(format!(
                "invalid single-page field Fumen for Sfinder compatibility: {error:?}"
            ))
        })?;
        Ok(Self {
            occupied_mask: board.occupied_mask(),
            grey_mask: board.grey_mask(),
            colored_mask: board.colored_mask(),
            visible_height: board.visible_height(),
        })
    }

    fn from_occupied_mask(mask: Ctk3FieldMask) -> Self {
        let occupied_mask = mask.occupied_mask();
        Self {
            occupied_mask,
            grey_mask: 0,
            colored_mask: occupied_mask,
            visible_height: mask.visible_height(),
        }
    }

    const fn occupied_mask(self) -> u64 {
        self.occupied_mask
    }

    const fn grey_mask(self) -> u64 {
        self.grey_mask
    }

    const fn colored_mask(self) -> u64 {
        self.colored_mask
    }

    const fn visible_height(self) -> u8 {
        self.visible_height
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ColoredTargetBoard {
    base_mask: String,
    target_mask: String,
    target_count: u32,
    visible_height: u8,
}

pub(crate) fn translate_command(tokens: &[String]) -> Result<Vec<String>, WebCommandError> {
    translate_command_with_origin(tokens, WebCompatibilityAuthority::PublicLegacyCompatibility)
        .map(TranslatedWebCommand::into_tokens)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranslatedWebCommand {
    tokens: Vec<String>,
    pc_chance_origin: Option<PcChanceIngressOrigin>,
    pc_score_origin: Option<PcScoreIngressOrigin>,
    pc_save_origin: Option<PcSaveIngressOrigin>,
}

impl TranslatedWebCommand {
    fn unchanged(tokens: &[String]) -> Self {
        Self {
            tokens: tokens.to_vec(),
            pc_chance_origin: None,
            pc_score_origin: None,
            pc_save_origin: None,
        }
    }

    pub(crate) fn tokens(&self) -> &[String] {
        &self.tokens
    }

    pub(crate) const fn pc_chance_origin(&self) -> Option<PcChanceIngressOrigin> {
        self.pc_chance_origin
    }

    pub(crate) const fn pc_score_origin(&self) -> Option<PcScoreIngressOrigin> {
        self.pc_score_origin
    }

    pub(crate) const fn pc_save_origin(&self) -> Option<PcSaveIngressOrigin> {
        self.pc_save_origin
    }

    fn into_tokens(self) -> Vec<String> {
        self.tokens
    }
}

pub(crate) fn translate_command_with_origin(
    tokens: &[String],
    compatibility_authority: WebCompatibilityAuthority,
) -> Result<TranslatedWebCommand, WebCommandError> {
    crate::web_command_parser::validate_pretranslation_pc_score_tokens(
        tokens,
        compatibility_authority,
    )?;
    let mut command_index = usize::from(tokens.first().map(String::as_str) == Some("clearra"));
    let Some(command) = tokens.get(command_index) else {
        return Ok(TranslatedWebCommand::unchanged(tokens));
    };
    let mut command = normalized_command(command);
    command_index += 1;

    let mut sfinder_namespace = false;
    if command == "sfinder" {
        sfinder_namespace = true;
        let Some(subcommand) = tokens.get(command_index) else {
            return Err(WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "sfinder requires a subcommand",
            ));
        };
        command = normalized_command(subcommand);
        command_index += 1;
    } else if !is_compatibility_command(&command) {
        return Ok(TranslatedWebCommand::unchanged(tokens));
    }

    let (args, worker_options) = CompatibilityWorkerOptions::extract(&tokens[command_index..])?;
    let (args, rule) = CompatibilityRule::extract(&args)?;
    if command == "verify" && worker_options.requested() {
        return Err(invalid("verify does not run a search worker pool"));
    }
    if command == "verify" && rule.requested {
        return Err(invalid("verify does not accept --rule"));
    }
    let mut output = match command.as_str() {
        "path" => translate_pc(PcPreset::Path, &args, rule.id, compatibility_authority),
        "chance" | "percent" => {
            translate_pc(PcPreset::Chance, &args, rule.id, compatibility_authority)
        }
        "minimals" => translate_pc(PcPreset::Minimals, &args, rule.id, compatibility_authority),
        "score" => translate_pc(PcPreset::Score, &args, rule.id, compatibility_authority),
        "score-minimals" => translate_pc(
            PcPreset::ScoreMinimals,
            &args,
            rule.id,
            compatibility_authority,
        ),
        "saves" => translate_pc(PcPreset::Saves, &args, rule.id, compatibility_authority),
        "best-save" => translate_pc(PcPreset::BestSave, &args, rule.id, compatibility_authority),
        "congruent" | "congruent-cover" | "setup-cover" | "cover-percent" => {
            translate_colored_target(&command, &args, false, rule.id)
        }
        "special-cover" => translate_colored_target(&command, &args, true, rule.id),
        // solution-finder `spin`/sfinder-man `spincover` search an unordered
        // structural inventory and then apply structural cover/min-cover
        // projections.  Lowering them to Clearra's ordered forward
        // `spin-finder` changes both the problem and the result contract, so
        // keep the compatibility boundary fail closed until the structural
        // variants are implemented explicitly.
        "spin-cover" | "spin" => Err(not_yet_representable("spin/spincover structural search")),
        "score-finder" => translate_score_finder(&args, rule.id),
        "pc-setup" | "best-setup" | "dpc-finder" => {
            translate_setup_finder(&command, &args, rule.id)
        }
        "verify" => translate_verify(&args),
        "cover" => translate_cover(&args, rule.id),
        "setup" => translate_colored_target(&command, &args, false, rule.id),
        "ren" | "util" | "parity" | "to-gray" | "to-fumen" | "render" | "special-minimals" => {
            Err(not_yet_representable(&command))
        }
        _ => Err(WebCommandError::new(
            WebCommandErrorCode::UnsupportedCommand,
            format!("unsupported Sfinder compatibility command '{command}'"),
        )),
    }?;
    worker_options.append_to(&mut output);
    let pc_chance_origin = match (compatibility_authority, command.as_str()) {
        (WebCompatibilityAuthority::InternalTypedCandidate, "chance") => {
            Some(PcChanceIngressOrigin::CompatibilityChance)
        }
        (WebCompatibilityAuthority::InternalTypedCandidate, "percent") if sfinder_namespace => {
            Some(PcChanceIngressOrigin::CompatibilityPercent)
        }
        _ => None,
    };
    let pc_score_origin = (compatibility_authority
        == WebCompatibilityAuthority::InternalTypedCandidate
        && command == "score")
        .then_some(PcScoreIngressOrigin::CompatibilityScore);
    let pc_save_origin = match command.as_str() {
        "saves" => Some(PcSaveIngressOrigin::CompatibilitySaves),
        "best-save" => Some(PcSaveIngressOrigin::CompatibilityBestSave),
        _ => None,
    };
    Ok(TranslatedWebCommand {
        tokens: output,
        pc_chance_origin,
        pc_score_origin,
        pc_save_origin,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CompatibilityWorkerSelection {
    Fixed(String),
    Automatic(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CompatibilityWorkerOptions {
    selection: Option<CompatibilityWorkerSelection>,
    use_all_logical_processors: bool,
}

impl CompatibilityWorkerOptions {
    fn extract(args: &[String]) -> Result<(Vec<String>, Self), WebCommandError> {
        let mut remaining = Vec::with_capacity(args.len());
        let mut options = Self::default();
        let mut cursor = 0usize;
        while cursor < args.len() {
            match args[cursor].as_str() {
                "--workers" | "--cpu-threads" => {
                    let option = args[cursor].clone();
                    let value = next(args, &mut cursor, &option)?.to_owned();
                    options.set_selection(
                        CompatibilityWorkerSelection::Fixed(value),
                        option.as_str(),
                    )?;
                }
                "--auto-workers" => {
                    let value = next(args, &mut cursor, "--auto-workers")?.to_owned();
                    options.set_selection(
                        CompatibilityWorkerSelection::Automatic(value),
                        "--auto-workers",
                    )?;
                }
                "--use-all-cpu-threads" => {
                    if options.use_all_logical_processors {
                        return Err(invalid("--use-all-cpu-threads may be specified only once"));
                    }
                    options.use_all_logical_processors = true;
                    cursor += 1;
                }
                _ => {
                    remaining.push(args[cursor].clone());
                    cursor += 1;
                }
            }
        }
        Ok((remaining, options))
    }

    fn set_selection(
        &mut self,
        selection: CompatibilityWorkerSelection,
        option: &str,
    ) -> Result<(), WebCommandError> {
        if self.selection.is_some() {
            return Err(invalid(format!(
                "{option} conflicts with an earlier worker selection"
            )));
        }
        self.selection = Some(selection);
        Ok(())
    }

    fn requested(&self) -> bool {
        self.selection.is_some() || self.use_all_logical_processors
    }

    fn append_to(self, output: &mut Vec<String>) {
        match self.selection {
            Some(CompatibilityWorkerSelection::Fixed(workers)) => {
                push_pair_owned(output, "--workers", workers);
            }
            Some(CompatibilityWorkerSelection::Automatic(workers)) => {
                push_pair_owned(output, "--auto-workers", workers);
            }
            None => {}
        }
        if self.use_all_logical_processors {
            output.push("--use-all-cpu-threads".to_owned());
        }
    }
}

fn normalized_command(command: &str) -> String {
    let normalized = command.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "bestsave" => "best-save".to_owned(),
        "bestsetup" => "best-setup".to_owned(),
        "congruentcover" => "congruent-cover".to_owned(),
        "coverpercent" => "cover-percent".to_owned(),
        "dpcfinder" => "dpc-finder".to_owned(),
        "pcsetup" => "pc-setup".to_owned(),
        "scoreminimals" => "score-minimals".to_owned(),
        "setupcover" => "setup-cover".to_owned(),
        "specialcover" => "special-cover".to_owned(),
        "specialminimals" => "special-minimals".to_owned(),
        "spincover" => "spin-cover".to_owned(),
        "tofumen" => "to-fumen".to_owned(),
        "togray" => "to-gray".to_owned(),
        _ => normalized,
    }
}

fn is_compatibility_command(command: &str) -> bool {
    matches!(
        command,
        "chance"
            | "minimals"
            | "special-minimals"
            | "special-cover"
            | "score"
            | "score-minimals"
            | "congruent"
            | "congruent-cover"
            | "cover-percent"
            | "saves"
            | "best-save"
            | "score-finder"
            | "spin-cover"
            | "setup-cover"
            | "pc-setup"
            | "best-setup"
            | "dpc-finder"
            | "parity"
            | "to-gray"
            | "to-fumen"
            | "render"
    )
}

#[derive(Debug)]
struct LegacyPcInput {
    field: CompatibilityFieldInput,
    piece_source: LegacyPcPieceSource,
    clear: u8,
    hold: bool,
}

#[derive(Debug)]
enum LegacyPcPieceSource {
    ExactQueue(String),
    Pattern(String),
}

impl LegacyPcPieceSource {
    const fn option(&self) -> &'static str {
        match self {
            Self::ExactQueue(_) => "--queue",
            Self::Pattern(_) => "--patterns",
        }
    }

    fn source(&self) -> &str {
        match self {
            Self::ExactQueue(source) | Self::Pattern(source) => source,
        }
    }
}

#[derive(Debug)]
struct ScoreFinderInput {
    field: CompatibilityFieldInput,
    queue: String,
    clear: u8,
    initial_b2b: bool,
    initial_combo: u32,
    b2b_end_bonus: i64,
}

fn translate_pc(
    preset: PcPreset,
    args: &[String],
    rule: &str,
    compatibility_authority: WebCompatibilityAuthority,
) -> Result<Vec<String>, WebCommandError> {
    let input = parse_legacy_pc_input(args)?;
    if matches!(preset, PcPreset::Saves | PcPreset::BestSave)
        && matches!(&input.piece_source, LegacyPcPieceSource::ExactQueue(_))
    {
        return Err(invalid(
            "saves and best-save require fixed bag-boundary provenance; --queue is not authoritative",
        ));
    }
    let board = decode_board(&input.field)?;
    let target = target_mask(input.clear)?;
    let normalized_board_mask = normalized_pc_board_mask(board);
    if normalized_board_mask & !target != 0 {
        return Err(invalid(format!(
            "the input field has cells above the requested {} clear lines",
            input.clear
        )));
    }
    let empty = target & !normalized_board_mask;
    if empty.count_ones() % 4 != 0 {
        return Err(invalid(
            "the target field does not contain a whole number of tetrominoes",
        ));
    }

    let piece_source = normalize_sfinder_pattern(input.piece_source.source())?;
    let mut output = vec![
        "clearra".to_owned(),
        "pc".to_owned(),
        "--lines".to_owned(),
        input.clear.to_string(),
        "--board-mask".to_owned(),
        format!("0x{normalized_board_mask:x}"),
        "--height".to_owned(),
        input.clear.to_string(),
        "--pieces".to_owned(),
        (empty.count_ones() / 4).to_string(),
        input.piece_source.option().to_owned(),
        piece_source,
        "--rule".to_owned(),
        rule.to_owned(),
    ];
    if preset == PcPreset::Chance
        && compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
    {
        output.insert(2, "chance".to_owned());
    }
    if preset == PcPreset::Score
        && compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
    {
        output.insert(2, "score".to_owned());
    }
    if preset == PcPreset::Minimals
        && compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
    {
        output.insert(2, "minimals".to_owned());
    }
    if preset == PcPreset::Path
        && compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
    {
        output.insert(2, "path".to_owned());
    }
    match preset {
        PcPreset::Saves => output.insert(2, "saves".to_owned()),
        PcPreset::BestSave => output.insert(2, "best-save".to_owned()),
        _ => {}
    }
    if !input.hold {
        output.push("--no-hold".to_owned());
    }
    match preset {
        PcPreset::Path => {
            if compatibility_authority == WebCompatibilityAuthority::PublicLegacyCompatibility {
                push_pair(&mut output, "--objective", "all");
            }
        }
        PcPreset::Chance => push_pair(&mut output, "--objective", "unique"),
        PcPreset::Minimals => {
            if compatibility_authority == WebCompatibilityAuthority::PublicLegacyCompatibility {
                push_pair(&mut output, "--objective", "minimum-cover");
            }
        }
        PcPreset::Score => {
            if compatibility_authority == WebCompatibilityAuthority::PublicLegacyCompatibility {
                push_pair(&mut output, "--objective", "all");
                output.push("--score".to_owned());
                push_pair(&mut output, "--score-profile", "jstris-ultra");
            }
        }
        PcPreset::ScoreMinimals => {
            push_pair(&mut output, "--objective", "minimum-cover");
            output.push("--score".to_owned());
            push_pair(&mut output, "--score-profile", "jstris-ultra");
        }
        PcPreset::Saves | PcPreset::BestSave => {}
    }
    Ok(output)
}

fn parse_legacy_pc_input(args: &[String]) -> Result<LegacyPcInput, WebCommandError> {
    let mut field = None;
    let mut piece_source = None;
    let mut clear = 4u8;
    let mut hold = true;
    let mut positional = Vec::new();
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-t" | "--tetfu" | "--fumen" => {
                let value = next(args, &mut cursor, "--fumen")?.to_owned();
                set_field_input(&mut field, CompatibilityFieldInput::Fumen(value), "--fumen")?;
            }
            "--field-mask-v1" => {
                let value = next(args, &mut cursor, "--field-mask-v1")?.to_owned();
                set_field_input(
                    &mut field,
                    CompatibilityFieldInput::OccupiedMask(value),
                    "--field-mask-v1",
                )?;
            }
            "-p" | "--pattern" | "--patterns" => {
                let value = next(args, &mut cursor, "--patterns")?.to_owned();
                set_legacy_pc_piece_source(
                    &mut piece_source,
                    LegacyPcPieceSource::Pattern(value),
                    "--patterns",
                )?;
            }
            "--queue" => {
                let value = next(args, &mut cursor, "--queue")?.to_owned();
                set_legacy_pc_piece_source(
                    &mut piece_source,
                    LegacyPcPieceSource::ExactQueue(value),
                    "--queue",
                )?;
            }
            "-c" | "--clear" | "--lines" => {
                clear = parse_lines(next(args, &mut cursor, "--lines")?)?;
            }
            "-H" | "--hold" => {
                let value = next(args, &mut cursor, "--hold")?;
                hold = parse_bool(value, "--hold")?;
            }
            "--no-hold" => {
                hold = false;
                cursor += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(invalid(format!(
                    "unsupported Sfinder compatibility option '{flag}'"
                )));
            }
            value => {
                positional.push(value.to_owned());
                cursor += 1;
            }
        }
    }

    let mut positional_index = 0usize;
    if field.is_none() {
        if let Some(value) = positional.get(positional_index) {
            field = Some(CompatibilityFieldInput::Fumen(value.clone()));
            positional_index += 1;
        }
    }
    if piece_source.is_none() {
        piece_source = positional
            .get(positional_index)
            .cloned()
            .map(LegacyPcPieceSource::Pattern);
        positional_index += usize::from(piece_source.is_some());
    }
    if let Some(value) = positional.get(positional_index) {
        clear = parse_lines(value)?;
        positional_index += 1;
    }
    if positional.len() > positional_index {
        return Err(invalid(
            "this compatibility command received unsupported legacy parameters",
        ));
    }

    Ok(LegacyPcInput {
        field: field.ok_or_else(|| {
            missing("Sfinder compatibility search requires a Fumen or CTK3 field")
        })?,
        piece_source: piece_source
            .ok_or_else(|| missing("Sfinder compatibility search requires a queue pattern"))?,
        clear,
        hold,
    })
}

fn translate_colored_target(
    command: &str,
    args: &[String],
    spin: bool,
    rule: &str,
) -> Result<Vec<String>, WebCommandError> {
    let (field, pattern, trailing) = first_field_and_pattern(args, command)?;
    if !trailing.is_empty() {
        return Err(invalid(format!(
            "{command} received unsupported legacy parameters"
        )));
    }
    let board = colored_target_board(&field)?;
    if board.target_count == 0 {
        return Err(invalid(format!(
            "{command} requires at least one target cell"
        )));
    }
    if board.target_count % 4 != 0 {
        return Err(invalid("colored target cells must have tetromino area"));
    }
    let mut output = vec![
        "clearra".to_owned(),
        "build-probability".to_owned(),
        "--base-mask".to_owned(),
        board.base_mask,
        "--target-mask".to_owned(),
        board.target_mask,
        "--height".to_owned(),
        board.visible_height.max(1).to_string(),
        "--patterns".to_owned(),
        normalize_sfinder_pattern(&pattern)?,
        "--rule".to_owned(),
        rule.to_owned(),
        "--no-mirror".to_owned(),
    ];
    if spin {
        push_pair(&mut output, "--aggregate", "spin");
        push_pair(&mut output, "--spin-profile", "t-spins");
    }
    Ok(output)
}

fn translate_cover(args: &[String], rule: &str) -> Result<Vec<String>, WebCommandError> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--base-mask-v1" | "--target-mask-v1"))
    {
        return translate_build_probability_cover(args, rule);
    }
    translate_legacy_solution_cover(args, rule)
}

fn translate_build_probability_cover(
    args: &[String],
    rule: &str,
) -> Result<Vec<String>, WebCommandError> {
    let mut base = None;
    let mut target = None;
    let mut pattern = None;
    let mut hold = true;
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "--base-mask-v1" => {
                let value = next(args, &mut cursor, "--base-mask-v1")?.to_owned();
                set_unique_value(&mut base, value, "--base-mask-v1")?;
            }
            "--target-mask-v1" => {
                let value = next(args, &mut cursor, "--target-mask-v1")?.to_owned();
                set_unique_value(&mut target, value, "--target-mask-v1")?;
            }
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                let value = next(args, &mut cursor, "--patterns")?.to_owned();
                set_unique_value(&mut pattern, value, "--patterns")?;
            }
            "-H" | "--hold" => {
                hold = parse_bool(next(args, &mut cursor, "--hold")?, "--hold")?;
            }
            "--no-hold" => {
                hold = false;
                cursor += 1;
            }
            "-c" | "--clear" | "--lines" => {
                return Err(invalid(
                    "two-field cover derives its height from base and target; clear/lines is unavailable",
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(invalid(format!(
                    "unsupported two-field cover compatibility option '{flag}'"
                )));
            }
            value => {
                return Err(invalid(format!(
                    "unexpected two-field cover token '{value}'"
                )));
            }
        }
    }

    let base = parse_ctk3_board_mask(
        &base.ok_or_else(|| missing("cover requires a base field"))?,
        "--base-mask-v1",
    )?;
    let target = parse_ctk3_board_mask(
        &target.ok_or_else(|| missing("cover requires a target field"))?,
        "--target-mask-v1",
    )?;
    let pattern = pattern.ok_or_else(|| missing("cover requires a queue pattern"))?;
    if target.is_empty() {
        return Err(invalid(
            "cover target must contain at least one occupied cell",
        ));
    }
    if target.count_ones() % 4 != 0 {
        return Err(invalid(
            "cover target occupied-cell count must be divisible by four",
        ));
    }
    if base.intersects(target) {
        return Err(invalid(
            "cover base and target must not overlap; target contains only cells to add",
        ));
    }
    let height = base.visible_height().max(target.visible_height()).max(1);
    if base.contains_completed_row(height) {
        return Err(invalid(
            "cover base must not contain an already completed row",
        ));
    }

    let mut output = vec![
        "clearra".to_owned(),
        "build-probability".to_owned(),
        "--base-mask".to_owned(),
        base.cli_hex(),
        "--target-mask".to_owned(),
        target.cli_hex(),
        "--height".to_owned(),
        height.to_string(),
        "--patterns".to_owned(),
        normalize_sfinder_pattern(&pattern)?,
        "--rule".to_owned(),
        rule.to_owned(),
        "--no-mirror".to_owned(),
    ];
    if !hold {
        output.push("--no-hold".to_owned());
    }
    Ok(output)
}

fn translate_legacy_solution_cover(
    args: &[String],
    rule: &str,
) -> Result<Vec<String>, WebCommandError> {
    let mut fumen = None;
    let mut pattern = None;
    let mut clear = 4u8;
    let mut hold = true;
    let mut positional = Vec::new();
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-t" | "--tetfu" | "--fumen" => {
                let value = next(args, &mut cursor, "--fumen")?.to_owned();
                set_unique_value(&mut fumen, value, "--fumen")?;
            }
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                let value = next(args, &mut cursor, "--patterns")?.to_owned();
                set_unique_value(&mut pattern, value, "--patterns")?;
            }
            "-c" | "--clear" | "--lines" => {
                clear = parse_lines(next(args, &mut cursor, "--lines")?)?
            }
            "-H" | "--hold" => hold = parse_bool(next(args, &mut cursor, "--hold")?, "--hold")?,
            "--no-hold" => {
                hold = false;
                cursor += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(invalid(format!(
                    "unsupported cover compatibility option '{flag}'"
                )))
            }
            value => {
                positional.push(value.to_owned());
                cursor += 1;
            }
        }
    }
    let mut positional_index = 0usize;
    if fumen.is_none() {
        if let Some(value) = positional.get(positional_index) {
            fumen = Some(value.clone());
            positional_index += 1;
        }
    }
    if pattern.is_none() {
        pattern = positional.get(positional_index).cloned();
        positional_index += usize::from(pattern.is_some());
    }
    let fumen = fumen.ok_or_else(|| missing("cover requires a solution Fumen"))?;
    let pattern = pattern.ok_or_else(|| missing("cover requires a queue pattern"))?;
    if let Some(value) = positional.get(positional_index) {
        clear = parse_lines(value)?;
        positional_index += 1;
    }
    if positional.len() > positional_index {
        return Err(invalid("cover received unsupported legacy parameters"));
    }

    let solutions = SourceFumenColoredFieldSet::decode(&fumen)
        .map_err(|error| invalid(format!("invalid supplied solution Fumen: {error:?}")))?;
    let initial_board_mask = solutions.initial_board_mask();
    let target = target_mask(clear)?;
    if initial_board_mask & !target != 0 {
        return Err(invalid(
            "the supplied solution starts above the clear target",
        ));
    }
    let empty = target & !initial_board_mask;
    if empty.count_ones() % 4 != 0 {
        return Err(invalid(
            "the supplied solution target has non-tetromino area",
        ));
    }
    let pieces = empty.count_ones() / 4;
    if solutions
        .identities()
        .iter()
        .any(|identity| identity.placement_count() != pieces as usize)
    {
        return Err(invalid(
            "a supplied solution page does not exactly fill the requested target",
        ));
    }

    let mut output = vec![
        "clearra".to_owned(),
        "pc".to_owned(),
        "--lines".to_owned(),
        clear.to_string(),
        "--board-mask".to_owned(),
        format!("0x{initial_board_mask:x}"),
        "--height".to_owned(),
        clear.to_string(),
        "--pieces".to_owned(),
        pieces.to_string(),
        "--patterns".to_owned(),
        normalize_sfinder_pattern(&pattern)?,
        "--objective".to_owned(),
        "all".to_owned(),
        "--rule".to_owned(),
        rule.to_owned(),
    ];
    output.push("--solution-fumen".to_owned());
    output.push(fumen);
    if !hold {
        output.push("--no-hold".to_owned());
    }
    Ok(output)
}

fn translate_score_finder(args: &[String], rule: &str) -> Result<Vec<String>, WebCommandError> {
    let input = parse_score_finder_input(args)?;
    if input
        .queue
        .chars()
        .any(|character| !"IOTSZJLioszjlt".contains(character))
    {
        return Err(invalid("score-finder requires an exact fixed queue"));
    }
    if input.initial_combo != 0 {
        return Err(invalid(
            "score-finder currently supports only initial_combo=0",
        ));
    }
    if input.b2b_end_bonus != 0 {
        return Err(invalid(
            "score-finder currently supports only b2b_end_bonus=0",
        ));
    }

    let board = decode_board(&input.field)?;
    let target = target_mask(input.clear)?;
    let normalized_board_mask = normalized_pc_board_mask(board);
    if normalized_board_mask & !target != 0 {
        return Err(invalid(format!(
            "the score-finder field has cells above the requested {} clear lines",
            input.clear
        )));
    }
    let empty = target & !normalized_board_mask;
    if empty.count_ones() == 0 || empty.count_ones() % 4 != 0 {
        return Err(invalid(
            "the score-finder target must require a positive whole number of tetrominoes",
        ));
    }

    let mut output = vec![
        "clearra".to_owned(),
        "pc".to_owned(),
        "score-finder".to_owned(),
        "--lines".to_owned(),
        input.clear.to_string(),
        "--board-mask".to_owned(),
        format!("0x{normalized_board_mask:x}"),
        "--height".to_owned(),
        input.clear.to_string(),
        "--pieces".to_owned(),
        (empty.count_ones() / 4).to_string(),
        "--queue".to_owned(),
        input.queue.to_ascii_uppercase(),
        "--rule".to_owned(),
        rule.to_owned(),
    ];
    if input.initial_b2b {
        push_pair(&mut output, "--initial-b2b", "1");
    }
    Ok(output)
}

fn parse_score_finder_input(args: &[String]) -> Result<ScoreFinderInput, WebCommandError> {
    let mut field = None;
    let mut queue = None;
    let mut clear = None;
    let mut initial_b2b = None;
    let mut initial_combo = None;
    let mut b2b_end_bonus = None;
    let mut positional = Vec::new();
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-t" | "--tetfu" | "--fumen" => {
                let value = next(args, &mut cursor, "--fumen")?.to_owned();
                set_field_input(&mut field, CompatibilityFieldInput::Fumen(value), "--fumen")?;
            }
            "--field-mask-v1" => {
                let value = next(args, &mut cursor, "--field-mask-v1")?.to_owned();
                set_field_input(
                    &mut field,
                    CompatibilityFieldInput::OccupiedMask(value),
                    "--field-mask-v1",
                )?;
            }
            "--board-mask-v1" => {
                return Err(invalid(
                    "score-finder perfect-clear search accepts fields up to six rows; use --field-mask-v1",
                ));
            }
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                let value = next(args, &mut cursor, "--queue")?.to_owned();
                set_unique_value(&mut queue, value, "--queue")?;
            }
            "-c" | "--clear" | "--lines" => {
                if clear.is_some() {
                    return Err(invalid(
                        "score-finder clear lines may be specified only once",
                    ));
                }
                clear = Some(parse_lines(next(args, &mut cursor, "--lines")?)?);
            }
            "--initial-b2b" => {
                if initial_b2b.is_some() {
                    return Err(invalid("--initial-b2b may be specified only once"));
                }
                initial_b2b = Some(parse_bool(
                    next(args, &mut cursor, "--initial-b2b")?,
                    "--initial-b2b",
                )?);
            }
            "--initial-combo" => {
                if initial_combo.is_some() {
                    return Err(invalid("--initial-combo may be specified only once"));
                }
                initial_combo = Some(parse_nonnegative_u32(
                    next(args, &mut cursor, "--initial-combo")?,
                    "--initial-combo",
                )?);
            }
            "--b2b-end-bonus" | "--b2b-bonus" => {
                if b2b_end_bonus.is_some() {
                    return Err(invalid("--b2b-end-bonus may be specified only once"));
                }
                b2b_end_bonus = Some(parse_i64(
                    next(args, &mut cursor, "--b2b-end-bonus")?,
                    "--b2b-end-bonus",
                )?);
            }
            flag if flag.starts_with('-') => {
                return Err(invalid(format!(
                    "unsupported score-finder compatibility option '{flag}'"
                )));
            }
            value => {
                positional.push(value.to_owned());
                cursor += 1;
            }
        }
    }

    let mut positional_index = 0usize;
    if field.is_none() {
        if let Some(value) = positional.get(positional_index) {
            field = Some(CompatibilityFieldInput::Fumen(value.clone()));
            positional_index += 1;
        }
    }
    if queue.is_none() {
        if let Some(value) = positional.get(positional_index) {
            queue = Some(value.clone());
            positional_index += 1;
        }
    }
    if clear.is_none() {
        if let Some(value) = positional.get(positional_index) {
            clear = Some(parse_lines(value)?);
            positional_index += 1;
        }
    }
    if initial_b2b.is_none() {
        if let Some(value) = positional.get(positional_index) {
            initial_b2b = Some(parse_bool(value, "initial_b2b")?);
            positional_index += 1;
        }
    }
    if initial_combo.is_none() {
        if let Some(value) = positional.get(positional_index) {
            initial_combo = Some(parse_nonnegative_u32(value, "initial_combo")?);
            positional_index += 1;
        }
    }
    if b2b_end_bonus.is_none() {
        if let Some(value) = positional.get(positional_index) {
            b2b_end_bonus = Some(parse_i64(value, "b2b_end_bonus")?);
            positional_index += 1;
        }
    }
    if positional.len() > positional_index {
        return Err(invalid(
            "score-finder received unsupported legacy parameters",
        ));
    }

    Ok(ScoreFinderInput {
        field: field.ok_or_else(|| missing("score-finder requires a Fumen or CTK3 field"))?,
        queue: queue.ok_or_else(|| missing("score-finder requires an exact fixed queue"))?,
        clear: clear.unwrap_or(4),
        initial_b2b: initial_b2b.unwrap_or(false),
        initial_combo: initial_combo.unwrap_or(0),
        b2b_end_bonus: b2b_end_bonus.unwrap_or(0),
    })
}

fn translate_setup_finder(
    command: &str,
    args: &[String],
    rule: &str,
) -> Result<Vec<String>, WebCommandError> {
    if args.len() != 1 || args[0].starts_with('-') {
        return Err(invalid(format!(
            "{command} accepts exactly one unordered remaining-piece inventory"
        )));
    }
    let remaining = &args[0];
    if remaining
        .chars()
        .any(|character| !"IOTSZJLioszjlt".contains(character))
    {
        return Err(invalid(format!(
            "{command} requires an unordered tetromino inventory"
        )));
    }
    let priority = match command {
        "best-setup" => "build",
        "dpc-finder" => "pc",
        _ => "all",
    };
    Ok(vec![
        "clearra".to_owned(),
        "setup-finder".to_owned(),
        "--remaining".to_owned(),
        remaining.to_ascii_uppercase(),
        "--priority".to_owned(),
        priority.to_owned(),
        "--rule".to_owned(),
        rule.to_owned(),
    ])
}

fn translate_verify(args: &[String]) -> Result<Vec<String>, WebCommandError> {
    let mut output = vec!["clearra".to_owned(), "verify".to_owned()];
    if let Some(scope) = args.first() {
        output.push(scope.to_owned());
    }
    if args.len() > 1 {
        return Err(invalid("verify accepts at most one scope"));
    }
    Ok(output)
}

fn first_field_and_pattern(
    args: &[String],
    command: &str,
) -> Result<(CompatibilityFieldInput, String, Vec<String>), WebCommandError> {
    let mut field = None;
    let mut pattern = None;
    let mut positional = Vec::new();
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-t" | "--tetfu" | "--fumen" => {
                let value = next(args, &mut cursor, "--fumen")?.to_owned();
                set_field_input(&mut field, CompatibilityFieldInput::Fumen(value), "--fumen")?;
            }
            "--field-mask-v1" => {
                let value = next(args, &mut cursor, "--field-mask-v1")?.to_owned();
                set_field_input(
                    &mut field,
                    CompatibilityFieldInput::OccupiedMask(value),
                    "--field-mask-v1",
                )?;
            }
            "--board-mask-v1" => {
                let value = next(args, &mut cursor, "--board-mask-v1")?.to_owned();
                set_field_input(
                    &mut field,
                    CompatibilityFieldInput::BoardMask(value),
                    "--board-mask-v1",
                )?;
            }
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                let value = next(args, &mut cursor, "--patterns")?.to_owned();
                set_unique_value(&mut pattern, value, "--patterns")?;
            }
            flag if flag.starts_with('-') => {
                return Err(invalid(format!(
                    "unsupported {command} compatibility option '{flag}'"
                )));
            }
            value => {
                positional.push(value.to_owned());
                cursor += 1;
            }
        }
    }

    let mut positional_index = 0usize;
    if field.is_none() {
        if let Some(value) = positional.get(positional_index) {
            field = Some(CompatibilityFieldInput::Fumen(value.clone()));
            positional_index += 1;
        }
    }
    if pattern.is_none() {
        if let Some(value) = positional.get(positional_index) {
            pattern = Some(value.clone());
            positional_index += 1;
        }
    }
    let trailing = positional[positional_index..].to_vec();
    Ok((
        field.ok_or_else(|| missing(format!("{command} requires a Fumen or CTK3 field")))?,
        pattern.ok_or_else(|| missing(format!("{command} requires a queue pattern")))?,
        trailing,
    ))
}

fn colored_target_board(
    source: &CompatibilityFieldInput,
) -> Result<ColoredTargetBoard, WebCommandError> {
    if let CompatibilityFieldInput::BoardMask(source) = source {
        let mask = parse_ctk3_board_mask(source, "--board-mask-v1")?;
        return Ok(ColoredTargetBoard {
            base_mask: "0x0".to_owned(),
            target_mask: mask.cli_hex(),
            target_count: mask.count_ones(),
            visible_height: mask.visible_height(),
        });
    }
    let board = decode_board(source)?;
    Ok(ColoredTargetBoard {
        base_mask: format!("0x{:x}", board.grey_mask()),
        target_mask: format!("0x{:x}", board.colored_mask()),
        target_count: board.colored_mask().count_ones(),
        visible_height: board.visible_height(),
    })
}

fn decode_board(source: &CompatibilityFieldInput) -> Result<CompatibilityBoard, WebCommandError> {
    match source {
        CompatibilityFieldInput::Fumen(source) => CompatibilityBoard::from_fumen(source),
        CompatibilityFieldInput::OccupiedMask(source) => {
            parse_ctk3_field_mask(source).map(CompatibilityBoard::from_occupied_mask)
        }
        CompatibilityFieldInput::BoardMask(_) => Err(invalid(
            "--board-mask-v1 is unavailable for this Sfinder compatibility command",
        )),
    }
}

fn target_mask(lines: u8) -> Result<u64, WebCommandError> {
    if !(1..=6).contains(&lines) {
        return Err(invalid("clear lines must be in 1..=6"));
    }
    let bits = u32::from(lines) * 10;
    Ok((1u64 << bits) - 1)
}

fn normalized_pc_board_mask(board: CompatibilityBoard) -> u64 {
    PcScenarioBoard::standard_10(u16::from(board.visible_height), board.occupied_mask())
        .after_initial_line_clear()
        .occupied_mask()
}

fn normalize_sfinder_pattern(source: &str) -> Result<String, WebCommandError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(missing("queue pattern cannot be empty"));
    }
    let characters: Vec<char> = source.chars().collect();
    let mut output = String::with_capacity(characters.len());
    let mut index = 0usize;
    while index < characters.len() {
        let character = characters[index];
        if character.is_whitespace() || character == ',' {
            index += 1;
            continue;
        }
        if character == '*' {
            if characters.get(index + 1) == Some(&'!') {
                output.push_str("P7");
                index += 2;
                continue;
            }
            if characters
                .get(index + 1)
                .is_some_and(|next| matches!(next, 'p' | 'P'))
            {
                output.push('P');
                index += 2;
                continue;
            }
            return Err(invalid("Sfinder '*' must be followed by ! or pN"));
        }
        if matches!(character, 'p' | 'P') && index > 0 && characters[index - 1] == ']' {
            index += 1;
            continue;
        }
        output.push(character.to_ascii_uppercase());
        index += 1;
    }
    Ok(output)
}

fn parse_lines(value: &str) -> Result<u8, WebCommandError> {
    value
        .parse::<u8>()
        .ok()
        .filter(|lines| (1..=6).contains(lines))
        .ok_or_else(|| invalid(format!("invalid clear-line count '{value}'")))
}

fn parse_bool(value: &str, option: &str) -> Result<bool, WebCommandError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "use" => Ok(true),
        "false" | "no" | "off" | "avoid" => Ok(false),
        _ => Err(invalid(format!("invalid {option} value '{value}'"))),
    }
}

fn parse_nonnegative_u32(value: &str, option: &str) -> Result<u32, WebCommandError> {
    value
        .parse::<u32>()
        .map_err(|_| invalid(format!("invalid {option} value '{value}'")))
}

fn parse_i64(value: &str, option: &str) -> Result<i64, WebCommandError> {
    value
        .parse::<i64>()
        .map_err(|_| invalid(format!("invalid {option} value '{value}'")))
}

fn set_field_input(
    target: &mut Option<CompatibilityFieldInput>,
    value: CompatibilityFieldInput,
    option: &str,
) -> Result<(), WebCommandError> {
    if target.is_some() {
        return Err(invalid(format!(
            "{option} conflicts with an earlier field input"
        )));
    }
    *target = Some(value);
    Ok(())
}

fn set_unique_value(
    target: &mut Option<String>,
    value: String,
    option: &str,
) -> Result<(), WebCommandError> {
    if target.is_some() {
        return Err(invalid(format!("{option} may be specified only once")));
    }
    *target = Some(value);
    Ok(())
}

fn set_legacy_pc_piece_source(
    target: &mut Option<LegacyPcPieceSource>,
    value: LegacyPcPieceSource,
    option: &str,
) -> Result<(), WebCommandError> {
    if target.is_some() {
        return Err(invalid(format!(
            "{option} conflicts with an earlier queue or pattern selection"
        )));
    }
    *target = Some(value);
    Ok(())
}

fn next<'a>(
    args: &'a [String],
    cursor: &mut usize,
    option: &str,
) -> Result<&'a str, WebCommandError> {
    *cursor += 1;
    let value = args
        .get(*cursor)
        .ok_or_else(|| missing(format!("{option} requires a value")))?;
    *cursor += 1;
    Ok(value)
}

fn push_pair(output: &mut Vec<String>, option: &str, value: &str) {
    output.push(option.to_owned());
    output.push(value.to_owned());
}

fn push_pair_owned(output: &mut Vec<String>, option: &str, value: String) {
    output.push(option.to_owned());
    output.push(value);
}

fn missing(message: impl Into<String>) -> WebCommandError {
    WebCommandError::new(WebCommandErrorCode::MissingValue, message)
}

fn invalid(message: impl Into<String>) -> WebCommandError {
    WebCommandError::new(WebCommandErrorCode::InvalidValue, message)
}

fn not_yet_representable(command: &str) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::UnsupportedCommand,
        format!(
            "Sfinder '{command}' has a distinct result contract and is not available through a different Clearra command"
        ),
    )
}

#[cfg(test)]
mod tests {
    use clearra_app::{PcResultProjection, PcScoreIngressOrigin, ProductCapabilityContract};
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_fumen::{
        ColoredSolutionFumenExporter, ColoredSolutionPage, ColoredSolutionPlacement,
    };

    use crate::WebCommandParser;

    use super::{normalize_sfinder_pattern, translate_command};

    fn option_value<'a>(tokens: &'a [String], option: &str) -> Option<&'a str> {
        tokens
            .windows(2)
            .find(|pair| pair[0] == option)
            .map(|pair| pair[1].as_str())
    }

    #[test]
    fn normalizes_only_sfinder_pattern_spelling() {
        assert_eq!(normalize_sfinder_pattern("I,*p4").unwrap(), "IP4");
        assert_eq!(normalize_sfinder_pattern("[oisz]p2").unwrap(), "[OISZ]2");
        assert_eq!(normalize_sfinder_pattern("*!").unwrap(), "P7");
    }

    #[test]
    fn leaves_clearra_commands_untouched() {
        let input = vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "--lines".to_owned(),
            "4".to_owned(),
        ];
        assert_eq!(translate_command(&input).unwrap(), input);
    }

    #[test]
    fn translates_legacy_chance_into_an_exact_pc_scenario() {
        let input = ["clearra", "chance", "v115@vhAAgH", "*p7,*p3", "4"].map(str::to_owned);
        let translated = translate_command(&input).expect("legacy chance translation");

        assert_eq!(translated[1], "pc");
        assert!(translated
            .windows(2)
            .any(|pair| pair == ["--patterns", "P7P3"]));
        assert!(translated.windows(2).any(|pair| pair == ["--pieces", "10"]));
        assert!(translated
            .windows(2)
            .any(|pair| pair == ["--objective", "unique"]));
    }

    #[test]
    fn explicit_legacy_pc_queue_remains_an_exact_queue() {
        let input = [
            "clearra",
            "sfinder",
            "path",
            "--field-mask-v1",
            "000000000000003f",
            "--queue",
            "i",
            "--lines",
            "1",
        ]
        .map(str::to_owned);
        let translated = translate_command(&input).expect("exact queue translation");

        assert!(translated.windows(2).any(|pair| pair == ["--queue", "I"]));
        assert!(!translated.iter().any(|token| token == "--patterns"));
    }

    #[test]
    fn six_line_pc_uses_field_occupancy_to_derive_the_exact_piece_window() {
        let input = [
            "clearra",
            "sfinder",
            "path",
            "--field-mask-v1",
            "00000f0000000000",
            "--patterns",
            "iotszjliotszjl",
            "--lines",
            "6",
        ]
        .map(str::to_owned);
        let translated = translate_command(&input).expect("six-line scenario translation");

        assert!(translated.windows(2).any(|pair| pair == ["--lines", "6"]));
        assert!(translated.windows(2).any(|pair| pair == ["--height", "6"]));
        assert!(translated.windows(2).any(|pair| pair == ["--pieces", "14"]));
        assert!(translated
            .windows(2)
            .any(|pair| pair == ["--patterns", "IOTSZJLIOTSZJL"]));
    }

    #[test]
    fn pc_compacts_completed_input_rows_before_deriving_the_piece_window() {
        let raw = [
            "clearra",
            "sfinder",
            "path",
            "--field-mask-v1",
            "000000003ff000ff",
            "--patterns",
            "oii",
            "--lines",
            "2",
        ]
        .map(str::to_owned);
        let normalized = [
            "clearra",
            "sfinder",
            "path",
            "--field-mask-v1",
            "00000000000000ff",
            "--patterns",
            "oii",
            "--lines",
            "2",
        ]
        .map(str::to_owned);

        let raw_translation = translate_command(&raw).expect("raw completed-row translation");
        let normalized_translation =
            translate_command(&normalized).expect("explicit normalized translation");

        assert_eq!(raw_translation, normalized_translation);
        assert_eq!(option_value(&raw_translation, "--lines"), Some("2"));
        assert_eq!(option_value(&raw_translation, "--height"), Some("2"));
        assert_eq!(option_value(&raw_translation, "--pieces"), Some("3"));
        assert_eq!(option_value(&raw_translation, "--board-mask"), Some("0xff"));
    }

    #[test]
    fn scenario_pc_accepts_explicit_odd_heights_from_one_through_five() {
        for (lines, pieces, pattern) in [
            ("1", "2", "IO"),
            ("3", "7", "IOTSZJL"),
            ("5", "12", "IOTSZJLIOTSZ"),
        ] {
            let input = [
                "clearra",
                "sfinder",
                "path",
                "--field-mask-v1",
                "0000000000000003",
                "--patterns",
                pattern,
                "--lines",
                lines,
            ]
            .map(str::to_owned);
            let translated = translate_command(&input).expect("odd-height scenario translation");

            assert!(translated.windows(2).any(|pair| pair == ["--lines", lines]));
            assert!(translated
                .windows(2)
                .any(|pair| pair == ["--height", lines]));
            assert!(translated
                .windows(2)
                .any(|pair| pair == ["--pieces", pieces]));
        }
    }

    #[test]
    fn compatibility_search_preserves_adaptive_worker_selection() {
        let input = [
            "clearra",
            "sfinder",
            "percent",
            "v115@vhAAgH",
            "P7P3",
            "4",
            "--auto-workers",
            "3",
            "--use-all-cpu-threads",
        ]
        .map(str::to_owned);

        let translated = translate_command(&input).expect("legacy percent translation");

        assert!(translated
            .windows(2)
            .any(|pair| pair == ["--auto-workers", "3"]));
        assert!(translated
            .iter()
            .any(|value| value == "--use-all-cpu-threads"));
    }

    #[test]
    fn compatibility_worker_alias_is_canonicalized_once() {
        let input = [
            "clearra",
            "sfinder",
            "chance",
            "v115@vhAAgH",
            "P7P3",
            "4",
            "--cpu-threads",
            "2",
        ]
        .map(str::to_owned);

        let translated = translate_command(&input).expect("legacy worker alias translation");

        assert!(translated.windows(2).any(|pair| pair == ["--workers", "2"]));
        assert!(!translated.iter().any(|value| value == "--cpu-threads"));
    }

    #[test]
    fn compatibility_worker_modes_are_mutually_exclusive() {
        let input = [
            "clearra",
            "sfinder",
            "chance",
            "v115@vhAAgH",
            "P7P3",
            "4",
            "--workers",
            "2",
            "--auto-workers",
            "3",
        ]
        .map(str::to_owned);

        let error = translate_command(&input).expect_err("conflicting worker modes");

        assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
    }

    #[test]
    fn compatibility_verify_rejects_worker_options() {
        let input = ["clearra", "sfinder", "verify", "kicks", "--workers", "2"].map(str::to_owned);

        let error = translate_command(&input).expect_err("verify has no worker pool");

        assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
    }

    #[test]
    fn compatibility_rule_is_common_to_every_search_translation() {
        let base = "0".repeat(60);
        let target = format!("{}f", "0".repeat(59));
        let cases = vec![
            (
                [
                    "clearra",
                    "sfinder",
                    "path",
                    "--field-mask-v1",
                    "0000000000000000",
                    "--patterns",
                    "IOTSZJLIOT",
                    "--lines",
                    "4",
                    "--rule",
                    "srs-plus",
                ]
                .map(str::to_owned)
                .to_vec(),
                "srs-plus",
            ),
            (
                [
                    "clearra",
                    "sfinder",
                    "setup",
                    "--field-mask-v1",
                    "000000000000000f",
                    "--patterns",
                    "I",
                    "--rule",
                    "srs",
                ]
                .map(str::to_owned)
                .to_vec(),
                "srs",
            ),
            (
                [
                    "clearra",
                    "sfinder",
                    "score-finder",
                    "--field-mask-v1",
                    "000000000000000f",
                    "--patterns",
                    "I",
                    "--rule",
                    "jstris-180",
                ]
                .map(str::to_owned)
                .to_vec(),
                "jstris-180",
            ),
            (
                [
                    "clearra",
                    "sfinder",
                    "best-setup",
                    "IOTS",
                    "--rule",
                    "srs-plus",
                ]
                .map(str::to_owned)
                .to_vec(),
                "srs-plus",
            ),
            (
                vec![
                    "clearra".to_owned(),
                    "sfinder".to_owned(),
                    "cover".to_owned(),
                    "--base-mask-v1".to_owned(),
                    base,
                    "--target-mask-v1".to_owned(),
                    target,
                    "--patterns".to_owned(),
                    "I".to_owned(),
                    "--rule".to_owned(),
                    "srs".to_owned(),
                ],
                "srs",
            ),
        ];

        for (input, expected) in cases {
            let translated = translate_command(&input).expect("rule-aware translation");
            assert_eq!(option_value(&translated, "--rule"), Some(expected));
        }

        let defaulted = ["clearra", "sfinder", "pc-setup", "IOTS"].map(str::to_owned);
        let translated = translate_command(&defaulted).expect("default rule translation");
        assert_eq!(option_value(&translated, "--rule"), Some("srs-plus"));
    }

    #[test]
    fn compatibility_rule_rejects_unknown_duplicate_and_verify_usage() {
        let invalid_cases = [
            ["clearra", "sfinder", "pc-setup", "IOTS", "--rule", "custom"]
                .map(str::to_owned)
                .to_vec(),
            [
                "clearra", "sfinder", "pc-setup", "IOTS", "--rule", "srs", "--rule", "srs-plus",
            ]
            .map(str::to_owned)
            .to_vec(),
            ["clearra", "sfinder", "verify", "kicks", "--rule", "srs"]
                .map(str::to_owned)
                .to_vec(),
        ];

        for input in invalid_cases {
            let error = translate_command(&input).expect_err("rule input must fail closed");
            assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
        }
    }

    #[test]
    fn score_finder_maps_the_fixed_queue_score_contract_to_pc() {
        let input = [
            "clearra",
            "sfinder",
            "score-finder",
            "v115@9gB8HeB8HeC8EeH8AeC8JeAgH",
            "SIJSTLZO",
            "5",
            "true",
        ]
        .map(str::to_owned);
        let translated = translate_command(&input).expect("score-finder translation");

        assert_eq!(translated[1], "pc");
        assert_eq!(translated[2], "score-finder");
        assert_eq!(option_value(&translated, "--lines"), Some("5"));
        assert_eq!(option_value(&translated, "--height"), Some("5"));
        assert_eq!(option_value(&translated, "--queue"), Some("SIJSTLZO"));
        assert_eq!(option_value(&translated, "--objective"), None);
        assert_eq!(option_value(&translated, "--score-profile"), None);
        assert_eq!(option_value(&translated, "--spin-profile"), None);
        assert_eq!(option_value(&translated, "--backend"), None);
        assert_eq!(option_value(&translated, "--initial-b2b"), Some("1"));
        assert!(!translated.iter().any(|value| value == "damage"));
        let request =
            WebCommandParser::parse_tokens(&input).expect("typed score-finder PC request");
        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcScoreFinder)
        );
        assert_eq!(
            request.pc_result_projection(),
            PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScoreFinder)
        );
    }

    #[test]
    fn score_finder_compacts_completed_input_rows_before_piece_validation() {
        let raw = [
            "clearra",
            "sfinder",
            "score-finder",
            "--field-mask-v1",
            "000000003ff000ff",
            "--queue",
            "OII",
            "--lines",
            "2",
            "--initial-b2b",
            "false",
        ]
        .map(str::to_owned);
        let normalized = [
            "clearra",
            "sfinder",
            "score-finder",
            "--field-mask-v1",
            "00000000000000ff",
            "--queue",
            "OII",
            "--lines",
            "2",
            "--initial-b2b",
            "false",
        ]
        .map(str::to_owned);

        let raw_translation = translate_command(&raw).expect("raw completed-row score-finder");
        let normalized_translation =
            translate_command(&normalized).expect("explicit normalized score-finder");

        assert_eq!(raw_translation, normalized_translation);
        assert_eq!(option_value(&raw_translation, "--pieces"), Some("3"));
        assert_eq!(option_value(&raw_translation, "--board-mask"), Some("0xff"));
    }

    #[test]
    fn score_finder_accepts_named_inputs_and_rejects_unrepresented_scoring() {
        let supported = [
            "clearra",
            "sfinder",
            "score-finder",
            "--field-mask-v1",
            "00000000000000ff",
            "--queue",
            "IOTSZJLI",
            "--lines",
            "4",
            "--initial-b2b",
            "false",
            "--initial-combo",
            "0",
            "--b2b-end-bonus",
            "0",
        ]
        .map(str::to_owned);
        let translated = translate_command(&supported).expect("supported score-finder options");
        assert_eq!(translated[1], "pc");
        assert_eq!(option_value(&translated, "--lines"), Some("4"));
        assert_eq!(option_value(&translated, "--initial-b2b"), None);

        for (option, value) in [("--initial-combo", "1"), ("--b2b-end-bonus", "100")] {
            let mut unsupported = supported.to_vec();
            let index = unsupported
                .iter()
                .position(|token| token == option)
                .expect("option in fixture");
            unsupported[index + 1] = value.to_owned();
            let error = translate_command(&unsupported).expect_err("unsupported scoring input");
            assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
        }
    }

    #[test]
    fn retired_cat_finder_spellings_are_rejected() {
        for command in ["cat-finder", "cat_finder", "catfinder"] {
            let input = ["clearra", "sfinder", command, "v115@vhAAgH", "I"].map(str::to_owned);
            let error = translate_command(&input).expect_err("retired command must be rejected");
            assert_eq!(error.code(), crate::WebCommandErrorCode::UnsupportedCommand);
        }
    }

    #[test]
    fn compatibility_rejects_legacy_parameters_it_cannot_preserve() {
        let multi_fumen = [
            "clearra",
            "sfinder",
            "cover",
            "--fumen",
            "v115@vhAAgH",
            "v115@vhAAgH",
            "--patterns",
            "P7",
        ]
        .map(str::to_owned);
        let error = translate_command(&multi_fumen).expect_err("multi-Fumen cover is not mapped");
        assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);

        let setup_options =
            ["clearra", "sfinder", "pc-setup", "IOTS", "--page", "2"].map(str::to_owned);
        let error = translate_command(&setup_options).expect_err("setup options are not dropped");
        assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
    }

    #[test]
    fn compatibility_rejects_structural_spin_instead_of_lowering_it_to_forward_search() {
        for command in ["spin", "spin-cover", "spincover"] {
            let input =
                ["clearra", "sfinder", command, "v115@vhAAgH", "TI", "TSS"].map(str::to_owned);

            let error = translate_command(&input)
                .expect_err("unordered structural spin must not become ordered forward spin");
            assert_eq!(error.code(), crate::WebCommandErrorCode::UnsupportedCommand);
            assert!(error.message().contains("distinct result contract"));
        }
    }

    #[test]
    fn cover_accepts_repeated_piece_colors_as_one_static_solution() {
        let page = ColoredSolutionPage::new(
            10,
            1,
            0b11u64 << 8,
            vec![
                ColoredSolutionPlacement::new(PieceKind::I, 0b1111),
                ColoredSolutionPlacement::new(PieceKind::I, 0b1111 << 4),
            ],
        )
        .expect("valid repeated-I page");
        let fumen = ColoredSolutionFumenExporter::encode(&[page]).expect("encoded fumen");
        let input = [
            "clearra".to_owned(),
            "sfinder".to_owned(),
            "cover".to_owned(),
            fumen.clone(),
            "II".to_owned(),
            "1".to_owned(),
            "--workers".to_owned(),
            "2".to_owned(),
        ];

        let translated = translate_command(&input).expect("cover translation");

        assert!(translated.windows(2).any(|pair| pair == ["--pieces", "2"]));
        assert!(translated
            .windows(2)
            .any(|pair| pair == ["--solution-fumen", fumen.as_str()]));
        assert!(translated.windows(2).any(|pair| pair == ["--workers", "2"]));

        let colored_target = [
            "clearra".to_owned(),
            "sfinder".to_owned(),
            "setup".to_owned(),
            fumen,
            "II".to_owned(),
        ];
        let translated = translate_command(&colored_target).expect("colored target translation");
        assert!(translated.iter().any(|value| value == "--no-mirror"));
    }

    #[test]
    fn canonical_occupancy_masks_match_an_empty_fumen_for_every_pc_preset() {
        for command in [
            "path",
            "percent",
            "chance",
            "minimals",
            "score",
            "score-minimals",
            "saves",
            "best-save",
        ] {
            let fumen = [
                "clearra",
                "sfinder",
                command,
                "--fumen",
                "v115@vhAAgH",
                "--patterns",
                "P7P3",
                "--lines",
                "4",
            ]
            .map(str::to_owned);
            let ctk3 = [
                "clearra",
                "sfinder",
                command,
                "--field-mask-v1",
                "0000000000000000",
                "--patterns",
                "P7P3",
                "--lines",
                "4",
            ]
            .map(str::to_owned);
            assert_eq!(
                translate_command(&ctk3).expect("CTK3 PC translation"),
                translate_command(&fumen).expect("Fumen PC translation"),
                "{command} must preserve its preset"
            );
        }
    }

    #[test]
    fn colorless_occupancy_masks_drive_target_and_forward_contracts() {
        let field_mask = "00000000000000ff";
        for command in [
            "setup",
            "congruent",
            "congruent-cover",
            "setup-cover",
            "cover-percent",
            "special-cover",
        ] {
            let input = [
                "clearra",
                "sfinder",
                command,
                "--field-mask-v1",
                field_mask,
                "--patterns",
                "II",
            ]
            .map(str::to_owned);
            let translated = translate_command(&input).expect("colorless target-mask translation");
            assert!(translated
                .windows(2)
                .any(|pair| pair == ["--base-mask", "0x0"]));
            assert!(translated
                .windows(2)
                .any(|pair| pair == ["--target-mask", "0xff"]));
            assert!(translated.windows(2).any(|pair| pair == ["--height", "1"]));
            assert!(translated.iter().any(|value| value == "--no-mirror"));
        }

        let score_finder = [
            "clearra",
            "sfinder",
            "score-finder",
            "--field-mask-v1",
            "000000000000000f",
            "--patterns",
            "I",
        ]
        .map(str::to_owned);
        let translated =
            translate_command(&score_finder).expect("colorless score-finder translation");
        assert!(translated
            .windows(2)
            .any(|pair| pair == ["--board-mask", "0xf"]));

        for command in ["spin-cover", "spin"] {
            let input = [
                "clearra",
                "sfinder",
                command,
                "--field-mask-v1",
                "000000000000000f",
                "--patterns",
                "I",
                "TSS",
            ]
            .map(str::to_owned);
            let error = translate_command(&input)
                .expect_err("structural compatibility input must remain fail closed");
            assert_eq!(error.code(), crate::WebCommandErrorCode::UnsupportedCommand);
        }
    }

    #[test]
    fn board240_masks_reach_colored_target_and_forward_native_commands() {
        let board_mask = format!("8{}7", "0".repeat(58));
        let native_mask = format!("0x{board_mask}");
        assert_eq!(board_mask.len(), 60);

        for command in [
            "setup",
            "congruent",
            "congruent-cover",
            "setup-cover",
            "cover-percent",
            "special-cover",
        ] {
            let input = vec![
                "clearra".to_owned(),
                "sfinder".to_owned(),
                command.to_owned(),
                "--board-mask-v1".to_owned(),
                board_mask.clone(),
                "--patterns".to_owned(),
                "I".to_owned(),
                "--rule".to_owned(),
                "srs-plus".to_owned(),
            ];
            let translated = translate_command(&input).expect("Board240 target translation");
            assert_eq!(option_value(&translated, "--base-mask"), Some("0x0"));
            assert_eq!(
                option_value(&translated, "--target-mask"),
                Some(native_mask.as_str())
            );
            assert_eq!(option_value(&translated, "--height"), Some("24"));
            WebCommandParser::parse_tokens(&input).expect("typed build-probability request");
        }

        for command in ["spin-cover", "spin"] {
            let input = vec![
                "clearra".to_owned(),
                "sfinder".to_owned(),
                command.to_owned(),
                "--board-mask-v1".to_owned(),
                board_mask.clone(),
                "--patterns".to_owned(),
                "I".to_owned(),
                "TSS".to_owned(),
                "--rule".to_owned(),
                "srs-x".to_owned(),
            ];
            let error = translate_command(&input)
                .expect_err("Board240 does not change the structural spin contract");
            assert_eq!(error.code(), crate::WebCommandErrorCode::UnsupportedCommand);
        }

        let score = vec![
            "clearra".to_owned(),
            "sfinder".to_owned(),
            "score-finder".to_owned(),
            "--board-mask-v1".to_owned(),
            board_mask,
            "--patterns".to_owned(),
            "I".to_owned(),
            "--rule".to_owned(),
            "srs".to_owned(),
        ];
        let error = translate_command(&score).expect_err("score-finder is a six-row PC search");
        assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
    }

    #[test]
    fn board240_compatibility_input_is_canonical_and_scoped() {
        for malformed in ["0".repeat(59), format!("{}A", "0".repeat(59))] {
            let input = vec![
                "clearra".to_owned(),
                "sfinder".to_owned(),
                "setup".to_owned(),
                "--board-mask-v1".to_owned(),
                malformed,
                "--patterns".to_owned(),
                "I".to_owned(),
            ];
            let error = translate_command(&input).expect_err("noncanonical Board240 input");
            assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
        }

        let board_mask = format!("{}f", "0".repeat(59));
        let conflicting = vec![
            "clearra".to_owned(),
            "sfinder".to_owned(),
            "setup".to_owned(),
            "--field-mask-v1".to_owned(),
            "000000000000000f".to_owned(),
            "--board-mask-v1".to_owned(),
            board_mask.clone(),
            "--patterns".to_owned(),
            "I".to_owned(),
        ];
        assert!(translate_command(&conflicting).is_err());

        let pc = vec![
            "clearra".to_owned(),
            "sfinder".to_owned(),
            "path".to_owned(),
            "--board-mask-v1".to_owned(),
            board_mask,
            "--patterns".to_owned(),
            "IOTSZJLIOT".to_owned(),
            "--lines".to_owned(),
            "4".to_owned(),
        ];
        let error = translate_command(&pc).expect_err("PC keeps the field-mask-v1 contract");
        assert_eq!(error.code(), crate::WebCommandErrorCode::InvalidValue);
    }

    #[test]
    fn two_field_cover_reaches_the_typed_build_probability_request() {
        let base = format!("{:060x}", 0x300u64);
        let target = format!("{:060x}", 0xffu64);
        let cover_request = WebCommandParser::parse(&format!(
            "clearra sfinder cover --base-mask-v1 {base} --target-mask-v1 {target} --patterns II --hold false"
        ))
        .expect("two-field cover request");
        let native_request = WebCommandParser::parse(
            "clearra build-probability --base-mask 0x300 --target-mask 0xff --height 1 --patterns II --rule srs-plus --no-mirror --no-hold",
        )
        .expect("native build-probability request");
        assert_eq!(cover_request, native_request);
    }

    #[test]
    fn two_field_cover_preserves_completed_target_rows_until_build_completion() {
        let base = "0".repeat(60);
        let target = format!("{:060x}", 0x000f_ffffu64);
        let input = vec![
            "clearra".to_owned(),
            "sfinder".to_owned(),
            "cover".to_owned(),
            "--base-mask-v1".to_owned(),
            base,
            "--target-mask-v1".to_owned(),
            target,
            "--patterns".to_owned(),
            "IOTSZ".to_owned(),
        ];

        let translated = translate_command(&input).expect("two completed target rows");

        assert_eq!(option_value(&translated, "--base-mask"), Some("0x0"));
        assert_eq!(option_value(&translated, "--target-mask"), Some("0xfffff"));
        assert_eq!(option_value(&translated, "--height"), Some("2"));
        WebCommandParser::parse_tokens(&input).expect("typed completed-row cover request");
    }

    #[test]
    fn compatibility_field_inputs_are_mutually_exclusive() {
        let input = [
            "clearra",
            "sfinder",
            "path",
            "--fumen",
            "v115@vhAAgH",
            "--field-mask-v1",
            "0000000000000000",
            "--patterns",
            "I",
        ]
        .map(str::to_owned);
        assert!(translate_command(&input).is_err());
    }
}
// SRP rationale: this module has one behavior-level change reason: validating and translating
// the supported Sfinder command dialect into exact typed Clearra requests.
