use clearra_pc4_qualifier::{
    generate_conditioned_local_relation, prove_conditioned_local_candidate_coverage,
    ConditionedLocalCoverageProofOptions, ConditionedLocalRelationGenerationOptions,
};
use clearra_rules::kicks::KickTableProfileId;
use std::{env, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "prove") {
        return run_proof(&args[1..]);
    }
    let mut profile = None;
    let mut queries = None;
    let mut pack = None;
    let mut catalog = None;
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1).ok_or_else(usage)?;
        match args[index].as_str() {
            "--profile" if profile.is_none() => profile = KickTableProfileId::parse(value),
            "--queries" if queries.is_none() => queries = Some(PathBuf::from(value)),
            "--pack" if pack.is_none() => pack = Some(PathBuf::from(value)),
            "--catalog" if catalog.is_none() => catalog = Some(PathBuf::from(value)),
            _ => return Err(usage()),
        }
        index += 2;
    }
    generate_conditioned_local_relation(&ConditionedLocalRelationGenerationOptions {
        profile: profile.ok_or_else(usage)?,
        queries: queries.ok_or_else(usage)?,
        pack: pack.ok_or_else(usage)?,
        catalog: catalog.ok_or_else(usage)?,
    })
}

fn run_proof(args: &[String]) -> Result<(), String> {
    let mut profile = None;
    let mut pack = None;
    let mut request = None;
    let mut report = None;
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1).ok_or_else(proof_usage)?;
        match args[index].as_str() {
            "--profile" if profile.is_none() => profile = KickTableProfileId::parse(value),
            "--pack" if pack.is_none() => pack = Some(PathBuf::from(value)),
            "--request" if request.is_none() => request = Some(PathBuf::from(value)),
            "--report" if report.is_none() => report = Some(PathBuf::from(value)),
            _ => return Err(proof_usage()),
        }
        index += 2;
    }
    prove_conditioned_local_candidate_coverage(&ConditionedLocalCoverageProofOptions {
        profile: profile.ok_or_else(proof_usage)?,
        pack: pack.ok_or_else(proof_usage)?,
        request: request.ok_or_else(proof_usage)?,
        report: report.ok_or_else(proof_usage)?,
    })
}

fn usage() -> String {
    format!(
        "usage: clearra-conditioned-local-relation --profile PROFILE --queries ABSOLUTE_JSON --pack ABSOLUTE_CLLR --catalog ABSOLUTE_JSON\nqueries accepts an explicit query-set.v1 or bounded cover-set.v1 source; both remain unqualified candidates\n{}",
        proof_usage()
    )
}

fn proof_usage() -> String {
    "usage: clearra-conditioned-local-relation prove --profile PROFILE --pack ABSOLUTE_CLLR --request ABSOLUTE_JSON --report ABSOLUTE_JSON".to_owned()
}
