use crate::{
    args::CliParser,
    cli_routing::route_invocation,
    output::{CliOutput, CliOutputDispatcher},
};

pub fn run() -> i32 {
    CliOutputDispatcher::dispatch(&run_with_args(std::env::args()))
}

pub fn run_with_args<I, S>(args: I) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    match CliParser::parse(args) {
        Ok(invocation) => route_invocation(invocation),
        Err(error) => error.into_output(),
    }
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "cli_entry_tests.rs"]
mod tests;
