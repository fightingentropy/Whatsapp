//! Exercises the real CoreFoundation source on the main thread. No account or GUI.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use zapfast::background::{wait, wake};

fn main() {
    // A signal before source creation must not disappear.
    wake();
    let started = Instant::now();
    wait(Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(1));

    // Work arriving while the main thread sleeps must wake it immediately.
    let ready = Arc::new(AtomicBool::new(false));
    let worker_ready = ready.clone();
    let started = Instant::now();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        worker_ready.store(true, Ordering::Release);
        wake();
    });
    while !ready.load(Ordering::Acquire) {
        wait(Duration::from_secs(2));
    }
    worker.join().unwrap();
    let wake_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert!(wake_ms < 1000.0, "worker wake was lost: {wake_ms} ms");

    // Repeated races between signal and sleep cover both sides of registration.
    for _ in 0..100 {
        let ready = Arc::new(AtomicBool::new(false));
        let worker_ready = ready.clone();
        let worker = std::thread::spawn(move || {
            worker_ready.store(true, Ordering::Release);
            wake();
        });
        let started = Instant::now();
        while !ready.load(Ordering::Acquire) {
            wait(Duration::from_secs(2));
        }
        worker.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1), "lost a wake");
    }
    // Drain the pending bit and latched CF signal, then check the idle deadline.
    wait(Duration::ZERO);
    wait(Duration::from_millis(1));
    let started = Instant::now();
    wait(Duration::from_millis(200));
    assert!(
        started.elapsed() >= Duration::from_millis(180),
        "idle loop spun"
    );
    println!(
        "background probe passed: worker wake {wake_ms:.2} ms (includes 30 ms delay), 100 wake races, idle deadline respected"
    );
}
