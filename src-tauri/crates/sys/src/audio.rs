//! Windows' audio devices through Core Audio (MMDevice, endpoint volume, audio sessions), for the
//! Sound block: their names, volume and mute, the default ones, and the processes recording from
//! a microphone (an active capture session). Nothing is listened to. The default device is set
//! through `IPolicyConfig`, the interface Windows' own Sound settings use (not documented, the
//! same since Windows 7).

use windows::core::{Interface, Result, GUID, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::{PKEY_Device_EnumeratorName, PKEY_Device_FriendlyName};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, AudioSessionStateActive, EDataFlow, IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, IMMEndpoint, MMDeviceEnumerator, DEVICE_STATE_ACTIVE, PKEY_AudioEndpoint_FormFactor,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantToStringAlloc, PropVariantToUInt32};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};

/// An active device.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Endpoint {
    /// Windows' id (`{0.0.1.00000000}.{…}`).
    pub id: String,
    pub name: String,
    pub input: bool,
    /// The bus Windows found it on: `BTHENUM`, `USB`, `HDAUDIO`…
    pub enumerator: String,
    /// `EndpointFormFactor`: 9 is a display's audio.
    pub form_factor: Option<u32>,
}

/// A device's volume (0 to 1) and mute; absent: not readable.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Level {
    pub volume: Option<f32>,
    pub muted: Option<bool>,
}

fn flow(input: bool) -> EDataFlow {
    if input { eCapture } else { eRender }
}

