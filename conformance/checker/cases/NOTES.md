# Arithmetic behind checker/cases
Written by tools/write_cases.py from the values worked by hand; one entry per case.

## empty_ops

No step. Final table is the input in key order (BRCA1 < EGFR < KRAS < MDM2 < TP53 in binary order). count 5 = 5: holds.

## filter_gt_dec

fc > 1.0: BRCA1 0.5 false; EGFR 3.0 true; KRAS missing -> missing, dropped; MDM2 -1.5 false; TP53 2.5 true. 2 rows. count 2 = 2: holds.

## filter_or_three_valued

BRCA1: false or false = false. EGFR: true or missing = true. KRAS: missing or true (0.010 < 0.050) = true. MDM2: false or true = true. TP53: true or true = true. 4 rows. exists BRCA1: absent, fails.

## filter_and_missing

BRCA1: false and false = false. EGFR: true and missing = missing, dropped. KRAS: missing and true = missing, dropped. MDM2: false and true = false. TP53: true and true = true. 1 row. count 1 = 1: holds.

## filter_not

Grammar: not := 'not' not | cmp, so the operand of not is a whole comparison and the author's parentheses are not required by precedence; {expr} drops them. BRCA1: not false = true. EGFR: not true = false. KRAS: not missing = missing, dropped. MDM2 true. TP53 false. 2 rows. count 2 = 2: holds.

## filter_str_eq

tissue = "liver": MDM2 and TP53 only. MDM2.n = 5 = 5: holds.

## filter_coalesce

coalesce(fc, 0): KRAS missing -> 0 (int widened to dec), 0 > 1 false. BRCA1 0.5 false; EGFR 3.0 true; MDM2 -1.5 false; TP53 2.5 true. 2 rows. exists KRAS: absent, fails.

## filter_int_vs_dec

int n against dec 2.5 compares as decimals: 3, 7, 5, 4 > 2.5 true; KRAS 2 false. 4 rows. count 4 >= 4: holds.

## filter_true_literal

The literal true keeps every row. 5 of 5. count 5 = 5: holds.

## filter_missing_literal

Any comparison with a missing operand is missing, so fc = missing is missing for every row and every row is dropped. 0 rows; final_rows_hash and key_map_hash are the hashes of the empty list. count 0 = 0: holds.

## filter_unary_minus

-1 is unary minus applied to the literal 1; rendered with no space after it. Only MDM2 (-1.5) is below -1. MDM2.fc = -1.5: holds.

## filter_backtick_columns

`p-value` has a hyphen and `or` is reserved, so both keep their backticks. r1: 0.010 < 0.050 and 1.5 > 1.0 true. r2: 0.200 < 0.050 false. r3: 0.040 < 0.050 and 2.0 > 1.0 true. 2 of 3. count 2 = 2: holds.

## filter_str_nfc

Row 1 is written in the CSV as 'cafe' + U+0301 (decomposed); the reader normalizes it to NFC 'café' (U+00E9), so the final row carries the precomposed form and the NFC-byte comparison with the literal matches. Row 2 'cafe' does not. 1 of 2. value eq by NFC bytes: holds.

## filter_str_vs_number_error

tissue is str and 1 is a number: 'Comparing a string with a number ... is a validation error E6004 type mismatch, raised before DuckDB runs'. No step runs; no rows, no hashes.

## select_two

select keeps the listed columns and the key column gene_id; columns stay in table order. Two-item list renders 'fc and tissue'. TP53.fc = 2.5: holds.

## select_three_with_key

Three listed columns render with ', and ' before the last. The kept columns are shown in table order (gene_id, p, n). count 5 = 5: holds.

## sort_asc_missing_last

fc ascending: MDM2 -1.5, BRCA1 0.5, TP53 2.5, EGFR 3.0, then KRAS (missing) last. TP53 is at position 3 = 3: holds.

## sort_desc_missing_first

Missing first: KRAS, then fc descending EGFR 3.0, TP53 2.5, BRCA1 0.5, MDM2 -1.5. KRAS at position 1: holds.

## sort_two_columns

tissue ascending (binary): breast < liver < lung. Within breast fc descending: EGFR 3.0, BRCA1 0.5. Within liver: TP53 2.5, MDM2 -1.5. lung: KRAS. TP53 at position 3: holds.

