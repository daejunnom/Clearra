fn main() {
    #[cfg(feature = "wasm-cpu-runtime")]
    let _native_build_probability_registration =
        clearra_app::register_system_native_build_probability_host();
    std::process::exit(clearra_cli::run());
}
