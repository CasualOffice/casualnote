//! The durable log (docs/23): per-document segments of checksummed frames, group commit, crash
//! recovery by scanning to the last good frame, snapshots and compaction. There is no I/O here
//! and no `cfg`: the store hands the host ordered requests, and "saved" is said only when they
//! have all completed, including the flush.

mod durable;
pub mod format;
pub mod host;
mod store;

pub use durable::{
    CompactOptions, CompactionPlan, Durable, GuardVerdict, base_snapshot_requests, plan_compaction,
};
pub use host::{CrashMode, Crashed, ImageFile, IoError, IoRequest, MemHost};
pub use store::{
    BatchOutcome, IoBatch, Issue, OpenRefused, Recovered, RecoveredDoc, SaveState, SealedSegment,
    SegmentPart, Store, StoreConfig, Ticket, doc_dir_files, local_path,
};
