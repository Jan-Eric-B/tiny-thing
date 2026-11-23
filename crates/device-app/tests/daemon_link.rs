//! The Device app's end of the local socket, against a stand-in Device
//! daemon.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_app::daemon_link;
use protocol::local::{AppMessage, DaemonMessage};
use protocol::{InputAction, MessageReader, MessageWriter};
use std::net::SocketAddr;
use std::ops::ControlFlow;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const PATIENCE: Duration = Duration::from_secs(5);

#[tokio::test]
async fn input_actions_reach_the_device_daemon_in_order() {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let (actions, pending_actions) = mpsc::channel(8);
    let (received, mut messages) = mpsc::unbounded_channel();
    tokio::spawn(daemon_link::run(
        listener.local_addr().unwrap(),
        pending_actions,
        move |message| match received.send(message) {
            Ok(()) => ControlFlow::Continue(()),
            Err(_) => ControlFlow::Break(()),
        },
    ));
    let (read, write) = listener.accept().await.unwrap().0.into_split();
    let mut reader = MessageReader::<_, AppMessage>::new(read);
    let mut writer = MessageWriter::<_, DaemonMessage>::new(write);

    // Once a message has come through, the link is fully up.
    writer.send(&DaemonMessage::NowPlaying(None)).await.unwrap();
    let first = tokio::time::timeout(PATIENCE, messages.recv()).await;
    assert_eq!(first.unwrap(), Some(DaemonMessage::NowPlaying(None)));

    actions.send(InputAction::PlayPause).await.unwrap();
    actions.send(InputAction::Next).await.unwrap();

    for expected in [InputAction::PlayPause, InputAction::Next] {
        let message = tokio::time::timeout(PATIENCE, reader.recv()).await;
        assert_eq!(message.unwrap().unwrap(), Some(AppMessage::Input(expected)));
    }
}

#[tokio::test]
async fn input_actions_from_while_the_device_daemon_was_away_are_dropped() {
    // A free port with nothing listening on it yet.
    let address = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    let (actions, pending_actions) = mpsc::channel(8);
    let (received, mut messages) = mpsc::unbounded_channel();
    tokio::spawn(daemon_link::run(
        address,
        pending_actions,
        move |message| match received.send(message) {
            Ok(()) => ControlFlow::Continue(()),
            Err(_) => ControlFlow::Break(()),
        },
    ));
    actions.send(InputAction::PlayPause).await.unwrap();
    actions.send(InputAction::Previous).await.unwrap();

    // The Device daemon comes up.
    let listener = TcpListener::bind(address).await.unwrap();
    let (read, write) = tokio::time::timeout(PATIENCE, listener.accept())
        .await
        .unwrap()
        .unwrap()
        .0
        .into_split();
    let mut reader = MessageReader::<_, AppMessage>::new(read);
    let mut writer = MessageWriter::<_, DaemonMessage>::new(write);
    writer.send(&DaemonMessage::NowPlaying(None)).await.unwrap();
    let first = tokio::time::timeout(PATIENCE, messages.recv()).await;
    assert_eq!(first.unwrap(), Some(DaemonMessage::NowPlaying(None)));

    actions.send(InputAction::Next).await.unwrap();

    let message = tokio::time::timeout(PATIENCE, reader.recv()).await;
    assert_eq!(
        message.unwrap().unwrap(),
        Some(AppMessage::Input(InputAction::Next)),
        "only the press made while connected arrives"
    );
}
