#![allow(non_camel_case_types)]

use std::io::{BufWriter, IoResult, IoError};
use std::os::errno;
use std::mem;
use libc;
use libc::{
    c_void, c_int, c_short, pid_t, mode_t,
    O_CREAT, O_WRONLY, SEEK_SET, EINTR, EACCES, EAGAIN
};
use ffi::{
    flock, O_SYNC, F_SETFD, F_GETLK,
    F_SETLK, F_WRLCK, F_UNLCK, FD_CLOEXEC
};

pub struct File {
    fd: c_int
}

macro_rules! check(
    ($expr:expr) => ({
        let mut ret;

        loop {
            ret = unsafe { $expr };

            if ret < 0 {
                let err = errno() as c_int;

                if err == EINTR {
                    continue;
                }

                return Err(IoError::from_errno(err as uint, false));
            }
            else {
                break;
            }
        }

        ret
    })
)

impl File {
    pub fn open(path: &Path, create: bool, write: bool, mode: u32) -> IoResult<File> {
        let mut flags = O_SYNC;

        if create { flags |= O_CREAT;  }
        if write  { flags |= O_WRONLY; }

        // Open the file descriptor
        let fd = check!(libc::open(path.to_c_str().as_ptr(), flags, mode as mode_t));

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

        let ret = check!(match libc::fcntl(self.fd, F_SETLK, &fl) {
            EACCES | EAGAIN => 1,
            v => v
        });

        Ok(ret == 0)
    }

    pub fn check(&mut self) -> IoResult<pid_t> {
        let mut fl: flock = unsafe { mem::zeroed() };

        fl.l_type = F_WRLCK;
        fl.l_whence = SEEK_SET as c_short;

        check!(libc::fcntl(self.fd, F_GETLK, &fl));

        if fl.l_type == F_UNLCK {
            Ok(0)
        }
        else {
            Ok(fl.l_pid)
        }
    }

    pub fn write(&mut self, pid: pid_t) -> IoResult<()> {
        let mut buf: [u8, ..20] = unsafe { mem::zeroed() };

        let len = {
            let mut reader = BufWriter::new(buf);

            try!(write!(&mut reader, "{}\n", pid));
            try!(reader.tell())
        };

        let mut pos = 0;

        while pos < len {
            let ptr = unsafe { buf.as_ptr().offset(pos as int) };
            pos += check!(libc::write(self.fd, ptr as *const c_void, len - pos)) as u64;
        }

        Ok(())
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd); }
    }
}
