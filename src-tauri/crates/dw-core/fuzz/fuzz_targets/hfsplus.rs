//! HFS+/HFSX: TN1150 volume header → catalog B-tree leaf chains →
//! cnid paths → extents overflow → orphaned-cnid sweep.
#![no_main]

use dw_core_fuzz::{drive, ram};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let dev = ram(data);
    if let Ok(mut engine) = dw_core::fs::hfsplus::HfsPlusEngine::new(&*dev) {
        let _ = drive(&mut engine);
    }
});
