use clearra_fumen::{SourceFumenBoard, SourceFumenColoredFieldSet};

use crate::{WebCommandError, WebCommandErrorCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PcPreset {
    Path,
    Chance,
    Minimals,
    Score,
    ScoreMinimals,
    Saves,
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
    fumen: String,
    pattern: String,
    clear: u8,
    hold: bool,
}

fn translate_pc(preset: PcPreset, args: &[String]) -> Result<Vec<String>, WebCommandError> {
    let input = parse_legacy_pc_input(args)?;
    let board = decode_board(&input.fumen)?;
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
    let mut fumen = None;
    let mut pattern = None;
    let mut clear = 4u8;
    let mut hold = true;
    let mut positional = Vec::new();
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-t" | "--tetfu" | "--fumen" => {
                fumen = Some(next(args, &mut cursor, "--fumen")?.to_owned());
            }
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                pattern = Some(next(args, &mut cursor, "--patterns")?.to_owned());
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

    if fumen.is_none() {
        fumen = positional.first().cloned();
    }
    if pattern.is_none() {
        pattern = positional.get(1).cloned();
    }
    if let Some(value) = positional.get(2) {
        clear = parse_lines(value)?;
    }
    if positional.len() > 3 {
        return Err(invalid(
            "this compatibility command received unsupported legacy parameters",
        ));
    }

    Ok(LegacyPcInput {
        fumen: fumen.ok_or_else(|| missing("Sfinder compatibility search requires a Fumen"))?,
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
    let (fumen, pattern, trailing) = first_two_positionals(args, command)?;
    if !trailing.is_empty() {
        return Err(invalid(format!(
            "{command} received unsupported legacy parameters"
        )));
    }
    let board = decode_board(&fumen)?;
    if board.colored_mask() == 0 {
        return Err(invalid(format!(
            "{command} requires colored target cells in its Fumen"
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
        format!("0x{:x}", board.occupied_mask()),
        "--height".to_owned(),
        board.visible_height().max(1).to_string(),
        "--patterns".to_owned(),
        normalize_sfinder_pattern(&pattern)?,
        "--rule".to_owned(),
        "jstris-180".to_owned(),
    ];
    if spin {
        push_pair(&mut output, "--aggregate", "spin");
        push_pair(&mut output, "--spin-profile", "t-spins");
    }
    Ok(output)
}

fn translate_cover(args: &[String]) -> Result<Vec<String>, WebCommandError> {
    let mut fumen = None;
    let mut pattern = None;
    let mut clear = 4u8;
    let mut hold = true;
    let mut positional = Vec::new();
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-t" | "--tetfu" | "--fumen" => {
                fumen = Some(next(args, &mut cursor, "--fumen")?.to_owned())
            }
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                pattern = Some(next(args, &mut cursor, "--patterns")?.to_owned())
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
    let fumen = fumen
        .or_else(|| positional.first().cloned())
        .ok_or_else(|| missing("cover requires a solution Fumen"))?;
    let pattern = pattern
        .or_else(|| positional.get(1).cloned())
        .ok_or_else(|| missing("cover requires a queue pattern"))?;
    if let Some(value) = positional.get(2) {
        clear = parse_lines(value)?;
    }
    if positional.len() > 3 {
        return Err(invalid("cover received unsupported legacy parameters"));
    }

    let solutions = SourceFumenColoredFieldSet::decode(&fumen)
        .map_err(|error| invalid(format!("invalid supplied solution Fumen: {error:?}")))?;
    let target = target_mask(clear)?;
    if solutions.initial_board_mask() & !target != 0 {
        return Err(invalid(
            "the supplied solution starts above the clear target",
        ));
    }
    let empty = target & !solutions.initial_board_mask();
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
        format!("0x{:x}", solutions.initial_board_mask()),
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
        "--solution-fumen".to_owned(),
        fumen,
    ];
    if !hold {
        output.push("--no-hold".to_owned());
    }
    Ok(output)
}

fn translate_spin_cover(args: &[String]) -> Result<Vec<String>, WebCommandError> {
    let (fumen, pattern, trailing) = first_two_positionals(args, "spin-cover")?;
    let spin_type = trailing.first().map(String::as_str).unwrap_or("TSS");
    if trailing.len() > 1 {
        return Err(invalid("spin-cover received unsupported legacy parameters"));
    }
    let (lines, category) = parse_spin_type(spin_type)?;
    let board = decode_board(&fumen)?;
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
    let (fumen, queue, trailing) = first_two_positionals(args, "cat-finder")?;
    if !trailing.is_empty() {
        return Err(invalid("cat-finder received unsupported legacy parameters"));
    }
    if queue
        .chars()
        .any(|character| !"IOTSZJLioszjlt".contains(character))
    {
        return Err(invalid("cat-finder requires an exact fixed queue"));
    }
    let board = decode_board(&fumen)?;
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
    let remaining = args
        .iter()
        .find(|value| !value.starts_with('-'))
        .ok_or_else(|| missing(format!("{command} requires remaining pieces")))?;
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

fn first_two_positionals(
    args: &[String],
    command: &str,
) -> Result<(String, String, Vec<String>), WebCommandError> {
    let mut values = Vec::new();
    let mut cursor = 0usize;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-t" | "--tetfu" | "--fumen" => {
                values.insert(0, next(args, &mut cursor, "--fumen")?.to_owned());
            }
            "-p" | "--pattern" | "--patterns" | "--queue" => {
                let value = next(args, &mut cursor, "--patterns")?.to_owned();
                let index = usize::from(!values.is_empty());
                values.insert(index, value);
            }
            flag if flag.starts_with('-') => {
                return Err(invalid(format!(
                    "unsupported {command} compatibility option '{flag}'"
                )));
            }
            value => {
                values.push(value.to_owned());
                cursor += 1;
            }
        }
    }
    if values.len() < 2 {
        return Err(missing(format!(
            "{command} requires a Fumen and a queue pattern"
        )));
    }
    Ok((values.remove(0), values.remove(0), values))
}

fn parse_spin_type(value: &str) -> Result<(&'static str, &'static str), WebCommandError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "TSS" | "TSM" => Ok(("1", "t")),
        "TSD" => Ok(("2", "t")),
        "TST" => Ok(("3", "t")),
        "TSPIN" | "T-SPIN" | "ANY" => Ok(("any", "t")),
        _ => Err(invalid(format!("unsupported spin-cover type '{value}'"))),
    }
}

fn decode_board(source: &str) -> Result<SourceFumenBoard, WebCommandError> {
    SourceFumenBoard::decode(source).map_err(|error| {
        invalid(format!(
            "invalid single-page field Fumen for Sfinder compatibility: {error:?}"
        ))
    })
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
    }
}
