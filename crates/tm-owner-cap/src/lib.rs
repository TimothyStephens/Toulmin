//! Toulmin owner capability (design 4.1, 4.2 rule 4, item 43; D2).
//!
//! [`OwnerCap`] is the value every Core owner operation and the Record's
//! main-chain append take. It has a private field and no `Clone`, `Copy`,
//! `Debug`, or `Default`, so the only way to hold one is to construct it,
//! and the only constructor is behind the `construct` cargo feature, which
//! `tm-cli` enables and no other crate does. `tm-agent` and `tm-bridge` are
//! built standalone in CI with the feature off, and
//! `scripts/ci/check-owner-cap.sh` fails if any crate other than `tm-cli`
//! names the constructor.

/// Proof that the caller is the owner's `tm` process. Every crate may name
/// this type; only `tm-cli` constructs it.
pub struct OwnerCap(());

#[cfg(feature = "construct")]
impl OwnerCap {
    /// The only constructor, available to `tm-cli` through the `construct`
    /// feature. One construction per process (item 83).
    #[must_use]
    pub fn new_for_cli() -> OwnerCap {
        OwnerCap(())
    }
}
