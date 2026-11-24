//! `C<n>` allocation (item 64; D16). The Core reads the highest `C<n>`
//! from the index's `claims` table ([`highest_owner_ref`]) and the ref of
//! the chain's last `claim.added` whose ref is a `C<n>`, and allocates
//! under the lock with [`allocate`], which confirms the two agree.
//! `S<n>.<m>` allocation arrives with gate B; both spellings are parsed by
//! `tm_formats::id::ClaimRef` (item 12).

use std::fmt;

use tm_formats::id::ClaimRef;
use tm_index::IndexTx;

use crate::Result;

/// The index and the chain disagree on the highest `C<n>`. What the Core
/// does then is not settled (runbook section 6); this carries both
/// numbers, 0 meaning none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mismatch {
    /// The highest `C<n>` in the index's `claims` table.
    pub index: u64,
    /// The `C<n>` of the chain's last owner `claim.added`.
    pub chain: u64,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the index's highest owner ref is C{} but the chain's last claim.added is C{}",
            self.index, self.chain
        )
    }
}

impl std::error::Error for Mismatch {}

fn owner_number(r: Option<ClaimRef>) -> u64 {
    match r {
        Some(ClaimRef::Owner(n)) => n,
        _ => 0,
    }
}

/// The next `C<n>`: one above the highest, `C1` for the first claim.
/// `index_highest` is what [`highest_owner_ref`] returned; `chain_last`
/// the ref of the chain's last `claim.added` with a `C<n>` ref.
///
/// # Errors
/// [`Mismatch`] when the two differ.
pub fn allocate(
    index_highest: Option<ClaimRef>,
    chain_last: Option<ClaimRef>,
) -> std::result::Result<ClaimRef, Mismatch> {
    let index = owner_number(index_highest);
    let chain = owner_number(chain_last);
    if index != chain {
        return Err(Mismatch { index, chain });
    }
    Ok(ClaimRef::Owner(index + 1))
}

/// The highest `C<n>` in the index's `claims` table, by number (`C10`
/// is above `C9`).
///
/// # Errors
/// `Error::Index` when SQLite refuses.
pub fn highest_owner_ref(tx: &mut IndexTx<'_>) -> Result<Option<ClaimRef>> {
    let rows = tx.query_rows("SELECT ref FROM claims WHERE ref LIKE 'C%'", &[])?;
    Ok(rows
        .iter()
        .filter_map(|r| r.text(0).and_then(ClaimRef::parse))
        .filter(|r| matches!(r, ClaimRef::Owner(_)))
        .max_by_key(|r| owner_number(Some(*r))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation() {
        assert_eq!(allocate(None, None), Ok(ClaimRef::Owner(1)));
        assert_eq!(
            allocate(Some(ClaimRef::Owner(9)), Some(ClaimRef::Owner(9))),
            Ok(ClaimRef::Owner(10))
        );
        assert_eq!(
            allocate(Some(ClaimRef::Owner(3)), Some(ClaimRef::Owner(4))),
            Err(Mismatch { index: 3, chain: 4 })
        );
        assert_eq!(
            allocate(None, Some(ClaimRef::Owner(1))),
            Err(Mismatch { index: 0, chain: 1 })
        );
    }
}
