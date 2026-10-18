//! ReFS: DBR gate → SUPB/CHKP/MSB page census → checkpoint
//! newest-generation selection (structure-level only — carving
//! covers content, the honest M3 boundary).
#![no_main]

use dw_core_fuzz::{drive, ram};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let dev = ram(data);
    if let Ok(mut engine) = dw_core::fs::refs::RefsEngine::new(&*dev) {
        let _ = drive(&mut engine);
    }
});
