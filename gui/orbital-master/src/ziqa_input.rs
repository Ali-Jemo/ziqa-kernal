//! Ziqa-native input reader for `ziqa-bga-direct` feature.
//! Reads packed `orbclient::Event` from the `input:` scheme or `/scheme/input/consumer`.

use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::FromRawFd;

use libredox::flag::{O_NONBLOCK, O_RDONLY};
use log::warn;

#[cfg(feature = "ziqa-bga-direct")]
pub struct ZiqaInput {
    file: Option<File>,
    warned: bool,
}

#[cfg(feature = "ziqa-bga-direct")]
impl ZiqaInput {
    pub fn open() -> Self {
        for path in ["input:", "/scheme/input/consumer"] {
            if let Some(file) = Self::open_path(path) {
                println!("ziqa input opened {}", path);
                return Self {
                    file: Some(file),
                    warned: false,
                };
            }
        }

        warn!("ziqa input disabled: input: and /scheme/input/consumer unavailable");
        Self {
            file: None,
            warned: true,
        }
    }

    fn open_path(path: &str) -> Option<File> {
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true).custom_flags(O_NONBLOCK as i32);
        if let Ok(file) = opts.open(path) {
            return Some(file);
        }

        let flags = (O_RDONLY | O_NONBLOCK) as usize;
        if let Ok(fd) = syscall::openat(usize::MAX, path, flags, 0) {
            // SAFETY: `fd` is returned by the kernel openat syscall and ownership is
            // transferred into `File`, which will close it on drop.
            return Some(unsafe { File::from_raw_fd(fd as _) });
        }

        match libredox::call::open(path, flags as i32, 0) {
            Ok(fd) => {
                // SAFETY: `fd` is returned by the kernel open call and ownership is
                // transferred into `File`, which will close it on drop.
                Some(unsafe { File::from_raw_fd(fd as _) })
            }
            Err(_) => None,
        }
    }

    pub fn read_events(&mut self, events: &mut [orbclient::Event]) -> usize {
        let Some(file) = self.file.as_mut() else {
            self.warned = true;
            return 0;
        };

        let event_size = std::mem::size_of::<orbclient::Event>();
        let byte_len = events.len() * event_size;
        // SAFETY: `events` is a contiguous mutable `[orbclient::Event]`; viewing it
        // as bytes for a nonblocking read preserves size and alignment and writes
        // only within the slice bounds.
        let buf =
            unsafe { std::slice::from_raw_parts_mut(events.as_mut_ptr() as *mut u8, byte_len) };

        match file.read(buf) {
            Ok(n) => n / event_size,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => 0,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => 0,
            Err(err) if err.raw_os_error() == Some(11) => 0,
            Err(err) => {
                warn!("ziqa input read failed: {}", err);
                self.file = None;
                self.warned = true;
                0
            }
        }
    }
}
