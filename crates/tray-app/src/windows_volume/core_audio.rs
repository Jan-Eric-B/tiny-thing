//! Core Audio itself: the [`Audio`] and [`Endpoint`] the Core Audio thread
//! follows the default output device with.

// Core Audio is a COM API: windows-rs marks every call `unsafe`, and the
// `#[implement]` macro for the callbacks generates unsafe code. The calls
// below are sound for the same reasons each time: COM is initialised on the
// calling thread ([`ComApartment`]), every interface pointer comes from
// windows-rs and is alive, and pointer arguments are null or point to live
// values. Only this module allows `unsafe_code`; the rest of the adapter
// keeps the workspace's deny.
#![allow(unsafe_code)]

use super::follower::{Audio, Command, Endpoint};
use super::level;
use std::marker::PhantomData;
use std::sync::mpsc;
use tray_core::volume::{Percent, Volume};
use windows::Win32::Foundation::{ERROR_NOT_FOUND, PROPERTYKEY};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator,
    IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{PCWSTR, implement};

/// The output device role that counts as the default: the one the Windows
/// volume flyout and the keyboard's volume keys act on. Windows settings
/// assign it together with the multimedia role.
const DEFAULT_ROLE: ERole = eConsole;

/// Core Audio on the Core Audio thread, registered for default output device
/// changes until dropped.
pub struct CoreAudio {
    enumerator: IMMDeviceEnumerator,
    device_events: IMMNotificationClient,
    volume_events: IAudioEndpointVolumeCallback,
    /// Declared last, so this thread leaves the apartment only after every
    /// COM object above is released.
    _apartment: ComApartment,
}

impl CoreAudio {
    /// Joins the multithreaded COM apartment, opens the device enumerator and
    /// registers for default output device changes. The callbacks post their
    /// events to `events`.
    pub fn open(events: mpsc::Sender<Command>) -> windows::core::Result<Self> {
        let apartment = ComApartment::join()?;
        // SAFETY: see the module comment.
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
        let device_events: IMMNotificationClient = DeviceEvents {
            events: events.clone(),
        }
        .into();
        let volume_events: IAudioEndpointVolumeCallback = VolumeEvents { events }.into();
        // SAFETY: see the module comment.
        unsafe { enumerator.RegisterEndpointNotificationCallback(&device_events) }?;
        Ok(Self {
            enumerator,
            device_events,
            volume_events,
            _apartment: apartment,
        })
    }
}

impl Audio for CoreAudio {
    type Endpoint = CoreAudioEndpoint;

    fn default_endpoint(&self) -> windows::core::Result<Option<CoreAudioEndpoint>> {
        // SAFETY: see the module comment.
        let device = match unsafe {
            self.enumerator
                .GetDefaultAudioEndpoint(eRender, DEFAULT_ROLE)
        } {
            Ok(device) => device,
            // Windows reports a computer without an output device this way.
            Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => return Ok(None),
            Err(error) => return Err(error),
        };
        // SAFETY: see the module comment; no activation parameters.
        let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }?;
        // SAFETY: see the module comment.
        unsafe { volume.RegisterControlChangeNotify(&self.volume_events) }?;
        Ok(Some(CoreAudioEndpoint {
            volume,
            events: self.volume_events.clone(),
        }))
    }
}

impl Drop for CoreAudio {
    fn drop(&mut self) {
        // SAFETY: see the module comment.
        if let Err(error) = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.device_events)
        } {
            tracing::debug!(%error, "cannot remove the device callback");
        }
    }
}

/// One output device's Volume, registered for its changes until dropped.
pub struct CoreAudioEndpoint {
    volume: IAudioEndpointVolume,
    events: IAudioEndpointVolumeCallback,
}

impl Endpoint for CoreAudioEndpoint {
    fn read(&self) -> windows::core::Result<Volume> {
        // SAFETY: see the module comment.
        let scalar = unsafe { self.volume.GetMasterVolumeLevelScalar() }?;
        // SAFETY: see the module comment.
        let muted = unsafe { self.volume.GetMute() }?.as_bool();
        Ok(Volume {
            level: level::from_scalar(scalar),
            muted,
        })
    }

    fn set_level(&self, level: Percent) -> windows::core::Result<()> {
        // SAFETY: see the module comment; no event context (null).
        unsafe {
            self.volume
                .SetMasterVolumeLevelScalar(level::to_scalar(level), std::ptr::null())
        }
    }
}

impl Drop for CoreAudioEndpoint {
    fn drop(&mut self) {
        // SAFETY: see the module comment.
        if let Err(error) = unsafe { self.volume.UnregisterControlChangeNotify(&self.events) } {
            // A removed device has dropped its callbacks already.
            tracing::debug!(%error, "cannot remove the Volume callback");
        }
    }
}

/// This thread's membership in the multithreaded COM apartment, left again
/// when dropped. Not `Send`, so it is dropped on the thread that joined.
struct ComApartment {
    _this_thread: PhantomData<*const ()>,
}

impl ComApartment {
    fn join() -> windows::core::Result<Self> {
        // SAFETY: see the module comment; no reserved pointer is passed.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        Ok(Self {
            _this_thread: PhantomData,
        })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful `CoInitializeEx` in `join`, on the
        // same thread (the type is not `Send`).
        unsafe { CoUninitialize() };
    }
}

/// Posts default output device changes to the Core Audio thread. Windows
/// calls it on threads of its own and requires it not to block, which a send
/// on an unbounded channel does not.
#[implement(IMMNotificationClient)]
struct DeviceEvents {
    events: mpsc::Sender<Command>,
}

impl IMMNotificationClient_Impl for DeviceEvents_Impl {
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _device: &PCWSTR,
    ) -> windows::core::Result<()> {
        // Windows reports each role and both directions separately.
        if flow == eRender && role == DEFAULT_ROLE {
            // Fails only once the thread has stopped.
            let _sent = self.events.send(Command::DefaultDeviceChanged);
        }
        Ok(())
    }

    fn OnDeviceStateChanged(
        &self,
        _device: &PCWSTR,
        _state: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _device: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _device: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _device: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Posts Volume and mute changes of the followed device to the Core Audio
/// thread, which reads the new Volume itself.
#[implement(IAudioEndpointVolumeCallback)]
struct VolumeEvents {
    events: mpsc::Sender<Command>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeEvents_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        // Fails only once the thread has stopped.
        let _sent = self.events.send(Command::VolumeChanged);
        Ok(())
    }
}
