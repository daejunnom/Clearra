use super::GpuCpuExactConfirmReport;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GpuCoverageBitsetOrError {
    CpuConfirmRequired,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuCoverageBitsetOrHelper;

impl GpuCoverageBitsetOrHelper {
    pub fn union_confirmed(
        confirm_report: &GpuCpuExactConfirmReport,
    ) -> Result<u128, GpuCoverageBitsetOrError> {
        if !confirm_report.gpu_result_cpu_confirmed()
            || !confirm_report.cpu_reference_and_gpu_result_match()
        {
            return Err(GpuCoverageBitsetOrError::CpuConfirmRequired);
        }

        Ok(confirm_report
            .confirmed_candidates()
            .iter()
            .fold(0u128, |union, candidate| union | candidate.coverage_bits()))
    }
}
