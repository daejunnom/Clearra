use clearra_pc4_qualifier::{
    generate_conditioned_local_relation, prove_conditioned_local_candidate_coverage,
    verify_v081_conditioned_product_candidate, ConditionedLocalCoverageProofOptions,
    ConditionedLocalRelationGenerationOptions,
};
use clearra_rules::kicks::KickTableProfileId;
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

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
    if args.first().is_some_and(|arg| arg == "verify-product") {
        return run_product_verification(&args[1..]);
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

fn run_product_verification(args: &[String]) -> Result<(), String> {
    let mut profile = None;
    let mut pack = None;
    let mut catalog = None;
    let mut source = None;
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1).ok_or_else(product_usage)?;
        match args[index].as_str() {
            "--profile" if profile.is_none() => profile = KickTableProfileId::parse(value),
            "--pack" if pack.is_none() => pack = Some(PathBuf::from(value)),
            "--catalog" if catalog.is_none() => catalog = Some(PathBuf::from(value)),
            "--source" if source.is_none() => source = Some(PathBuf::from(value)),
            _ => return Err(product_usage()),
        }
        index += 2;
    }
    let profile = profile.ok_or_else(product_usage)?;
    let pack = pack.ok_or_else(product_usage)?;
    let catalog = catalog.ok_or_else(product_usage)?;
    let source = source.ok_or_else(product_usage)?;
    if !pack.is_absolute()
        || !catalog.is_absolute()
        || !source.is_absolute()
        || pack == catalog
        || pack == source
        || catalog == source
    {
        return Err(product_usage());
    }
    let pack_bytes = read_bounded_regular(&pack, 16 * 1024 * 1024)?;
    let catalog_bytes = read_bounded_regular(&catalog, 512 * 1024)?;
    let source_bytes = read_bounded_regular(&source, 16 * 1024 * 1024)?;
    let verified = verify_v081_conditioned_product_candidate(
        profile,
        &pack_bytes,
        &catalog_bytes,
        &source_bytes,
    )?;
    println!(
        "conditioned_product_candidate=source_bound contexts={} records={} bytes={} proof_nodes={} release_authority=false",
        verified.supported_contexts,
        verified.bounded_cover.candidate.record_count,
        verified.bounded_cover.candidate.pack_bytes,
        verified.bounded_cover.visited_proof_nodes,
    );
    Ok(())
}

fn read_bounded_regular(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > maximum as u64
    {
        return Err("conditioned candidate input is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(metadata.len() as usize)
        .map_err(|_| "conditioned candidate input allocation failed")?;
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > maximum {
        return Err("conditioned candidate input changed size during read".into());
    }
    Ok(bytes)
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
        "usage: clearra-conditioned-local-relation --profile PROFILE --queries ABSOLUTE_JSON --pack ABSOLUTE_CLLR --catalog ABSOLUTE_JSON\nqueries accepts query-set.v1, bounded cover-set.v1, or solver-cover-set.v1; all remain unqualified candidates\n{}\n{}",
        product_usage(),
        proof_usage()
    )
}

fn product_usage() -> String {
    "usage: clearra-conditioned-local-relation verify-product --profile PROFILE --pack ABSOLUTE_CLLR --catalog ABSOLUTE_JSON --source ABSOLUTE_SOLVER_COVER_JSON".to_owned()
}

fn proof_usage() -> String {
    "usage: clearra-conditioned-local-relation prove --profile PROFILE --pack ABSOLUTE_CLLR --request ABSOLUTE_JSON --report ABSOLUTE_JSON".to_owned()
}
