#![cfg(feature = "search-stage-profiling")]

use std::{
    ffi::{c_void, CStr},
    ptr::NonNull,
};

use crate::raw::bindings::search_profile;

pub(crate) struct RawSearchProfile {
    profile: NonNull<c_void>,
    active: bool,
}

impl RawSearchProfile {
    pub(crate) fn create() -> Option<Self> {
        NonNull::new(search_profile::create()).map(|profile| Self {
            profile,
            active: false,
        })
    }

    pub(crate) fn start(&mut self) -> bool {
        if self.active || !search_profile::start(self.profile.as_ptr()) {
            return false;
        }
        self.active = true;
        true
    }

    pub(crate) fn stop(&mut self) {
        if self.active {
            search_profile::stop(self.profile.as_ptr());
            self.active = false;
        }
    }

    pub(crate) fn stage_count(&self) -> usize {
        search_profile::stage_count()
    }

    pub(crate) fn stage_name(&self, stage: usize) -> Option<String> {
        let pointer = search_profile::stage_name(stage);
        if pointer.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(pointer) }
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }

    pub(crate) fn duration_ns(&self, stage: usize) -> u64 {
        search_profile::duration_ns(self.profile.as_ptr(), stage)
    }

    pub(crate) fn invocation_count(&self, stage: usize) -> u64 {
        search_profile::invocation_count(self.profile.as_ptr(), stage)
    }

    pub(crate) fn work_item_count(&self, stage: usize) -> u64 {
        search_profile::work_item_count(self.profile.as_ptr(), stage)
    }
}

impl Drop for RawSearchProfile {
    fn drop(&mut self) {
        self.stop();
        search_profile::release(self.profile.as_ptr());
    }
}
