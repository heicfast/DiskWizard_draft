//! The RawFixer model (SDR): boot diagnosis (FAT/exFAT/NTFS geometry
//! gates, FS-specific backups, exFAT CRC32) + plan construction. The
//! write path is caller-mediated and stays out of fuzz scope.
#![no_main]

use dw_core_fuzz::ram;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let dev = ram(data);
    let d = dw_core::repair::diagnose(&*dev);
    let _ = dw_core::repair::plan(&*dev, &d);
});
