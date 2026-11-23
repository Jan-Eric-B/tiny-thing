fn main() -> Result<(), slint_build::CompileError> {
    // Slint's testing backend finds elements through debug info; release
    // builds for the Device leave it out.
    let debug_build = std::env::var("PROFILE").is_ok_and(|profile| profile == "debug");
    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug_build);
    slint_build::compile_with_config("ui/app.slint", config)
}
