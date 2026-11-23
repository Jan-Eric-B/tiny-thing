//! The Windows [`VolumeControl`]: the master Volume of the default output
//! device, through Core Audio (`IAudioEndpointVolume`).
//!
//! Core Audio is COM, so all of it lives on one thread of its own, which joins
//! the multithreaded apartment, owns every COM object and is the only one to
//! call them. The rest of the Tray app talks to that thread through a channel
//! of [`Command`]s.
//!
//! Event driven: Windows calls back when the default output device changes
//! (`IMMNotificationClient`) and when the followed device's Volume or mute
//! state changes (`IAudioEndpointVolumeCallback`), including changes the Tray
//! app makes itself. The callbacks only post a command; the thread then reads
//! the Volume once and publishes it if it changed. Nothing polls while Core
//! Audio works.
//!
//! When it does not (Core Audio cannot be opened, the device is invalidated,
//! the audio service restarts), the thread releases all of Core Audio, keeps
//! the last Volume and opens Core Audio again on a timer ([`retry_delay`]).
//!
//! [`core_audio`] is the only part that needs `unsafe`; [`follower`] holds
//! the logic, tested without Core Audio.

mod core_audio;
mod follower;
mod level;

use crate::retry::retry_delay;
use core_audio::CoreAudio;
use follower::{Command, Follower};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::{oneshot, watch};
use tray_core::volume::{Step, Volume, VolumeControl, VolumeError};

/// How long [`WindowsVolumeControl::start`] waits for the first reading. Core
/// Audio answers in milliseconds; this only guards against a hang.
const FIRST_READING_TIMEOUT: Duration = Duration::from_secs(2);

/// The master Volume of the computer's default output device, following the
/// default as the user switches devices.
pub struct WindowsVolumeControl {
    commands: mpsc::Sender<Command>,
    /// Shared with the Core Audio thread, and kept here so the channel stays
    /// open as long as the control lives.
    volume: Arc<watch::Sender<Option<Volume>>>,
    thread: Option<JoinHandle<()>>,
}

impl WindowsVolumeControl {
    /// Starts the Core Audio thread and returns once it has published the
    /// first Volume, so [`volume`](VolumeControl::volume) holds it from the
    /// start.
    ///
    /// Never fails: without an output device the Volume is `None`, and when
    /// Core Audio fails the thread tries again. If the thread cannot even be
    /// started, the Volume stays unknown and every step fails; Now Playing
    /// does not depend on it.
    pub async fn start() -> Self {
        let (commands, inbox) = mpsc::channel();
        let volume = Arc::new(watch::Sender::new(None));
        let (first_reading, first_read) = oneshot::channel();
        let events = commands.clone();
        let published = Arc::clone(&volume);
        let spawned = std::thread::Builder::new()
            .name("core-audio".to_owned())
            .spawn(move || {
                let open = move || CoreAudio::open(events.clone());
                Follower::new(open, published, retry_delay).serve(&inbox, first_reading);
            });
        let thread = match spawned {
            Ok(thread) => Some(thread),
            Err(error) => {
                tracing::error!(%error, "cannot start the Core Audio thread; no Volume on the Device");
                None
            }
        };
        // Ends early when the thread does; the Volume is then unknown.
        if tokio::time::timeout(FIRST_READING_TIMEOUT, first_read)
            .await
            .is_err()
        {
            tracing::warn!("no Volume from Core Audio within {FIRST_READING_TIMEOUT:?}");
        }
        Self {
            commands,
            volume,
            thread,
        }
    }

    /// Stops following and waits until the callbacks are removed from
    /// Windows. Dropping the control stops following too, but without
    /// waiting.
    pub async fn stop(mut self) {
        // Fails only when the thread has ended already.
        let _sent = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take()
            && tokio::task::spawn_blocking(move || thread.join())
                .await
                .is_err()
        {
            tracing::debug!("cannot wait for the Core Audio thread to end");
        }
    }
}

impl Drop for WindowsVolumeControl {
    fn drop(&mut self) {
        // Fails only when the thread has ended already (after `stop`).
        let _sent = self.commands.send(Command::Stop);
    }
}

impl VolumeControl for WindowsVolumeControl {
    fn volume(&self) -> watch::Receiver<Option<Volume>> {
        self.volume.subscribe()
    }

    async fn step(&self, step: Step) -> Result<(), VolumeError> {
        let (answer, answered) = oneshot::channel();
        self.commands
            .send(Command::Step(step, answer))
            .map_err(|_| thread_gone())?;
        answered.await.map_err(|_| thread_gone())?
    }
}

fn thread_gone() -> VolumeError {
    VolumeError::Platform("the Core Audio thread has stopped".into())
}
