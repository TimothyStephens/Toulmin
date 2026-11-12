# Workspace fixtures (design item 82a)

One small project every suite shares: `tm-files` tests (item 52), the
scenario tests (item 82), the CLI tests (item 86), and the smoke test.
The data is invented; its only purpose is that each claim below has one
determinate answer under 11.18.

## Files and handles

Registered in this order, so the handles are fixed:

| Handle | File | Spec | Key |
| --- | --- | --- | --- |
| `F1` | `de.tsv` (gene_id, log2FoldChange, padj, baseMean) | `specs/de.json` | `gene_id` |
| `F2` | `samples.tsv` (sample_id, condition, batch) | `specs/samples.json` | `(condition, batch)` |
| `F3` | `counts.tsv` (gene_id, S1 to S6) | `specs/counts.json` | `gene_id` |
| `F4` | `figure.pdf` (two pages) | | |
| `F5` | `figure.png` (panels `A` and `B`) | | |
| `T1` | `report.md` (ten spans, `C1` to `C10`) | | |

`de.tsv` has twelve genes; `G12` has `padj` `NA`, which the spec reads as
missing. `samples.tsv` has six samples, three per condition, one per
batch, so `(condition, batch)` is a key. `counts.tsv` has integer counts
whose control triples are divisible by three, so a mean of them is exact.

## The ten claims and their intended answers

Each `claims/<n>/` holds `args.txt` (the `tm claim add` arguments, one
per line, unquoted), and `steps.json` and `assert.json` where the kind
needs them. `expected.json` holds each claim's verdict and flags and is
a stub (`null` per claim) until P8 fills it from the scenario tests; the
answers below are the design's intent, stated in prose on purpose so no
test reads them.

1. **C1** (`filter` + `sort` + `take`, `rank`). Of the eight genes with
   `padj < 0.05` (G12's missing `padj` drops it), sorted by fold change
   descending, the first five are G01, G02, G03, G04, G05. G05's rank is
   5: **holds**.
2. **C2** (`filter`, `value`). Among the three genes with `padj < 0.01`
   (G01, G10, G11), G10's `log2FoldChange` is `-2.40`; the claim asserts
   `-2.50` within `0.01`: **fails**. The paper overstates it; this is the
   fixture's one wrong claim.
3. **C3** (`derive`, `group` with `mean` and `count`, `value`). `up` is
   `log2FoldChange > 0`; the `true` group holds G01 to G07 and G12, eight
   genes: `n` is 8, **holds**. (`mean_lfc` of that group is 1.8625.)
4. **C4** (`join` + `derive`, `compare_rows`). Inner join with `counts`;
   `ctrl_mean` is (S1 + S2 + S3) / 3: 110 for G01, 300 for G11; G01's is
   less: **holds**.
5. **C5** (`rank` with `scope: group`, `exists`). Within each `up`
   group, rank by `baseMean` descending; rank 1 is G05 (200.0) among the
   upregulated and G10 (110.0) among the downregulated; after
   `filter rank = 1`, G10 is present: **holds**.
6. **C6** (`select` + `filter`, `count`). Keep `gene_id` and `padj`,
   filter `padj < 0.01`: G01, G10, G11, three rows: **holds**.
7. **C7** (qualitative, `whole_file`). Cites `F2@v1`; the evidence
   exists: **holds**, no system check beyond that.
8. **C8** (qualitative, `page`). Cites page 2 of `F4@v1`, which has two
   pages: **holds**.
9. **C9** (qualitative, `panel`). Cites panel `B` of `F5@v1`, registered
   with panels `A` and `B`: **holds**.
10. **C10** (mixed, `cells` with a composite key, `value`). Cites the row
    `(treated, 2)` of `F2@v1` with no steps; its `sample_id` is `S5`:
    **holds**.

So nine claims hold and one (C2) fails, which gives the static view's
G1 scan something to show.

## ACCEPTANCE.md

The owner's checklist for the manual gate A exit criterion, signed by
date in the file.
