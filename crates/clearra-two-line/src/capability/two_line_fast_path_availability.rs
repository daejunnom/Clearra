use super::two_line_fallback_reason::TwoLineFallbackReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TwoLineFastPathUnavailableReason {
    TableUnavailable,
    RunnerUnavailable,
}

impl TwoLineFastPathUnavailableReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::TableUnavailable => "two_line_table_unavailable",
            Self::RunnerUnavailable => "two_line_runner_unavailable",
        }
    }
}
impl TwoLineFastPathUnavailableReason {
    pub fn fallback_reason(self) -> TwoLineFallbackReason {
        match self {
            Self::TableUnavailable => TwoLineFallbackReason::FastPathTableUnavailable,
            Self::RunnerUnavailable => TwoLineFallbackReason::FastPathRunnerUnavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TwoLineFastPathAvailability {
    table_available: bool,
    runner_available: bool,
}

impl TwoLineFastPathAvailability {
    pub fn new(table_available: bool, runner_available: bool) -> Self {
        Self {
            table_available,
            runner_available,
        }
    }
}
impl TwoLineFastPathAvailability {
    pub fn current_scope() -> Self {
        Self::mvp2_table_acceleration_excluded()
    }
}
impl TwoLineFastPathAvailability {
    pub fn mvp1() -> Self {
        Self::mvp2_table_acceleration_excluded()
    }
}
impl TwoLineFastPathAvailability {
    pub fn mvp2() -> Self {
        Self::mvp2_table_acceleration_excluded()
    }
}
impl TwoLineFastPathAvailability {
    pub fn mvp2_table_acceleration_excluded() -> Self {
        Self::new(false, false)
    }
}
impl TwoLineFastPathAvailability {
    pub fn available_for_tests() -> Self {
        Self::new(true, true)
    }
}
impl TwoLineFastPathAvailability {
    pub fn table_available(self) -> bool {
        self.table_available
    }
}
impl TwoLineFastPathAvailability {
    pub fn runner_available(self) -> bool {
        self.runner_available
    }
}
impl TwoLineFastPathAvailability {
    pub fn is_available(self) -> bool {
        self.table_available && self.runner_available
    }
}
impl TwoLineFastPathAvailability {
    pub fn unavailable_reason(self) -> Option<TwoLineFastPathUnavailableReason> {
        if !self.table_available {
            Some(TwoLineFastPathUnavailableReason::TableUnavailable)
        } else if !self.runner_available {
            Some(TwoLineFastPathUnavailableReason::RunnerUnavailable)
        } else {
            None
        }
    }
}
impl TwoLineFastPathAvailability {
    pub fn fallback_reason(self) -> Option<TwoLineFallbackReason> {
        self.unavailable_reason()
            .map(TwoLineFastPathUnavailableReason::fallback_reason)
    }
}

#[cfg(test)]
#[path = "two_line_fast_path_availability_tests.rs"]
mod tests;
