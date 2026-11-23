//! Runs the Tray core with a fake `MediaSource` that reports one fixed Now
//! Playing, for trying the Simulator without a media player. Input actions
//! from the Device are logged but change nothing, so an optimistic play/pause
//! on the Device falls back after a moment:
//!
//! ```text
//! cargo run -p device-daemon
//! cargo run -p device-app
//! cargo run -p tray-core --example fake-now-playing -- --title "Teardrop" --artist "Massive Attack"
//! ```

use clap::Parser;
use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use tray_core::Session;
use tray_core::media::{FakeMediaSource, NowPlaying, PlaybackState};

#[derive(Debug, Parser)]
struct Args {
    /// Address of the Device daemon.
    #[arg(long, default_value_t = SocketAddr::from((Ipv4Addr::LOCALHOST, protocol::DEVICE_PORT)))]
    device: SocketAddr,

    /// Title to report as Now Playing.
    #[arg(long, default_value = "Teardrop")]
    title: String,

    /// Artist to report as Now Playing.
    #[arg(long, default_value = "Massive Attack")]
    artist: String,

    /// Report Now Playing as paused instead of playing.
    #[arg(long)]
    paused: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    let media = FakeMediaSource::new();
    media.set(Some(NowPlaying {
        title: args.title,
        artist: args.artist,
        playback: if args.paused {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        },
    }));

    let session = match Session::connect(args.device).await {
        Ok(session) => session,
        Err(error) => {
            tracing::error!(device = %args.device, %error, "cannot start a session");
            return ExitCode::FAILURE;
        }
    };

    tokio::select! {
        result = session.run(&media) => match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(%error, "session ended");
                ExitCode::FAILURE
            }
        },
        _ = tokio::signal::ctrl_c() => ExitCode::SUCCESS,
    }
}