## sort_tiebreak_key

Ties within a tissue break by gene_id ascending: breast BRCA1, EGFR; liver MDM2, TP53; lung KRAS. EGFR at position 2: holds.

## sort_three_columns

tissue descending: lung, liver, breast ('li' < 'lu' in binary). liver by n ascending: TP53 4, MDM2 5. breast: BRCA1 3, EGFR 7. fc never decides. Third sort column is rendered with a second ', then '. MDM2 at position 3: holds.

## sort_str_binary

Binary order of UTF-8 bytes: 'A' 0x41 < 'B' 0x42 < 'a' 0x61 < 'c' 0x63, so Apple (id 4), Banana (2), apple (1), cherry (3). id 1 at position 3: holds.

## sort_date_desc

d descending chronologically: b 2026-03-01, a 2026-01-05, c 2025-12-31, then d (missing) last. c at position 3: holds.

## take_after_sort

Sorted: EGFR 3.0, TP53 2.5, BRCA1 0.5, MDM2 -1.5, KRAS. First 2: EGFR, TP53. exists TP53: holds.

## take_zero

take.n may be 0. Final table empty; hashes of the empty list. count 0 = 0: holds.

## take_beyond_end

Only 5 rows exist, so take 10 keeps all 5 in sorted order; {k} is 10 as written. count 5 <= 5: holds.

## take_after_rank

rank gives a total order (rank ascending then key): b 1, c 1, a 3, e 4, d 5. First 2: b, c. count 2 = 2: holds.

## rank_min

score descending: b 20, c 20, a 10, e 5, d missing last. SQL RANK: b 1, c 1, a 3, e 4, d 5. Order rank ascending then key. a.rank = 3: holds.

## rank_max

Inverse RANK gives a tie group its largest position: b 2, c 2, a 3, e 4, d 5. b.rank = 2: holds.

## rank_dense

DENSE_RANK: b 1, c 1, a 2, e 3, d 4. e.rank = 3: holds.

## rank_ordinal

ROW_NUMBER ordered by score then key: b 1, c 2 (key b < c), a 3, e 4, d 5. c.rank = 2: holds.

## rank_missing_first

As if sorted missing first: d 1, b 2, c 2, a 4, e 5. d.rank = 1: holds.

## rank_missing_exclude

exclude drops d (missing score): b 1, c 1, a 3, e 4; 4 rows. exists d: absent, fails.

## rank_group_scope

grp x: b 20 -> 1, a 10 -> 2, e 5 -> 3. grp y: c 20 -> 1, d missing last -> 2. Table order rank ascending then key: b, c, a, d, e. d.rank = 2: holds.

## rank_asc_dense_position

score ascending dense: e 5 -> 1, a 10 -> 2, b 20 and c 20 -> 3, d missing -> 4. Order e, a, b, c, d. The rank assertion is the 1-based position, not the rank column: c is 4th: holds.

## rank_two_missing_first

Two missing rows placed first tie with each other (SQL RANK treats NULLs as equal): d 1, f 1, b 3, c 3, a 5, e 6. f.rank = 1: holds.

## derive_add_int

int + int gives int: 3+1=4, 7+1=8, 2+1=3, 5+1=6, 4+1=5. TP53.n1 = 5: holds.

## derive_mul_scale

Multiplication scale is the sum of the scales (1 + 2 = 3): 0.5*1.10 = 0.550, 3.0*1.10 = 3.300, missing, -1.5*1.10 = -1.650, 2.5*1.10 = 2.750. 2.750 = 2.75 as decimals: holds.

## derive_div_exact

/ always gives dec, exact when it terminates: 3/4 = 0.75, 7/4 = 1.75, 2/4 = 0.5, 5/4 = 1.25, 4/4 = 1 (ideal exponent 0, as Python decimal writes it). TP53.q 1 = 1: holds.

## derive_div_rounds

Non-terminating division rounds to 34 significant digits, half-even: 3/3 = 1; 7/3 = 2 + 33 threes; 2/3 = 0.(33 sixes)7 (the leading 0 is not significant, so 34 digits after the point); 5/3 = 1.(32 sixes)7; 4/3 = 1.(33 threes). |2.333...3 - 2.3333333333| = 0.0000000000333... <= 0.000000001: holds.

## derive_div_zero

