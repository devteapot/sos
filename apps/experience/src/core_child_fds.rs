use std::io;
use std::mem::MaybeUninit;
use std::os::unix::process::CommandExt;
use std::process::Command;

// Linux UAPI <linux/close_range.h>; libc does not export this Linux 5.11+
// constant on its Android target.
const CLOSE_RANGE_CLOEXEC: u32 = 1 << 2;

const FIRST_NON_STANDARD_FD: libc::c_int = 3;

fn close_range_cloexec_is_unsupported(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::EINVAL | libc::ENOSYS))
}

/// Marks every descriptor in `[first, end)` close-on-exec without closing it.
///
/// The Rust standard library keeps a private pipe open until `execve` so the
/// child can report a launch error to its parent. Closing the whole range in a
/// `pre_exec` callback would also close that pipe and could turn an exec
/// failure into a false successful spawn. `fcntl(F_SETFD)` preserves the pipe
/// until exec while still applying the same fail-closed inheritance boundary.
fn mark_fd_range_cloexec(first: libc::c_int, end: libc::rlim_t) -> io::Result<()> {
    let end = end.min(libc::c_int::MAX as libc::rlim_t);
    for raw_fd in (first as libc::rlim_t)..end {
        let fd = raw_fd as libc::c_int;
        // SAFETY: `fcntl` accepts any integer descriptor. Closed descriptors
        // return EBADF and are deliberately skipped.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EBADF) {
                continue;
            }
            return Err(error);
        }
        // SAFETY: `fd` was observed open above and F_SETFD only updates its
        // descriptor flags. Preserve any flags the kernel already returned.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn mark_all_non_standard_fds_cloexec() -> io::Result<()> {
    let mut limit = MaybeUninit::<libc::rlimit>::uninit();
    // SAFETY: getrlimit initializes the provided rlimit on success.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limit.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful getrlimit call initialized `limit`.
    let limit = unsafe { limit.assume_init() };
    mark_fd_range_cloexec(FIRST_NON_STANDARD_FD, limit.rlim_cur)
}

/// Installs the fail-closed, async-signal-safe boundary used immediately
/// before exec. `Command` has already duplicated the requested pipes onto
/// descriptors 0/1/2 when this callback runs. Linux 5.11+ atomically applies
/// CLOEXEC to the remaining range. Android's 5.10 kernel has `close_range`
/// but predates `CLOSE_RANGE_CLOEXEC`, so EINVAL/ENOSYS falls back to the
/// async-signal-safe `fcntl` loop above. Both paths preserve Rust's private
/// launch-error pipe until exec while preventing every inherited GPUI,
/// dma-buf, surface, input, device, and service descriptor from surviving it.
pub(crate) fn restrict_to_standard_fds(command: &mut Command) {
    // SAFETY: the callback invokes only the close_range syscall and constructs
    // an io::Error from errno. It does not allocate, lock, or inspect process
    // state after fork. Failure aborts spawn instead of leaking descriptors.
    unsafe {
        command.pre_exec(|| {
            let result = libc::syscall(libc::SYS_close_range, 3_u32, u32::MAX, CLOSE_RANGE_CLOEXEC);
            if result == 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if close_range_cloexec_is_unsupported(&error) {
                mark_all_non_standard_fds_cloexec()
            } else {
                Err(error)
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::process::Stdio;

    use super::*;

    #[test]
    fn android_5_10_close_range_flag_rejection_uses_the_safe_fallback() {
        assert!(close_range_cloexec_is_unsupported(
            &io::Error::from_raw_os_error(libc::EINVAL)
        ));
        assert!(close_range_cloexec_is_unsupported(
            &io::Error::from_raw_os_error(libc::ENOSYS)
        ));
        assert!(!close_range_cloexec_is_unsupported(
            &io::Error::from_raw_os_error(libc::EPERM)
        ));
    }

    #[test]
    fn fallback_marks_an_open_descriptor_and_ignores_closed_descriptors() {
        let inherited = OpenOptions::new().read(true).open("/dev/null").unwrap();
        let minimum = 256;
        // SAFETY: F_DUPFD_CLOEXEC duplicates a live descriptor. The returned
        // descriptor is owned by this test and closed below.
        let duplicated = unsafe { libc::fcntl(inherited.as_raw_fd(), libc::F_DUPFD, minimum) };
        assert!(duplicated >= minimum);
        // SAFETY: duplicated is live and owned by this test.
        assert_eq!(unsafe { libc::fcntl(duplicated, libc::F_SETFD, 0) }, 0);

        mark_fd_range_cloexec(duplicated - 1, (duplicated + 2) as libc::rlim_t).unwrap();

        // SAFETY: duplicated remains live until the explicit close below.
        assert_ne!(
            unsafe { libc::fcntl(duplicated, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        // SAFETY: this test uniquely owns duplicated.
        assert_eq!(unsafe { libc::close(duplicated) }, 0);
    }

    #[test]
    fn child_keeps_only_pipe_backed_standard_streams() {
        let leaked = tempfile::NamedTempFile::new().unwrap();
        let leaked_path = leaked.path().to_string_lossy().into_owned();
        let inherited = OpenOptions::new().read(true).open(leaked.path()).unwrap();
        assert!(inherited.as_raw_fd() >= 3);
        // Deliberately model a graphics/device descriptor that lacks CLOEXEC.
        // SAFETY: inherited is live and remains owned by this test.
        assert_eq!(
            unsafe { libc::fcntl(inherited.as_raw_fd(), libc::F_SETFD, 0) },
            0
        );

        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(
                "for descriptor in /proc/self/fd/*; do \
                 target=$(readlink \"$descriptor\" 2>/dev/null || true); \
                 [ \"$target\" != \"$SOS_TEST_LEAK\" ] || exit 90; \
                 done; read value; printf '%s' \"$value\"",
            )
            .env("SOS_TEST_LEAK", leaked_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        restrict_to_standard_fds(&mut command);
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"stdio-survives\n")
            .unwrap();
        let output = child.wait_with_output().unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, b"stdio-survives");
    }

    #[test]
    fn cloexec_range_preserves_exec_failure_reporting() {
        let mut command = Command::new("/sos-test-path-that-must-not-exist");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        restrict_to_standard_fds(&mut command);

        assert_eq!(command.spawn().unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
