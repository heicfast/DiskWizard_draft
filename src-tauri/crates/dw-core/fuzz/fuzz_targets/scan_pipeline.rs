//! The FULL controller pipeline: FS probe → partition dispatch →
//! engine quick+deep → carve sweep → tree merge/grading. The
//! highest-level integration fuzz — hostile media end to end.
#![no_main]

use dw_core_fuzz::ram;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let dev = ram(data);
    let mut obs = dw_core::scan::NoObserver;
    let _ = dw_core::scan::run_scan(dev, "fuzz", &mut obs);
});
