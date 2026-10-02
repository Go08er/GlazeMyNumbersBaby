//! Moving clipboard data through the pipes Wayland hands us, with deadlines:
//! a peer that never reads (or never writes) can't hold a thread forever.

use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

/// How long one transfer may take in total.
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(5);

fn set_nonblocking(fd: RawFd) -> bool {
    // SAFETY: fcntl on a file descriptor we own; no memory is passed.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        flags >= 0 && libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) >= 0
    }
}

/// Wait until `fd` is ready for `events` or the deadline passes.
fn ready(fd: RawFd, events: libc::c_short, deadline: Instant) -> bool {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return false;
    }
    let mut p = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    let ms = left.as_millis().clamp(1, i32::MAX as u128) as libc::c_int;
    // SAFETY: one valid pollfd on the stack.
    unsafe { libc::poll(&mut p, 1, ms) > 0 }
}

/// Read everything from `fd` until EOF; `None` if it takes too long or is
/// longer than `max` bytes.
pub fn read_all(fd: OwnedFd, max: usize) -> Option<Vec<u8>> {
    if !set_nonblocking(fd.as_raw_fd()) {
        return None;
    }
    let deadline = Instant::now() + TRANSFER_TIMEOUT;
    let mut file = std::fs::File::from(fd);
    let mut out = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match file.read(&mut chunk) {
            Ok(0) => return Some(out),
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                if out.len() > max {
                    return None;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if !ready(file.as_raw_fd(), libc::POLLIN, deadline) {
                    return None;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// Write all of `data` to `fd`; gives up after the deadline.
pub fn write_all(fd: OwnedFd, data: &[u8]) -> bool {
    if !set_nonblocking(fd.as_raw_fd()) {
        return false;
    }
    let deadline = Instant::now() + TRANSFER_TIMEOUT;
    let mut file = std::fs::File::from(fd);
    let mut done = 0;
    while done < data.len() {
        match file.write(&data[done..]) {
            Ok(0) => return false,
            Ok(n) => done += n,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if !ready(file.as_raw_fd(), libc::POLLOUT, deadline) {
                    return false;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipe() -> (OwnedFd, OwnedFd) {
        let mut fds = [0; 2];
        // SAFETY: fds has room for two descriptors.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: both were just returned by pipe() and are owned here.
        unsafe {
            use std::os::fd::FromRawFd;
            (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1]))
        }
    }

    #[test]
    fn transfers_and_limits() {
        let (r, w) = pipe();
        let t = std::thread::spawn(move || write_all(w, &[7u8; 200_000]));
        let got = read_all(r, 1 << 20).unwrap();
        assert!(t.join().unwrap());
        assert_eq!(got.len(), 200_000);

        // Oversized: rejected, not truncated.
        let (r, w) = pipe();
        let t = std::thread::spawn(move || write_all(w, &[1u8; 5000]));
        assert!(read_all(r, 4096).is_none());
        let _ = t.join();
    }
}
