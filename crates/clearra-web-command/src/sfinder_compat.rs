use clearra_fumen::{SourceFumenBoard, SourceFumenColoredFieldSet};

use crate::{
    ctk3_mask_input::{parse_ctk3_board_mask, parse_ctk3_field_mask, Ctk3FieldMask},
    WebCommandError, WebCommandErrorCode,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PcPreset {
    Path,
    Chance,
    Minimals,
    Score,
    ScoreMinimals,
    Saves,
}

#[derive(Debug)]
enum CompatibilityFieldInput {
    Fumen(String),
    OccupiedMask(String),
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

pub(crate) fn translate_command(tokens: &[String]) -> Result<Vec<String>, WebCommandError> {
    let mut command_index = usize::from(tokens.first().map(String::as_str) == Some("clearra"));
    let Some(command) = tokens.get(command_index) else {
        return Ok(tokens.to_vec());
    };
    let mut command = normalized_command(command);
    command_index += 1;

    if command == "sfinder" {
        let Some(subcommand) = tokens.get(command_index) else {
            return Err(WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "sfinder requires a subcommand",
            ));
        };
        command = normalized_command(subcommand);
        command_index += 1;
    } else if !is_compatibility_command(&command) {
        return Ok(tokens.to_vec());
    }

    let (args, worker_options) = CompatibilityWorkerOptions::extract(&tokens[command_index..])?;
    if command == "verify" && worker_options.requested() {
        return Err(invalid("verify does not run a search worker pool"));
    }
    let mut output = match command.as_str() {
        "path" => translate_pc(PcPreset::Path, &args),
        "chance" | "percent" => translate_pc(PcPreset::Chance, &args),
        "minimals" => translate_pc(PcPreset::Minimals, &args),
        "score" => translate_pc(PcPreset::Score, &args),
        "score-minimals" => translate_pc(PcPreset::ScoreMinimals, &args),
        "saves" | "best-save" => translate_pc(PcPreset::Saves, &args),
        "congruent" | "congruent-cover" | "setup-cover" | "cover-percent" => {
            translate_colored_target(&command, &args, false)
        }
        "special-cover" => translate_colored_target(&command, &args, true),
        "spin-cover" | "spin" => translate_spin_cover(&args),
        "cat-finder" => translate_cat_finder(&args),
        "pc-setup" | "best-setup" | "dpc-finder" => translate_setup_finder(&command, &args),
        "verify" => translate_verify(&args),
        "cover" => translate_cover(&args),
        "setup" => translate_colored_target(&command, &args, false),
        "ren" | "util" | "parity" | "to-gray" | "to-fumen" | "render" | "special-minimals" => {
            Err(not_yet_representable(&command))
        }
        _ => Err(WebCommandError::new(
            WebCommandErrorCode::UnsupportedCommand,
            format!("unsupported Sfinder compatibility command '{command}'"),
        )),
    }?;
    worker_options.append_to(&mut output);
    Ok(output)
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
        "catfinder" => "cat-finder".to_owned(),
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
            | "cat-finder"
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
    pattern: String,
    clear: u8,
    hold: bool,
}

fn translate_pc(preset: PcPreset, args: &[String]) -> Result<Vec<String>, WebCommandError> {
    let input = parse_legacy_pc_input(args)?;
    let board = decode_board(&input.field)?;
    let target = target_mask(input.clear)?;
    if board.occupied_mask() & !target != 0 {
        return Err(invalid(format!(
            "the input field has cells above the requested {} clear lines",
            input.clear
        )));
    }
    let empty = target & !board.occupied_mask();
    if empty.count_ones() % 4 != 0 {
        return Err(invalid(
            "the target field does not contain a whole number of tetrominoes",
        ));
    }

    let mut output = vec![
        "clearra".to_owned(),
        "pc".to_owned(),
        "--lines".to_owned(),
        input.clear.to_string(),
        "--board-mask".to_owned(),
        format!("0x{:x}", board.occupied_mask()),
        "--height".to_owned(),
        input.clear.to_string(),
        "--pieces".to_owned(),
        (empty.count_ones() / 4).to_string(),
        "--patterns".to_owned(),
        normalize_sfinder_pattern(&input.pattern)?,
        "--rule".to_owned(),
        "jstris-180".to_owned(),
    ];
    if !input.hold {
        output.push("--no-hold".to_owned());
    }
    match preset {
        PcPreset::Path => push_pair(&mut output, "--objective", "all"),
        PcPreset::Chance => push_pair(&mut output, "--objective", "unique"),
        PcPreset::Minimals => push_pair(&mut output, "--objective", "minimum-cover"),
        PcPreset::Score => {
            push_pair(&mut output, "--objective", "all");
            output.push("--score".to_owned());
            push_pair(&mut output, "--score-profile", "jstris-ultra");
        }
        PcPreset::ScoreMinimals => {
            push_pair(&mut output, "--objective", "minimum-cover");
            output.push("--score".to_owned());
            push_pair(&mut output, "--score-profile", "jstris-ultra");
        }
        PcPreset::Saves => {
            push_pair(&mut output, "--objective", "all");
            output.push("--solution-probabilities".to_owned());
        }
    }
    Ok(output)
}

fn parse_legacy_pc_input(args: &[String]) -> Result<LegacyPcInput, WebCommandError> {
    let mut field = None;
    let mut pattern = None;
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
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                let value = next(args, &mut cursor, "--patterns")?.to_owned();
                set_unique_value(&mut pattern, value, "--patterns")?;
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
    if pattern.is_none() {
        pattern = positional.get(positional_index).cloned();
        positional_index += usize::from(pattern.is_some());
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
        pattern: pattern
            .ok_or_else(|| missing("Sfinder compatibility search requires a queue pattern"))?,
        clear,
        hold,
    })
}

