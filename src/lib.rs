#![crate_name = "pidfile"]
#![allow(unstable)]

extern crate libc;
extern crate nix;

#[macro_use]
extern crate log;

use std::fmt;
use std::io::{FilePermission, IoResult, IoError, FileNotFound};
use std::path::{BytesContainer, Path};
use std::str::FromStr;
use libc::pid_t;
use nix::sys::stat::stat;
use file::File;

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "ffi_darwin.rs"]
mod ffi;

#[cfg(target_os = "linux")]
#[path = "ffi_linux.rs"]
mod ffi;

#[cfg(unix)]
#[path = "file_posix.rs"]
mod file;

pub fn at<B: BytesContainer>(path: B) -> Request {
    Request {
        pid: pid(),
        path: Path::new(path),
        perm: FilePermission::from_bits(0o644)
            .expect("0o644 is not a valid file permission")
    }
}

pub struct Request {
    pid: pid_t,
    path: Path,
    perm: FilePermission
}

impl Request {
    pub fn lock(self) -> LockResult<Lock> {
        let res = File::open(&self.path, true, true, self.perm.bits());
        let mut f = try!(res.map_err(LockError::io_error));

        if !try!(f.lock().map_err(LockError::io_error)) {
            debug!("lock not acquired; conflict");
            return Err(LockError::conflict());
        }

        debug!("lock acquired");

        try!(f.truncate().map_err(LockError::io_error));
        try!(f.write(self.pid).map_err(LockError::io_error));

        debug!("lockfile written");

        return Ok(Lock {
            pidfile: Pidfile { pid: self.pid as u32 },
            handle: f,
            path: self.path
        })
    }

    pub fn check(self) -> IoResult<Option<Pidfile>> {
        debug!("checking for lock");
        let mut f = match File::open(&self.path, false, false, 0) {
            Ok(v) => v,
            Err(e) => {
                match e.kind {
                    FileNotFound => {
                        debug!("no lock acquired -- file not found");
                        return Ok(None)
                    },
                    _ => {
                        debug!("error checking for lock; err={}", e);
                        return Err(e)
                    }
                }
            }
        };

        let pid = try!(f.check());

        if pid == 0 {
            debug!("no lock acquired -- file exists");
            return Ok(None);
        }

        debug!("lock acquired; pid={}", pid);

        Ok(Some(Pidfile { pid: pid as u32 }))
    }
}

/// Represents a pidfile that exists at the requested location and has an
/// active lock.
#[derive(Clone, Show, Copy)]
pub struct Pidfile {
    pid: u32
}

impl Pidfile {
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

pub struct Lock {
    pidfile: Pidfile,
    path: Path,
    #[allow(dead_code)]
    handle: File,
}

impl Lock {
    pub fn pidfile(&self) -> Pidfile {
        self.pidfile
    }

    pub fn ensure_current(&self) -> Result<(), Option<u32>> {
        // 1. stat the current fd
        //    - if error, try to read the pid, if it exists
        //      - if success, return Err(Some(new_pid))
        //      - otherwise, return Err(None)
        // 2. stat the path
        //    - if error, return Err(None)
        // 3. compare the inodes in the two stat results
        //    - if same, return Ok(())
        //    - otherwise, try to read the pid
        //      - if success, return Err(Some(new_pid))
        //      - otherwise, return Err(None)
        //

        let current_stat = match self.handle.stat() {
            Err(_) => return Err(self.read_pid()),
            Ok(stat) => stat
        };

        let path_stat = try!(stat(&self.path).map_err(|_| None));

        if current_stat.st_ino == path_stat.st_ino {
            Ok(())
        } else {
            Err(self.read_pid())
        }
    }

    fn read_pid(&self) -> Option<u32> {
        let mut f = std::io::File::open(&self.path);

        let s = match f.read_to_string() {
            Ok(val) => val,
            Err(_) => return None
        };

        s.as_slice().lines().nth(0).and_then(|l| FromStr::from_str(l))
    }
}

#[derive(Show)]
pub struct LockError {
    pub conflict: bool,
    pub io: Option<IoError>,
}

impl LockError {
    fn conflict() -> LockError {
        LockError {
            conflict: true,
            io: None
        }
    }

    fn io_error(err: IoError) -> LockError {
        LockError {
            conflict: false,
            io: Some(err)
        }
    }
}

impl fmt::Show for Lock {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Lock {{ pidfile: {:?}, path: {:?} }}", self.pidfile, self.path)
    }
}

pub type LockResult<T> = Result<T, LockError>;

fn pid() -> pid_t {
    unsafe { libc::getpid() }
}
