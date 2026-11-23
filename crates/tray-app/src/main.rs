//! The Tray app: the Tray core with the Tauri shell and the platform
//! adapters. Windows only so far; Linux and macOS follow behind the same
//! interfaces.

// A release build starts without a console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod forwarding;
#[cfg(windows)]
mod shell;
#[cfg(windows)]
mod windows_media;

use std::process::ExitCode;

#[cfg(windows)]
#[derive(Debug, clap::Parser)]
#[command(version)]
struct Args {
    /// Address of the Device daemon: localhost for the Simulator.
    #[arg(long, default_value_t = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, protocol::DEVICE_PORT)))]
    device: std::net::SocketAddr,
}

#[cfg(windows)]
fn main() -> ExitCode {
    use clap::Parser;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();
    tracing::info!(version = %tray_core::TRAY_APP_VERSION, device = %args.device, "Tray app starting");

    match shell::run(args.device) {
        Ok(code) => {
            tracing::info!(code, "Tray app quit");
            u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
        }
        Err(error) => {
            tracing::error!(%error, "the Tray app failed");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("The TinyThing Tray app runs on Windows only so far.");
    ExitCode::FAILURE
}