fn translate_colored_target(
    command: &str,
    args: &[String],
    spin: bool,
) -> Result<Vec<String>, WebCommandError> {
    let (field, pattern, trailing) = first_field_and_pattern(args, command)?;
    if !trailing.is_empty() {
        return Err(invalid(format!(
            "{command} received unsupported legacy parameters"
        )));
    }
    let board = decode_board(&field)?;
    if board.colored_mask() == 0 {
        return Err(invalid(format!(
            "{command} requires at least one target cell"
        )));
    }
    if board.colored_mask().count_ones() % 4 != 0 {
        return Err(invalid("colored target cells must have tetromino area"));
    }
    let mut output = vec![
        "clearra".to_owned(),
        "build-probability".to_owned(),
        "--base-mask".to_owned(),
        format!("0x{:x}", board.grey_mask()),
        "--target-mask".to_owned(),
        format!("0x{:x}", board.colored_mask()),
        "--height".to_owned(),
        board.visible_height().max(1).to_string(),
        "--patterns".to_owned(),
        normalize_sfinder_pattern(&pattern)?,
        "--rule".to_owned(),
        "jstris-180".to_owned(),
        "--no-mirror".to_owned(),
    ];
    if spin {
        push_pair(&mut output, "--aggregate", "spin");
        push_pair(&mut output, "--spin-profile", "t-spins");
    }
    Ok(output)
}

fn translate_cover(args: &[String]) -> Result<Vec<String>, WebCommandError> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--base-mask-v1" | "--target-mask-v1"))
    {
        return translate_build_probability_cover(args);
    }
    translate_legacy_solution_cover(args)
}

fn translate_build_probability_cover(args: &[String]) -> Result<Vec<String>, WebCommandError> {
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
        "jstris-180".to_owned(),
        "--no-mirror".to_owned(),
    ];
    if !hold {
        output.push("--no-hold".to_owned());
    }
    Ok(output)
}

fn translate_legacy_solution_cover(args: &[String]) -> Result<Vec<String>, WebCommandError> {
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
        "jstris-180".to_owned(),
    ];
    output.push("--solution-fumen".to_owned());
    output.push(fumen);
    if !hold {
        output.push("--no-hold".to_owned());
    }
    Ok(output)
}

fn translate_spin_cover(args: &[String]) -> Result<Vec<String>, WebCommandError> {
    let (field, pattern, trailing) = first_field_and_pattern(args, "spin-cover")?;
    let spin_type = trailing.first().map(String::as_str).unwrap_or("TSS");
    if trailing.len() > 1 {
        return Err(invalid("spin-cover received unsupported legacy parameters"));
    }
    let (lines, category) = parse_spin_type(spin_type)?;
    let board = decode_board(&field)?;
    Ok(vec![
        "clearra".to_owned(),
        "spin-finder".to_owned(),
        "--board-mask".to_owned(),
        format!("0x{:x}", board.occupied_mask()),
        "--height".to_owned(),
        board.visible_height().max(8).to_string(),
        "--patterns".to_owned(),
        normalize_sfinder_pattern(&pattern)?,
        "--rule".to_owned(),
        "jstris-180".to_owned(),
        "--spin-profile".to_owned(),
        "t-spins".to_owned(),
        "--spin-category".to_owned(),
        category.to_owned(),
        "--lines".to_owned(),
        lines.to_owned(),
    ])
}

