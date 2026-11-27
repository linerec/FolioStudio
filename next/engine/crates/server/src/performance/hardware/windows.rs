use super::Hardware;
use std::{collections::BTreeSet, ffi::c_void, ptr};

#[repr(C)]
#[derive(Default)]
struct Memory {
    size: u32,
    load: u32,
    physical_bytes: u64,
    remaining: [u64; 6],
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(status: *mut Memory) -> i32;
    fn GetSystemCpuSetInformation(
        buf: *mut u8,
        len: u32,
        written: *mut u32,
        process: *mut c_void,
        flags: u32,
    ) -> i32;
}

pub(super) fn platform(hw: &mut Hardware) {
    let mut mem = Memory {
        size: size_of::<Memory>() as u32,
        ..Memory::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut mem) } != 0 {
        hw.ram_mb = Some(mem.physical_bytes / (1024 * 1024));
    }
    let mut len = 0;
    unsafe {
        GetSystemCpuSetInformation(ptr::null_mut(), 0, &mut len, ptr::null_mut(), 0);
    }
    if len == 0 || len > 4 * 1024 * 1024 {
        return;
    }
    let mut words = vec![0u64; (len as usize).div_ceil(8)];
    if unsafe {
        GetSystemCpuSetInformation(words.as_mut_ptr().cast(), len, &mut len, ptr::null_mut(), 0)
    } == 0
    {
        return;
    }
    // The API returned the initialized byte count in the aligned buffer.
    if len as usize > words.len() * size_of::<u64>() {
        return;
    }
    let mut records =
        unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), len as usize) };
    let mut cores = BTreeSet::new();
    while records.len() >= 8 {
        let size = u32::from_le_bytes(records[..4].try_into().unwrap()) as usize;
        if size < 8 || size > records.len() {
            return;
        }
        if records[4..8] == [0; 4] && size >= 20 {
            cores.insert((
                records[18],
                u16::from_le_bytes([records[12], records[13]]),
                records[15],
            ));
        }
        records = &records[size..];
    }
    if let (Some(first), Some(last)) = (cores.first(), cores.last()) {
        hw.physical_cores = Some(cores.len());
        if first.0 != last.0 {
            hw.performance_cores = Some(cores.iter().filter(|c| c.0 == last.0).count());
        }
    }
}

#[repr(C)]
struct DisplayDevice {
    size: u32,
    name: [u16; 32],
    description: [u16; 128],
    flags: u32,
    device_id: [u16; 128],
    device_key: [u16; 128],
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumDisplayDevicesW(
        device: *const u16,
        index: u32,
        info: *mut DisplayDevice,
        flags: u32,
    ) -> i32;
}

pub fn adapters() -> Option<Vec<String>> {
    let mut names = BTreeSet::new();
    for index in 0..128 {
        let mut device: DisplayDevice = unsafe { std::mem::zeroed() };
        device.size = size_of::<DisplayDevice>() as u32;
        if unsafe { EnumDisplayDevicesW(ptr::null(), index, &mut device, 0) } == 0 {
            break;
        }
        let len = device
            .description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(device.description.len());
        if len > 0 {
            names.insert(String::from_utf16_lossy(&device.description[..len]));
        }
    }
    (!names.is_empty()).then(|| names.into_iter().collect())
}
