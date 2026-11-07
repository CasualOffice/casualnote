//! Encrypted sync through any folder (docs/56). No I/O and no network here: the host performs what
//! a round asks for, using four verbs on storage. Each device writes only its own new objects.
//! The cryptography has had no outside review.

pub mod crypto;
mod session;

pub use crypto::{KdfParams, KeyRecord, Keys, Refusal};
pub use session::key_record_key as session_key_record_key;
pub use session::{
    Config, DeviceRow, Entry, Failure, HostRequest, HostResponse, ROOT_KEY, ReviewMarker,
    RootState, RoundSummary, Session, Status, Step, StorageOutcome, StorageRequest, SyncState,
    new_root, read_root,
};
