use clap::Parser;
use device_app::screen::Screen;
use device_app::{AppWindow, daemon_link};
use protocol::InputAction;
use protocol::local::DaemonMessage;
use slint::ComponentHandle;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::ControlFlow;
use std::process::ExitCode;
use tokio::sync::mpsc;

/// How many input actions may wait for the local socket. Far more than anyone
/// presses while one goes out; beyond that, presses are dropped.
const ACTION_QUEUE: usize = 32;

/// Draws the Device's screen; on the computer it runs as the Simulator.
///
/// In the Simulator, keys stand in for the Device's dial and buttons:
/// Enter is the dial press (play/pause), 1 is button 1 (previous song),
/// 2 is button 2 (play/pause) and 3 is button 3 (next song). The right and
/// left arrow keys turn the dial clockwise and counter-clockwise (Volume up
/// and down), and so does the mouse wheel. The on-screen controls work with
/// the mouse.
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
    // Messages and input actions cross over through channels.
    let (actions, pending_actions) = mpsc::channel::<InputAction>(ACTION_QUEUE);
    let (messages, mut incoming) = mpsc::unbounded_channel::<DaemonMessage>();
    let link = std::thread::Builder::new()
        .name("daemon-link".to_owned())
        .spawn(move || run_link(args.daemon_address, pending_actions, messages));
    if let Err(error) = link {
        tracing::error!(%error, "cannot start the local socket thread");
        return ExitCode::FAILURE;
    }

    let screen = Screen::new(&window, move |action| {
        if let Err(error) = actions.try_send(action) {
            tracing::warn!(?action, %error, "input action dropped");
        }
    });
    let showing = slint::spawn_local(async move {
        while let Some(message) = incoming.recv().await {
            screen.show(message);
        }
    });
    if let Err(error) = showing {
        tracing::error!(%error, "cannot show messages from the Device daemon");
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

/// Runs the local socket until the UI shuts down.
fn run_link(
    address: SocketAddr,
    actions: mpsc::Receiver<InputAction>,
    messages: mpsc::UnboundedSender<DaemonMessage>,
) {
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
    runtime.block_on(daemon_link::run(address, actions, |message| {
        match messages.send(message) {
            Ok(()) => ControlFlow::Continue(()),
            // The UI has shut down.
            Err(_) => ControlFlow::Break(()),
        }
    }));
}
