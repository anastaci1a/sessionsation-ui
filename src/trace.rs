//! Opt-in timing trace for diagnosing input latency. Enabled only when
//! `SESSIONSATION_UI_TRACE` names a file; otherwise every call is a cheap no-op.
use std::{
    io::Write,
    sync::{Mutex, OnceLock},
    time::Instant,
};
struct Sink {
    started: Instant,
    file: Mutex<std::io::BufWriter<std::fs::File>>,
}
static SINK: OnceLock<Option<Sink>> = OnceLock::new();
fn sink() -> Option<&'static Sink> {
    SINK.get_or_init(|| {
        let path = std::env::var_os("SESSIONSATION_UI_TRACE")?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        let started = Instant::now();
        let mut clock = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: clock_gettime only writes the provided timespec.
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut clock) };
        let mut file = std::io::BufWriter::new(file);
        let _ = writeln!(
            file,
            "0 start monotonic_ns={}",
            clock.tv_sec as i128 * 1_000_000_000 + clock.tv_nsec as i128
        );
        Some(Sink {
            started,
            file: Mutex::new(file),
        })
    })
    .as_ref()
}
pub fn enabled() -> bool {
    sink().is_some()
}
/// Records `kind detail` with a monotonic microsecond timestamp.
pub fn event(kind: &str, detail: impl std::fmt::Display) {
    if let Some(s) = sink() {
        let us = s.started.elapsed().as_micros();
        if let Ok(mut f) = s.file.lock() {
            let _ = writeln!(f, "{us} {kind} {detail}");
        }
    }
}
pub fn flush() {
    if let Some(s) = sink()
        && let Ok(mut f) = s.file.lock()
    {
        let _ = f.flush();
    }
}
