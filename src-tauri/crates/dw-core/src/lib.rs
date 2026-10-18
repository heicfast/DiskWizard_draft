//! # dw-core — the DiskWizard recovery engine
//!
//! The platform-independent heart of DiskWizard: filesystem scan
//! engines (quick + deep), RAW signature carving, found-file tree
//! building, recoverability grading, the recovery (export) engine and
//! scan-session persistence — all implemented against the
//! [`blockdev::BlockDevice`] seam so the entire engine compiles and
//! unit-tests on any host (the DiskGenie `core/` pattern, per
//! `docs/CORE-PLAN.md` §2).
//!
//! Layering (dependencies point downward only):
//!
//! ```text
//! scan / recover / session        orchestration + persistence
//!   └─ tree / grade               results assembly
//!       └─ fs::* | raw::*         per-FS engines + carving
//!           └─ partition          MBR/EBR/GPT/LDM + lost partitions
//!               └─ blockdev       sector IO abstraction (+image/cache)
//! ```
//!
//! Research lineage — every module cites the artifact it implements:
//! `easeus-drw-research/architecture/05-scan-engine-architecture.md`
//! (engine family + tree building), `artifacts/SearchDLL/*` (per-FS
//! deep dives), `artifacts/PartitionRecovery/*` (tables, RawFixer),
//! `artifacts/SaveScan/*` (.rsf), `artifacts/UIUX/*` (result branches,
//! grading display contract).

pub mod blockdev;
pub mod error;
pub mod fs;
pub mod partition;
pub mod raw;
pub mod recover;
pub mod scan;
pub mod session;
pub mod tree;
pub mod types;

#[cfg(test)]
pub(crate) mod testutil;

pub use error::{CoreError, CoreResult};
pub use types::{
    Category, Extent, FileAttrs, FoundEntry, FsKind, NodeOrigin, Recoverability, Timestamps,
};
