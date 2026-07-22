use clearra_pc_graph::request::PcCountPolicy;

pub(super) fn count_policy(policy: Option<&str>) -> Result<PcCountPolicy, String> {
    match policy.unwrap_or("count-all") {
        "first-solution" | "first" => Ok(PcCountPolicy::FirstSolution),
        "count-all" | "all" => Ok(PcCountPolicy::CountAll),
        "count-unique" | "unique" => Ok(PcCountPolicy::CountUnique),
        other => Err(format!("unsupported count_policy '{other}'")),
    }
}
