#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockReachabilityPolicy {
    HarddropOnly,
    LockReachability,
    SpawnAndLockReachability,
}

impl LockReachabilityPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HarddropOnly => "harddrop-only",
            Self::LockReachability => "lock-reachability",
            Self::SpawnAndLockReachability => "spawn-and-lock-reachability",
        }
    }
}
impl LockReachabilityPolicy {
    pub fn requires_lock_reachability(self) -> bool {
        !matches!(self, Self::HarddropOnly)
    }
}
impl LockReachabilityPolicy {
    pub fn requires_spawn_reachability(self) -> bool {
        matches!(self, Self::SpawnAndLockReachability)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomRuleBoardBackend {
    Board64,
    Board128,
    Wide,
}

impl CustomRuleBoardBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Board64 => "board64",
            Self::Board128 => "board128",
            Self::Wide => "wide",
        }
    }
}
impl CustomRuleBoardBackend {
    pub fn runtime_supported(self) -> bool {
        matches!(self, Self::Board64)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomRuleRuntimeFeature {
    CompactCDescriptor,
    StandardTetrominoPieces,
    Board64Search,
    Board128Search,
    WideBoardSearch,
}

impl CustomRuleRuntimeFeature {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompactCDescriptor => "compact-c-descriptor",
            Self::StandardTetrominoPieces => "standard-tetromino-pieces",
            Self::Board64Search => "board64-search",
            Self::Board128Search => "board128-search",
            Self::WideBoardSearch => "wide-board-search",
        }
    }
}
impl CustomRuleRuntimeFeature {
    pub fn runtime_supported(self) -> bool {
        matches!(
            self,
            Self::CompactCDescriptor | Self::StandardTetrominoPieces | Self::Board64Search
        )
    }
}
