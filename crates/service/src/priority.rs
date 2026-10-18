// SPDX-License-Identifier: AGPL-3.0-or-later

//! Lowering the CPU and disk priority of background work, so it yields to
//! the air when the machine is busy: the analyzer and the librarian lower
//! themselves, the engine lowers its fallback analysis thread.
//!
//! On Linux, CPU niceness and I/O priority belong to threads, and new
//! threads inherit them: call [`lower_current_thread`] before starting the
//! runtime to lower a whole process. Lowering needs no privilege. Between
//! containers, the CPU is shared by the containers' weights (`cpu_shares`
//! in Compose); niceness orders the threads inside one.

/// Background work: nice 10, and the lowest best-effort I/O priority.
pub const BACKGROUND_NICE: i32 = 10;

/// Raises the calling thread's niceness to `nice` (0 to 19, higher yields
/// more; never lowered) and sets its I/O priority to the lowest of the best-effort class, and
/// returns a description of what failed, if anything. Nothing happens off
/// Linux, or when `nice` is 0 or less.
///
/// The I/O priority only matters with an I/O scheduler that uses it (BFQ);
/// with others it is accepted and ignored.
pub fn lower_current_thread(nice: i32) -> Result<(), String> {
    if nice <= 0 {
        return Ok(());
    }
    imp::lower(nice.min(19))
}

/// Like [`lower_current_thread`], from a setting (`HERON_NICE`) read
/// leniently: a value that is not a niceness from 0 to 19 is reported and
/// [`BACKGROUND_NICE`] used instead.
pub fn lower_current_thread_to(setting: &str) -> Result<(), String> {
    match setting.trim().parse::<i32>() {
        Ok(nice) if (0..=19).contains(&nice) => lower_current_thread(nice),
        _ => {
            let lowered = lower_current_thread(BACKGROUND_NICE);
            let invalid =
                format!("invalid HERON_NICE {setting:?} (0 to 19): using {BACKGROUND_NICE}");
            Err(match lowered {
                Ok(()) => invalid,
                Err(error) => format!("{invalid}; {error}"),
            })
        }
    }
}

/// The calling thread's niceness, `None` off Linux.
pub fn current_nice() -> Option<i32> {
    imp::current_nice()
}

#[cfg(target_os = "linux")]
mod imp {
    // From linux/ioprio.h.
    const IOPRIO_CLASS_SHIFT: libc::c_int = 13;
    const IOPRIO_CLASS_BE: libc::c_int = 2;
    const IOPRIO_WHO_PROCESS: libc::c_int = 1;
    const LOWEST_BEST_EFFORT: libc::c_int = 7;

    // Linux keeps niceness and I/O priority per thread: `who` 0 (with
    // PRIO_PROCESS / IOPRIO_WHO_PROCESS) is the calling thread.
    #[allow(unsafe_code)]
    pub fn lower(nice: i32) -> Result<(), String> {
        let mut failures = Vec::new();
        // Only ever raised: lowering it again would need privileges, and
        // an operator may have set it higher.
        let current = current_nice().unwrap_or(0);
        // SAFETY: plain system calls on the calling thread, no pointers.
        let set = if current < nice {
            unsafe { libc::setpriority(libc::PRIO_PROCESS as _, 0, nice) }
        } else {
            0
        };
        if set != 0 {
            failures.push(format!("nice: {}", std::io::Error::last_os_error()));
        }
        let ioprio = (IOPRIO_CLASS_BE << IOPRIO_CLASS_SHIFT) | LOWEST_BEST_EFFORT;
        // SAFETY: as above.
        let set = unsafe { libc::syscall(libc::SYS_ioprio_set, IOPRIO_WHO_PROCESS, 0, ioprio) };
        if set != 0 {
            failures.push(format!("I/O priority: {}", std::io::Error::last_os_error()));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join(", "))
        }
    }

    #[allow(unsafe_code)]
    pub fn current_nice() -> Option<i32> {
        // SAFETY: a plain system call on the calling thread. -1 is also a
        // valid niceness, but lowering never produces it.
        Some(unsafe { libc::getpriority(libc::PRIO_PROCESS as _, 0) })
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    pub fn lower(_nice: i32) -> Result<(), String> {
        Ok(())
    }

    pub fn current_nice() -> Option<i32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_the_calling_thread_and_the_threads_it_starts() {
        // On a thread of its own: the test runner's threads keep theirs.
        std::thread::spawn(|| {
            let before = current_nice();
            lower_current_thread(BACKGROUND_NICE).unwrap();
            if cfg!(target_os = "linux") {
                assert_eq!(
                    current_nice(),
                    Some(BACKGROUND_NICE.max(before.unwrap_or(0)))
                );
                let inherited = std::thread::spawn(current_nice).join().unwrap();
                assert_eq!(inherited, current_nice());
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn zero_changes_nothing() {
        std::thread::spawn(|| {
            let before = current_nice();
            lower_current_thread(0).unwrap();
            assert_eq!(current_nice(), before);
        })
        .join()
        .unwrap();
    }
}
