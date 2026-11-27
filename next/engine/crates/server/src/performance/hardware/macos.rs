use super::Hardware;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

unsafe extern "C" {
    fn sysctlbyname(
        name: *const c_char,
        out: *mut c_void,
        size: *mut usize,
        input: *mut c_void,
        input_size: usize,
    ) -> c_int;
}

fn number(name: &CStr) -> Option<u64> {
    let mut bytes = [0u8; 8];
    let mut len = bytes.len();
    let rc = unsafe {
        sysctlbyname(
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    match (rc, len) {
        (0, 4) => Some(u32::from_ne_bytes(bytes[..4].try_into().unwrap()).into()),
        (0, 8) => Some(u64::from_ne_bytes(bytes)),
        _ => None,
    }
}

pub(super) fn platform(hw: &mut Hardware) {
    hw.ram_mb = number(c"hw.memsize").map(|bytes| bytes / (1024 * 1024));
    hw.physical_cores = number(c"hw.physicalcpu").and_then(|n| n.try_into().ok());
    if number(c"hw.nperflevels").is_some_and(|n| n > 1) {
        hw.performance_cores = number(c"hw.perflevel0.physicalcpu").and_then(|n| n.try_into().ok());
    }
}
