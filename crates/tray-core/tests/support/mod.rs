//! Seam 1 harness: the real Device daemon on loopback ports, with a recording
//! stand-in for the Device app on its local socket.

use device_daemon::{DaemonConfig, DeviceDaemon};
use protocol::local::{AppMessage, DaemonMessage};
use protocol::{MessageReader, MessageWriter};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::task::JoinHandle;

/// How long a test waits for something that should happen at once.
pub const PATIENCE: Duration = Duration::from_secs(5);

/// A Device daemon running in the test process.
pub struct RunningDaemon {
    pub tray_address: SocketAddr,
    pub app_address: SocketAddr,
    task: JoinHandle<()>,
}

impl Drop for RunningDaemon {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub async fn start_device_daemon() -> RunningDaemon {
    let loopback_any_port = SocketAddr::from(([127, 0, 0, 1], 0));
    let daemon = DeviceDaemon::bind(&DaemonConfig {
        tray_address: loopback_any_port,
        app_address: loopback_any_port,
    })
    .await
    .unwrap();
    let tray_address = daemon.tray_address().unwrap();
    let app_address = daemon.app_address().unwrap();
    let task = tokio::spawn(async move {
        daemon.run().await;
    });
    RunningDaemon {
        tray_address,
        app_address,
        task,
    }
}

/// Stands in for the Device app: connects to the Device daemon's local socket
/// and records every message it receives.
pub struct RecordingDeviceApp {
    reader: MessageReader<OwnedReadHalf, DaemonMessage>,
    _writer: MessageWriter<OwnedWriteHalf, AppMessage>,
    received: Vec<DaemonMessage>,
}

impl RecordingDeviceApp {
    pub async fn connect(app_address: SocketAddr) -> Self {
        let (read, write) = TcpStream::connect(app_address).await.unwrap().into_split();
        Self {
            reader: MessageReader::new(read),
            _writer: MessageWriter::new(write),
            received: Vec::new(),
        }
    }

    /// Waits until `expected` arrives, failing the test after [`PATIENCE`].
    /// The match and everything received before it are used up, so a later
    /// call only sees newer messages.
    pub async fn wait_for(&mut self, expected: &DaemonMessage) {
        let found = tokio::time::timeout(PATIENCE, async {
            loop {
                if let Some(index) = self.received.iter().position(|m| m == expected) {
                    self.received.drain(..=index);
                    return;
                }
                let message = self
                    .reader
                    .recv()
                    .await
                    .unwrap()
                    .expect("the Device daemon closed the local socket");
                self.received.push(message);
            }
        })
        .await;
        assert!(
            found.is_ok(),
            "expected {expected:?}, received only {:?}",
            self.received
        );
    }

    /// Waits until the Device daemon closes the local socket, failing the test
    /// after [`PATIENCE`]. Messages still arriving before that are recorded.
    #[allow(
        dead_code,
        reason = "each test file compiles this module, not all of them use it"
    )]
    pub async fn wait_until_closed(&mut self) {
        let closed = tokio::time::timeout(PATIENCE, async {
            while let Ok(Some(message)) = self.reader.recv().await {
                self.received.push(message);
            }
        })
        .await;
        assert!(closed.is_ok(), "the local socket is still open");
    }
}
