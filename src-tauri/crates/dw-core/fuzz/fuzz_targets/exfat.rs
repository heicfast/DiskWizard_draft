//! exFAT: boot region → entry sets (InUse triples) → the QA-5
//! chain-resolution ladder (FAT walk, NoFatChain, damaged-FAT
//! re-derivation).
#![no_main]

use dw_core_fuzz::{drive, ram};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut engine) = dw_core::fs::exfat::ExFatEngine::new(ram(data)) {
        let _ = drive(&mut engine);
    }
});
