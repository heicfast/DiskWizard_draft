//! RAW carving over hostile media: the signature table + every format
//! grammar (the §5.1-7 bounded-streaming-validator contract — a
//! candidate hit must never allocate unbounded memory or loop
//! forever).
#![no_main]

use dw_core::blockdev::BlockRead;
use dw_core_fuzz::{CounterSink, ram};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let dev = ram(data);
    let cap = dev.capacity_bytes();
    let carver = dw_core::raw::RawCarver::new(dev);
    let mut sink = CounterSink::default();
    let ctrl = dw_core::fs::ScanControl::new();
    let _ = carver.carve(&mut sink, &ctrl, 0, cap);
});
