use std::{env, fs, path::PathBuf};

use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_core_executor::compile_pc4_compact_tablebase;
use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::ProblemCompiler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: compile_pc4_compact_tablebase OUTPUT")?;
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines());
    let problem = ProblemCompiler::compile_opening_pc(&query)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let artifact = compile_pc4_compact_tablebase(&problem)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, artifact.bytes())?;
    println!(
        "certified_states={} bytes={} catalog={:016x} compiler={:016x} sha256={}",
        artifact.certified_state_count(),
        artifact.bytes().len(),
        artifact.catalog_identity(),
        artifact.compiler_identity(),
        hex(artifact.payload_sha256())
    );
    Ok(())
}

fn hex(bytes: [u8; 32]) -> String {
    bytes
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
