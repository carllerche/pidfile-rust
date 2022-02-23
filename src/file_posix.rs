#![allow(non_camel_case_types)]

use std::{io, mem};
use std::io::Write;
use std::path::Path;
use std::os::unix::ffi::OsStrExt;
use libc;
use libc::{
    c_void, c_int, c_short, pid_t, mode_t, size_t,
    O_CREAT, O_WRONLY, SEEK_SET, EINTR, EACCES, EAGAIN
};
use nix;
use nix::errno::{Errno, errno};
use nix::fcntl::{self, open};
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

                debug!("errno={}", err);

                if err == EINTR {
                    continue;
                }

                return Err(from_raw_os_error(err));
            }
            else {
                break;
            }
        }

        ret
    })
}

macro_rules! nix_check {
    ($expr:expr) => ({
        let mut ret;

        loop {
            let res = $expr;

            debug!("ffi; expr={}; success={}", stringify!($expr), res.is_ok());

            match res {
                Ok(v) => {
                    ret = v;
                    break;
                }
                Err(e) => {
                    if e.errno() == Errno::EINTR {
                        continue;
                    }

                    return Err(from_raw_os_error(e.errno() as c_int));
                }
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
        let mut flags = fcntl::O_SYNC;

        if create { flags = flags | fcntl::O_CREAT;  }
        if write  { flags = flags | fcntl::O_WRONLY; }

        // Open the file descriptor
        let fd = nix_check!(open(path, flags, stat::Mode::from_bits(mode as mode_t).unwrap()));

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

fn from_raw_os_error(err: i32) -> io::Error {
    io::Error::from_raw_os_error(err)
}
