//! # dw-win — the DiskWizard Windows platform layer
//!
//! Everything Windows-specific behind the `dw-core` seams:
//! raw-IO [`BlockDevice`] implementations (`\\.\PhysicalDriveN`,
//! volume handles), physical-disk + volume enumeration (the
//! `list_devices` inventory), SMART multi-transport health reports,
//! BitLocker status/unlock, and the licensing hardware-identity
//! providers (MachineGuid / volume serial / CPUID / DPAPI).
//!
//! This crate only compiles on Windows (`cfg(windows)` gate in the
//! app); every module cites its research artifact lineage.

#![cfg(windows)]

pub mod admin;
pub mod bitlocker;
pub mod blockdev;
pub mod devices;
pub mod license_plat;
pub mod smart;
pub mod volume;
