//! FAT12/16/32: boot parse → chain walks → deleted reanimation (the
//! 0xE5 path) → LFN assembly.
#![no_main]

use dw_core_fuzz::{drive, ram};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut engine) = dw_core::fs::fat::FatEngine::new(ram(data)) {
        let _ = drive(&mut engine);
    }
});