fn translate_cat_finder(args: &[String]) -> Result<Vec<String>, WebCommandError> {
    let (field, queue, trailing) = first_field_and_pattern(args, "cat-finder")?;
    if !trailing.is_empty() {
        return Err(invalid("cat-finder received unsupported legacy parameters"));
    }
    if queue
        .chars()
        .any(|character| !"IOTSZJLioszjlt".contains(character))
    {
        return Err(invalid("cat-finder requires an exact fixed queue"));
    }
    let board = decode_board(&field)?;
    Ok(vec![
        "clearra".to_owned(),
        "damage".to_owned(),
        "--board-mask".to_owned(),
        format!("0x{:x}", board.occupied_mask()),
        "--height".to_owned(),
        board.visible_height().max(8).to_string(),
        "--queue".to_owned(),
        queue.to_ascii_uppercase(),
        "--rule".to_owned(),
        "jstris-180".to_owned(),
        "--spin-profile".to_owned(),
        "t-spins".to_owned(),
    ])
}

fn translate_setup_finder(command: &str, args: &[String]) -> Result<Vec<String>, WebCommandError> {
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
        "jstris-180".to_owned(),
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

fn parse_spin_type(value: &str) -> Result<(&'static str, &'static str), WebCommandError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "TSS" => Ok(("1", "t")),
        "TSM" => Err(not_yet_representable("spin-cover TSM")),
        "TSD" => Ok(("2", "t")),
        "TST" => Ok(("3", "t")),
        "TSPIN" | "T-SPIN" | "ANY" => Ok(("any", "t")),
        _ => Err(invalid(format!("unsupported spin-cover type '{value}'"))),
    }
}

fn decode_board(source: &CompatibilityFieldInput) -> Result<CompatibilityBoard, WebCommandError> {
    match source {
        CompatibilityFieldInput::Fumen(source) => CompatibilityBoard::from_fumen(source),
        CompatibilityFieldInput::OccupiedMask(source) => {
            parse_ctk3_field_mask(source).map(CompatibilityBoard::from_occupied_mask)
        }
    }
}

fn target_mask(lines: u8) -> Result<u64, WebCommandError> {
    if !(1..=6).contains(&lines) {
        return Err(invalid("clear lines must be in 1..=6"));
    }
    let bits = u32::from(lines) * 10;
    Ok((1u64 << bits) - 1)
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
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_fumen::{
        ColoredSolutionFumenExporter, ColoredSolutionPage, ColoredSolutionPlacement,
    };

    use crate::WebCommandParser;

    use super::{normalize_sfinder_pattern, translate_command};

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
            "spin-cover",
            "v115@vhAAgH",
            "TI",
            "TSS",
            "--cpu-threads",
            "2",
        ]
        .map(str::to_owned);

        let translated = translate_command(&input).expect("legacy spin-cover translation");

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
    fn compatibility_rejects_t_spin_mini_until_the_target_contract_can_distinguish_it() {
        let input = [
            "clearra",
            "sfinder",
            "spin-cover",
            "v115@vhAAgH",
            "TI",
            "TSM",
        ]
        .map(str::to_owned);

        let error = translate_command(&input).expect_err("TSM must not become TSS");
        assert_eq!(error.code(), crate::WebCommandErrorCode::UnsupportedCommand);
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

        for (command, next, trailing) in [
            ("spin-cover", "I", Some("TSS")),
            ("spin", "I", Some("TSS")),
            ("cat-finder", "I", None),
        ] {
            let mut input = vec![
                "clearra".to_owned(),
                "sfinder".to_owned(),
                command.to_owned(),
                "--field-mask-v1".to_owned(),
                "000000000000000f".to_owned(),
                "--patterns".to_owned(),
                next.to_owned(),
            ];
            if let Some(value) = trailing {
                input.push(value.to_owned());
            }
            let translated = translate_command(&input).expect("colorless forward translation");
            assert!(translated
                .windows(2)
                .any(|pair| pair == ["--board-mask", "0xf"]));
        }
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
            "clearra build-probability --base-mask 0x300 --target-mask 0xff --height 1 --patterns II --rule jstris-180 --no-mirror --no-hold",
        )
        .expect("native build-probability request");
        assert_eq!(cover_request, native_request);
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
