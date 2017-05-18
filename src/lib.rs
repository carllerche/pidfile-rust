//! Crate allows to lock a pidfile for a process to prevent another from
//! starting as long the lock is held.

#![crate_name = "pidfile"]

extern crate libc;
extern crate nix;
extern crate tempdir;

#[macro_use]
extern crate log;

use file::File;
use libc::pid_t;
use nix::sys::stat::stat;
use std::{fmt, io};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "ffi_darwin.rs"]
mod ffi;

#[cfg(target_os = "linux")]
#[path = "ffi_linux.rs"]
mod ffi;

#[cfg(target_os = "freebsd")]
#[path = "ffi_linux.rs"]
mod ffi;

#[cfg(unix)]
#[path = "file_posix.rs"]
mod file;

pub fn at<S: AsRef<Path> + ?Sized>(path: &S) -> Request {
    Request {
        pid: pid(),
        path: PathBuf::from(path.as_ref()),
        perm: 0o644,
    }
}

pub struct Request {
    pid: pid_t,
    path: PathBuf,
    perm: u32,
}

impl Request {
    pub fn lock(self) -> LockResult<Lock> {
        let res = File::open(&*self.path, true, true, self.perm);
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

    pub fn check(self) -> io::Result<Option<Pidfile>> {
        debug!("checking for lock");
        let mut f = match File::open(&self.path, false, false, 0) {
            Ok(v) => v,
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::NotFound => {
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
#[derive(Clone, Debug, Copy)]
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
    path: PathBuf,
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

        let path_stat = try!(stat(&*self.path).map_err(|_| None));

        if current_stat.st_ino == path_stat.st_ino {
            Ok(())
        } else {
            Err(self.read_pid())
        }
    }

    fn read_pid(&self) -> Option<u32> {
        if let Ok(mut f) = std::fs::File::open(&self.path) {
            let mut s = String::new();

            if let Ok(_) = f.read_to_string(&mut s) {
                return s.lines().nth(0)
                    .and_then(|s| FromStr::from_str(s).ok())
            }
        }

        None
    }
}

#[derive(Debug)]
pub struct LockError {
    pub conflict: bool,
    pub io: Option<io::Error>,
}

impl LockError {
    fn conflict() -> LockError {
        LockError {
            conflict: true,
            io: None
        }
    }

    fn io_error(err: io::Error) -> LockError {
        LockError {
            conflict: false,
            io: Some(err)
        }
    }
}

impl fmt::Debug for Lock {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Lock {{ pidfile: {:?}, path: {:?} }}", self.pidfile, self.path)
    }
}

pub type LockResult<T> = Result<T, LockError>;

fn pid() -> pid_t {
    unsafe { libc::getpid() }
}

#[cfg(test)]
mod tests {

    #[test]
    fn main() {
		use std::thread;
        use at;
        use tempdir::TempDir;

        let p = TempDir::new("").expect("create temp dir");

        assert!(p.path().exists());

        let p = p.path().join("pidfile");

		// run couple threads

		let allcan = (0 .. 3)
			.map(|_| {
                let pc = p.clone();
				let atit = move || {
					at(&pc).lock()
				};

				thread::spawn(atit)
			})
			.collect::<Vec<_>>()
			.into_iter()
			.map(|t| t.join())
			.collect::<Vec<_>>()
            .into_iter()
            .all(|v| v.is_ok());

		assert!(allcan);


    }
}
