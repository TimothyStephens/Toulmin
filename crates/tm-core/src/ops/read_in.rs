//! Read-in (section 9's `read_in` row; 3.1; item 74).
//!
//! Every locked command first reads in: in gate A there are no journals
//! and no receipts, so read-in scans nothing, and the entry point exists so
//! every locked command has one path. What it does produce is the lock's
//! own queue (3.1): a `chain.torn_tail_truncated` for a tail truncated at
//! lock time and a `lock.taken_over` for a takeover, which `batch.rs` puts
//! first in the command's batch.

use tm_record::Lock;

use crate::Result;
use crate::batch;

/// What read-in found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadIn {
    /// A torn tail was truncated at lock time (`chain.torn_tail_truncated`
    /// is queued).
    pub torn_tail: Option<tm_record::TornTail>,
    /// The lock was taken over from a stale holder (`lock.taken_over` is
    /// queued).
    pub taken_over: Option<tm_record::LockHolder>,
    /// Journal lines read in (gate B).
    pub journal_lines: u64,
    /// Receipts read in (gate C).
    pub receipts: u64,
}

/// Run read-in under `lock`.
///
/// # Errors
/// `E1002` if a queued body does not validate (never for the lock's own).
pub fn read_in(_core: &mut super::Core, lock: &Lock) -> Result<ReadIn> {
    // Validate the queued bodies now, so a failure surfaces before the
    // command does its own work.
    let _ = batch::queued(lock)?;
    Ok(ReadIn {
        torn_tail: lock.torn_tail(),
        taken_over: lock.taken_over().cloned(),
        journal_lines: 0,
        receipts: 0,
    })
}
