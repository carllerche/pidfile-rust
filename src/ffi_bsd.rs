#![allow(non_camel_case_types)]

use libc::{off_t, pid_t, c_int, c_short};
pub use libc::O_SYNC;

#[repr(C)]
pub struct flock {
    pub l_start: off_t,
    pub l_len: off_t,
    pub l_pid: pid_t,
    pub l_type: c_short,
    pub l_whence: c_short,

    // not actually here, but brings in line with freebsd
    pub l_sysid: c_int,
}

pub static FD_CLOEXEC: c_int = 1;
pub static F_UNLCK: c_short  = 2;
pub static F_WRLCK: c_short  = 3;
pub static F_SETFD: c_int    = 2;
pub static F_GETLK: c_int    = 7;
pub static F_SETLK: c_int    = 8;
