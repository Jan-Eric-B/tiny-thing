use clap::Parser;
use device_app::{AppWindow, daemon_link, screen};
use slint::ComponentHandle;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::ControlFlow;
use std::process::ExitCode;

/// Draws the Device's screen; on the computer it runs as the Simulator.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Loopback address of the Device daemon's local socket.
    #[arg(long, default_value_t = SocketAddr::from((Ipv4Addr::LOCALHOST, protocol::LOCAL_APP_PORT)))]
    daemon_address: SocketAddr,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    let window = match AppWindow::new() {
        Ok(window) => window,
        Err(error) => {
            tracing::error!(%error, "cannot create the window");
            return ExitCode::FAILURE;
        }
    };

    // The local socket runs on its own thread; the UI thread only draws.
    let weak_window = window.as_weak();
    let link = std::thread::Builder::new()
        .name("daemon-link".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    tracing::error!(%error, "cannot start the local socket runtime");
                    return;
                }
            };
            runtime.block_on(daemon_link::run(args.daemon_address, |message| {
                match weak_window
                    .upgrade_in_event_loop(move |window| screen::show(&window, message))
                {
                    Ok(()) => ControlFlow::Continue(()),
                    // The UI has shut down.
                    Err(_) => ControlFlow::Break(()),
                }
            }));
        });
    if let Err(error) = link {
        tracing::error!(%error, "cannot start the local socket thread");
        return ExitCode::FAILURE;
    }

    match window.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the window failed");
            ExitCode::FAILURE
        }
    }
}