Parentheses are required (precedence) and kept. BRCA1: 3/(3-3) = 3/0 -> missing, not an error. EGFR 7/4 = 1.75; KRAS 2/-1 = -2; MDM2 5/2 = 2.5; TP53 4/1 = 4. BRCA1.q is missing: a missing cell never satisfies any comparison: fails.

## derive_parens_kept

{expr} is re-serialized with spaces around binary operators; the parentheses are required by precedence. (3+1)*2 = 8, 16, 6, 12, 10. KRAS.m = 6: holds.

## derive_parens_dropped

Parentheses around the higher-precedence product are not required and are dropped. 3+2 = 5, 9, 4, 7, 6. BRCA1.m = 5: holds.

## derive_abs

abs keeps dec and scale: 0.5, 3.0, missing, 1.5, 2.5. MDM2.a = 1.5: holds.

## derive_unary_minus

Unary minus keeps int, rendered with no space: -3, -7, -2, -5, -4. TP53.neg = -4: holds.

## derive_sub_int_dec

int with dec gives dec at the larger scale (1): 3-0.5 = 2.5, 7-3.0 = 4.0, missing, 5-(-1.5) = 6.5, 4-2.5 = 1.5. MDM2.d = 6.5: holds.

## derive_str_eq_bool

String equality gives bool: false, false, false, true, true. TP53.liv eq true: holds.

## derive_bool_missing

Comparison with a missing operand is missing: BRCA1 false, EGFR true, KRAS missing, MDM2 false, TP53 true. KRAS.big is missing: fails.

## derive_coalesce_widen

coalesce of dec and int gives dec; KRAS takes n = 2, written as dec '2' (an int widened to dec carries no fractional digits). Others keep fc. KRAS.c 2 = 2: holds.

## derive_log_domain

log is the natural logarithm through IEEE double, rounded to 15 significant digits. log(0.0) and log(-1.0) are at or below zero: missing. log(1.0) = 0. log(100.0) = 4.605170185988092 -> 4.60517018598809. Missing stays missing. |4.60517018598809 - 4.60517| = 0.00000018598809 <= 0.00001: holds.

## derive_log2

log2(8.0) = 3 exactly; log2(0.5) = -1; log2(1000.0) = 9.965784284662087 -> 9.96578428466209 (15 significant digits); log2(0.0): missing. a.l 3 = 3: holds.

## derive_log10

log10(8.0) = 0.9030899869919435 -> 0.903089986991944; log10(0.5) = -0.3010299956639812 -> -0.301029995663981; log10(1000.0) = 3; log10(0.0): missing. c.l 3 = 3.0: holds.

## derive_exp

exp(0) = 1; exp(1) = 2.718281828459045 -> 2.71828182845905; exp(1000) overflows the double: missing, not an error; exp(-1) = 0.36787944117144233 -> 0.367879441171442; missing stays missing. c.y is missing: fails.

## derive_rounds_at_34_digits

Re-derived from the revised text (was derive_overflow_error). x = 10000000000000000.5 (18 significant digits, scale 1). The exact product is 100000000000000010000000000000000.25: 33 integer digits + 2 fraction digits = 35 significant digits, scale 2. 'dec results round to 34 significant digits, half-even' and 'Rounding at 34 significant digits is not an overflow: the result rounds and the check goes on'. Dropping the last digit 5 exactly half-way, the kept digit 2 is even, so it stays: 100000000000000010000000000000000.2, scale 1 (rounding overrides the sum-of-scales rule). Adjusted exponent 32, far below Emax 999999, so no Overflow. a.sq equals the rounded value: holds.

## derive_date_arith_error

Arithmetic on a date is E6004 (Dates row), checked before anything runs.

## derive_missing_no_type_error

'a derive whose result has no type is E6004': the bare literal missing has no type.

## derive_name_present_error

'a step that would add a column whose name is already present' is E6002, field at the member that names it.

## derive_then_filter

m = n*2: 6, 14, 4, 10, 8. m > 8 keeps EGFR 14 and MDM2 10 (8 is not > 8). EGFR.m = 14: holds.

## group_count

Groups in tuple sort order: breast {BRCA1, EGFR}, liver {MDM2, TP53}, lung {KRAS}. count counts every member, missing included (KRAS.fc is missing): 2, 2, 1. member_count 2, 2, 1. member_keys_hash = sha256 of the canonical JSON list of key tuples sorted in binary order. lung.cnt = 1: holds.

