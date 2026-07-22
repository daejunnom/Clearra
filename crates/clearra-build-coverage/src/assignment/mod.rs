pub mod assignment_csp;
pub mod assignment_exact_cover;
pub mod assignment_trace;
pub mod slot_assignment;

pub use assignment_exact_cover::{AssignmentExactCoverBridge, AssignmentExactCoverResult};
pub use assignment_trace::AssignmentTrace;
pub use slot_assignment::{AssignedSlot, SlotAssignment};
