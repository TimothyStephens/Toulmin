Task: P1 of the Toulmin V1 gate A plan. Read docs/design/toulmin_v1_design.md,
specifically sections 11.1 to 11.23, the Detailed Implementation Plan's phase
table and items 11 to 25, 88, 90, 93, and 94, and the Verification Plan rows
for Conformance: formats and Spec drift. Build exactly those items and nothing
else.

Before writing any file under conformance/formats/, re-read
docs/design/implementation_readiness_report.md, session 2 round 1. Every
Blocker and Major there that is not already resolved in the design must be
resolved by editing the design document and recording the decision in the
commit message, never by deciding it in code first. A conformance case freezes
a format; the design is where a format is decided.

docs/design/runbook-to-p1.md section 6 lists twelve known findings that are
deliberately deferred to the phase where they surface. If you hit one of those
twelve in P1, fix it as that table says and note it. Do not widen the phase to
address the ones belonging to later phases.

Phase checkpoint: cargo test -p tm-formats passes, and
python3 reference/formats/test_formats.py passes, and the two produce identical
IDs for every case under conformance/formats/. Do not start P2.

Report: the checkpoint output, every design edit you made with its reason, and
any item you could not build without inventing something, as a numbered list.
