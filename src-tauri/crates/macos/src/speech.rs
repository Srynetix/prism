//! Speech: macOS's own speech recognition, on this Mac only (`requiresOnDeviceRecognition`), on a
//! recording already in memory: the dictation's `system` engine. Nothing is sent to Apple.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_avf_audio::{AVAudioCommonFormat, AVAudioFormat, AVAudioPCMBuffer};
use objc2_foundation::{NSArray, NSError, NSLocale, NSOperationQueue, NSString};
use objc2_speech::{SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus};

#[link(name = "Speech", kind = "framework")]
unsafe extern "C" {}
#[link(name = "AVFAudio", kind = "framework")]
unsafe extern "C" {}

/// Whether Prism may use speech recognition (System Settings › Privacy › Speech Recognition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    NotDetermined,
    Denied,
    Restricted,
    Authorized,
}

fn auth_of(s: SFSpeechRecognizerAuthorizationStatus) -> Auth {
    match s {
        SFSpeechRecognizerAuthorizationStatus::Authorized => Auth::Authorized,
        SFSpeechRecognizerAuthorizationStatus::Denied => Auth::Denied,
        SFSpeechRecognizerAuthorizationStatus::Restricted => Auth::Restricted,
        _ => Auth::NotDetermined,
    }
}

/// The current authorization, without asking.
pub fn authorization() -> Auth {
    // SAFETY: a class method with no argument.
    auth_of(unsafe { SFSpeechRecognizer::authorizationStatus() })
}

/// Asks for the authorization when it was never given nor refused (macOS shows its sheet, with
/// `NSSpeechRecognitionUsageDescription`), waiting up to `wait` for the answer. Blocks: call it off
/// the main thread.
pub fn request_authorization(wait: Duration) -> Auth {
    let now = authorization();
    if now != Auth::NotDetermined {
        return now;
    }
    let (tx, rx) = mpsc::channel();
    let handler = RcBlock::new(move |s: SFSpeechRecognizerAuthorizationStatus| {
        let _ = tx.send(auth_of(s));
    });
    // SAFETY: a class method taking a block of the signature it calls (copied by Speech); the
    // app's Info.plist carries NSSpeechRecognitionUsageDescription, without which it aborts.
    unsafe { SFSpeechRecognizer::requestAuthorization(&handler) };
    rx.recv_timeout(wait).unwrap_or(Auth::NotDetermined)
}

/// A recognizer for `locale` (`fr-FR`, `en`), with its replies on a queue of its own (not the main
/// one, which may be the caller's).
fn recognizer(locale: &str) -> Option<Retained<SFSpeechRecognizer>> {
    let id = NSString::from_str(locale);
    // SAFETY: NSLocale's initializer with an identifier, then SFSpeechRecognizer's with a locale:
    // nil (an unknown language) is checked; the queue is a new one, kept by the recognizer.
    unsafe {
        let loc = NSLocale::initWithLocaleIdentifier(NSLocale::alloc(), &id);
        let r = SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), &loc)?;
        r.setQueue(&NSOperationQueue::new());
        Some(r)
    }
}

/// macOS recognizes `locale` on this Mac (its model installed: System Settings › Keyboard ›
/// Dictation), without sending the audio anywhere.
pub fn on_device(locale: &str) -> bool {
    // SAFETY: two getters of a recognizer just made.
    recognizer(locale).is_some_and(|r| unsafe { r.supportsOnDeviceRecognition() && r.isAvailable() })
}

/// `samples` (mono, `rate` Hz) as text in `locale`, recognized on this Mac only; `hints` are words
/// it should expect. Silence is an empty text. Blocks up to `wait`: call it off the main thread.
pub fn transcribe(samples: &[f32], rate: f64, locale: &str, hints: &[String], wait: Duration) -> Result<String, String> {
    if authorization() != Auth::Authorized {
        return Err("speech recognition not allowed".into());
    }
    let recognizer = recognizer(locale).ok_or_else(|| format!("no speech recognition for {locale}"))?;
    // SAFETY: getters of the recognizer.
    if !unsafe { recognizer.supportsOnDeviceRecognition() } {
        return Err(format!("no on-device speech recognition for {locale}"));
    }
    let frames = u32::try_from(samples.len()).map_err(|_| "recording too long".to_string())?;
    // SAFETY: AVAudioFormat's initializer for 32-bit float PCM, one channel, non-interleaved: nil checked.
    let format = unsafe { AVAudioFormat::initWithCommonFormat_sampleRate_channels_interleaved(AVAudioFormat::alloc(), AVAudioCommonFormat::PCMFormatFloat32, rate, 1, false) }.ok_or("no audio format")?;
    // SAFETY: a PCM buffer of that format holding `frames` frames: nil checked.
    let buffer = unsafe { AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(AVAudioPCMBuffer::alloc(), &format, frames.max(1)) }.ok_or("no audio buffer")?;
    // SAFETY: a float, non-interleaved, one-channel buffer: `floatChannelData` points to one
    // channel pointer, itself to `frames` writable floats (its capacity); the length is then set
    // to what was written.
    unsafe {
        let channels = buffer.floatChannelData();
        if channels.is_null() {
            return Err("no audio buffer".into());
        }
        let data = (*channels).as_ptr();
        std::ptr::copy_nonoverlapping(samples.as_ptr(), data, samples.len());
        buffer.setFrameLength(frames);
    }
    // SAFETY: the request's documented setters, then the buffer appended and the audio ended.
    let request = unsafe {
        let request = SFSpeechAudioBufferRecognitionRequest::new();
        request.setRequiresOnDeviceRecognition(true);
        request.setShouldReportPartialResults(false);
        request.setAddsPunctuation(true);
        if !hints.is_empty() {
            let words: Vec<Retained<NSString>> = hints.iter().take(100).map(|h| NSString::from_str(h)).collect();
            request.setContextualStrings(&NSArray::from_retained_slice(&words));
        }
        request.appendAudioPCMBuffer(&buffer);
        request.endAudio();
        request
    };
    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    let handler = RcBlock::new(move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
        // SAFETY: Speech passes a result or an error, either possibly nil: each checked before use.
        unsafe {
            if let Some(e) = error.as_ref() {
                // 1110: no speech heard (kAFAssistantErrorDomain), 301: cancelled.
                let code = e.code();
                let _ = tx.send(if code == 1110 || code == 301 { Ok(String::new()) } else { Err(e.localizedDescription().to_string()) });
            } else if let Some(r) = result.as_ref()
                && r.isFinal()
            {
                let _ = tx.send(Ok(r.bestTranscription().formattedString().to_string()));
            }
        }
    });
    // SAFETY: the recognizer starts a task for the request; the block is copied by Speech; the
    // recognizer, request and task stay alive in this frame until the answer or the wait's end.
    let task = unsafe { recognizer.recognitionTaskWithRequest_resultHandler(&request, &handler) };
    let out = rx.recv_timeout(wait).unwrap_or_else(|_| Err("speech recognition timed out".into()));
    // SAFETY: a method with no argument, harmless once the task is over.
    unsafe { task.cancel() };
    drop((request, recognizer));
    out
}
