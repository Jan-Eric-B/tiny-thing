//! The Tray shell: a Tauri app with an icon in the notification area and no
//! window (so no taskbar entry either).

use crate::forwarding::{Stopped, forward_to_device};
use std::net::SocketAddr;
use std::time::Duration;
use tauri::async_runtime::{self, JoinHandle};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{App, AppHandle};
use tokio::sync::oneshot;

const QUIT: &str = "quit";
const TOOLTIP: &str = "TinyThing";

/// How long Quit waits for forwarding to remove its Windows event handlers.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// Runs the Tray app until Quit is chosen from the tray icon's menu, then
/// stops forwarding to the Device and returns the exit code Tauri reports.
///
/// When forwarding stops before that, the tray icon's tooltip says why: the
/// Tray app has no window, and a release build no console.
pub fn run(device: SocketAddr) -> tauri::Result<i32> {
    let app = tauri::Builder::default().build(tauri::generate_context!())?;
    let tray_icon = add_tray_icon(&app)?;
    let (quit, quit_requested) = oneshot::channel::<()>();
    let forwarding = async_runtime::spawn(forward_to_device(device, quit_requested));
    let shown = async_runtime::spawn(show_when_stopped(forwarding, tray_icon));

    let exit_code = app.run_return(|_, _| {});

    drop(quit);
    // The timer must be created inside the runtime, so in the async block.
    let stopped =
        async_runtime::block_on(async { tokio::time::timeout(STOP_TIMEOUT, shown).await });
    if stopped.is_err() {
        tracing::warn!("forwarding did not stop within {STOP_TIMEOUT:?}");
    }
    Ok(exit_code)
}

fn add_tray_icon(app: &App) -> tauri::Result<TrayIcon> {
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;
    let mut tray_icon = TrayIconBuilder::with_id("tiny-thing")
        .tooltip(TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app: &AppHandle, event| {
            if event.id().as_ref() == QUIT {
                app.exit(0);
            }
        });
    match app.default_window_icon() {
        Some(icon) => tray_icon = tray_icon.icon(icon.clone()),
        None => tracing::warn!("no app icon bundled; the tray icon stays blank"),
    }
    tray_icon.build(app)
}

/// Waits for forwarding to end and, unless Quit ended it, logs why and puts
/// the reason into the tray icon's tooltip.
async fn show_when_stopped(forwarding: JoinHandle<Result<(), Stopped>>, tray_icon: TrayIcon) {
    let reason = match forwarding.await {
        Ok(Ok(())) => return,
        Ok(Err(stopped)) => {
            tracing::error!(error = %stopped, "forwarding to the Device stopped");
            stopped.reason()
        }
        Err(error) => {
            tracing::error!(%error, "forwarding to the Device failed");
            "stopped after an internal error"
        }
    };
    let tooltip = format!("{TOOLTIP}: {reason}, restart to retry");
    if let Err(error) = tray_icon.set_tooltip(Some(tooltip)) {
        tracing::warn!(%error, "cannot show why forwarding stopped");
    }
}