## group_sum_int

sum keeps int: breast 3+7 = 10, liver 5+4 = 9, lung 2. breast.s = 10: holds.

## group_sum_dec_all_missing

sum ignores missing and keeps dec at scale 1: breast 0.5+3.0 = 3.5, liver -1.5+2.5 = 1.0, lung has only a missing member so the sum is missing. lung.s missing: fails.

## group_min_max

Two aggregates render as 'lo = min(p) and hi = max(p)'. min and max ignore missing: breast {0.200, missing} -> 0.200 / 0.200; liver {0.040, 0.001} -> 0.001 / 0.040; lung 0.010. compare_rows liver.lo 0.001 < breast.hi 0.200: holds.

## group_min_str

min on str keeps str and uses binary order: BRCA1 < EGFR, MDM2 < TP53, KRAS. liver.g = MDM2: holds.

## group_mean_int

mean is a DOUBLE rounded to 15 significant digits and written shortest: (3+7)/2 = 5 -> '5'; (5+4)/2 = 4.5; 2/1 = 2. liver.m = 4.5: holds.

## group_mean_inexact

x = {a 10, b 20, e 5}: 35/3 = 11.666666666666666 -> 11.6666666666667 (15 significant digits, half-even on the 16th). y = {c 20, d missing}: the missing member is ignored (as DuckDB avg does; checker.md states this only for sum, min, max), 20/1 = 20. member_count counts d: 3 and 2. |11.6666666666667 - 11.6667| = 0.0000333333333 <= 0.0001: holds.

## group_median_even

median is a DOUBLE: an even member count takes the midpoint of the two middle values: breast {3, 7} -> 5; liver {4, 5} -> 4.5; lung {2} -> 2. breast.md 5 = 5: holds.

## group_median_odd

x = {5, 10, 20} -> 10; y = {20, missing} -> 20 (missing ignored). x.md = 10: holds.

## group_two_by_columns

Tuples in sort order: (ctl,A), (ctl,B), (trt,A), (trt,B). (trt,A) has members (s2,A) and (s3,A); (trt,B) has (s2,B) whose val is missing but count counts it. The output key is (cond, gene), so the assertion row is [trt, A]. k = 2: holds.

## group_max_date

max takes a date and keeps it: g1 {2026-01-05, 2026-03-01} -> 2026-03-01; g2 {2025-12-31, missing} -> 2025-12-31. g2.mx = 2025-12-31: holds.

## group_three_aggregates

Three aggregates render with ', and ' before the last. breast: 2, 10, 7; liver: 2, 9, 5; lung: 1, 2, 2. breast.mx = 7: holds.

## join_inner

ann has BRCA1, KRAS, TP53, ZZZ3. Inner join drops EGFR and MDM2 (unmatched left) and never adds ZZZ3. Every right column is added with the prefix 'ann.', key included. Left order kept. TP53.ann.chrom = 17: holds.

## join_left

Left join keeps EGFR and MDM2 with missing in ann.gene_id and ann.chrom. MDM2.ann.chrom is missing, and a missing cell never satisfies any comparison, ne included: fails.

## join_left_nonkey_column

The on pair covers the right key (tissue); the left column need not be a key. breast and liver match; lung (KRAS) does not and gets missing. Output key stays gene_id. TP53.tiss.organ = hepatic: holds.

## join_composite_on

Two on pairs render as a two-item list with ' and '. meta has (s1,A), (s2,B), (s3,A); (s1,B) and (s2,A) are dropped. Output key is the left key (sample, gene). (s2,B).meta.qc = fail: holds.

## join_then_filter_prefixed

A prefixed column has a dot, which IDENT does not allow, so the expression needs backticks and {expr} keeps them. chrom 17: BRCA1, TP53; KRAS 12 dropped. count 2 = 2: holds.

## composite_filter_value

Key is (sample, gene); the assertion row lists both in key order. trt rows: (s2,A), (s2,B), (s3,A). The key map holds two-element tuples. (s2,A).val = 3.0: holds.

## composite_sort

{key} is the two-column key, rendered 'sample and gene'. Missing first: (s2,B); then 5.0 (s3,A), 3.0 (s2,A), 2.0 (s1,B), 1.0 (s1,A). (s3,A) at position 2: holds.