/// COM on this thread (once each: the next calls are no-ops), then the device enumerator.
fn enumerator() -> Result<IMMDeviceEnumerator> {
    // SAFETY: COM initialised for this thread in the multithreaded apartment, no reserved pointer;
    // an S_FALSE (already initialised) or a mode already set leaves COM usable for what follows.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    // SAFETY: a documented class id, no aggregation, the interface type checked by the call.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

/// A string Windows allocated (CoTaskMemAlloc), read then freed.
fn take(p: PWSTR) -> String {
    // SAFETY: `p` is a NUL-terminated string Windows just returned and nobody else holds; it is
    // read once, then freed with the allocator that made it, and never used again.
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

fn text(d: &IMMDevice, key: &PROPERTYKEY) -> Option<String> {
    // SAFETY: the store is read-only and outlives the value read; the value is converted while alive.
    let p = unsafe {
        let store = d.OpenPropertyStore(STGM_READ).ok()?;
        let v = store.GetValue(key).ok()?;
        PropVariantToStringAlloc(&v).ok()?
    };
    Some(take(p)).filter(|s| !s.is_empty())
}

fn number(d: &IMMDevice, key: &PROPERTYKEY) -> Option<u32> {
    // SAFETY: as in `text`, the value converted while alive.
    unsafe {
        let store = d.OpenPropertyStore(STGM_READ).ok()?;
        let v = store.GetValue(key).ok()?;
        PropVariantToUInt32(&v).ok()
    }
}

fn endpoint(d: &IMMDevice, input: bool) -> Option<Endpoint> {
    // SAFETY: a device of the enumerator, its id returned for us to free (`take`).
    let id = take(unsafe { d.GetId() }.ok()?);
    Some(Endpoint { id, name: text(d, &PKEY_Device_FriendlyName).unwrap_or_default(), input, enumerator: text(d, &PKEY_Device_EnumeratorName).unwrap_or_default(), form_factor: number(d, &PKEY_AudioEndpoint_FormFactor) })
}

fn active(e: &IMMDeviceEnumerator, input: bool) -> Vec<IMMDevice> {
    // SAFETY: COM objects of this thread, each index under the collection's count.
    unsafe {
        let Ok(all) = e.EnumAudioEndpoints(flow(input), DEVICE_STATE_ACTIVE) else { return Vec::new() };
        (0..all.GetCount().unwrap_or(0)).filter_map(|i| all.Item(i).ok()).collect()
    }
}

/// The active devices, outputs then inputs.
pub fn endpoints() -> Vec<Endpoint> {
    let Ok(e) = enumerator() else { return Vec::new() };
    [false, true].into_iter().flat_map(|input| active(&e, input).into_iter().filter_map(move |d| endpoint(&d, input))).collect()
}

fn default_device(input: bool) -> Result<IMMDevice> {
    let e = enumerator()?;
    // SAFETY: a COM object of this thread, valid flow and role.
    unsafe { e.GetDefaultAudioEndpoint(flow(input), eConsole) }
}

fn volume_of(d: &IMMDevice) -> Result<IAudioEndpointVolume> {
    // SAFETY: the endpoint volume is an interface every audio endpoint offers; no parameters.
    unsafe { d.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
}

/// The default device of a side (the input: the microphone), and its volume and mute.
pub fn default_endpoint(input: bool) -> Option<(Endpoint, Level)> {
    let d = default_device(input).ok()?;
    let found = endpoint(&d, input)?;
    let vol = volume_of(&d).ok();
    // SAFETY: plain reads of the endpoint's volume interface.
    let level = vol.map_or(Level::default(), |v| unsafe { Level { volume: v.GetMasterVolumeLevelScalar().ok(), muted: v.GetMute().ok().map(|b| b.as_bool()) } });
    Some((found, level))
}

/// The default device's volume, 0 to 1.
pub fn set_volume(input: bool, level: f32) -> Result<()> {
    let v = volume_of(&default_device(input)?)?;
    // SAFETY: a level in 0–1, no event context (null is allowed).
    unsafe { v.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null()) }
}

pub fn set_mute(input: bool, muted: bool) -> Result<()> {
    let v = volume_of(&default_device(input)?)?;
    // SAFETY: no event context (null is allowed).
    unsafe { v.SetMute(muted, std::ptr::null()) }
}

/// What Windows' Sound settings call to change the default device. Only `SetDefaultEndpoint` is
/// called; the methods before it hold its place in the table and are never called.
mod policy {
    #![allow(non_snake_case)]
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, HRESULT, PCWSTR};

    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: IUnknown {
        fn GetMixFormat(&self) -> HRESULT;
        fn GetDeviceFormat(&self) -> HRESULT;
        fn ResetDeviceFormat(&self) -> HRESULT;
        fn SetDeviceFormat(&self) -> HRESULT;
        fn GetProcessingPeriod(&self) -> HRESULT;
        fn SetProcessingPeriod(&self) -> HRESULT;
        fn GetShareMode(&self) -> HRESULT;
        fn SetShareMode(&self) -> HRESULT;
        fn GetPropertyValue(&self) -> HRESULT;
        fn SetPropertyValue(&self) -> HRESULT;
        fn SetDefaultEndpoint(&self, id: PCWSTR, role: i32) -> HRESULT;
    }

    /// `id`'s device as the default of `role`.
    pub fn set_default_endpoint(policy: &IPolicyConfig, id: PCWSTR, role: i32) -> windows::core::Result<()> {
        // SAFETY: the policy client implements this table on every Windows since 7; the caller's
        // id is NUL-terminated and outlives the call.
        unsafe { policy.SetDefaultEndpoint(id, role) }.ok()
    }
}
use policy::IPolicyConfig;

const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// The device the system uses by default, for every role. `false`: the device is not of that
/// side (an input is never made the output).
pub fn set_default(id: &str, input: bool) -> Result<bool> {
    let e = enumerator()?;
    // SAFETY: a COM object of this thread and a NUL-terminated id that outlives the call.
    let d = unsafe { e.GetDevice(&HSTRING::from(id))? };
    let endpoint: IMMEndpoint = d.cast()?;
    // SAFETY: a plain read.
    if unsafe { endpoint.GetDataFlow()? } != flow(input) {
        return Ok(false);
    }
    let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
    // SAFETY: Windows' policy client class, no aggregation, the interface checked by the call.
    let policy: IPolicyConfig = unsafe { CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)? };
    for role in [eConsole, eMultimedia, eCommunications] {
        policy::set_default_endpoint(&policy, PCWSTR(wide.as_ptr()), role.0)?;
    }
    Ok(true)
}

/// The processes recording now: an active session on a microphone.
pub fn recording() -> Vec<u32> {
    let Ok(e) = enumerator() else { return Vec::new() };
    let mut pids = Vec::new();
    for d in active(&e, true) {
        // SAFETY: COM objects of this thread, each index under the enumerator's count; the session
        // control is cast to its documented second interface.
        unsafe {
            let Ok(manager) = d.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
            for j in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(session) = sessions.GetSession(j) else { continue };
                if session.GetState().ok() != Some(AudioSessionStateActive) {
                    continue;
                }
                let Ok(pid) = session.cast::<IAudioSessionControl2>().and_then(|s| s.GetProcessId()) else { continue };
                if pid != 0 && !pids.contains(&pid) {
                    pids.push(pid);
                }
            }
        }
    }
    pids
}
