use clearra_pc4_qualifier::{
    generate_conditioned_reachability, ConditionedReachabilityGenerationOptions,
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
    let mut profile = None;
    let mut queries = None;
    let mut pack = None;
    let mut catalog = None;
    let mut workers = None;
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1).ok_or_else(usage)?;
        match args[index].as_str() {
            "--profile" if profile.is_none() => profile = KickTableProfileId::parse(value),
            "--queries" if queries.is_none() => queries = Some(PathBuf::from(value)),
            "--pack" if pack.is_none() => pack = Some(PathBuf::from(value)),
            "--catalog" if catalog.is_none() => catalog = Some(PathBuf::from(value)),
            "--workers" if workers.is_none() => {
                workers = Some(value.parse::<usize>().map_err(|_| usage())?)
            }
            _ => return Err(usage()),
        }
        index += 2;
    }
    generate_conditioned_reachability(&ConditionedReachabilityGenerationOptions {
        profile: profile.ok_or_else(usage)?,
        queries: queries.ok_or_else(usage)?,
        pack: pack.ok_or_else(usage)?,
        catalog: catalog.ok_or_else(usage)?,
        workers: workers.ok_or_else(usage)?,
    })
}

fn usage() -> String {
    "usage: clearra-conditioned-reachability --profile PROFILE --queries ABSOLUTE_JSON --pack ABSOLUTE_CLBR --catalog ABSOLUTE_JSON --workers N".to_owned()
}
