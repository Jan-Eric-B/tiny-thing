fn main() {
    // The Tauri shell exists on Windows only so far (see Cargo.toml). A build
    // script runs on the host, so the target comes from Cargo, not `cfg`.
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        tauri_build::build();
    }
}
