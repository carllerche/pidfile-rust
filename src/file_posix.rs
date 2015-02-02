#![allow(non_camel_case_types)]

use std::old_io::{BufWriter, IoResult, IoError};
use std::os::errno;
use std::ffi::CString;
use std::mem;
use libc;
use libc::{
    c_void, c_int, c_short, pid_t, mode_t, size_t,
    O_CREAT, O_WRONLY, SEEK_SET, EINTR, EACCES, EAGAIN
};
use nix::sys::stat;
use nix::SysResult;
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

                return Err(IoError::from_errno(err as usize, false));
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
    pub fn open(path: &Path, create: bool, write: bool, mode: u32) -> IoResult<File> {
        let mut flags = O_SYNC;

        if create { flags |= O_CREAT;  }
        if write  { flags |= O_WRONLY; }

        // Open the file descriptor
        let fd = check!(libc::open(CString::from_slice(path.as_vec()).as_ptr(), flags, mode as mode_t));

        // Set to close on exec
        check!(libc::fcntl(fd, F_SETFD, FD_CLOEXEC));

        return Ok(File { fd: fd });
    }

    pub fn truncate(&mut self) -> IoResult<()> {
        check!(libc::ftruncate(self.fd, 0));
        Ok(())
    }

    pub fn lock(&mut self) -> IoResult<bool> {
        let mut fl: flock = unsafe { mem::zeroed() };

        fl.l_type = F_WRLCK;
        fl.l_whence = SEEK_SET as c_short;

        let ret = check!(setlk(self.fd, &fl));

        Ok(ret == 0)
    }

    pub fn check(&mut self) -> IoResult<pid_t> {
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

    pub fn write(&mut self, pid: pid_t) -> IoResult<()> {
        let mut buf: [u8; 20] = unsafe { mem::zeroed() };

        let len = {
            let mut reader = BufWriter::new(buf.as_mut_slice());

            try!(write!(&mut reader, "{}\n", pid));
            try!(reader.tell())
        };

        let mut pos = 0;

        while pos < len {
            let ptr = unsafe { buf.as_ptr().offset(pos as isize) };
            let ret = check!(libc::write(self.fd, ptr as *const c_void, (len - pos) as size_t));
            pos += ret as u64;
        }

        Ok(())
    }

    pub fn stat(&self) -> SysResult<stat::FileStat> {
        stat::fstat(self.fd)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        debug!("closing file");
        unsafe { libc::close(self.fd); }
    }
}