## composite_group_sum

Member key tuples are two-element. s1: 1.0+2.0 = 3.0; s2: 3.0 + missing -> 3.0; s3: 5.0. Output key is sample alone. compare_rows s1.s = s2.s: 3.0 = 3.0: holds.

## nan_inf_sort_missing_last

The spec maps NaN and Inf to values, and 'nan, +inf, -inf' are 'placed by the sort's missing rule', so b (nan), c (+inf), d (-inf) and e (missing) all go last, in key order. Before them x descending: a 1.5, f 0.25. float64 values are the shortest round-trip decimal. f at position 2: holds.

## nan_inf_filter_excluded

nan and inf are 'treated as missing by filters', so x > 0 is missing for b, c, d and e; a 1.5 and f 0.25 remain. count 2 = 2: holds.

## nan_cell_fails

No step; the final table is the input in key order with the nan and inf typed values. b.x is nan: never satisfies any comparison: fails.

## inf_cell_fails

c.x is +inf; an inf cell gives fails even for gt (11.18.3 Missing row).

## nan_inf_mapped_to_missing

Same file, spec nan: missing and inf: missing, so b, c, d read as missing like e. Missing first in key order: b, c, d, e; then f 0.25, a 1.5. a at position 6: holds.

## filter_date_lt_date

Dates compare chronologically: a 01-05 < 01-01 false; b equal false; c 2025-12-31 < 2026-06-30 true; d missing. 1 of 4. c.d = 2025-12-31: holds.

## filter_date_vs_datetime_error

'a date and a datetime do not compare': E6004 at the expression, before anything runs.

## value_datetime_lt

a.ts 2026-01-05T10:00:00Z is before 2026-01-05T12:00:00Z: holds.

## value_date_between

2026-01-01 <= 2026-03-01 <= 2026-12-31, bounds inclusive, no tol: holds.

## value_date_tol_error

'op on dates: tol is E6004'; the schema is known before running, so it is refused at /tol.

## count_ne

2 rows; 2 != 3 (|2 - 3| = 1 > 0): holds.

## count_lt

2 < 3: holds.

## count_le_fails

2 <= 1 is false: fails.

## count_gt

5 > 4: holds.

## count_between

2 <= 2 <= 2, inclusive: holds.

## count_eq_fails

2 = 3 is false: fails.

## value_ne_holds

|2.5 - 1.0| = 1.5 > 0: holds.

## value_lt_holds

2.5 < 3 (dec against int): holds.

## value_le_edge

2.5 <= 2.5: holds.

## value_gt_fails

2.5 > 2.5 is false with m = 0: fails.

## value_ge_tol

m = 0.1; 2.5 >= 2.6 - 0.1 = 2.5: holds.

## value_between_rel

lo - 0.25*1.0 = 0.75 <= 2.5 <= hi + 0.25*2.0 = 2.5: holds (each bound widened by its own |bound|).

## value_eq_abs_rel_sum

m = 0.1 + 0.05*2.8 = 0.1 + 0.14 = 0.24; |3.0 - 2.8| = 0.2 <= 0.24: holds.

## value_str_lt_error

lt on a str expected is 'an operator that the type of expected does not allow': E6004 at /op.

## value_str_tol_error

tol on a string comparison is E6004 at /tol even when both parts are 0 (the member is present).

## value_bool_ne

ne on booleans is allowed: BRCA1.liv false != true: holds.

## value_absent_after_filter

KRAS is dropped by the filter; an absent row is fails, not an error.

## compare_rows_gt

2.5 > -1.5: holds.

## compare_rows_le_cross_columns

Different columns of the same row, dec against int: 0.5 <= 3: holds.

## compare_rows_ne_tol

m is taken from the right cell: 0.2*2.5 = 0.5. ne needs |a - b| > m; |3.0 - 2.5| = 0.5 is not > 0.5: fails (the edge belongs to eq, not ne).

## compare_rows_absent_left

ZZZ3 is not in de: either absent is fails.

## compare_rows_lt_fails

2.5 < -1.5 is false: fails.

## compare_rows_ge_eq

0.200 >= 0.200: holds.

## exists_after_join_inner

EGFR has no ann row and the join is inner: absent, fails.

