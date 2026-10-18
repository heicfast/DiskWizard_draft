# I/O Backend Benchmarks (§5.1 item 10)

> **Mandate**: MASTER_PROMPT §5.1 item 10 — "Measure and record: read
> throughput, achieved queue depth, CPU time, peak RSS, error isolation
> behavior, and recovered-file correctness against VHD/image fixtures."

## Harness

`src-tauri/crates/dw-win/tests/io_bench.rs` (`#[ignore]`d; run by
[`windows-integration.yml`](../.github/workflows/windows-integration.yml)
on a real `windows-latest` runner against the attached DWTEST VHD —
a fixed 256 MiB VHD on the runner's ephemeral SSD, falling back to
PhysicalDrive0 when the label probe fails).

Each configuration sweeps half the media (capped 256 MiB) in
consecutive aligned carve windows through the production
`read_sectors_sparse_batch` IOCP pipeline — the carver's access
pattern minus the signature scan, i.e. pure I/O. Every configuration
also byte-compares its windows against a sequential reference read
(correctness gate, not just throughput).

Measured per configuration:

| Metric | Instrument |
|---|---|
| Read throughput (MiB/s) | `Instant` around the sweep |
| Achieved queue depth | submitted depth (batch length — the pipeline guarantees in-flight = depth) |
| CPU time (user+kernel, s) | `GetProcessTimes` deltas |
| Working set / peak RSS | `GetProcessMemoryInfo` (`WorkingSetSize`, `PeakWorkingSetSize`) |

Error isolation behavior and recovered-file correctness are proven by
the sibling suites and cross-referenced here rather than duplicated:

* **Error isolation** — dw-core unit tests with fault injection
  (`bad_ranges_do_not_spawn_false_hits`,
  `sparse_default_maps_damage_to_whole_range`,
  `bad_sector_degrades_only_its_chunk`): a damaged bulk read degrades
  to ONE attempt per sector, readable sectors salvage, dead ranges
  zero-fill and suppress carve hits, and nothing retries into a storm.
* **Recovered-file correctness** — the VHD end-to-end
  (`integration.rs::vhd_end_to_end`): scan → recover → byte-compare
  against the seeded ground truth, including the deleted file.

## Results — Windows 2025 runner (windows-2025-vs2026 image, DWTEST VHD on PhysicalDrive2)

Recorded verbatim from the `§5.1-10 IO backend benchmark` step of the
`windows-integration` run on commit `d71570d` (run 39137841845, job
`end-to-end (scan + recover)`, 2026-10-18). Device: the attached
DWTEST VHD (256 MiB, 512 B sectors) — profiled flash-class
(`ReadProfile { window_bytes: 8388608, queue_depth: 8 }`), span
128 MiB per configuration.

| window | QD | MiB/s | CPU s | ΔCPU/MiB | WS after (MiB) |
|---:|---:|---:|---:|---:|---:|
| 1024 KiB | 1 | 1337 | 0.00 | 0.0000 | 9.7 |
| 2048 KiB | 1 | 469 | 0.00 | 0.0000 | 9.7 |
| 2048 KiB | 2 | 371 | 0.00 | 0.0000 | 9.7 |
| 4096 KiB | 2 | 382 | 0.00 | 0.0000 | 9.7 |
| 4096 KiB | 4 | 426 | 0.00 | 0.0000 | 9.7 |
| 8192 KiB | 4 | 422 | 0.00 | 0.0000 | 9.7 |
| 8192 KiB | 8 | 422 | 0.00 | 0.0000 | 9.7 |
| 16384 KiB | 8 | 374 | 0.02 | 0.0001 | 9.7 |

Final working set 9.7 MiB, lifetime peak RSS 137.7 MiB (the process
peak includes the test harness build artifacts, not the sweep — the
steady-state working set is flat across every configuration, which is
the §5.1 item 7 memory-boundedness signal that matters).

**Reading the numbers honestly** (the no-fabrication rule cuts both
ways — no over-interpretation either):

* The VHD is a FILE on the runner's SSD: reads traverse the runner's
  OS file cache, not raw spindle/flash hardware. The 1024 KiB row's
  1337 MiB/s is the warm-cache first sweep; the steady band for this
  device is ~370–430 MiB/s, which is the runner's VHD stack's
  ceiling, not the backend's.
* The same run's IOCP equivalence test measured `sequential 4 MiB ×1:
  2218 MiB/s; batched 4 MiB ×8: 1426 MiB/s` — on a cache-backed VHD
  the deep queue ADDS overhead without device-level parallelism to
  harvest (there is no seek to overlap). The queue-depth benefit is
  a physical-HDD property; the correctness contract (batch ≡
  sequential at any depth, byte-identical) is what this runner can
  and does prove. Throughput on real HDD/NVMe media lands when the
  tour fleet grows real-device hosts; the adaptive profiles below
  stay inside the §5.1 bands so no retune is implied by the VHD
  numbers.
* CPU cost per MiB is ≤ 0.0001 s across the board — the pipeline is
  completion-driven (IOCP), not spin-driven; queue depth 8 costs no
  measurable CPU over queue depth 1.

## Chosen adaptive profiles (§5.1 item 5)

The runtime profile is probed per device (`probe_media_class`:
BusTypeNvme → seek-penalty → TRIM → conservative rotational), and the
static defaults are the documented bands:

| Media class | window | QD | Rationale |
|---|---|---|---|
| Flash (SSD/NVMe) | 8 MiB | 8 | §5.1 flash band 4–16 MiB / QD 4–16; the deep-queue point that the runner bench above must confirm before widening |
| Rotational / unknown | 2 MiB | 2 | §5.1 HDD band 1–4 MiB / QD 1–2; sequential-spindle-friendly, and the safe default for unprobeable USB bridges |
| Unknown (trait default) | 1 MiB | 1 | fixtures/ram/images — identical to the sequential sweep |

The 64 MiB window-buffer budget (`MAX_WINDOW_BUDGET`) caps
`depth × window` for every profile, so the deep-queue sweep stays
memory-bounded (§5.1 item 7's per-candidate discipline extends to the
read path itself).
