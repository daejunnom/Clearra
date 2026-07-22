use std::{marker::PhantomData, rc::Rc};

use crate::raw::search_profile::RawSearchProfile;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSearchProfileStage {
    pub name: String,
    pub duration_ns: u64,
    pub invocation_count: u64,
    pub work_item_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeSearchProfileError {
    AllocationFailed,
    ProfilingDisabledOrAlreadyActive,
}

pub struct NativeSearchProfileSession {
    profile: RawSearchProfile,
    _thread_bound: PhantomData<Rc<()>>,
}

impl NativeSearchProfileSession {
    pub fn start() -> Result<Self, NativeSearchProfileError> {
        let mut profile =
            RawSearchProfile::create().ok_or(NativeSearchProfileError::AllocationFailed)?;
        if !profile.start() {
            return Err(NativeSearchProfileError::ProfilingDisabledOrAlreadyActive);
        }
        Ok(Self {
            profile,
            _thread_bound: PhantomData,
        })
    }

    pub fn finish(mut self) -> Vec<NativeSearchProfileStage> {
        self.stop();
        let stage_count = self.profile.stage_count();
        (0..stage_count)
            .map(|stage| NativeSearchProfileStage {
                name: self
                    .profile
                    .stage_name(stage)
                    .unwrap_or_else(|| "unknown".to_owned()),
                duration_ns: self.profile.duration_ns(stage),
                invocation_count: self.profile.invocation_count(stage),
                work_item_count: self.profile.work_item_count(stage),
            })
            .collect()
    }

    fn stop(&mut self) {
        self.profile.stop();
    }
}