## exists_group_key

After group the key is the by tuple, so exists takes [lung]: present, holds.

## rank_assert_tol

TP53 is 3rd; |3 - 4| = 1 <= abs 1: holds.

## rank_assert_fails

TP53 is 3rd, expected 1, m = 0: fails.

## rank_assert_missing_row_first

The rank assertion is a position; the missing-fc row KRAS sorted first is at 1: holds.

## exp_literal_filter_lt

1e-10 is an EXPNUM literal, a dec: positionally 0.0000000001 (scale 10). padj is a scale-12 column, each cell written at scale 12: a 0.000000000001, b 0.000000000050 (CSV 0.00000000005), c 0.000000000200 (CSV 0.0000000002), d 0.030000000000 (CSV 0.03), e missing. a 1e-12 < 1e-10 true; b 5e-11 < 1e-10 true; c 2e-10 false; d false; e missing, dropped. The description keeps the literal as written, '1e-10'. exists b: holds.

## exp_literal_filter_upper_e

Capital E and the mantissa's trailing zero are part of the literal as written, so {expr} keeps '2.0E-10' while adding the spaces around '<='. 2.0E-10 = 0.00000000020 (scale 11). a, b below; c 0.000000000200 equals it, so <= holds; d 0.03 false; e missing. 3 of 5. exists c: holds.

## derive_exp_literal_unary_minus

-2.5e-1 is unary minus applied to the literal 2.5e-1; the minus inside the exponent belongs to the literal. 2.5e-1 = 0.25 (scale 2), negated -0.25. Rendered with no space after the unary minus and the literal as written: 'fc * -2.5e-1'. Products take scale 1 + 2 = 3: 0.5*-0.25 = -0.125; 3.0*-0.25 = -0.750; KRAS missing; -1.5*-0.25 = 0.375; 2.5*-0.25 = -0.625. MDM2.adj = 0.375: holds.

## derive_exp_literal_binary_minus

'a -1e-10 is a - 1e-10': the minus after a column is binary. 1e-1 = 0.1 (scale 1). Subtraction takes the larger scale, 1: 0.5-0.1 = 0.4; 3.0-0.1 = 2.9; missing; -1.5-0.1 = -1.6; 2.5-0.1 = 2.4. Rendered 'fc - 1e-1'. TP53.adj = 2.4: holds.

## exp_cells_dec_column

Cells written with exponents in a dec column are converted exactly to positional notation: 3.2e-15 = 0.0000000000000032 (scale 16), 1.0e-12 = 0.0000000000010 (scale 13), 4.5E-3 = 0.0045 (scale 4), 0.02 (scale 2). The column scale is 16, the largest seen; every value is written at scale 16: a 0.0000000000000032, b 0.0000000000010000, c 0.0045000000000000, e 0.0200000000000000. q < 0.001: a, b true; c 0.0045 false; d missing; e 0.02 false. 2 of 5. Typed values stay positional, so expected is written 0.0000000000000032: holds.

## exp_scale3_trailing_zero

Scale-3 column. 1.0e-3 is 0.0010 positionally (scale 4) and 0.0010 is scale 4 as written; writing either at scale 3 gives 0.001 without changing its value (only trailing zeros go), so neither is E3102. 0.001 stays 0.001; 0.25 becomes 0.250. v = 0.001: a, b, c true; d false. 3 of 4. compare_rows a.v 0.001 = b.v 0.001: holds. At table.spec this file would show scale 4 (trailing zeros count), so the case reads the file as a later version under a scale-3 spec, where 'a later version's trailing zeros still fit'.

## derive_overflow_emax

9.9e999999 has adjusted exponent 999999, inside Emax, so the literal is accepted. Left-associative: (x * 9.9e999999) * 10. 1 * 9.9e999999 = 9.9e999999 (adjusted 999999, no overflow); * 10 = 9.9e1000000, adjusted exponent 1000000 > Emax 999999: the General Decimal Arithmetic Overflow condition, verdict error with E5002. Row-level fields null. The literal is quantized to its positional scale (0) before use; the product is the same number either way, rounded to 34 digits, so the outcome does not change (AMBIGUITIES.md 39). E5002 points at the member holding the expression, /ops/0/expr (11.18.2 Types).

## exp_literal_out_of_range

