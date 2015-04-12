#![allow(non_camel_case_types)]

use std::{io, mem};
use std::io::{ErrorKind, Write};
use std::ffi::AsOsStr;
use std::path::Path;
use libc;
use libc::{
    c_void, c_int, c_short, pid_t, mode_t, size_t,
    O_CREAT, O_WRONLY, SEEK_SET, EINTR, EACCES, EAGAIN
};
use nix;
use nix::errno::errno;
use nix::sys::stat;
use ffi::{
    flock, O_SYNC, F_SETFD, F_GETLK,
    F_SETLK, F_WRLCK, F_UNLCK, FD_CLOEXEC
};

pub struct File {
    fd: c_int
}

macro_rules! check {
    ($expr:expr) => ({
        let mut ret;

        loop {
            ret = unsafe { $expr };

            debug!("ffi; expr={}; ret={}", stringify!($expr), ret);

            if ret < 0 {
                let err = errno() as c_int;

                if err == EINTR {
                    continue;
                }

                return Err(io::Error::from_os_error(err));
            }
            else {
                break;
            }
        }

        ret
    })
}

unsafe fn setlk(fd: c_int, fl: &flock) -> c_int {
    let ret = libc::fcntl(fd, F_SETLK, fl as *const flock);

    if ret < 0 {
        match errno() as c_int {
            EACCES | EAGAIN => return 1,
            _ => {}
        }
    }

    ret
}

impl File {
    pub fn open(path: &Path, create: bool, write: bool, mode: u32) -> io::Result<File> {
        let mut flags = O_SYNC;

        if create { flags |= O_CREAT;  }
        if write  { flags |= O_WRONLY; }

        let ptr = if let Some(path_string) = path.as_os_str().to_cstring() {
            path_string.as_bytes_with_nul().as_ptr() as *const i8
        } else {
            return Err(io::Error::new(ErrorKind::Other, "Could not convert path to c_str"));
        };

        // Open the file descriptor
        let fd = check!(libc::open(ptr, flags, mode as mode_t));

        // Set to close on exec
        check!(libc::fcntl(fd, F_SETFD, FD_CLOEXEC));

        return Ok(File { fd: fd });
    }

    pub fn truncate(&mut self) -> io::Result<()> {
        check!(libc::ftruncate(self.fd, 0));
        Ok(())
    }

    pub fn lock(&mut self) -> io::Result<bool> {
        let mut fl: flock = unsafe { mem::zeroed() };

        fl.l_type = F_WRLCK;
        fl.l_whence = SEEK_SET as c_short;

        let ret = check!(setlk(self.fd, &fl));

        Ok(ret == 0)
    }

    pub fn check(&mut self) -> io::Result<pid_t> {
        let mut fl: flock = unsafe { mem::zeroed() };

        fl.l_type = F_WRLCK;
        fl.l_whence = SEEK_SET as c_short;

        check!(libc::fcntl(self.fd, F_GETLK, &fl as *const flock));

        if fl.l_type == F_UNLCK {
            Ok(0)
        }
        else {
            Ok(fl.l_pid)
        }
    }

    pub fn write(&mut self, pid: pid_t) -> io::Result<()> {
        let mut buf: [u8; 20] = unsafe { mem::zeroed() };

        let len = {
            let mut reader = io::Cursor::new(&mut buf[..]);

            try!(write!(&mut reader, "{}\n", pid));
            reader.position()
        };

        let mut pos = 0;

        while pos < len {
            let ptr = unsafe { buf.as_ptr().offset(pos as isize) };
            let ret = check!(libc::write(self.fd, ptr as *const c_void, (len - pos) as size_t));
            pos += ret as u64;
        }

        Ok(())
    }

    pub fn stat(&self) -> nix::Result<stat::FileStat> {
        stat::fstat(self.fd)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        debug!("closing file");
        unsafe { libc::close(self.fd); }
    }
}
