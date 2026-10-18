//! ext2/3/4: superblock gates → group descriptors → inode
//! tables/extent trees → deleted-name shapes → orphan sweep.
#![no_main]

use dw_core_fuzz::{drive, ram};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut engine) = dw_core::fs::ext::ExtEngine::new(ram(data)) {
        let _ = drive(&mut engine);
    }
});
