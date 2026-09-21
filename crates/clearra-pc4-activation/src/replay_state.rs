use crate::{
    error::{ActivationError, Result},
    signed_envelope::require_identity,
    VerifiedRolloutPointer,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDecision {
    Initial,
    Advanced,
    Idempotent,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReplayState {
    highest_sequence: Option<u64>,
    pointer_identity: Option<String>,
    selected_generation_identity: Option<String>,
}

impl ReplayState {
    pub const fn empty() -> Self {
        Self {
            highest_sequence: None,
            pointer_identity: None,
            selected_generation_identity: None,
        }
    }

    pub fn from_persisted(
        highest_sequence: u64,
        pointer_identity: String,
        selected_generation_identity: String,
    ) -> Result<Self> {
        if highest_sequence == 0 {
            return Err(ActivationError::new("pc4_activation_replay_state"));
        }
        require_identity(&pointer_identity, "pc4_activation_replay_state")?;
        require_identity(&selected_generation_identity, "pc4_activation_replay_state")?;
        Ok(Self {
            highest_sequence: Some(highest_sequence),
            pointer_identity: Some(pointer_identity),
            selected_generation_identity: Some(selected_generation_identity),
        })
    }

    pub const fn highest_sequence(&self) -> Option<u64> {
        self.highest_sequence
    }

    pub fn pointer_identity(&self) -> Option<&str> {
        self.pointer_identity.as_deref()
    }

    pub fn selected_generation_identity(&self) -> Option<&str> {
        self.selected_generation_identity.as_deref()
    }

    pub(crate) fn preview(
        &self,
        pointer: &VerifiedRolloutPointer,
        now_unix_seconds: u64,
        bootstrap_min_sequence: u64,
    ) -> Result<(ReplayDecision, Self)> {
        if now_unix_seconds < pointer.issued_at_unix_seconds()
            || now_unix_seconds >= pointer.expires_at_unix_seconds()
        {
            return Err(ActivationError::new("pc4_activation_rollout_expired"));
        }
        if pointer.sequence() < bootstrap_min_sequence {
            return Err(ActivationError::new(
                "pc4_activation_rollout_below_bootstrap",
            ));
        }
        let next = Self {
            highest_sequence: Some(pointer.sequence()),
            pointer_identity: Some(pointer.pointer_identity().to_owned()),
            selected_generation_identity: Some(pointer.selected_generation_identity().to_owned()),
        };
        let Some(current_sequence) = self.highest_sequence else {
            return Ok((ReplayDecision::Initial, next));
        };
        if pointer.sequence() < current_sequence {
            return Err(ActivationError::new("pc4_activation_rollout_replay"));
        }
        if pointer.sequence() == current_sequence {
            if self.pointer_identity() == Some(pointer.pointer_identity())
                && self.selected_generation_identity()
                    == Some(pointer.selected_generation_identity())
            {
                return Ok((ReplayDecision::Idempotent, self.clone()));
            }
            return Err(ActivationError::new("pc4_activation_rollout_equivocation"));
        }
        if pointer.sequence() != current_sequence.saturating_add(1)
            || pointer.previous_pointer_identity() != self.pointer_identity()
        {
            return Err(ActivationError::new("pc4_activation_rollout_chain_gap"));
        }
        let current_generation = self
            .selected_generation_identity()
            .ok_or(ActivationError::new("pc4_activation_replay_state"))?;
        if !pointer
            .retained_generations()
            .iter()
            .any(|entry| entry.generation_identity() == current_generation)
        {
            return Err(ActivationError::new("pc4_activation_rollback_history"));
        }
        Ok((ReplayDecision::Advanced, next))
    }
}
