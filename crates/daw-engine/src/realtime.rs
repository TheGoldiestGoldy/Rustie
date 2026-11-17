//! Linux-only RT scheduling negotiation. All scheduling operations run on the
//! *control* thread, never on the audio callback. JACK is already managed by PipeWire.
//!
//! The audio callback only publishes its Linux TID once via a nonblocking gettid syscall.
//! We first use the caller's existing RLIMIT_RTPRIO to request SCHED_FIFO. If that
//! fails, we ask RTKit over the system D-Bus (outside the audio thread).

use std::fmt;

pub const DEFAULT_RT_PRIORITY: i32 = 10;

pub fn current_kernel_tid() -> i32 {
    // SAFETY: Linux SYS_gettid takes no pointers and returns the kernel TID.
    // Called once per stream, without allocation, locking or waiting.
    unsafe { libc::syscall(libc::SYS_gettid) as i32 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriorityStatus {
    AlreadyRealtime { scheduler: i32 },
    GrantedDirect,
    GrantedRtkit,
    NotGranted(String),
    Unavailable,
}

impl fmt::Display for PriorityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRealtime { scheduler } => write!(f, "already RT policy {scheduler}"),
            Self::GrantedDirect => f.write_str("SCHED_FIFO via realtime user permissions"),
            Self::GrantedRtkit => f.write_str("RTKit via system D-Bus"),
            Self::NotGranted(reason) => write!(f, "real-time promotion unavailable: {reason}"),
            Self::Unavailable => f.write_str("no RT callback TID yet"),
        }
    }
}

pub fn query_thread_scheduler(tid: i32) -> PriorityStatus {
    if tid <= 0 {
        return PriorityStatus::Unavailable;
    }
    // SAFETY: Linux syscall only queries scheduler associated with known live TID.
    let policy = unsafe { libc::sched_getscheduler(tid) };
    match policy {
        libc::SCHED_FIFO | libc::SCHED_RR => PriorityStatus::AlreadyRealtime { scheduler: policy },
        _ => PriorityStatus::NotGranted(format!("scheduler policy {policy}")),
    }
}

pub fn request_realtime_for_tid(tid: i32, priority: i32) -> PriorityStatus {
    if tid <= 0 {
        return PriorityStatus::Unavailable;
    }
    if matches!(
        query_thread_scheduler(tid),
        PriorityStatus::AlreadyRealtime { .. }
    ) {
        return query_thread_scheduler(tid);
    }
    let requested = priority.clamp(1, 20);
    let scheduling = libc::sched_param {
        sched_priority: requested,
    };
    // SAFETY: scheduling points to a valid struct, tid is a live Linux thread ID.
    // Crucially this call occurs on the control thread, NOT the RT audio callback.
    if unsafe { libc::sched_setscheduler(tid, libc::SCHED_FIFO, &scheduling) } == 0 {
        return PriorityStatus::GrantedDirect;
    }

    // RTKit is only a fallback, and D-Bus operations (possibly blocking) are allowed here.
    let result = zbus::blocking::Connection::system().and_then(|connection| {
        connection
            .call_method(
                Some("org.freedesktop.RealtimeKit1"),
                "/org/freedesktop/RealtimeKit1",
                Some("org.freedesktop.RealtimeKit1"),
                "MakeThreadRealtime",
                &(tid as u64, requested as u32),
            )
            .map(|_| ())
    });
    match result {
        Ok(()) => PriorityStatus::GrantedRtkit,
        Err(error) => PriorityStatus::NotGranted(error.to_string()),
    }
}
