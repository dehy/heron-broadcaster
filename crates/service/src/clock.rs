// SPDX-License-Identifier: AGPL-3.0-or-later

//! Whether the host clock is synchronized, from the kernel's NTP state
//! (read-only `adjtimex`, no `CAP_SYS_TIME` needed). Containers share the
//! host's clock, so any service can read it.
//!
//! Heron never sets the clock: it only tells operators when schedules might
//! be off.

use serde::Serialize;

/// Past this estimated error or offset, the clock is reported as off.
pub const MAX_ERROR_MS: f64 = 1_000.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ClockStatus {
    /// `None` where the kernel's state cannot be read (not Linux).
    pub synchronized: Option<bool>,
    /// The offset the kernel is correcting, in ms.
    pub offset_ms: Option<f64>,
    /// The kernel's estimate of the clock's error, in ms.
    pub estimated_error_ms: Option<f64>,
    /// Why the clock cannot be trusted, for people. `None` when it is fine
    /// or unknown.
    pub warning: Option<String>,
}

impl ClockStatus {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn from_kernel(synchronized: bool, offset_ms: f64, estimated_error_ms: f64) -> Self {
        let warning = if !synchronized {
            Some("the server clock is not synchronized (is NTP running on the host?)".to_owned())
        } else if offset_ms.abs() > MAX_ERROR_MS || estimated_error_ms > MAX_ERROR_MS {
            Some(format!(
                "the server clock may be off by {:.1} s",
                offset_ms.abs().max(estimated_error_ms) / 1000.0
            ))
        } else {
            None
        };
        Self {
            synchronized: Some(synchronized),
            offset_ms: Some(offset_ms),
            estimated_error_ms: Some(estimated_error_ms),
            warning,
        }
    }

    fn unknown() -> Self {
        Self {
            synchronized: None,
            offset_ms: None,
            estimated_error_ms: None,
            warning: None,
        }
    }
}

impl ClockStatus {
    /// Records `heron_clock_synchronized`, `heron_clock_offset_seconds` and
    /// `heron_clock_estimated_error_seconds`. Nothing where the kernel's
    /// state cannot be read.
    pub fn record_metrics(&self) {
        if let Some(synchronized) = self.synchronized {
            metrics::gauge!("heron_clock_synchronized").set(f64::from(u8::from(synchronized)));
        }
        if let Some(offset_ms) = self.offset_ms {
            metrics::gauge!("heron_clock_offset_seconds").set(offset_ms / 1_000.0);
        }
        if let Some(error_ms) = self.estimated_error_ms {
            metrics::gauge!("heron_clock_estimated_error_seconds").set(error_ms / 1_000.0);
        }
    }
}

/// Reads the kernel's clock synchronization state.
#[cfg(target_os = "linux")]
pub fn status() -> ClockStatus {
    // SAFETY: `timex` is plain data, valid when zeroed; with `modes` 0,
    // adjtimex only fills it in and changes nothing.
    #[allow(unsafe_code)]
    let (state, timex) = unsafe {
        let mut timex: libc::timex = std::mem::zeroed();
        let state = libc::adjtimex(&mut timex);
        (state, timex)
    };
    if state < 0 {
        return ClockStatus::unknown();
    }
    let synchronized = state != libc::TIME_ERROR && timex.status & libc::STA_UNSYNC == 0;
    let offset_ms = if timex.status & libc::STA_NANO != 0 {
        timex.offset as f64 / 1e6
    } else {
        timex.offset as f64 / 1e3
    };
    ClockStatus::from_kernel(synchronized, offset_ms, timex.esterror as f64 / 1e3)
}

/// Reads the kernel's clock synchronization state: unknown off Linux.
#[cfg(not(target_os = "linux"))]
pub fn status() -> ClockStatus {
    ClockStatus::unknown()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_when_unsynchronized_or_off() {
        assert_eq!(ClockStatus::from_kernel(true, 0.2, 12.0).warning, None);
        assert!(
            ClockStatus::from_kernel(false, 0.0, 0.0)
                .warning
                .unwrap()
                .contains("not synchronized")
        );
        assert_eq!(
            ClockStatus::from_kernel(true, -2_500.0, 3.0)
                .warning
                .as_deref(),
            Some("the server clock may be off by 2.5 s")
        );
        assert!(
            ClockStatus::from_kernel(true, 0.0, 1_600.0)
                .warning
                .is_some()
        );
    }

    #[test]
    fn reading_never_fails() {
        let status = status();
        if cfg!(target_os = "linux") {
            assert!(status.synchronized.is_some());
        }
    }
}
