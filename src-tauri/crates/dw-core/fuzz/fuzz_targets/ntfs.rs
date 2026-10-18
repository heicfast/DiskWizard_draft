//! NTFS: boot parse → MFT walk (fixups, attributes, runlists,
//! resident streams) → deleted-viability $Bitmap scoring.
#![no_main]

use dw_core_fuzz::{drive, ram};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut engine) = dw_core::fs::ntfs::NtfsEngine::new(ram(data)) {
        let _ = drive(&mut engine);
    }
});
