use super::diagnostic_code::DiagnosticCode;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNamespace {
    Area,
    Backend,
    Board,
    Build,
    Core,
    Frontend,
    Gui,
    Memory,
    Objective,
    Pc,
    Piece,
    Rule,
    Render,
    Score,
    Setup,
    Spin,
    Supply,
    TwoLine,
}

impl DiagnosticCode {
    pub fn namespace(self) -> DiagnosticNamespace {
        let code = self.as_str();
        if code.contains("_AREA_") {
            DiagnosticNamespace::Area
        } else if code.contains("_BACKEND_")
            || code.starts_with("E_BACKEND")
            || code.starts_with("W_BACKEND")
        {
            DiagnosticNamespace::Backend
        } else if code.contains("_BOARD_") {
            DiagnosticNamespace::Board
        } else if code.contains("_BUILD") {
            DiagnosticNamespace::Build
        } else if code.starts_with("E_CORE") {
            DiagnosticNamespace::Core
        } else if code.contains("_FRONTEND_") {
            DiagnosticNamespace::Frontend
        } else if code.contains("_GUI_") {
            DiagnosticNamespace::Gui
        } else if code.contains("_MEMORY") || code.contains("_C_MEMORY") {
            DiagnosticNamespace::Memory
        } else if code.contains("_RENDER_") {
            DiagnosticNamespace::Render
        } else if code.contains("_OBJECTIVE") {
            DiagnosticNamespace::Objective
        } else if code.contains("_PC_") {
            DiagnosticNamespace::Pc
        } else if code.contains("_PIECE") || code.contains("_BAG") {
            DiagnosticNamespace::Piece
        } else if code.contains("_RULE") || code.contains("_KICK") {
            DiagnosticNamespace::Rule
        } else if code.contains("_SCORE") {
            DiagnosticNamespace::Score
        } else if code.contains("_SETUP") {
            DiagnosticNamespace::Setup
        } else if code.contains("_SPIN") {
            DiagnosticNamespace::Spin
        } else if code.contains("_SUPPLY") {
            DiagnosticNamespace::Supply
        } else {
            DiagnosticNamespace::TwoLine
        }
    }
}
