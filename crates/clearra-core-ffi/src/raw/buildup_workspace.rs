use std::{ffi::c_void, ptr::NonNull};

use crate::{
    native::buildup_geometry_language::{
        CNativeBuildUpGeometryLanguageEdge, CNativeBuildUpGeometryLanguageEdgeV2,
        CNativeBuildUpGeometryLanguageNode, CNativeBuildUpGeometryLanguageNodeV2,
        CNativeBuildUpGeometryLanguageReport, CNativeBuildUpGeometryLanguageReportV2,
    },
    native::{CNativeBuildUpEnumerationLimits, CNativeBuildVariantBuffer},
    problem::CBuildUpProblem,
};

pub(crate) struct RawBuildUpWorkspace {
    pointer: NonNull<c_void>,
}

impl RawBuildUpWorkspace {
    pub(crate) fn create() -> Option<Self> {
        NonNull::new(crate::raw::bindings::buildup_workspace::create())
            .map(|pointer| Self { pointer })
    }

    pub(crate) fn verify_first(
        &mut self,
        problem: &CBuildUpProblem,
        output: &mut CNativeBuildVariantBuffer,
    ) -> i32 {
        crate::raw::bindings::buildup_workspace::verify_first(
            problem,
            self.pointer.as_ptr(),
            output,
        )
    }

    pub(crate) fn exists(&mut self, problem: &CBuildUpProblem) -> i32 {
        crate::raw::bindings::buildup_workspace::exists(problem, self.pointer.as_ptr())
    }

    pub(crate) fn enumerate(
        &mut self,
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpEnumerationLimits,
        output: &mut CNativeBuildVariantBuffer,
    ) -> i32 {
        crate::raw::bindings::buildup_workspace::enumerate(
            problem,
            limits,
            self.pointer.as_ptr(),
            output,
        )
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        crate::raw::bindings::buildup_workspace::retained_bytes(self.pointer.as_ptr())
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut c_void {
        self.pointer.as_ptr()
    }

    pub(crate) fn query_geometry_language(
        &mut self,
        problem: &CBuildUpProblem,
        report: &mut CNativeBuildUpGeometryLanguageReport,
    ) -> i32 {
        crate::raw::bindings::buildup_workspace::export_geometry_language(
            problem,
            self.pointer.as_ptr(),
            core::ptr::null_mut(),
            0,
            core::ptr::null_mut(),
            0,
            report,
        )
    }

    pub(crate) fn export_geometry_language(
        &mut self,
        problem: &CBuildUpProblem,
        nodes: &mut [CNativeBuildUpGeometryLanguageNode],
        edges: &mut [CNativeBuildUpGeometryLanguageEdge],
        report: &mut CNativeBuildUpGeometryLanguageReport,
    ) -> i32 {
        crate::raw::bindings::buildup_workspace::export_geometry_language(
            problem,
            self.pointer.as_ptr(),
            nodes.as_mut_ptr(),
            nodes.len(),
            edges.as_mut_ptr(),
            edges.len(),
            report,
        )
    }

    pub(crate) fn prepare_geometry_language_v2(
        &mut self,
        problem: &CBuildUpProblem,
        transition_mode: i32,
        report: &mut CNativeBuildUpGeometryLanguageReportV2,
    ) -> i32 {
        crate::raw::bindings::buildup_workspace::prepare_geometry_language_v2(
            problem,
            self.pointer.as_ptr(),
            transition_mode,
            report,
        )
    }

    pub(crate) fn copy_prepared_geometry_language_v2(
        &mut self,
        nodes: &mut [CNativeBuildUpGeometryLanguageNodeV2],
        edges: &mut [CNativeBuildUpGeometryLanguageEdgeV2],
        report: &mut CNativeBuildUpGeometryLanguageReportV2,
    ) -> i32 {
        crate::raw::bindings::buildup_workspace::copy_prepared_geometry_language_v2(
            self.pointer.as_ptr(),
            nodes.as_mut_ptr(),
            nodes.len(),
            edges.as_mut_ptr(),
            edges.len(),
            report,
        )
    }
}

impl Drop for RawBuildUpWorkspace {
    fn drop(&mut self) {
        crate::raw::bindings::buildup_workspace::release(self.pointer.as_ptr());
    }
}
