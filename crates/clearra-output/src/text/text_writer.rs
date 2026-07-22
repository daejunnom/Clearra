#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextWriter;

impl TextWriter {
    pub fn line(label: &str, value: impl std::fmt::Display) -> String {
        format!("{label}: {value}")
    }
}
impl TextWriter {
    pub fn lines(lines: &[String]) -> String {
        lines.join("\n")
    }
}
impl TextWriter {
    pub fn replay_trace_lines(trace: &clearra_replay::ReplayTrace) -> Vec<String> {
        let mut lines = vec![
            Self::line("kind", "replay-trace"),
            Self::line("variant_id", trace.variant_id()),
            Self::line("representative", trace.representative()),
            Self::line("sample", trace.sample()),
            Self::line("trace_steps", trace.trace_steps()),
            Self::line("canonical_key", trace.canonical_key()),
            Self::line(
                "colored_cells",
                trace.colored_cell_ownership().owned_cell_count(),
            ),
        ];
        for step in trace.solution_trace().steps() {
            let placement = step.placement();
            lines.push(Self::line(
                &format!("step_{}", step.step_index()),
                format!(
                    "piece={} rotation={} x={} y={} cleared_lines={}",
                    placement.piece_kind().as_ascii(),
                    placement.rotation().quarter_turns(),
                    placement.x(),
                    placement.y(),
                    step.line_clear().cleared_lines()
                ),
            ));
        }
        lines
    }
}
impl TextWriter {
    pub fn replay_trace(trace: &clearra_replay::ReplayTrace) -> String {
        Self::lines(&Self::replay_trace_lines(trace))
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};

    use super::*;

    #[test]
    fn replay_trace_renders_to_text() {
        let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
        let input = BuildVariantReplayInput::new(
            "variant-1",
            layout,
            0x003f,
            vec![BuildVariantOperation::new(
                PieceKind::I,
                RotationState::Zero,
                6,
                0,
            )],
        );
        let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

        let rendered = TextWriter::replay_trace(&trace);

        assert!(rendered.contains("kind: replay-trace"));
        assert!(rendered.contains("variant_id: variant-1"));
        assert!(rendered.contains("representative: true"));
        assert!(rendered.contains("sample: true"));
        assert!(rendered.contains("cleared_lines=1"));
    }
}
