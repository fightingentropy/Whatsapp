//! A persistent main-run-loop source for work arriving while the window is closed.
//! Signalling is latched, including before the main thread begins waiting.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopGetMain, CFRunLoopRef, CFRunLoopRunInMode, CFRunLoopSourceContext,
    CFRunLoopSourceCreate, CFRunLoopSourceRef, CFRunLoopSourceSignal, CFRunLoopWakeUp,
    kCFRunLoopDefaultMode,
};

static PENDING: AtomicBool = AtomicBool::new(false);
static SOURCE: OnceLock<Source> = OnceLock::new();

struct Source {
    source: CFRunLoopSourceRef,
    run_loop: CFRunLoopRef,
}

// The source is retained for the process lifetime and added only on the main
// thread. CoreFoundation explicitly permits signalling and waking from other
// threads. No other operations on these references cross threads.
unsafe impl Send for Source {}
unsafe impl Sync for Source {}

extern "C" fn performed(_: *const std::ffi::c_void) {}

/// Wake the background event loop without creating AppKit objects on a worker.
pub fn wake() {
    PENDING.store(true, Ordering::Release);
    if let Some(source) = SOURCE.get() {
        // Safety: both references are valid for the process lifetime.
        unsafe {
            CFRunLoopSourceSignal(source.source);
            CFRunLoopWakeUp(source.run_loop);
        }
    }
}

/// Wait on the main thread until work, a native event, or a real deadline.
pub fn wait(duration: Duration) {
    assert!(
        objc2::MainThreadMarker::new().is_some(),
        "background wait must run on the main thread"
    );
    SOURCE.get_or_init(|| {
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: std::ptr::null_mut(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: performed,
        };
        // Safety: the source has a static callback and no borrowed context.
        // The retained source and the main run loop live for the process.
        unsafe {
            let source = CFRunLoopSourceCreate(std::ptr::null(), 0, &mut context);
            assert!(!source.is_null(), "could not create background wake source");
            let run_loop = CFRunLoopGetMain();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopDefaultMode);
            Source { source, run_loop }
        }
    });
    if PENDING.swap(false, Ordering::AcqRel) || duration.is_zero() {
        return;
    }
    // A signal between the swap and this call stays pending in CoreFoundation,
    // so an event can never disappear between draining the queue and sleeping.
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, duration.as_secs_f64(), 1);
    }
}
