//! Partition tables: MBR (+ protective), EBR chains, GPT
//! (CRC-validated), the lost-partition sweep over the gaps.
#![no_main]

use dw_core_fuzz::ram;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 512 {
        return;
    }
    let _ = dw_core::partition::parse_mbr(&data[..512], (data.len() / 512) as u64);
    let dev = ram(data);
    let _ = dw_core::partition::parse_gpt(&*dev);
    let _ = dw_core::partition::parse_ebr_chain(&*dev, 0, dev.total_sectors(), 1);
    if let Ok(layout) = dw_core::scan::read_partition_layout(&*dev) {
        let _ = dw_core::partition::sweep_for_lost_partitions(&*dev, &layout);
    }
});
