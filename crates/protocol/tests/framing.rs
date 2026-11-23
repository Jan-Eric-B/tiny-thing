//! What a receiver sees when the byte stream is cut short, oversized, cancelled
//! halfway, or speaks another protocol version, and when the peer never
//! completes its Handshake.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use protocol::{
    DeviceMessage, FrameError, HANDSHAKE_TIMEOUT, Handshake, HandshakeError, MAX_FRAME_LEN,
    MessageReader, MessageWriter, NowPlaying, PROTOCOL_VERSION, ProtocolVersion, SoftwareVersion,
    TrayMessage,
};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::time::Instant;

/// Writes raw bytes as the peer would put them on the wire, then closes.
async fn reader_over(bytes: Vec<u8>) -> MessageReader<tokio::io::DuplexStream, TrayMessage> {
    let (mut near, far) = tokio::io::duplex(64 * 1024);
    near.write_all(&bytes).await.unwrap();
    drop(near);
    MessageReader::new(far)
}

fn frame(payload: &[u8]) -> Vec<u8> {
    let length = u32::try_from(payload.len()).unwrap();
    let mut bytes = length.to_be_bytes().to_vec();
    bytes.extend_from_slice(payload);
    bytes
}

#[tokio::test]
async fn closing_between_messages_is_a_clean_end() {
    let mut reader = reader_over(Vec::new()).await;
    assert!(matches!(reader.recv().await, Ok(None)));
}

#[tokio::test]
async fn closing_inside_the_length_prefix_is_an_error() {
    let mut reader = reader_over(vec![0, 0]).await;
    assert!(matches!(reader.recv().await, Err(FrameError::Truncated)));
}

#[tokio::test]
async fn closing_inside_the_payload_is_an_error() {
    let mut bytes = frame(&[1, 2, 3, 4]);
    bytes.truncate(6);
    let mut reader = reader_over(bytes).await;
    assert!(matches!(reader.recv().await, Err(FrameError::Truncated)));
}

#[tokio::test]
async fn closing_right_after_the_length_prefix_is_an_error() {
    let mut bytes = frame(&[1, 2, 3, 4]);
    bytes.truncate(4);
    let mut reader = reader_over(bytes).await;
    assert!(matches!(reader.recv().await, Err(FrameError::Truncated)));
}

#[tokio::test]
async fn a_receive_cancelled_halfway_through_a_frame_loses_nothing() {
    let message = TrayMessage::NowPlaying(Some(NowPlaying {
        title: "Teardrop".to_owned(),
        artist: "Massive Attack".to_owned(),
    }));
    let bytes = frame(&postcard::to_allocvec(&message).unwrap());
    let (mut near, far) = tokio::io::duplex(64 * 1024);
    let mut reader = MessageReader::<_, TrayMessage>::new(far);

    // Half the frame arrives, then the receive is dropped, as a `select!`
    // does when another branch (a heartbeat tick) wins.
    let (first, rest) = bytes.split_at(bytes.len() / 2);
    near.write_all(first).await.unwrap();
    let cancelled = tokio::time::timeout(Duration::from_millis(50), reader.recv()).await;
    assert!(cancelled.is_err(), "the frame cannot be complete yet");

    near.write_all(rest).await.unwrap();
    assert_eq!(reader.recv().await.unwrap(), Some(message));
}

#[tokio::test]
async fn a_frame_longer_than_the_limit_is_refused_before_reading_it() {
    let length = u32::try_from(MAX_FRAME_LEN + 1).unwrap();
    let mut reader = reader_over(length.to_be_bytes().to_vec()).await;
    assert!(matches!(
        reader.recv().await,
        Err(FrameError::TooLarge { length: l }) if l == MAX_FRAME_LEN + 1
    ));
}

#[tokio::test]
async fn a_payload_that_is_not_a_message_is_an_error() {
    let mut reader = reader_over(frame(&[0xFF, 0xFF, 0xFF])).await;
    assert!(matches!(reader.recv().await, Err(FrameError::Decode(_))));
}

fn our_handshake() -> Handshake {
    Handshake {
        protocol_version: PROTOCOL_VERSION,
        software_version: SoftwareVersion::new(0, 1, 0),
    }
}

#[tokio::test]
async fn a_handshake_from_another_protocol_version_is_reported_even_if_its_layout_changed() {
    // A future version may lay out the rest of its Handshake differently; only
    // the leading protocol version is guaranteed to stay where it is.
    let future = ProtocolVersion(PROTOCOL_VERSION.0 + 1);
    let mut payload = postcard::to_allocvec(&future).unwrap();
    payload.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

    let (peer, ours) = tokio::io::duplex(1024);
    let (read, write) = tokio::io::split(ours);
    let (peer_read, mut peer_write) = tokio::io::split(peer);
    peer_write.write_all(&frame(&payload)).await.unwrap();

    let mut reader = MessageReader::<_, TrayMessage>::new(read);
    let mut writer = MessageWriter::<_, DeviceMessage>::new(write);
    let result = our_handshake().exchange(&mut reader, &mut writer).await;

    assert!(
        matches!(
            result,
            Err(HandshakeError::ProtocolMismatch { ours, theirs })
                if ours == PROTOCOL_VERSION && theirs == future
        ),
        "{result:?}"
    );
    // Our own Handshake still went out, so the peer can report the mismatch too.
    let mut peer_reader = MessageReader::<_, Handshake>::new(peer_read);
    assert_eq!(peer_reader.recv().await.unwrap(), Some(our_handshake()));
}

#[tokio::test]
async fn a_peer_that_closes_before_its_handshake_is_reported() {
    let mut reader = MessageReader::<_, TrayMessage>::new(tokio::io::empty());
    let mut writer = MessageWriter::<_, DeviceMessage>::new(tokio::io::sink());
    let result = our_handshake().exchange(&mut reader, &mut writer).await;
    assert!(matches!(result, Err(HandshakeError::Closed)), "{result:?}");
}

#[tokio::test(start_paused = true)]
async fn a_peer_that_never_sends_its_handshake_times_out() {
    // The peer's end stays open but silent, like a half-open connection.
    let (_peer, ours) = tokio::io::duplex(1024);
    let (read, write) = tokio::io::split(ours);
    let mut reader = MessageReader::<_, TrayMessage>::new(read);
    let mut writer = MessageWriter::<_, DeviceMessage>::new(write);

    let started = Instant::now();
    let result = our_handshake().exchange(&mut reader, &mut writer).await;

    assert!(
        matches!(result, Err(HandshakeError::TimedOut)),
        "{result:?}"
    );
    assert_eq!(started.elapsed(), HANDSHAKE_TIMEOUT);
}
