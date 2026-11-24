# tm-claims

Toulmin's Claims library (design 4.1, sections 5.6, 7, 11.22, items 63
to 71): claims, refs, revisions, the 7.3 evidence-exists table, the 5.6
flags and labels, reviews, report-span scanning, drift, and the Claims
fold. A library in the sense of 4.2: it calls no other library, takes
Checker results, Files lookups, and report text from the Core, and
returns events (`KnownEvent`s) for the Core to batch; it never writes the
record. Its dependencies are `tm-formats`, `tm-index`, `serde_json`, and
`unicode-normalization`, so it builds and tests without DuckDB.

| Module | Item | What |
| --- | --- | --- |
| `claim` | 63 | `Kind` (7.2), `State` (gate A's `claims.state` and its transitions), `Content` (what every revision restates), `Claim`, `Revision`, `current_revision` (owner claims: the latest), the `claim.added`, `claim.revised`, `claim.retracted`, `claim.superseded`, and `claim.checked` events, and `against` in 11.16's order (the key map's version first) |
| `refs` | 64 | `allocate` (the next `C<n>`, after confirming the index's highest against the chain's last `claim.added`; a disagreement is a `Mismatch`) and `highest_owner_ref` (by number, from `claims`) |
| `evidence_exists` | 65 | `cell`: 7.3 as an exhaustive match, one arm per cell; `Situation::of` (the column reading under 7.3); `evaluate` (runs the citation type's pinned test only where the cell needs it); `Evidence::combine` (the claim's `evidence` and its `detail` text) |
| `flags` | 66 | `Name`: every 5.6 name with its kind, subject, clearing action, and where gate A raises it (Claims fold, labels from rows, scan-derived); the fold's reasons; `span_flag` |
| `reviews` | 67 | `Review` and its `claim.reviewed` event (`E6201` for an unknown ref) |
| `spans` | 68 | `scan`: the 11.22 scanner (logical lines, the one escape, attributes, fences, heading classes) |
| `drift` | 69 | `normalize` (NFC, whitespace runs, trim) and `drift` (same, drifted, or not checkable for inline code) |
| `fold` | 70 | `fold(&mut IndexTx, &Line)` into `reports`, `claims`, `claim_revisions`, `claim_cites`, `claim_keys`, `claim_events`, `verdicts`, `reviews`, and the event-derived `flags` rows; never `spans` |

## Tests

`cargo test -p tm-claims` runs the unit tests, `tests/claims.rs` (every
cell of 7.3 with the pinned test passing and failing, the Retired column,
the column reading, the combination and detail text, and the fold over
11.17's `schema.sql` read from the design: rows, state, both flags,
`claim_keys` replacement, idempotence, a review that leaves verdicts
alone, and that no other table is written), and `tests/spans.rs` (item
71's span fixtures under `tests/fixtures/spans/` and the workspace
`fixtures/report.md`).
