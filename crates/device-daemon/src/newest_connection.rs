//! The accept loop both listeners share: the newest connection wins.

use std::fmt::Display;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::task::AbortOnDropHandle;

/// How long to wait before accepting again after `accept` failed, so a
/// persistent failure (such as running out of file handles) does not spin.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Accepts connections on `listener` for as long as it runs and serves each
/// one with `handle` in its own task. A new connection ends the previous one.
///
/// There is exactly one Tray app and one Device app, so a new connection means
/// the old peer is gone, even when its connection still looks open: a USB
/// unplug or a sleeping computer leaves a half-open connection behind that
/// would otherwise block every reconnect until TCP gives up on it.
///
/// Dropping the returned future ends the current connection as well.
pub(crate) async fn serve<H, C, E>(listener: TcpListener, peer_kind: &'static str, mut handle: H)
where
    H: FnMut(TcpStream) -> C,
    C: Future<Output = Result<(), E>> + Send + 'static,
    E: Display,
{
    let mut current: Option<AbortOnDropHandle<()>> = None;
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                if current
                    .take()
                    .is_some_and(|previous| !previous.is_finished())
                {
                    tracing::info!(%peer, "a new {peer_kind} connection replaces the previous one");
                }
                tracing::info!(%peer, "{peer_kind} connected");
                let connection = handle(stream);
                current = Some(AbortOnDropHandle::new(tokio::spawn(async move {
                    match connection.await {
                        Ok(()) => tracing::info!(%peer, "{peer_kind} closed the connection"),
                        Err(error) => tracing::warn!(%peer, %error, "{peer_kind} connection ended"),
                    }
                })));
            }
            Err(error) => {
                tracing::warn!(%error, "accepting a {peer_kind} connection failed");
                tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
            }
        }
    }
}
