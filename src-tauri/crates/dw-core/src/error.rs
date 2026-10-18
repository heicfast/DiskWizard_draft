//! Typed engine errors.
//!
//! Error-code discipline mirrors the research contract
//! (`artifacts/SearchDLL/searchdll-family--architecture.md`): engine
//! failures are structured, non-fatal where possible (a bad sector
//! degrades one extent, never the scan), and carry a stable code the
//! app layer can surface or log. The vendor format was
//! `HRESULT-style 0x5002xxxx`; we keep a compact enum instead — the
//! semantic contract (open-fail vs read-fail vs corrupt-metadata) is
//! what matters downstream.

use std::fmt;

/// Result alias used across the engine.
pub type CoreResult<T> = Result<T, CoreError>;

/// Engine error taxonomy. Cheap to construct, `Send`, no backtrace
/// bloat (hot paths create these per-record in degraded conditions).
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The device/volume could not be opened or the handle went bad.
    #[error("device open failed: {0}")]
    OpenFailed(String),
    /// A raw read failed at (offset, len) — typically a bad sector.
    #[error("read failed at offset {offset:#x} len {len:#x}: {source}")]
    ReadFailed {
        offset: u64,
        len: u64,
        source: std::io::Error,
    },
    /// The scan was cancelled through its token.
    #[error("cancelled")]
    Cancelled,
    /// The scan was paused (engines treat this as a re-checkpoint).
    #[error("paused")]
    Paused,
    /// Metadata failed structural validation beyond salvage.
    #[error("corrupt {structure}: {detail}")]
    Corrupt {
        structure: &'static str,
        detail: String,
    },
    /// Filesystem not recognized / unsupported for this target.
    #[error("unsupported filesystem: {0:?}")]
    Unsupported(crate::types::FsKind),
    /// A malformed on-disk structure was recovered around (engines
    /// log + skip); distinct from [`CoreError::Corrupt`] which is fatal
    /// for the owning structure.
    #[error("salvage skipped {structure} at {at:#x}: {detail}")]
    SalvageSkipped {
        structure: &'static str,
        at: u64,
        detail: String,
    },
    /// Destination-side failures during recovery.
    #[error("destination error: {0}")]
    Destination(String),
    /// Session (.rsf) format errors.
    #[error("session format: {0}")]
    Session(String),
    /// Anything else (bug-guard: engines should prefer a typed arm).
    #[error("{0}")]
    Other(String),
}

impl CoreError {
    /// True when this error means "the data here is unreadable" — the
    /// grading pass uses this to mark extents Poor without failing.
    #[must_use]
    pub fn is_media_damage(&self) -> bool {
        matches!(self, Self::ReadFailed { .. })
    }

    /// Stable machine code for logging/telemetry (never shown raw).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::OpenFailed(_) => "open_failed",
            Self::ReadFailed { .. } => "read_failed",
            Self::Cancelled => "cancelled",
            Self::Paused => "paused",
            Self::Corrupt { .. } => "corrupt",
            Self::Unsupported(_) => "unsupported_fs",
            Self::SalvageSkipped { .. } => "salvage_skipped",
            Self::Destination(_) => "destination",
            Self::Session(_) => "session",
            Self::Other(_) => "other",
        }
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// Human-oriented formatter used by the app log (keeps Display terse).
pub fn describe(e: &CoreError) -> String {
    fmt::format(format_args!("[{}] {e}", e.code()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(CoreError::Cancelled.code(), "cancelled");
        assert_eq!(
            CoreError::Corrupt {
                structure: "MFT",
                detail: "bad fixup".into()
            }
            .code(),
            "corrupt"
        );
    }

    #[test]
    fn read_errors_classify_as_media_damage() {
        let e = CoreError::ReadFailed {
            offset: 4096,
            len: 512,
            source: std::io::Error::other("CRC"),
        };
        assert!(e.is_media_damage());
        assert!(!CoreError::Cancelled.is_media_damage());
    }
}
