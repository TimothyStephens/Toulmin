# 11.20 pages against the read interface

Date: 2026-11-10. For each of the six gate A pages, does every query named in the Queries column return every field the Sections column requires? Checked against section 9's read-interface table (lines 692-717) and 11.20 (line 1093 onward). Six findings. Nothing here needs fixing before P1; all of it lands on item 78 at P7 except where noted.

---

## 1. `claims.evidence` has no paging parameter, but the claim page pages (Major)

11.20's claim page says cited rows are "paged at `view.rows_per_page` so a claim over a large final table still renders". `claims.evidence` takes one parameter, `ref`, and returns every cited row. `files.rows`, by contrast, takes "key values or row range".

So the page can only page client-side, and 11.20 also says every page embeds its source data as `tm-data`. A claim over a 200,000-row final table therefore embeds 200,000 rows in the HTML and pages the DOM, which is not what the sentence claims to do.

This is my own doing: the setting was added on 2026-11-10 when the owner chose no cap on `claims.max_final_rows`, and I did not carry it back to the query.

Options:
- (a) `claims.evidence` gains a `range` parameter like `files.rows`, and the page writes one `tm-data` entry per page of rows. Honest, and it makes `view --out` write N files for a large claim.
- (b) `claims.evidence` gains a `limit` parameter; the page embeds the first `view.rows_per_page` rows and states plainly that the rest are available through `tm claim evidence --json`. Simplest, and the static view is explicitly not the complete interface.
- (c) Revert to a cap on final rows, which is what I originally recommended.

Recommendation: (b). The static view is for reading evidence a human can review; a claim over 200,000 rows is not reviewable in any layout, and the CLI keeps the data reachable.

## 2. The claim page shows "flags with clearing actions"; its query does not return actions (Major)

`claims.show` returns "Full claim, all revisions, verdicts with SQL and versions, reviews, flags, spans". Only `flags.list` promises "Open flags with reasons and next command". The claim page's Sections column requires the clearing action, so as specified it cannot render it.

Options:
- (a) Add `flags.list` (by subject) to the claim page's Queries column.
- (b) Make `claims.show`'s flags carry reason and next command, the same shape `flags.list` returns.

Recommendation: (b), and say in section 9 that a flag is always returned in one shape wherever it appears. One shape, one renderer, and the G9 parity test covers it once.

## 3. Three pages call one query more than once; `tm-data` is keyed by query name (Major, already logged)

11.20: `tm-data` holds "one object keyed by query name (with the parameters used as a nested `params` object)". But:
- the claim page calls `files.lineage` once per cited version, and `claims.citing_rows` once per cited version and key set;
- the file page calls `claims.list` once per version.

Keyed by name, the second call overwrites the first, and the G9 parity test compares whichever survived.

This is the `tm-data` block layout finding already in `runbook-to-p1.md` section 6, deferred to P8. This check confirms which pages trigger it and how often: it is not an edge case, it fires on the two most important pages in the gate A set.

Fix, when P8 reaches it: `tm-data` becomes a list of `{"query","params","result"}` rather than an object keyed by query name.

## 4. Two queries are listed but no section renders them (Minor)

- `reports/<report>.html` names `claims.list` (by report); the only section is spans.
- `files/<file>.html` names `files.lineage` (latest version); the sections are identity, versions, parse spec, and citing claims.

Either a section is missing or the query should be dropped. This is not free to leave: the G9 parity test compares every embedded query against the CLI's `--json`, so a query nothing renders is still a maintained contract.

Recommendation: drop `files.lineage` from the file page (gate A lineage is a single node, so it shows nothing), and give the report page a section that uses `claims.list` - a per-span verdict and flag summary beside each span, which is what makes the report page worth opening.

## 5. The report page requires file order; the query does not promise it (Nit)

Sections say "spans in file order". `reports.spans` returns "Spans with refs, recorded sentence, current text, drift state" with no stated ordering. The `spans` table has `line`, so the data exists.

Recommendation: state the ordering in section 9's return column, as 11.18.2 does for every operation that produces an order.

## 6. The audit page links to the audit report; the summary may not carry the link (Nit)

`audit.html` shows "the last audit summary with a link to its report". `audit.last` returns "Summary of the last audit". `audit.completed`'s body is `{"report_hash","since","counts"}` - a hash, not a path. `tm audit --out DIR` writes into `DIR/audit/`, so the path is a convention of the writing command, not a recorded fact.

Recommendation: state that `audit.last` returns the report's relative path within the export directory alongside `report_hash`, or that the page constructs `audit/<report_hash>.html` by convention. Either works; it needs choosing before item 79.

---

## Not findings

`index.html`, `flags.html`, and the identity, versions, and parse spec sections of `files/<file>.html` all check out: every field a section names is in the return of a query the page already calls.
