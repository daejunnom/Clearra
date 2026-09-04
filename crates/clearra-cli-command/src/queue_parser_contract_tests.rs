use std::{fs, path::PathBuf};

use clearra_supply::queue::{
    queue_parser::parse_fixed_sequence, queue_pattern_expression::QueuePatternExpression,
};

#[test]
fn rust_production_queue_parsers_match_the_shared_ts_corpus() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/queue_parser_contract.tsv");
    let source = fs::read_to_string(path).expect("shared queue parser corpus");

    for line in source
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let columns = line.split('\t').collect::<Vec<_>>();
        assert_eq!(columns.len(), 6, "contract column count: {line}");
        let input = columns[0];
        let valid = columns[1] == "true";
        let canonical = (columns[2] != "-").then_some(columns[2]);
        let kind = columns[3];
        let sequence_len =
            (columns[4] != "-").then(|| columns[4].parse::<usize>().expect("sequence length"));
        let expected_error = (columns[5] != "-").then_some(columns[5]);

        let observed = match kind {
            "fixed" => parse_fixed_sequence(input)
                .map(|queue| {
                    let source = queue
                        .pieces()
                        .iter()
                        .map(|piece| piece.as_ascii())
                        .collect();
                    (source, queue.len())
                })
                .map_err(|_| "invalid"),
            "pattern" => QueuePatternExpression::parse(input, 5_764_801)
                .map(|pattern| (pattern.source().to_owned(), pattern.sequence_len()))
                .map_err(|_| "invalid"),
            value => panic!("unknown queue corpus kind {value}"),
        };

        assert_eq!(observed.is_ok(), valid, "{input}");
        match observed {
            Ok((observed_canonical, observed_len)) => {
                assert_eq!(Some(observed_canonical.as_str()), canonical, "{input}");
                assert_eq!(Some(observed_len), sequence_len, "{input}");
                assert_eq!(expected_error, None, "{input}");
            }
            Err(error) => assert_eq!(Some(error), expected_error, "{input}"),
        }
    }
}
