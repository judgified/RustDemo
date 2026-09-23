//! Linux terminal raw mode so the demo can read single keypresses.

use std::io::{self, Write};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};

const ICANON: u32 = 0x0000_0002;
const ECHO: u32 = 0x0000_0008;
const TCSANOW: i32 = 0;
const VMIN: usize = 6;
const VTIME: usize = 5;
const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 2048;
const POLLIN: i16 = 1;
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn tcgetattr(fd: i32, termios: *mut Termios) -> i32;
    fn tcsetattr(fd: i32, actions: i32, termios: *const Termios) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    fn signal(sig: i32, handler: usize) -> usize;
}

/// Restores cooked mode, the cursor, and the main screen on drop.
pub struct RawMode {
    original: Termios,
    original_flags: i32,
    previous_int: usize,
    previous_term: usize,
}

impl RawMode {
    pub fn acquire() -> io::Result<Self> {
        INTERRUPTED.store(false, Ordering::SeqCst);
        let mut slot = MaybeUninit::<Termios>::uninit();
        let rc = unsafe { tcgetattr(0, slot.as_mut_ptr()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        let original = unsafe { slot.assume_init() };

        let mut raw = original;
        raw.c_lflag &= !(ICANON | ECHO);
        raw.c_cc[VMIN] = 0;
        raw.c_cc[VTIME] = 0;
        let rc = unsafe { tcsetattr(0, TCSANOW, &raw) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }

        let flags = unsafe { fcntl(0, F_GETFL, 0) };
        if flags < 0 {
            unsafe { tcsetattr(0, TCSANOW, &original) };
            return Err(io::Error::last_os_error());
        }
        if unsafe { fcntl(0, F_SETFL, flags | O_NONBLOCK) } < 0 {
            unsafe { tcsetattr(0, TCSANOW, &original) };
            return Err(io::Error::last_os_error());
        }

        let mut out = io::stdout();
        write!(out, "\x1b[?1049h\x1b[?25l")?;
        out.flush()?;

        let previous_int = unsafe { signal(SIGINT, on_signal as usize) };
        let previous_term = unsafe { signal(SIGTERM, on_signal as usize) };

        Ok(Self {
            original,
            original_flags: flags,
            previous_int,
            previous_term,
        })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = write!(out, "\x1b[?25h\x1b[?1049l");
        let _ = out.flush();
        unsafe {
            tcsetattr(0, TCSANOW, &self.original);
            fcntl(0, F_SETFL, self.original_flags);
            signal(SIGINT, self.previous_int);
            signal(SIGTERM, self.previous_term);
        }
    }
}

/// Wait up to `timeout_ms` and return whatever keys are waiting.
pub fn poll_keys(timeout_ms: i32) -> io::Result<Vec<u8>> {
    let mut fd = PollFd {
        fd: 0,
        events: POLLIN,
        revents: 0,
    };
    let rc = unsafe { poll(&mut fd, 1, timeout_ms) };
    if rc < 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::Interrupted {
            return Ok(Vec::new());
        }
        return Err(err);
    }
    if rc == 0 {
        return Ok(Vec::new());
    }

    let mut buf = [0u8; 64];
    let n = unsafe { read(0, buf.as_mut_ptr(), buf.len()) };
    if n < 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::WouldBlock {
            return Ok(Vec::new());
        }
        return Err(err);
    }
    Ok(buf[..n as usize].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn termios_matches_the_linux_layout() {
        assert_eq!(std::mem::size_of::<Termios>(), 60);
        assert_eq!(std::mem::offset_of!(Termios, c_ispeed), 52);
        assert_eq!(std::mem::offset_of!(Termios, c_ospeed), 56);
        assert_eq!(std::mem::size_of::<PollFd>(), 8);
    }
}