1e1000000 has adjusted exponent 1000000, above Emax 999999: E6004 at validation, pointing at the where member (11.18.1 names this exact literal).

## group_missing_values_one_group

g is float64 with nan and inf mapped to values: a 1.5, b missing, c nan, d +inf, e 1.5, f -inf. 'Every missing, nan, or inf value in a by component falls in one group ... and that group's value in the output is missing, whatever its members held'; 'Missing groups sort first'. Group 1: g missing, members b, c, d, f: sum 20+30+40+60 = 150, member_count 4. Group 2: g 1.5, members a, e: sum 10+50 = 60 (int, exact), member_count 2. The key map is [[missing], [1.5]]. The assertion locates the 1.5 group by value (a row holding missing is avoided, AMBIGUITIES.md 42): 60 = 60: holds.

## group_min_max_equal_values

A dec column is written at one scale, so 2 and 2.0 can only differ in a derived column. m = coalesce(x, n): a 2.0 (x), b 2 (n widened, scale 0, written '2'), c 3.5, d 2, e 2.0. Input order to group is a, b, c, d, e. Group g {a 2.0, b 2, c 3.5}: the minimum value 2 is held by a and b, equal under =; the first in input order is a, so lo is '2.0'; hi 3.5. Group h {d 2, e 2.0}: both are minimum and maximum; the first is d, so lo and hi are '2'. compare_rows g.lo 2.0 = h.lo 2 as decimals: holds; the written forms differ in final_rows.

## sort_missing_key_component_first

A key holds missing only after a group on a column with missing values. Group by (a, b): (x, missing) {r1}, (x, p) {r2, r3}, (x, q) {r4}, (y, p) {r5}, in that order (missing component first). k = count(v): 1, 2, 1, 1. The key is now (a, b). Sort by k descending: (x, p) k 2 first; (x, missing), (x, q), (y, p) tie at 1 and are broken by the key ascending, 'a missing key component sorting first': (x, missing) before (x, q) since a ties at x. 'missing values placed last' concerns k, which has none. Final order (x,p), (x,missing), (x,q), (y,p). (x, q) is at position 3; it would be 2 if the missing component sorted last: holds.

## float64_overflow_cell

In a float64 column 'any other text is read as an IEEE double ... a finite number whose double overflows (1e400) being E3102, so only the declared infinity tokens produce infinity'. 1e400 exceeds the largest double (about 1.8e308): E3102, verdict error, row-level fields null. checker.md gives no field for E3102 on a cell; a CSV cell is not a JSON member, so field is null (errors.md allows null; AMBIGUITIES.md 41).

## assert_names_missing_group

sc2 scores: a 10, b 20, c 20, d missing, e 5, f missing. Group by score: the two missing values fall in one group (11.18.2 group), sorted first, by value missing; then 5 {e}, 10 {a}, 20 {b, c}. n = count(id): 2, 1, 1, 2; member_count the same. The output key is (score), so the final key of the first group is [missing]. 11.9 lets a row component be missing 'to name a missing key component', and 11.18.3 'Locating a row' matches it by grouping identity, not by =. The row [missing] locates the first group; its n is 2 = 2: holds.

## rank_group_by_missing_partition

g is float64 with nan and inf as values: a 1.5, b missing, c nan, d +inf, e 1.5, f -inf; t is p or q. Group by (g, t): every missing, nan, or inf g falls in one group per t, with g written missing: (missing, p) {b, d} s 60; (missing, q) {c, f} s 90; (1.5, p) {a} s 10; (1.5, q) {e} s 50. Missing groups sort first: that is the group order. The key is now (g, t). Rank by s descending within each g: the two missing-keyed groups form one partition (11.18.2 rank: every missing value in a group_by component is one partition): (missing, q) 90 -> 1, (missing, p) 60 -> 2. Partition 1.5: (1.5, q) 50 -> 1, (1.5, p) 10 -> 2. Table order is rank ascending then the key, 'a missing key component sorting first': rank 1: (missing, q) before (1.5, q); rank 2: (missing, p) before (1.5, p). Final order (missing,q), (1.5,q), (missing,p), (1.5,p). The row [missing, p] is located by grouping identity (11.18.3) and is at position 3; it would be 4 if the missing component sorted last: holds. The rank column follows member_keys_hash as the last column.
