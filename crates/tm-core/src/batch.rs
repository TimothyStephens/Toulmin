//! Batch assembly (item 75; 11.21): a queued `chain.torn_tail_truncated`
//! first, then a `lock.taken_over` when the lock was taken over, then the
//! command's events; `seq`, `prev`, `batch`, `epoch`, and IDs follow from
//! `HEAD` and the lock's epoch; the framed batch is handed to the Record's
//! `append` with the `&OwnerCap`. All or nothing: the Record writes the
//! whole batch or none of it.

use tm_formats::envelope::{Actor, Source};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::EventId;
use tm_owner_cap::OwnerCap;
use tm_record::{Batch, Head, Lock, Record};

use crate::Result;

/// The events the lock queued at acquisition (3.1): a torn tail it
/// truncated and a takeover it made, in that order.
///
/// # Errors
/// `E1002` when a body does not validate (never for the lock's own).
pub fn queued(lock: &Lock) -> Result<Vec<KnownEvent>> {
    let mut out = Vec::new();
    if let Some(t) = lock.torn_tail() {
        out.push(KnownEvent::new(
            EventType::ChainTornTailTruncated,
            &t.body(),
        )?);
    }
    if let Some(b) = lock.taken_over_body() {
        out.push(KnownEvent::new(EventType::LockTakenOver, &b)?);
    }
    Ok(out)
}

/// What a commit wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// Every event's ID, in batch order (the queued events first).
    pub ids: Vec<EventId>,
    /// How many queued events preceded the command's own.
    pub queued: usize,
    /// `HEAD` after the append.
    pub head: Head,
}

impl Committed {
    /// The IDs of the command's own events, in order.
    #[must_use]
    pub fn own(&self) -> &[EventId] {
        &self.ids[self.queued..]
    }
}

/// Build and append one batch: the queued events, then `events`.
///
/// # Errors
/// `E1002` from the envelope writer; the Record's `E2102` (fenced out),
/// `E2103`, `E2008`, or `Error::Io`.
pub fn commit(
    record: &Record,
    cap: &OwnerCap,
    lock: &Lock,
    at: &str,
    actor: &Actor,
    src: &Source,
    events: &[KnownEvent],
) -> Result<Committed> {
    let mut all = queued(lock)?;
    let queued = all.len();
    all.extend(events.iter().cloned());
    let head = record.head()?;
    let batch = Batch::build(&head, lock.epoch(), at, actor, Some(src), &all)?;
    let head = record.append(cap, lock, &batch)?;
    Ok(Committed {
        ids: batch.ids().to_vec(),
        queued,
        head,
    })
}

/// The IDs `commit` would give `events` now (the queued events first),
/// without appending: a `claim.checked` names its revision's ID, which is
/// decided by the batch the two share.
///
/// # Errors
/// `E1002` from the envelope writer.
pub fn preview_ids(
    record: &Record,
    lock: &Lock,
    at: &str,
    actor: &Actor,
    src: &Source,
    events: &[KnownEvent],
) -> Result<Vec<EventId>> {
    let mut all = queued(lock)?;
    let queued = all.len();
    all.extend(events.iter().cloned());
    let head = record.head()?;
    let batch = Batch::build(&head, lock.epoch(), at, actor, Some(src), &all)?;
    Ok(batch.ids()[queued..].to_vec())
}
