//! Runs the Tray core with a fake `MediaSource` that reports one fixed Now
//! Playing and a fake `VolumeControl`, for trying the Simulator without a
//! media player and without touching the computer's Volume. Transport
//! controls from the Device are logged but change nothing, so an optimistic
//! play/pause or seek on the Device falls back after a moment; Volume steps
//! change the fake Volume, which the Device then shows:
//!
//! ```text
//! cargo run -p device-daemon
//! cargo run -p device-app
//! cargo run -p tray-core --example fake-now-playing -- --title "Teardrop" --artist "Massive Attack" --artwork cover.jpg --duration 331 --position 40 --rate 1.5
//! ```

use clap::Parser;
use protocol::PlaybackRate;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use tokio::time::Instant;
use tray_core::Session;
use tray_core::media::{Artwork, FakeMediaSource, NowPlaying, PlaybackState, Progress};
use tray_core::volume::{FakeVolumeControl, Percent, Volume};

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

    /// Length of the song in seconds; without it, Now Playing has no
    /// progress (like a live stream).
    #[arg(long)]
    duration: Option<u64>,

    /// Position in the song at the start, in seconds.
    #[arg(long, default_value = "0")]
    position: u64,

    /// How fast the song plays, as a multiple of normal speed (above zero,
    /// at most 8).
    #[arg(long, default_value = "1", value_parser = rate)]
    rate: PlaybackRate,

    /// Image file (PNG, JPEG, BMP or WebP) to report as the Artwork.
    #[arg(long)]
    artwork: Option<PathBuf>,

    /// Volume level to start at, in percent.
    #[arg(long, default_value = "50", value_parser = percent)]
    volume: Percent,

    /// Report the Volume as muted.
    #[arg(long)]
    muted: bool,
}

/// Reads a level from 0 to 100.
fn percent(argument: &str) -> Result<Percent, String> {
    let value: u8 = argument.parse().map_err(|error| format!("{error}"))?;
    Percent::try_from(value).map_err(|error| error.to_string())
}

/// Reads a playback rate such as `1.5`.
fn rate(argument: &str) -> Result<PlaybackRate, String> {
    let value: f64 = argument.parse().map_err(|error| format!("{error}"))?;
    PlaybackRate::from_f64(value).ok_or_else(|| "not above zero and at most 8".to_owned())
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

    let artwork = match &args.artwork {
        Some(path) => match std::fs::read(path) {
            Ok(encoded) => match Artwork::from_encoded(encoded) {
                Ok(artwork) => Some(artwork),
                Err(error) => {
                    tracing::error!(path = %path.display(), %error, "not an Artwork");
                    return ExitCode::FAILURE;
                }
            },
            Err(error) => {
                tracing::error!(path = %path.display(), %error, "cannot read the Artwork");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let progress = match args.duration.filter(|&duration| duration > 0) {
        Some(duration) => {
            let duration = Duration::from_secs(duration);
            match protocol::Progress::new(
                Duration::from_secs(args.position),
                duration,
                args.rate,
                true,
            ) {
                Ok(taken) => Some(Progress {
                    taken,
                    at: Instant::now(),
                }),
                Err(error) => {
                    tracing::error!(%error, "not a song length");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => None,
    };

    let media = FakeMediaSource::new();
    media.set(Some(NowPlaying {
        title: args.title,
        artist: args.artist,
        playback: if args.paused {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        },
        artwork,
        progress,
    }));

    let volume = FakeVolumeControl::new(Volume {
        level: args.volume,
        muted: args.muted,
    });

    let session = match Session::connect(args.device).await {
        Ok(session) => session,
        Err(error) => {
            tracing::error!(device = %args.device, %error, "cannot start a session");
            return ExitCode::FAILURE;
        }
    };

    tokio::select! {
        result = session.run(&media, &volume) => match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(%error, "session ended");
                ExitCode::FAILURE
            }
        },
        _ = tokio::signal::ctrl_c() => ExitCode::SUCCESS,
    }
}
