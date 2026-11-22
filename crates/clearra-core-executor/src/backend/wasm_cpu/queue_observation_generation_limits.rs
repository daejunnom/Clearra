//! Optional generation budgets, separate from encoded-ID capacity.
//!
//! Product owners impose no implicit node/edge/member/work budget. Their
//! callers still enforce explicit request budgets, allocation failures,
//! cancellation and the outer runtime supervisor. A finite development
//! fixture is injected by its owner; it is never a product default.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct GenerationLimits {
    nodes: Option<usize>,
    edges: Option<usize>,
    members: Option<usize>,
    work: Option<usize>,
}

impl GenerationLimits {
    pub(super) const fn nodes(self, encoded_capacity: usize) -> usize {
        match self.nodes {
            Some(limit) if limit < encoded_capacity => limit,
            _ => encoded_capacity,
        }
    }

    pub(super) const fn edges(self, encoded_capacity: usize) -> usize {
        match self.edges {
            Some(limit) if limit < encoded_capacity => limit,
            _ => encoded_capacity,
        }
    }

    pub(super) const fn members(self) -> usize {
        match self.members {
            Some(limit) => limit,
            None => usize::MAX,
        }
    }

    pub(super) const fn work(self) -> usize {
        match self.work {
            Some(limit) => limit,
            None => usize::MAX,
        }
    }

    #[cfg(test)]
    pub(super) const DEVELOPMENT: Self = Self {
        nodes: Some(1_000_000),
        edges: Some(8_000_000),
        members: Some(8_000_000),
        work: Some(64_000_000),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_generation_has_no_implicit_development_budget() {
        let product = GenerationLimits::default();
        assert_eq!(product.nodes(u32::MAX as usize), u32::MAX as usize);
        assert_eq!(product.edges(u32::MAX as usize), u32::MAX as usize);
        assert_eq!(product.members(), usize::MAX);
        assert_eq!(product.work(), usize::MAX);
        assert!(product.nodes(u32::MAX as usize) > GenerationLimits::DEVELOPMENT.nodes(usize::MAX));
        assert!(product.members() > GenerationLimits::DEVELOPMENT.members());
        assert!(product.work() > GenerationLimits::DEVELOPMENT.work());
    }

    #[test]
    fn explicit_fixture_budget_cannot_expand_an_encoded_namespace() {
        let fixture = GenerationLimits::DEVELOPMENT;
        assert_eq!(fixture.nodes(17), 17);
        assert_eq!(fixture.nodes(usize::MAX), 1_000_000);
        assert_eq!(fixture.edges(31), 31);
        assert_eq!(fixture.edges(usize::MAX), 8_000_000);
        assert_eq!(fixture.members(), 8_000_000);
        assert_eq!(fixture.work(), 64_000_000);
    }
}
