mod args;
mod family_artifact;
mod scenario;

const HELP: &str = "+usage: clearra-pc-family-artifact --output-dir PATH [--scenario pco-6p|tsar-cannon|empty-4l]...
       [--rule srs-plus|srs] [--count all|unique] [--backend cpu|auto] [--workers N]
       [--use-all-cpu-threads] [--cpu-warmup] [--max-patterns N] [--max-candidates N]
       [--max-frontier-states N] [--skip-fumen] [--no-fallback]

Run under clearra-manage runtime run --producer benchmark --profile benchmark-search.
Writes a Rust CPU family receipt, solution JSONL, and optional line-separated Fumen sections.
CountAll exports exact coverage over the materialized pattern universe; CountUnique omits coverage.";

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        println!("{HELP}");
        return;
    }
    let result = args::ArtifactArgs::parse(arguments).and_then(|args| {
        if std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
            || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search")
            || std::env::var_os("CLEARRA_RUNTIME_RUN_ID").is_none_or(|id| id.is_empty())
        {
            return Err("run via clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout SECONDS -- <artifact command>".to_owned());
        }
        family_artifact::run(args)
    });
    if let Err(error) = result {
        eprintln!("clearra-pc-family-artifact: {error}");
        std::process::exit(1);
    }
}
