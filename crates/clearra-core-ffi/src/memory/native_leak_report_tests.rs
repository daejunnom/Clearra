use super::NativeLeakReport;
use crate::memory::CClrMemLeakReport;

#[test]
fn native_memory_leak_report_maps_to_core_leak_report() {
    let report = NativeLeakReport::from_abi(CClrMemLeakReport {
        live_scopes: 3,
        live_allocations: 11,
        live_gpu_buffers: 2,
        pending_release_queue: 1,
        pending_gpu_buffer_releases: 4,
        released_scopes: 5,
        aborted_scopes: 0,
        double_releases: 2,
        canary_failures: 1,
        poison_detections: 3,
    });

    let core_report = report.to_core_leak_report();

    assert_eq!(core_report.live_scopes(), 3);
    assert!(!core_report.is_zero());
    assert_eq!(report.as_abi().live_gpu_buffers, 2);
}

#[test]
fn native_memory_leak_report_maps_to_diagnostic_material() {
    let report = NativeLeakReport::from_abi(CClrMemLeakReport {
        live_scopes: 3,
        live_allocations: 11,
        live_gpu_buffers: 2,
        pending_release_queue: 1,
        pending_gpu_buffer_releases: 4,
        released_scopes: 5,
        aborted_scopes: 0,
        double_releases: 2,
        canary_failures: 1,
        poison_detections: 3,
    });

    let material = report.to_diagnostic_material();

    assert_eq!(material.live_scopes, 3);
    assert_eq!(material.live_allocations, 11);
    assert_eq!(material.live_gpu_buffers, 2);
    assert_eq!(material.pending_release_queue, 1);
    assert_eq!(material.pending_gpu_buffer_releases, 4);
    assert_eq!(material.double_releases, 2);
    assert_eq!(material.canary_failures, 1);
    assert_eq!(material.poison_detections, 3);
}
