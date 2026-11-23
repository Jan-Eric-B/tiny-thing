//! What the Core Audio thread does, apart from Core Audio itself: carry out
//! commands, follow the default output device, publish the Volume when it
//! changes, and recover from failures. Core Audio sits behind [`Audio`] and
//! [`Endpoint`], so all of this is tested with a scripted fake.

use std::sync::{Arc, mpsc};
use std::time::Duration;
use tokio::sync::{oneshot, watch};
use tray_core::volume::{Percent, Step, Volume, VolumeError, stepped};

/// What the Core Audio thread is asked to do, by the Tray app or by a
/// Windows callback.
pub enum Command {
    /// Move the level one step and unmute, reading, setting and unmuting on
    /// the same device.
    Step(Step, oneshot::Sender<Result<(), VolumeError>>),
    /// From Windows: another output device became the default.
    DefaultDeviceChanged,
    /// From Windows: the followed device's Volume or mute state changed.
    VolumeChanged,
    Stop,
}

/// Core Audio, opened and registered for default output device changes; the
/// registration ends when it is dropped.
pub trait Audio {
    type Endpoint: Endpoint;

    /// The default output device's Volume, registered for its changes until
    /// dropped; `None` when the computer has no output device.
    fn default_endpoint(&self) -> windows::core::Result<Option<Self::Endpoint>>;
}

/// One output device's Volume.
pub trait Endpoint {
    fn read(&self) -> windows::core::Result<Volume>;
    /// Sets the level, leaving the mute state as it is.
    fn set_level(&self, level: Percent) -> windows::core::Result<()>;
    /// Turns muting off, leaving the level as it is.
    fn unmute(&self) -> windows::core::Result<()>;
}

/// The Core Audio thread's state.
pub struct Follower<A: Audio, O> {
    /// Declared before `audio`, so an endpoint is released before the Core
    /// Audio it came from.
    followed: Followed<A::Endpoint>,
    audio: Option<A>,
    open: O,
    published: Arc<watch::Sender<Option<Volume>>>,
    /// Failures in a row, reset by a successful read.
    failures: u32,
    retry_delay: fn(u32) -> Option<Duration>,
}

/// What the Volume comes from right now.
enum Followed<E> {
    Device(E),
    /// Windows reports no output device.
    NoDevice,
    /// Following failed; a retry is due.
    Lost,
}

impl<A: Audio, O: FnMut() -> windows::core::Result<A>> Follower<A, O> {
    /// A follower that opens Core Audio with `open`, publishes to `published`
    /// and waits `retry_delay(failures)` before trying again after failures.
    pub fn new(
        open: O,
        published: Arc<watch::Sender<Option<Volume>>>,
        retry_delay: fn(u32) -> Option<Duration>,
    ) -> Self {
        Self {
            followed: Followed::Lost,
            audio: None,
            open,
            published,
            failures: 0,
            retry_delay,
        }
    }

