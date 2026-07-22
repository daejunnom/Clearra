use crate::{
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
    query::build_coverage_limits::BuildCoverageLimits,
    template::build_template::BuildTemplate,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildCoverageQuery {
    template: BuildTemplate,
    domains: Vec<SlotDomain>,
    constraints: Vec<SlotConstraint>,
    pattern_count: usize,
    limits: BuildCoverageLimits,
}

impl BuildCoverageQuery {
    pub fn new(
        template: BuildTemplate,
        domains: Vec<SlotDomain>,
        constraints: Vec<SlotConstraint>,
        pattern_count: usize,
        limits: BuildCoverageLimits,
    ) -> Self {
        Self {
            template,
            domains,
            constraints,
            pattern_count,
            limits,
        }
    }
}
impl BuildCoverageQuery {
    pub fn template(&self) -> &BuildTemplate {
        &self.template
    }
}
impl BuildCoverageQuery {
    pub fn domains(&self) -> &[SlotDomain] {
        &self.domains
    }
}
impl BuildCoverageQuery {
    pub fn constraints(&self) -> &[SlotConstraint] {
        &self.constraints
    }
}
impl BuildCoverageQuery {
    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }
}
impl BuildCoverageQuery {
    pub fn limits(&self) -> BuildCoverageLimits {
        self.limits
    }
}
