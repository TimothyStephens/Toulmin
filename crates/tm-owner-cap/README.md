# tm-owner-cap

The `OwnerCap` type of design 4.2 rule 4: a value with a private field and
no `Clone`, `Copy`, `Debug`, or `Default`, taken by reference by every
Core owner operation and by `tm-record`'s main-chain `append`. Its only
constructor, `OwnerCap::new_for_cli`, exists behind the `construct` cargo
feature, which `tm-cli` enables and no other crate does.

`scripts/ci/check-owner-cap.sh` fails when any crate other than this one
and `tm-cli` names `new_for_cli`, and when a binary crate other than
`tm-cli` enables the feature. Tests of the crates below `tm-cli` therefore
never hold a capability: `tm-record`'s own tests and its generator append
through a crate-private path behind the public `append` (items 40 and 42).