    /// Follows the default output device, signals `first_reading` once its
    /// Volume is published (or cannot be), then carries out commands until
    /// [`Command::Stop`].
    pub fn serve(mut self, inbox: &mpsc::Receiver<Command>, first_reading: oneshot::Sender<()>) {
        self.follow_default_device();
        // The receiver is gone only when `start` gave up waiting.
        let _sent = first_reading.send(());
        loop {
            let command = match (self.retry_delay)(self.failures) {
                None => inbox.recv().ok(),
                Some(delay) => match inbox.recv_timeout(delay) {
                    Ok(command) => Some(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        self.follow_default_device();
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => None,
                },
            };
            match command {
                Some(Command::Step(step, answer)) => {
                    // The receiver is gone only when the caller gave up.
                    let _sent = answer.send(self.step(step));
                }
                Some(Command::VolumeChanged) => self.publish(),
                Some(Command::DefaultDeviceChanged) => self.follow_default_device(),
                // Without senders nothing can ask for anything any more.
                Some(Command::Stop) | None => break,
            }
        }
    }

    /// Follows the current default output device and publishes its Volume,
    /// opening Core Audio first when it is not open.
    fn follow_default_device(&mut self) {
        // Releases the endpoint followed so far.
        self.followed = Followed::Lost;
        let audio = match self.audio.take() {
            Some(audio) => audio,
            None => match (self.open)() {
                Ok(audio) => audio,
                Err(error) => return self.lose(&error, "cannot open Core Audio"),
            },
        };
        match audio.default_endpoint() {
            Ok(Some(endpoint)) => {
                tracing::info!("following the Volume of the default output device");
                self.followed = Followed::Device(endpoint);
            }
            Ok(None) => {
                tracing::info!("no output device; no Volume to follow");
                self.followed = Followed::NoDevice;
            }
            Err(error) => {
                // Core Audio itself may be what failed (the audio service
                // restarted, say), so it is opened afresh on the retry.
                drop(audio);
                return self.lose(&error, "cannot follow the default output device");
            }
        }
        self.audio = Some(audio);
        self.publish();
    }

    /// Reads the Volume and publishes it if it differs from what was
    /// published last. `None` only when Windows reports no output device;
    /// while following fails, the last Volume stays.
    fn publish(&mut self) {
        let reading = match &self.followed {
            Followed::Device(endpoint) => endpoint.read().map(Some),
            Followed::NoDevice => Ok(None),
            Followed::Lost => return,
        };
        let volume = match reading {
            Ok(volume) => volume,
            Err(error) => return self.lose(&error, "cannot read the Volume"),
        };
        self.failures = 0;
        self.published.send_if_modified(|last| {
            if *last == volume {
                return false;
            }
            tracing::debug!(?volume, "Volume changed");
            *last = volume;
            true
        });
    }

    /// Moves the level of the followed device one step and unmutes it, like
    /// the Windows volume keys, reading, setting and unmuting on that same
    /// device.
    fn step(&mut self, step: Step) -> Result<(), VolumeError> {
        if matches!(self.followed, Followed::Lost) {
            // A step is worth trying again for at once.
            self.follow_default_device();
        }
        let result = match &self.followed {
            Followed::Device(endpoint) => endpoint.read().and_then(|current| {
                endpoint.set_level(stepped(current.level, step))?;
                if current.muted {
                    endpoint.unmute()?;
                }
                Ok(())
            }),
            Followed::NoDevice => return Err(VolumeError::NoOutputDevice),
            Followed::Lost => {
                return Err(VolumeError::Platform(
                    "the Volume cannot be followed right now; trying again".into(),
                ));
            }
        };
        result.map_err(|error| {
            self.lose(&error, "cannot step the Volume");
            VolumeError::Platform(Box::new(error))
        })
    }

    /// Releases the followed device and Core Audio after `error`, keeping the
    /// last Volume; the serve loop tries again after [`Self::retry_delay`].
    fn lose(&mut self, error: &windows::core::Error, what: &str) {
        self.followed = Followed::Lost;
        self.audio = None;
        self.failures = self.failures.saturating_add(1);
        let retry_in = (self.retry_delay)(self.failures);
        if self.failures == 1 {
            tracing::warn!(%error, ?retry_in, "{what}; trying again");
        } else {
            tracing::debug!(%error, failures = self.failures, ?retry_in, "{what}; trying again");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::thread::JoinHandle;
    use std::time::Instant;
    use windows::Win32::Foundation::E_FAIL;
    use windows::Win32::Media::Audio::AUDCLNT_E_DEVICE_INVALIDATED;

    const SPEAKERS: &str = "speakers";
    const HEADPHONES: &str = "headphones";

    /// How long a test waits for something that should happen at once.
    const PATIENCE: Duration = Duration::from_secs(2);

    fn volume(level: u8) -> Volume {
        Volume {
            level: Percent::new(level).expect("a test level is at most 100"),
            muted: false,
        }
    }

    /// Retries at once, so the tests do not wait for the real schedule.
    fn quick_retry(failures: u32) -> Option<Duration> {
        (failures > 0).then_some(Duration::from_millis(1))
    }

    /// The computer as the fake Core Audio sees it; the test changes it.
    struct Computer {
        /// Whether Core Audio can be opened and asked for the default device.
        core_audio: bool,
        /// Counts audio service restarts: Core Audio objects from before
        /// one fail for good.
        audio_service: u32,
        default_device: Option<&'static str>,
        devices: HashMap<&'static str, Device>,
    }

    struct Device {
        volume: Volume,
        /// Whether reading and setting fail, as for a device being removed.
        invalidated: bool,
    }

    type Shared = Arc<Mutex<Computer>>;

    fn lock(computer: &Shared) -> std::sync::MutexGuard<'_, Computer> {
        computer
            .lock()
            .expect("no test panics while holding the lock")
    }

    struct FakeAudio {
        computer: Shared,
        audio_service: u32,
    }

    impl Audio for FakeAudio {
        type Endpoint = FakeEndpoint;

        fn default_endpoint(&self) -> windows::core::Result<Option<FakeEndpoint>> {
            let computer = lock(&self.computer);
            if !computer.core_audio || computer.audio_service != self.audio_service {
                return Err(windows::core::Error::from_hresult(E_FAIL));
            }
            Ok(computer.default_device.map(|device| FakeEndpoint {
                device,
                computer: Arc::clone(&self.computer),
                audio_service: self.audio_service,
            }))
        }
    }

    struct FakeEndpoint {
        device: &'static str,
        computer: Shared,
        audio_service: u32,
    }

    impl FakeEndpoint {
        fn with_device<T>(&self, act: impl FnOnce(&mut Device) -> T) -> windows::core::Result<T> {
            let mut computer = lock(&self.computer);
            let audio_service = computer.audio_service;
            match computer.devices.get_mut(self.device) {
                Some(device) if !device.invalidated && audio_service == self.audio_service => {
                    Ok(act(device))
                }
                _ => Err(windows::core::Error::from_hresult(
                    AUDCLNT_E_DEVICE_INVALIDATED,
                )),
            }
        }
    }

    impl Endpoint for FakeEndpoint {
        fn read(&self) -> windows::core::Result<Volume> {
            self.with_device(|device| device.volume)
        }

        fn set_level(&self, level: Percent) -> windows::core::Result<()> {
            self.with_device(|device| device.volume.level = level)
        }

        fn unmute(&self) -> windows::core::Result<()> {
            self.with_device(|device| device.volume.muted = false)
        }
    }

    /// A Core Audio thread on the fake computer.
    struct Running {
        computer: Shared,
        commands: mpsc::Sender<Command>,
        volume: watch::Receiver<Option<Volume>>,
        thread: Option<JoinHandle<()>>,
    }

    /// A computer with the speakers at `speakers` and the headphones at
    /// `headphones`, the speakers the default.
    fn computer(speakers: u8, headphones: u8) -> Computer {
        let device = |level| Device {
            volume: volume(level),
            invalidated: false,
        };
        Computer {
            core_audio: true,
            audio_service: 0,
            default_device: Some(SPEAKERS),
            devices: HashMap::from([
                (SPEAKERS, device(speakers)),
                (HEADPHONES, device(headphones)),
            ]),
        }
    }

    /// Starts the Core Audio thread on `computer`; returns once it signals
    /// its first reading.
    fn start(computer: Computer) -> Running {
        let computer = Arc::new(Mutex::new(computer));
        let published = Arc::new(watch::Sender::new(None));
        let volume = published.subscribe();
        let (commands, inbox) = mpsc::channel();
        let (first_reading, first_read) = oneshot::channel();
        let for_thread = Arc::clone(&computer);
        let thread = std::thread::spawn(move || {
            let open = move || {
                let computer = lock(&for_thread);
                if computer.core_audio {
                    Ok(FakeAudio {
                        computer: Arc::clone(&for_thread),
                        audio_service: computer.audio_service,
                    })
                } else {
                    Err(windows::core::Error::from_hresult(E_FAIL))
                }
            };
            Follower::new(open, published, quick_retry).serve(&inbox, first_reading);
        });
        first_read
            .blocking_recv()
            .expect("the thread signals its first reading");
        Running {
            computer,
            commands,
            volume,
            thread: Some(thread),
        }
    }

    impl Running {
        fn change(&self, change: impl FnOnce(&mut Computer)) {
            change(&mut lock(&self.computer));
        }

        fn post(&self, command: Command) {
            self.commands
                .send(command)
                .expect("the thread runs until the test ends");
        }

        /// Steps and waits for the result. Commands run in order, so every
        /// command posted before has been carried out by then.
        fn step(&self, step: Step) -> Result<(), VolumeError> {
            let (answer, answered) = oneshot::channel();
            self.post(Command::Step(step, answer));
            answered.blocking_recv().expect("every step is answered")
        }

        fn level_of(&self, device: &str) -> u8 {
            lock(&self.computer).devices[device].volume.level.get()
        }

        fn published(&self) -> Option<Volume> {
            *self.volume.borrow()
        }

        /// Waits until `expected` is published, failing after [`PATIENCE`].
        fn wait_for(&self, expected: Option<Volume>) {
            let deadline = Instant::now() + PATIENCE;
            while self.published() != expected {
                assert!(
                    Instant::now() < deadline,
                    "expected {expected:?}, published is {:?}",
                    self.published()
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            let _sent = self.commands.send(Command::Stop);
            if let Some(thread) = self.thread.take() {
                let _joined = thread.join();
            }
        }
    }

    #[test]
    fn the_default_devices_volume_is_published_before_the_first_reading_is_signalled() {
        let running = start(computer(40, 10));

        assert_eq!(running.published(), Some(volume(40)));
    }

    #[test]
    fn a_step_reads_and_sets_the_level_of_the_default_device() {
        let running = start(computer(40, 10));

        running.step(Step::Up).expect("the step is carried out");

        assert_eq!(running.level_of(SPEAKERS), 42);
    }

    #[test]
    fn a_step_while_muted_unmutes_and_sets_the_level() {
        let mut muted = computer(40, 10);
        muted
            .devices
            .get_mut(SPEAKERS)
            .expect("speakers exist")
            .volume
            .muted = true;
        let running = start(muted);

        running.step(Step::Down).expect("the step is carried out");

        assert_eq!(lock(&running.computer).devices[SPEAKERS].volume, volume(38));
    }

    #[test]
    fn a_switch_of_the_default_device_moves_the_volume_and_the_steps_over() {
        let running = start(computer(40, 10));

        running.change(|computer| computer.default_device = Some(HEADPHONES));
        running.post(Command::DefaultDeviceChanged);
        // Queued right behind the switch: it acts on the headphones' own
        // level, never on the speakers' level plus a step.
        running.step(Step::Up).expect("the step is carried out");

        assert_eq!(running.level_of(HEADPHONES), 12);
        assert_eq!(running.level_of(SPEAKERS), 40);
        assert_eq!(running.published(), Some(volume(10)));
    }

    #[test]
    fn a_volume_change_is_published_and_an_unchanged_volume_is_not_published_again() {
        let mut running = start(computer(40, 10));
        running.volume.mark_unchanged();

        running.post(Command::VolumeChanged);
        running.step(Step::Up).expect("the step is carried out");
        assert!(
            !running.volume.has_changed().expect("the channel is open"),
            "the same Volume was published again"
        );
        // Windows reports the step itself as a change.
        running.post(Command::VolumeChanged);

        running.wait_for(Some(volume(42)));
    }

    #[test]
    fn no_output_device_publishes_none_and_a_step_reports_it() {
        let running = start(computer(40, 10));

        running.change(|computer| computer.default_device = None);
        running.post(Command::DefaultDeviceChanged);

        assert!(matches!(
            running.step(Step::Up),
            Err(VolumeError::NoOutputDevice)
        ));
        assert_eq!(running.published(), None);
    }

    #[test]
    fn a_failed_read_keeps_the_last_volume_and_recovers_without_an_event() {
        let running = start(computer(40, 10));

        running.change(|computer| {
            let speakers = computer.devices.get_mut(SPEAKERS).expect("speakers exist");
            speakers.invalidated = true;
            speakers.volume = volume(45);
        });
        running.post(Command::VolumeChanged);
        assert!(running.step(Step::Up).is_err());
        assert_eq!(running.published(), Some(volume(40)));

        running.change(|computer| {
            computer
                .devices
                .get_mut(SPEAKERS)
                .expect("speakers exist")
                .invalidated = false;
        });

        running.wait_for(Some(volume(45)));
    }

    #[test]
    fn core_audio_unavailable_at_start_is_followed_once_it_becomes_available() {
        let running = start(Computer {
            core_audio: false,
            ..computer(40, 10)
        });
        assert_eq!(running.published(), None);
        assert!(matches!(
            running.step(Step::Up),
            Err(VolumeError::Platform(_))
        ));

        running.change(|computer| computer.core_audio = true);

        running.wait_for(Some(volume(40)));
    }

    #[test]
    fn core_audio_is_opened_again_after_an_audio_service_restart() {
        let running = start(computer(40, 10));

        running.change(|computer| {
            computer.audio_service += 1;
            computer
                .devices
                .get_mut(SPEAKERS)
                .expect("speakers exist")
                .volume = volume(30);
        });
        running.post(Command::VolumeChanged);

        running.wait_for(Some(volume(30)));
    }
}
