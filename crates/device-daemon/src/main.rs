use clap::Parser;
use device_daemon::{DEVICE_DAEMON_VERSION, DaemonConfig, DeviceDaemon};
use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;

/// The always-running TinyThing background program on the Device.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Address to listen on for the Tray app: the USB network interface on a
    /// real Device, loopback for the Simulator.
    #[arg(long, default_value_t = SocketAddr::from((Ipv4Addr::LOCALHOST, protocol::DEVICE_PORT)))]
    tray_address: SocketAddr,

    /// Loopback address to listen on for the Device app.
    #[arg(long, default_value_t = SocketAddr::from((Ipv4Addr::LOCALHOST, protocol::LOCAL_APP_PORT)))]
    app_address: SocketAddr,
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

    let config = DaemonConfig {
        tray_address: args.tray_address,
        app_address: args.app_address,
    };
    let daemon = match DeviceDaemon::bind(&config).await {
        Ok(daemon) => daemon,
        Err(error) => {
            tracing::error!(%error, ?config, "cannot listen");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(
        version = %DEVICE_DAEMON_VERSION,
        tray_address = %config.tray_address,
        app_address = %config.app_address,
        "Device daemon listening"
    );

    tokio::select! {
        () = daemon.run() => ExitCode::SUCCESS,
        _ = tokio::signal::ctrl_c() => ExitCode::SUCCESS,
    }
}
