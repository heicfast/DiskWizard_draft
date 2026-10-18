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

## Results — windows-latest runner (ephemeral SSD, VHD 256 MiB)

Recorded from the `windows-integration` run on commit `0f2d98e`+ (the
first run carrying the bench step; latest run link in the workflow
history — the table below is transcribed verbatim from the
`§5.1-10 IO backend benchmark` step log).

| window | QD | MiB/s | CPU s | ΔCPU/MiB | WS after (MiB) |
|---:|---:|---:|---:|---:|---:|
| 1024 KiB | 1 | _pending first run_ | | | |
| 2048 KiB | 1 | | | | |
| 2048 KiB | 2 | | | | |
| 4096 KiB | 2 | | | | |
| 4096 KiB | 4 | | | | |
| 8192 KiB | 4 | | | | |
| 8192 KiB | 8 | | | | |
| 16384 KiB | 8 | | | | |

**Numbers land here within one CI cycle of the harness merge** — the
session agent transcribes them from the run log (the instruction
honesty rule: no fabricated benchmarks; the cells stay empty until a
real run fills them).

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
