#!/usr/bin/env python3
"""Lays out the hand-worked Toulmin Checker conformance cases (checker.md item 92).

Every row count, final row, description sentence, and verdict in this file was
worked by hand from checker.md. The script does no evaluation: it writes the
case files, resolves the three hashes (`member_keys_hash`, `final_rows_hash`,
`key_map_hash`) with the reference canonical JSON in ../reference-formats, and
emits checker/cases/NOTES.md with the arithmetic behind every case.

Run: python3 -I tools/write_cases.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(ROOT, "reference-formats"))
from ids import id_of  # noqa: E402

CASES_DIR = os.path.join(ROOT, "checker", "cases")
EVAL_DIR = os.path.join(ROOT, "checker", "assertions", "eval")


# ---- typed values ---------------------------------------------------------

def I(v):
    return {"t": "int", "v": str(v)}


def D(v):
    return {"t": "dec", "v": v}


def S(v):
    return {"t": "str", "v": v}


def B(v):
    return {"t": "bool", "v": v}


def DA(v):
    return {"t": "date", "v": v}


def DT(v):
    return {"t": "datetime", "v": v}


M = {"t": "missing"}
NAN = {"t": "nan"}
PINF = {"t": "inf", "s": "+"}
NINF = {"t": "inf", "s": "-"}


class MKH:
    """member_keys_hash placeholder: the members' key tuples, already in 11.10 order."""

    def __init__(self, tuples):
        self.tuples = tuples

    def resolve(self):
        return S(id_of(self.tuples))


# ---- parse specs ------------------------------------------------------------

def col(name, type_, scale=None):
    c = {"name": name, "type": type_, "missing": ["NA"]}
    if scale is not None:
        c["scale"] = scale
    return c


def spec(columns, key, nan="missing", inf="missing"):
    return {
        "spec": 1,
        "format": "csv",
        "delimiter": ",",
        "header": True,
        "columns": columns,
        "key": key,
        "nan": nan,
        "inf": inf,
        "encoding": "utf-8",
    }


# ---- tables -----------------------------------------------------------------
# Each table: csv text, spec, column list, key columns, rows keyed by key tuple.

TABLES = {}


def table(name, csv, sp, cols, key, rows):
    TABLES[name] = {"csv": csv, "spec": sp, "cols": cols, "key": key, "rows": rows}


table(
    "de",
    "gene_id,fc,p,tissue,n\n"
    "BRCA1,0.5,0.200,breast,3\n"
    "EGFR,3.0,NA,breast,7\n"
    "KRAS,NA,0.010,lung,2\n"
    "MDM2,-1.5,0.040,liver,5\n"
    "TP53,2.5,0.001,liver,4\n",
    spec([col("gene_id", "str"), col("fc", "dec", 1), col("p", "dec", 3),
          col("tissue", "str"), col("n", "int")], ["gene_id"]),
    ["gene_id", "fc", "p", "tissue", "n"], ["gene_id"],
    {
        "BRCA1": [S("BRCA1"), D("0.5"), D("0.200"), S("breast"), I(3)],
        "EGFR": [S("EGFR"), D("3.0"), M, S("breast"), I(7)],
        "KRAS": [S("KRAS"), M, D("0.010"), S("lung"), I(2)],
        "MDM2": [S("MDM2"), D("-1.5"), D("0.040"), S("liver"), I(5)],
        "TP53": [S("TP53"), D("2.5"), D("0.001"), S("liver"), I(4)],
    },
)

table(
    "sc",
    "id,score,grp\na,10,x\nb,20,x\nc,20,y\nd,NA,y\ne,5,x\n",
    spec([col("id", "str"), col("score", "int"), col("grp", "str")], ["id"]),
    ["id", "score", "grp"], ["id"],
    {
        "a": [S("a"), I(10), S("x")],
        "b": [S("b"), I(20), S("x")],
        "c": [S("c"), I(20), S("y")],
        "d": [S("d"), M, S("y")],
        "e": [S("e"), I(5), S("x")],
    },
)

table(
    "sc2",
    "id,score,grp\na,10,x\nb,20,x\nc,20,y\nd,NA,y\ne,5,x\nf,NA,y\n",
    spec([col("id", "str"), col("score", "int"), col("grp", "str")], ["id"]),
    ["id", "score", "grp"], ["id"],
    {
        "a": [S("a"), I(10), S("x")],
        "b": [S("b"), I(20), S("x")],
        "c": [S("c"), I(20), S("y")],
        "d": [S("d"), M, S("y")],
        "e": [S("e"), I(5), S("x")],
        "f": [S("f"), M, S("y")],
    },
)

table(
    "pv",
    "id,p-value,or\nr1,0.010,1.5\nr2,0.200,0.8\nr3,0.040,2.0\n",
    spec([col("id", "str"), col("p-value", "dec", 3), col("or", "dec", 1)], ["id"]),
    ["id", "p-value", "or"], ["id"],
    {
        "r1": [S("r1"), D("0.010"), D("1.5")],
        "r2": [S("r2"), D("0.200"), D("0.8")],
        "r3": [S("r3"), D("0.040"), D("2.0")],
    },
)

table(
    "nm",
    "id,name\n1,café\n2,cafe\n",
    spec([col("id", "int"), col("name", "str")], ["id"]),
    ["id", "name"], ["id"],
    {
        "1": [I(1), S("café")],
        "2": [I(2), S("cafe")],
    },
)

table(
    "w",
    "id,name\n1,apple\n2,Banana\n3,cherry\n4,Apple\n",
    spec([col("id", "int"), col("name", "str")], ["id"]),
    ["id", "name"], ["id"],
    {
        "1": [I(1), S("apple")],
        "2": [I(2), S("Banana")],
        "3": [I(3), S("cherry")],
        "4": [I(4), S("Apple")],
    },
)

table(
    "dom",
    "id,x\na,0.0\nb,-1.0\nc,1.0\nd,100.0\ne,NA\n",
    spec([col("id", "str"), col("x", "dec", 1)], ["id"]),
    ["id", "x"], ["id"],
    {
        "a": [S("a"), D("0.0")],
        "b": [S("b"), D("-1.0")],
        "c": [S("c"), D("1.0")],
        "d": [S("d"), D("100.0")],
        "e": [S("e"), M],
    },
)

table(
    "pw",
    "id,x\na,8.0\nb,0.5\nc,1000.0\nd,0.0\n",
    spec([col("id", "str"), col("x", "dec", 1)], ["id"]),
    ["id", "x"], ["id"],
    {
        "a": [S("a"), D("8.0")],
        "b": [S("b"), D("0.5")],
        "c": [S("c"), D("1000.0")],
        "d": [S("d"), D("0.0")],
    },
)

table(
    "ex",
    "id,x\na,0\nb,1\nc,1000\nd,-1\ne,NA\n",
    spec([col("id", "str"), col("x", "int")], ["id"]),
    ["id", "x"], ["id"],
    {
        "a": [S("a"), I(0)],
        "b": [S("b"), I(1)],
        "c": [S("c"), I(1000)],
        "d": [S("d"), I(-1)],
        "e": [S("e"), M],
    },
)

table(
    "big",
    "id,x\na,10000000000000000.5\n",
    spec([col("id", "str"), col("x", "dec", 1)], ["id"]),
    ["id", "x"], ["id"],
    {"a": [S("a"), D("10000000000000000.5")]},
)

table(
    "cw",
    "id,i,x\na,100000000000000001,NA\nb,NA,2.5\nc,3,4.0\nd,NA,NA\n",
    spec([col("id", "str"), col("i", "int"), col("x", "dec", 1)], ["id"]),
    ["id", "i", "x"], ["id"],
    {
        "a": [S("a"), I(100000000000000001), M],
        "b": [S("b"), M, D("2.5")],
        "c": [S("c"), I(3), D("4.0")],
        "d": [S("d"), M, M],
    },
)

table(
    "ev",
    "id,d,d2,ts,k\n"
    "a,2026-01-05,2026-01-01,2026-01-05T10:00:00Z,g1\n"
    "b,2026-03-01,2026-03-01,2026-03-01T00:00:00Z,g1\n"
    "c,2025-12-31,2026-06-30,2025-12-31T23:59:59Z,g2\n"
    "d,NA,2026-01-01,NA,g2\n",
    spec([col("id", "str"), col("d", "date"), col("d2", "date"),
          col("ts", "datetime"), col("k", "str")], ["id"]),
    ["id", "d", "d2", "ts", "k"], ["id"],
    {
        "a": [S("a"), DA("2026-01-05"), DA("2026-01-01"), DT("2026-01-05T10:00:00Z"), S("g1")],
        "b": [S("b"), DA("2026-03-01"), DA("2026-03-01"), DT("2026-03-01T00:00:00Z"), S("g1")],
        "c": [S("c"), DA("2025-12-31"), DA("2026-06-30"), DT("2025-12-31T23:59:59Z"), S("g2")],
        "d": [S("d"), M, DA("2026-01-01"), M, S("g2")],
    },
)

table(
    "ann",
    "gene_id,chrom\nBRCA1,17\nKRAS,12\nTP53,17\nZZZ3,1\n",
    spec([col("gene_id", "str"), col("chrom", "int")], ["gene_id"]),
    ["gene_id", "chrom"], ["gene_id"],
    {
        "BRCA1": [S("BRCA1"), I(17)],
        "KRAS": [S("KRAS"), I(12)],
        "TP53": [S("TP53"), I(17)],
        "ZZZ3": [S("ZZZ3"), I(1)],
    },
)

table(
    "tiss",
    "tissue,organ\nbreast,mammary\nliver,hepatic\n",
    spec([col("tissue", "str"), col("organ", "str")], ["tissue"]),
    ["tissue", "organ"], ["tissue"],
    {
        "breast": [S("breast"), S("mammary")],
        "liver": [S("liver"), S("hepatic")],
    },
)

table(
    "obs",
    "sample,gene,val,cond\n"
    "s1,A,1.0,ctl\ns1,B,2.0,ctl\ns2,A,3.0,trt\ns2,B,NA,trt\ns3,A,5.0,trt\n",
    spec([col("sample", "str"), col("gene", "str"), col("val", "dec", 1),
          col("cond", "str")], ["sample", "gene"]),
    ["sample", "gene", "val", "cond"], ["sample", "gene"],
    {
        "s1,A": [S("s1"), S("A"), D("1.0"), S("ctl")],
        "s1,B": [S("s1"), S("B"), D("2.0"), S("ctl")],
        "s2,A": [S("s2"), S("A"), D("3.0"), S("trt")],
        "s2,B": [S("s2"), S("B"), M, S("trt")],
        "s3,A": [S("s3"), S("A"), D("5.0"), S("trt")],
    },
)

table(
    "meta",
    "sample,gene,qc\ns1,A,pass\ns2,B,fail\ns3,A,pass\n",
    spec([col("sample", "str"), col("gene", "str"), col("qc", "str")], ["sample", "gene"]),
    ["sample", "gene", "qc"], ["sample", "gene"],
    {
        "s1,A": [S("s1"), S("A"), S("pass")],
        "s2,B": [S("s2"), S("B"), S("fail")],
        "s3,A": [S("s3"), S("A"), S("pass")],
    },
)

FL_CSV = "id,x\na,1.5\nb,NaN\nc,Inf\nd,-Inf\ne,NA\nf,0.25\n"
table(
    "fl",
    FL_CSV,
    spec([col("id", "str"), col("x", "float64")], ["id"], nan="value", inf="value"),
    ["id", "x"], ["id"],
    {
        "a": [S("a"), D("1.5")],
        "b": [S("b"), NAN],
        "c": [S("c"), PINF],
        "d": [S("d"), NINF],
        "e": [S("e"), M],
        "f": [S("f"), D("0.25")],
    },
)
table(
    "flm",  # same file, NaN and Inf mapped to missing
    FL_CSV,
    spec([col("id", "str"), col("x", "float64")], ["id"], nan="missing", inf="missing"),
    ["id", "x"], ["id"],
    {
        "a": [S("a"), D("1.5")],
        "b": [S("b"), M],
        "c": [S("c"), M],
        "d": [S("d"), M],
        "e": [S("e"), M],
        "f": [S("f"), D("0.25")],
    },
)


table(
    "gw",
    "gene_id,padj\na,0.000000000001\nb,0.00000000005\nc,0.0000000002\nd,0.03\ne,NA\n",
    spec([col("gene_id", "str"), col("padj", "dec", 12)], ["gene_id"]),
    ["gene_id", "padj"], ["gene_id"],
    {
        "a": [S("a"), D("0.000000000001")],
        "b": [S("b"), D("0.000000000050")],
        "c": [S("c"), D("0.000000000200")],
        "d": [S("d"), D("0.030000000000")],
        "e": [S("e"), M],
    },
)

table(
    "tiny",
    "id,q\na,3.2e-15\nb,1.0e-12\nc,4.5E-3\nd,NA\ne,0.02\n",
    spec([col("id", "str"), col("q", "dec", 16)], ["id"]),
    ["id", "q"], ["id"],
    {
        "a": [S("a"), D("0.0000000000000032")],
        "b": [S("b"), D("0.0000000000010000")],
        "c": [S("c"), D("0.0045000000000000")],
        "d": [S("d"), M],
        "e": [S("e"), D("0.0200000000000000")],
    },
)

table(
    "s3",
    "id,v\na,1.0e-3\nb,0.0010\nc,0.001\nd,0.25\n",
    spec([col("id", "str"), col("v", "dec", 3)], ["id"]),
    ["id", "v"], ["id"],
    {
        "a": [S("a"), D("0.001")],
        "b": [S("b"), D("0.001")],
        "c": [S("c"), D("0.001")],
        "d": [S("d"), D("0.250")],
    },
)

table(
    "one",
    "id,x\na,1\n",
    spec([col("id", "str"), col("x", "int")], ["id"]),
    ["id", "x"], ["id"],
    {"a": [S("a"), I(1)]},
)


table(
    "gm",
    "id,g,v\na,1.5,10\nb,NA,20\nc,NaN,30\nd,Inf,40\ne,1.5,50\nf,-Inf,60\n",
    spec([col("id", "str"), col("g", "float64"), col("v", "int")], ["id"], nan="value", inf="value"),
    ["id", "g", "v"], ["id"],
    {
        "a": [S("a"), D("1.5"), I(10)],
        "b": [S("b"), M, I(20)],
        "c": [S("c"), NAN, I(30)],
        "d": [S("d"), PINF, I(40)],
        "e": [S("e"), D("1.5"), I(50)],
        "f": [S("f"), NINF, I(60)],
    },
)

table(
    "mm",
    "id,grp,x,n\na,g,2.0,1\nb,g,NA,2\nc,g,3.5,5\nd,h,NA,2\ne,h,2.0,9\n",
    spec([col("id", "str"), col("grp", "str"), col("x", "dec", 1), col("n", "int")], ["id"]),
    ["id", "grp", "x", "n"], ["id"],
    {
        "a": [S("a"), S("g"), D("2.0"), I(1)],
        "b": [S("b"), S("g"), M, I(2)],
        "c": [S("c"), S("g"), D("3.5"), I(5)],
        "d": [S("d"), S("h"), M, I(2)],
        "e": [S("e"), S("h"), D("2.0"), I(9)],
    },
)

table(
    "pk",
    "id,a,b,v\nr1,x,NA,1\nr2,x,p,2\nr3,x,p,3\nr4,x,q,4\nr5,y,p,5\n",
    spec([col("id", "str"), col("a", "str"), col("b", "str"), col("v", "int")], ["id"]),
    ["id", "a", "b", "v"], ["id"],
    {
        "r1": [S("r1"), S("x"), M, I(1)],
        "r2": [S("r2"), S("x"), S("p"), I(2)],
        "r3": [S("r3"), S("x"), S("p"), I(3)],
        "r4": [S("r4"), S("x"), S("q"), I(4)],
        "r5": [S("r5"), S("y"), S("p"), I(5)],
    },
)

table(
    "ov",
    "id,x\na,1.5\nb,1e400\n",
    spec([col("id", "str"), col("x", "float64")], ["id"]),
    ["id", "x"], ["id"],
    {"a": [S("a"), D("1.5")], "b": [S("b"), None]},
)


table(
    "gp",
    "id,g,t,v\na,1.5,p,10\nb,NA,p,20\nc,NaN,q,30\nd,Inf,p,40\ne,1.5,q,50\nf,-Inf,q,60\n",
    spec([col("id", "str"), col("g", "float64"), col("t", "str"), col("v", "int")], ["id"], nan="value", inf="value"),
    ["id", "g", "t", "v"], ["id"],
    {
        "a": [S("a"), D("1.5"), S("p"), I(10)],
        "b": [S("b"), M, S("p"), I(20)],
        "c": [S("c"), NAN, S("q"), I(30)],
        "d": [S("d"), PINF, S("p"), I(40)],
        "e": [S("e"), D("1.5"), S("q"), I(50)],
        "f": [S("f"), NINF, S("q"), I(60)],
    },
)


# ---- row builders -------------------------------------------------------------

def rows(tname, order, add=None, drop=None, prefix=None):
    """Rows of table `tname` in `order` (key labels). `add` maps label -> {col: value}
    appended after the table's columns; `drop` removes columns; `prefix` renames
    columns to `<prefix>.<col>` (right side of a join)."""
    t = TABLES[tname]
    out = []
    for label in order:
        row = {}
        for c, v in zip(t["cols"], t["rows"][label]):
            if drop and c in drop:
                continue
            row[(prefix + "." + c) if prefix else c] = v
        if add:
            row.update(add[label])
        out.append(row)
    return out


def cols_of(tname, add=(), drop=(), prefix=None):
    t = TABLES[tname]
    base = [((prefix + "." + c) if prefix else c) for c in t["cols"] if c not in drop]
    return base + list(add)


def joined(left, lorder, right, match, kind):
    """Join rows: `match` maps left label -> right label or None."""
    lt, rt = TABLES[left], TABLES[right]
    out = []
    for label in lorder:
        row = dict(zip(lt["cols"], lt["rows"][label]))
        rl = match.get(label)
        for c in rt["cols"]:
            row[right + "." + c] = rt["rows"][rl][rt["cols"].index(c)] if rl is not None else M
        out.append(row)
    return out


def tup(tname, label):
    t = TABLES[tname]
    return [t["rows"][label][t["cols"].index(k)] for k in t["key"]]


def steps(tables, frm, ops):
    return {"steps": 1, "tables": tables, "from": frm, "ops": ops}


def filt(where):
    return {"op": "filter", "v": 1, "where": where}


def select(columns):
    return {"op": "select", "v": 1, "columns": columns}


def sort(by, missing):
    return {"op": "sort", "v": 1, "by": [{"column": c, "dir": d} for c, d in by], "missing": missing}


def take(n):
    return {"op": "take", "v": 1, "n": n}


def rank(by, dir_, ties, missing, scope="all", group_by=None):
    op = {"op": "rank", "v": 1, "by": by, "dir": dir_, "ties": ties, "missing": missing, "scope": scope}
    if group_by is not None:
        op["group_by"] = group_by
    return op


def derive(as_, expr):
    return {"op": "derive", "v": 1, "as": as_, "expr": expr}


def group(by, aggregates):
    return {"op": "group", "v": 1, "by": by,
            "aggregates": [{"as": a, "fn": f, "column": c} for a, f, c in aggregates]}


def join(right, on, kind):
    return {"op": "join", "v": 1, "right": right,
            "on": [{"left": l, "right": r} for l, r in on], "kind": kind}


def a_value(row, column, op, expected, tol=None):
    a = {"assert": 1, "kind": "value", "row": row, "column": column, "op": op, "expected": expected}
    if tol:
        a["tol"] = tol
    return a


def a_rank(row, expected, tol=None):
    a = {"assert": 1, "kind": "rank", "row": row, "expected": I(expected)}
    if tol:
        a["tol"] = tol
    return a


def a_count(op, expected):
    return {"assert": 1, "kind": "count", "op": op, "expected": expected}


def a_cmp(lrow, lcol, rrow, rcol, op, tol=None):
    a = {"assert": 1, "kind": "compare_rows", "left": {"row": lrow, "column": lcol},
         "right": {"row": rrow, "column": rcol}, "op": op}
    if tol:
        a["tol"] = tol
    return a


def a_exists(row):
    return {"assert": 1, "kind": "exists", "row": row}


def tol(abs_, rel):
    return {"abs": abs_, "rel": rel}


# ---- group helpers ------------------------------------------------------------

def grp_rows(by_values, aggs, member_count, member_tuples):
    """One group row: by column values, aggregate values, member_count, member_keys_hash.
    `member_tuples` must already be in 11.10 order."""
    row = {}
    row.update(by_values)
    row.update(aggs)
    row["member_count"] = I(member_count)
    row["member_keys_hash"] = MKH(member_tuples)
    return row


# ---- the cases --------------------------------------------------------------------

CASES = []


def case(name, tables, frm, ops, assertion, verdict, steps_exp=None, columns=None,
         final_rows=None, key_cols=None, error=None, notes=""):
    """steps_exp: list of (rows, description). key_cols: the final table's key columns,
    used to form the key-map tuples from the final rows (left key for joins, by tuple
    for groups, the row key otherwise).
    expected.json follows item 92: `error` is always present, null unless the verdict
    is error, and on error every row-level field is null."""
    handles = {t: f"F{i + 1}@v1" for i, t in enumerate(tables)}
    CASES.append(dict(name=name, tables=tables, steps=steps(handles, frm, ops),
                      assertion=assertion, verdict=verdict, steps_exp=steps_exp,
                      columns=columns, final_rows=final_rows, key_cols=key_cols,
                      error=error, notes=notes))


DE_ALL = ["BRCA1", "EGFR", "KRAS", "MDM2", "TP53"]
DE_COLS = cols_of("de")

# -- no ops ------------------------------------------------------------------------
case("empty_ops", ["de"], "de", [],
     a_count("eq", I(5)), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"],
     notes="No step. Final table is the input in key order (BRCA1 < EGFR < KRAS < MDM2 < TP53 "
           "in binary order). count 5 = 5: holds.")

# -- filter --------------------------------------------------------------------------
case("filter_gt_dec", ["de"], "de", [filt("fc > 1.0")],
     a_count("eq", I(2)), "holds",
     [(2, "Keep rows where fc > 1.0 (missing values excluded); 2 of 5 rows remain.")],
     DE_COLS, rows("de", ["EGFR", "TP53"]), ["gene_id"],
     notes="fc > 1.0: BRCA1 0.5 false; EGFR 3.0 true; KRAS missing -> missing, dropped; "
           "MDM2 -1.5 false; TP53 2.5 true. 2 rows. count 2 = 2: holds.")

case("filter_or_three_valued", ["de"], "de", [filt("fc > 1.0 or p < 0.050")],
     a_exists([S("BRCA1")]), "fails",
     [(4, "Keep rows where fc > 1.0 or p < 0.050 (missing values excluded); 4 of 5 rows remain.")],
     DE_COLS, rows("de", ["EGFR", "KRAS", "MDM2", "TP53"]), ["gene_id"],
     notes="BRCA1: false or false = false. EGFR: true or missing = true. KRAS: missing or true "
           "(0.010 < 0.050) = true. MDM2: false or true = true. TP53: true or true = true. "
           "4 rows. exists BRCA1: absent, fails.")

case("filter_and_missing", ["de"], "de", [filt("fc > 1.0 and p < 0.050")],
     a_count("eq", I(1)), "holds",
     [(1, "Keep rows where fc > 1.0 and p < 0.050 (missing values excluded); 1 of 5 rows remain.")],
     DE_COLS, rows("de", ["TP53"]), ["gene_id"],
     notes="BRCA1: false and false = false. EGFR: true and missing = missing, dropped. KRAS: "
           "missing and true = missing, dropped. MDM2: false and true = false. TP53: true and "
           "true = true. 1 row. count 1 = 1: holds.")

case("filter_not", ["de"], "de", [filt("not (fc > 1.0)")],
     a_count("eq", I(2)), "holds",
     [(2, "Keep rows where not fc > 1.0 (missing values excluded); 2 of 5 rows remain.")],
     DE_COLS, rows("de", ["BRCA1", "MDM2"]), ["gene_id"],
     notes="Grammar: not := 'not' not | cmp, so the operand of not is a whole comparison and the "
           "author's parentheses are not required by precedence; {expr} drops them. BRCA1: not "
           "false = true. EGFR: not true = false. KRAS: not missing = missing, dropped. MDM2 true. "
           "TP53 false. 2 rows. count 2 = 2: holds.")

case("filter_str_eq", ["de"], "de", [filt('tissue = "liver"')],
     a_value([S("MDM2")], "n", "eq", I(5)), "holds",
     [(2, 'Keep rows where tissue = "liver" (missing values excluded); 2 of 5 rows remain.')],
     DE_COLS, rows("de", ["MDM2", "TP53"]), ["gene_id"],
     notes="tissue = \"liver\": MDM2 and TP53 only. MDM2.n = 5 = 5: holds.")

case("filter_coalesce", ["de"], "de", [filt("coalesce(fc, 0) > 1")],
     a_exists([S("KRAS")]), "fails",
     [(2, "Keep rows where coalesce(fc, 0) > 1 (missing values excluded); 2 of 5 rows remain.")],
     DE_COLS, rows("de", ["EGFR", "TP53"]), ["gene_id"],
     notes="coalesce(fc, 0): KRAS missing -> 0 (int widened to dec), 0 > 1 false. BRCA1 0.5 false; "
           "EGFR 3.0 true; MDM2 -1.5 false; TP53 2.5 true. 2 rows. exists KRAS: absent, fails.")

case("filter_int_vs_dec", ["de"], "de", [filt("n > 2.5")],
     a_count("ge", I(4)), "holds",
     [(4, "Keep rows where n > 2.5 (missing values excluded); 4 of 5 rows remain.")],
     DE_COLS, rows("de", ["BRCA1", "EGFR", "MDM2", "TP53"]), ["gene_id"],
     notes="int n against dec 2.5 compares as decimals: 3, 7, 5, 4 > 2.5 true; KRAS 2 false. "
           "4 rows. count 4 >= 4: holds.")

case("filter_true_literal", ["de"], "de", [filt("true")],
     a_count("eq", I(5)), "holds",
     [(5, "Keep rows where true (missing values excluded); 5 of 5 rows remain.")],
     DE_COLS, rows("de", DE_ALL), ["gene_id"],
     notes="The literal true keeps every row. 5 of 5. count 5 = 5: holds.")

case("filter_missing_literal", ["de"], "de", [filt("fc = missing")],
     a_count("eq", I(0)), "holds",
     [(0, "Keep rows where fc = missing (missing values excluded); 0 of 5 rows remain.")],
     DE_COLS, [], ["gene_id"],
     notes="Any comparison with a missing operand is missing, so fc = missing is missing for every "
           "row and every row is dropped. 0 rows; final_rows_hash and key_map_hash are the hashes "
           "of the empty list. count 0 = 0: holds.")

case("filter_unary_minus", ["de"], "de", [filt("fc < -1")],
     a_value([S("MDM2")], "fc", "eq", D("-1.5")), "holds",
     [(1, "Keep rows where fc < -1 (missing values excluded); 1 of 5 rows remain.")],
     DE_COLS, rows("de", ["MDM2"]), ["gene_id"],
     notes="-1 is unary minus applied to the literal 1; rendered with no space after it. Only "
           "MDM2 (-1.5) is below -1. MDM2.fc = -1.5: holds.")

case("filter_backtick_columns", ["pv"], "pv", [filt("`p-value` < 0.050 and `or` > 1.0")],
     a_count("eq", I(2)), "holds",
     [(2, "Keep rows where `p-value` < 0.050 and `or` > 1.0 (missing values excluded); 2 of 3 rows remain.")],
     cols_of("pv"), rows("pv", ["r1", "r3"]), ["id"],
     notes="`p-value` has a hyphen and `or` is reserved, so both keep their backticks. r1: 0.010 < "
           "0.050 and 1.5 > 1.0 true. r2: 0.200 < 0.050 false. r3: 0.040 < 0.050 and 2.0 > 1.0 "
           "true. 2 of 3. count 2 = 2: holds.")

case("filter_str_nfc", ["nm"], "nm", [filt('name = "café"')],
     a_value([I(1)], "name", "eq", S("café")), "holds",
     [(1, 'Keep rows where name = "café" (missing values excluded); 1 of 2 rows remain.')],
     cols_of("nm"), rows("nm", ["1"]), ["id"],
     notes="Row 1 is written in the CSV as 'cafe' + U+0301 (decomposed); the reader normalizes it "
           "to NFC 'café' (U+00E9), so the final row carries the precomposed form and the "
           "NFC-byte comparison with the literal matches. Row 2 'cafe' does not. 1 of 2. "
           "value eq by NFC bytes: holds.")

case("filter_str_vs_number_error", ["de"], "de", [filt("tissue = 1")],
     a_count("eq", I(0)), "error", error=("E6004", "/ops/0/where"),
     notes="tissue is str and 1 is a number: 'Comparing a string with a number ... is a validation "
           "error E6004 type mismatch, raised before DuckDB runs'. No step runs; no rows, no hashes.")

# -- select --------------------------------------------------------------------------
case("select_two", ["de"], "de", [select(["fc", "tissue"])],
     a_value([S("TP53")], "fc", "eq", D("2.5")), "holds",
     [(5, "Keep columns fc and tissue; 5 rows.")],
     ["gene_id", "fc", "tissue"], rows("de", DE_ALL, drop={"p", "n"}), ["gene_id"],
     notes="select keeps the listed columns and the key column gene_id; columns stay in table "
           "order. Two-item list renders 'fc and tissue'. TP53.fc = 2.5: holds.")

case("select_three_with_key", ["de"], "de", [select(["n", "gene_id", "p"])],
     a_count("eq", I(5)), "holds",
     [(5, "Keep columns n, gene_id, and p; 5 rows.")],
     ["gene_id", "p", "n"], rows("de", DE_ALL, drop={"fc", "tissue"}), ["gene_id"],
     notes="Three listed columns render with ', and ' before the last. The kept columns are shown "
           "in table order (gene_id, p, n). count 5 = 5: holds.")

# -- sort -----------------------------------------------------------------------------
case("sort_asc_missing_last", ["de"], "de", [sort([("fc", "asc")], "last")],
     a_rank([S("TP53")], 3), "holds",
     [(5, "Sort by fc ascending (missing values placed last; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["MDM2", "BRCA1", "TP53", "EGFR", "KRAS"]), ["gene_id"],
     notes="fc ascending: MDM2 -1.5, BRCA1 0.5, TP53 2.5, EGFR 3.0, then KRAS (missing) last. "
           "TP53 is at position 3 = 3: holds.")

case("sort_desc_missing_first", ["de"], "de", [sort([("fc", "desc")], "first")],
     a_rank([S("KRAS")], 1), "holds",
     [(5, "Sort by fc descending (missing values placed first; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["KRAS", "EGFR", "TP53", "BRCA1", "MDM2"]), ["gene_id"],
     notes="Missing first: KRAS, then fc descending EGFR 3.0, TP53 2.5, BRCA1 0.5, MDM2 -1.5. "
           "KRAS at position 1: holds.")

case("sort_two_columns", ["de"], "de", [sort([("tissue", "asc"), ("fc", "desc")], "last")],
     a_rank([S("TP53")], 3), "holds",
     [(5, "Sort by tissue ascending, then fc descending (missing values placed last; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["EGFR", "BRCA1", "TP53", "MDM2", "KRAS"]), ["gene_id"],
     notes="tissue ascending (binary): breast < liver < lung. Within breast fc descending: EGFR 3.0, "
           "BRCA1 0.5. Within liver: TP53 2.5, MDM2 -1.5. lung: KRAS. TP53 at position 3: holds.")

case("sort_tiebreak_key", ["de"], "de", [sort([("tissue", "asc")], "last")],
     a_rank([S("EGFR")], 2), "holds",
     [(5, "Sort by tissue ascending (missing values placed last; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["BRCA1", "EGFR", "MDM2", "TP53", "KRAS"]), ["gene_id"],
     notes="Ties within a tissue break by gene_id ascending: breast BRCA1, EGFR; liver MDM2, TP53; "
           "lung KRAS. EGFR at position 2: holds.")

case("sort_three_columns", ["de"], "de",
     [sort([("tissue", "desc"), ("n", "asc"), ("fc", "asc")], "first")],
     a_rank([S("MDM2")], 3), "holds",
     [(5, "Sort by tissue descending, then n ascending, then fc ascending (missing values placed first; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["KRAS", "TP53", "MDM2", "BRCA1", "EGFR"]), ["gene_id"],
     notes="tissue descending: lung, liver, breast ('li' < 'lu' in binary). liver by n ascending: "
           "TP53 4, MDM2 5. breast: BRCA1 3, EGFR 7. fc never decides. Third sort column is "
           "rendered with a second ', then '. MDM2 at position 3: holds.")

case("sort_str_binary", ["w"], "w", [sort([("name", "asc")], "last")],
     a_rank([I(1)], 3), "holds",
     [(4, "Sort by name ascending (missing values placed last; ties broken by id); 4 rows.")],
     cols_of("w"), rows("w", ["4", "2", "1", "3"]), ["id"],
     notes="Binary order of UTF-8 bytes: 'A' 0x41 < 'B' 0x42 < 'a' 0x61 < 'c' 0x63, so Apple (id 4), "
           "Banana (2), apple (1), cherry (3). id 1 at position 3: holds.")

case("sort_date_desc", ["ev"], "ev", [sort([("d", "desc")], "last")],
     a_rank([S("c")], 3), "holds",
     [(4, "Sort by d descending (missing values placed last; ties broken by id); 4 rows.")],
     cols_of("ev"), rows("ev", ["b", "a", "c", "d"]), ["id"],
     notes="d descending chronologically: b 2026-03-01, a 2026-01-05, c 2025-12-31, then d "
           "(missing) last. c at position 3: holds.")

# -- take ---------------------------------------------------------------------------------
case("take_after_sort", ["de"], "de", [sort([("fc", "desc")], "last"), take(2)],
     a_exists([S("TP53")]), "holds",
     [(5, "Sort by fc descending (missing values placed last; ties broken by gene_id); 5 rows."),
      (2, "Take the first 2 rows; 2 rows remain.")],
     DE_COLS, rows("de", ["EGFR", "TP53"]), ["gene_id"],
     notes="Sorted: EGFR 3.0, TP53 2.5, BRCA1 0.5, MDM2 -1.5, KRAS. First 2: EGFR, TP53. "
           "exists TP53: holds.")

case("take_zero", ["de"], "de", [sort([("fc", "asc")], "last"), take(0)],
     a_count("eq", I(0)), "holds",
     [(5, "Sort by fc ascending (missing values placed last; ties broken by gene_id); 5 rows."),
      (0, "Take the first 0 rows; 0 rows remain.")],
     DE_COLS, [], ["gene_id"],
     notes="take.n may be 0. Final table empty; hashes of the empty list. count 0 = 0: holds.")

case("take_beyond_end", ["de"], "de", [sort([("fc", "asc")], "last"), take(10)],
     a_count("le", I(5)), "holds",
     [(5, "Sort by fc ascending (missing values placed last; ties broken by gene_id); 5 rows."),
      (5, "Take the first 10 rows; 5 rows remain.")],
     DE_COLS, rows("de", ["MDM2", "BRCA1", "TP53", "EGFR", "KRAS"]), ["gene_id"],
     notes="Only 5 rows exist, so take 10 keeps all 5 in sorted order; {k} is 10 as written. "
           "count 5 <= 5: holds.")

case("take_after_rank", ["sc"], "sc", [rank("score", "desc", "min", "last"), take(2)],
     a_count("eq", I(2)), "holds",
     [(5, "Rank by score descending within all rows (ties: min; missing values placed last); 5 rows."),
      (2, "Take the first 2 rows; 2 rows remain.")],
     cols_of("sc", add=["rank"]),
     rows("sc", ["b", "c"], add={"b": {"rank": I(1)}, "c": {"rank": I(1)}}), ["id"],
     notes="rank gives a total order (rank ascending then key): b 1, c 1, a 3, e 4, d 5. First 2: "
           "b, c. count 2 = 2: holds.")

# -- rank -----------------------------------------------------------------------------------
SC_COLS_R = cols_of("sc", add=["rank"])


def sc_ranked(order, ranks, tname="sc"):
    return rows(tname, order, add={k: {"rank": I(v)} for k, v in ranks.items()})


case("rank_min", ["sc"], "sc", [rank("score", "desc", "min", "last")],
     a_value([S("a")], "rank", "eq", I(3)), "holds",
     [(5, "Rank by score descending within all rows (ties: min; missing values placed last); 5 rows.")],
     SC_COLS_R, sc_ranked(["b", "c", "a", "e", "d"], {"b": 1, "c": 1, "a": 3, "e": 4, "d": 5}), ["id"],
     notes="score descending: b 20, c 20, a 10, e 5, d missing last. SQL RANK: b 1, c 1, a 3, "
           "e 4, d 5. Order rank ascending then key. a.rank = 3: holds.")

case("rank_max", ["sc"], "sc", [rank("score", "desc", "max", "last")],
     a_value([S("b")], "rank", "eq", I(2)), "holds",
     [(5, "Rank by score descending within all rows (ties: max; missing values placed last); 5 rows.")],
     SC_COLS_R, sc_ranked(["b", "c", "a", "e", "d"], {"b": 2, "c": 2, "a": 3, "e": 4, "d": 5}), ["id"],
     notes="Inverse RANK gives a tie group its largest position: b 2, c 2, a 3, e 4, d 5. "
           "b.rank = 2: holds.")

case("rank_dense", ["sc"], "sc", [rank("score", "desc", "dense", "last")],
     a_value([S("e")], "rank", "eq", I(3)), "holds",
     [(5, "Rank by score descending within all rows (ties: dense; missing values placed last); 5 rows.")],
     SC_COLS_R, sc_ranked(["b", "c", "a", "e", "d"], {"b": 1, "c": 1, "a": 2, "e": 3, "d": 4}), ["id"],
     notes="DENSE_RANK: b 1, c 1, a 2, e 3, d 4. e.rank = 3: holds.")

case("rank_ordinal", ["sc"], "sc", [rank("score", "desc", "ordinal", "last")],
     a_value([S("c")], "rank", "eq", I(2)), "holds",
     [(5, "Rank by score descending within all rows (ties: ordinal; missing values placed last); 5 rows.")],
     SC_COLS_R, sc_ranked(["b", "c", "a", "e", "d"], {"b": 1, "c": 2, "a": 3, "e": 4, "d": 5}), ["id"],
     notes="ROW_NUMBER ordered by score then key: b 1, c 2 (key b < c), a 3, e 4, d 5. "
           "c.rank = 2: holds.")

case("rank_missing_first", ["sc"], "sc", [rank("score", "desc", "min", "first")],
     a_value([S("d")], "rank", "eq", I(1)), "holds",
     [(5, "Rank by score descending within all rows (ties: min; missing values placed first); 5 rows.")],
     SC_COLS_R, sc_ranked(["d", "b", "c", "a", "e"], {"d": 1, "b": 2, "c": 2, "a": 4, "e": 5}), ["id"],
     notes="As if sorted missing first: d 1, b 2, c 2, a 4, e 5. d.rank = 1: holds.")

case("rank_missing_exclude", ["sc"], "sc", [rank("score", "desc", "min", "exclude")],
     a_exists([S("d")]), "fails",
     [(4, "Rank by score descending within all rows (ties: min; missing values excluded); 4 rows.")],
     SC_COLS_R, sc_ranked(["b", "c", "a", "e"], {"b": 1, "c": 1, "a": 3, "e": 4}), ["id"],
     notes="exclude drops d (missing score): b 1, c 1, a 3, e 4; 4 rows. exists d: absent, fails.")

case("rank_group_scope", ["sc"], "sc", [rank("score", "desc", "min", "last", "group", ["grp"])],
     a_value([S("d")], "rank", "eq", I(2)), "holds",
     [(5, "Rank by score descending within each grp (ties: min; missing values placed last); 5 rows.")],
     SC_COLS_R, sc_ranked(["b", "c", "a", "d", "e"], {"b": 1, "c": 1, "a": 2, "d": 2, "e": 3}), ["id"],
     notes="grp x: b 20 -> 1, a 10 -> 2, e 5 -> 3. grp y: c 20 -> 1, d missing last -> 2. Table order "
           "rank ascending then key: b, c, a, d, e. d.rank = 2: holds.")

case("rank_asc_dense_position", ["sc"], "sc", [rank("score", "asc", "dense", "last")],
     a_rank([S("c")], 4), "holds",
     [(5, "Rank by score ascending within all rows (ties: dense; missing values placed last); 5 rows.")],
     SC_COLS_R, sc_ranked(["e", "a", "b", "c", "d"], {"e": 1, "a": 2, "b": 3, "c": 3, "d": 4}), ["id"],
     notes="score ascending dense: e 5 -> 1, a 10 -> 2, b 20 and c 20 -> 3, d missing -> 4. Order e, a, "
           "b, c, d. The rank assertion is the 1-based position, not the rank column: c is 4th: holds.")

case("rank_two_missing_first", ["sc2"], "sc2", [rank("score", "desc", "min", "first")],
     a_value([S("f")], "rank", "eq", I(1)), "holds",
     [(6, "Rank by score descending within all rows (ties: min; missing values placed first); 6 rows.")],
     cols_of("sc2", add=["rank"]),
     sc_ranked(["d", "f", "b", "c", "a", "e"], {"d": 1, "f": 1, "b": 3, "c": 3, "a": 5, "e": 6}, "sc2"), ["id"],
     notes="Two missing rows placed first tie with each other (SQL RANK treats NULLs as equal): d 1, "
           "f 1, b 3, c 3, a 5, e 6. f.rank = 1: holds.")

# -- derive ---------------------------------------------------------------------------------
def de_add(colname, values):
    return rows("de", DE_ALL, add={k: {colname: v} for k, v in zip(DE_ALL, values)})


case("derive_add_int", ["de"], "de", [derive("n1", "n + 1")],
     a_value([S("TP53")], "n1", "eq", I(5)), "holds",
     [(5, "Add column n1 = n + 1; 5 rows.")],
     DE_COLS + ["n1"], de_add("n1", [I(4), I(8), I(3), I(6), I(5)]), ["gene_id"],
     notes="int + int gives int: 3+1=4, 7+1=8, 2+1=3, 5+1=6, 4+1=5. TP53.n1 = 5: holds.")

case("derive_mul_scale", ["de"], "de", [derive("s", "fc * 1.10")],
     a_value([S("TP53")], "s", "eq", D("2.75")), "holds",
     [(5, "Add column s = fc * 1.10; 5 rows.")],
     DE_COLS + ["s"], de_add("s", [D("0.550"), D("3.300"), M, D("-1.650"), D("2.750")]), ["gene_id"],
     notes="Multiplication scale is the sum of the scales (1 + 2 = 3): 0.5*1.10 = 0.550, 3.0*1.10 = "
           "3.300, missing, -1.5*1.10 = -1.650, 2.5*1.10 = 2.750. 2.750 = 2.75 as decimals: holds.")

case("derive_div_exact", ["de"], "de", [derive("q", "n / 4")],
     a_value([S("TP53")], "q", "eq", I(1)), "holds",
     [(5, "Add column q = n / 4; 5 rows.")],
     DE_COLS + ["q"], de_add("q", [D("0.75"), D("1.75"), D("0.5"), D("1.25"), D("1")]), ["gene_id"],
     notes="/ always gives dec, exact when it terminates: 3/4 = 0.75, 7/4 = 1.75, 2/4 = 0.5, 5/4 = "
           "1.25, 4/4 = 1 (ideal exponent 0, as Python decimal writes it). TP53.q 1 = 1: holds.")

case("derive_div_rounds", ["de"], "de", [derive("q", "n / 3")],
     a_value([S("EGFR")], "q", "eq", D("2.3333333333"), tol("0.000000001", "0")), "holds",
     [(5, "Add column q = n / 3; 5 rows.")],
     DE_COLS + ["q"], de_add("q", [D("1"), D("2.333333333333333333333333333333333"),
                                   D("0.6666666666666666666666666666666667"),
                                   D("1.666666666666666666666666666666667"),
                                   D("1.333333333333333333333333333333333")]), ["gene_id"],
     notes="Non-terminating division rounds to 34 significant digits, half-even: 3/3 = 1; 7/3 = 2 + 33 "
           "threes; 2/3 = 0.(33 sixes)7 (the leading 0 is not significant, so 34 digits after the "
           "point); 5/3 = 1.(32 sixes)7; 4/3 = 1.(33 threes). |2.333...3 - 2.3333333333| = "
           "0.0000000000333... <= 0.000000001: holds.")

case("derive_div_zero", ["de"], "de", [derive("q", "n / (n - 3)")],
     a_value([S("BRCA1")], "q", "eq", I(1)), "fails",
     [(5, "Add column q = n / (n - 3); 5 rows.")],
     DE_COLS + ["q"], de_add("q", [M, D("1.75"), D("-2"), D("2.5"), D("4")]), ["gene_id"],
     notes="Parentheses are required (precedence) and kept. BRCA1: 3/(3-3) = 3/0 -> missing, not an "
           "error. EGFR 7/4 = 1.75; KRAS 2/-1 = -2; MDM2 5/2 = 2.5; TP53 4/1 = 4. BRCA1.q is "
           "missing: a missing cell never satisfies any comparison: fails.")

case("derive_parens_kept", ["de"], "de", [derive("m", "(n+1)*2")],
     a_value([S("KRAS")], "m", "eq", I(6)), "holds",
     [(5, "Add column m = (n + 1) * 2; 5 rows.")],
     DE_COLS + ["m"], de_add("m", [I(8), I(16), I(6), I(12), I(10)]), ["gene_id"],
     notes="{expr} is re-serialized with spaces around binary operators; the parentheses are "
           "required by precedence. (3+1)*2 = 8, 16, 6, 12, 10. KRAS.m = 6: holds.")

case("derive_parens_dropped", ["de"], "de", [derive("m", "n + (1 * 2)")],
     a_value([S("BRCA1")], "m", "eq", I(5)), "holds",
     [(5, "Add column m = n + 1 * 2; 5 rows.")],
     DE_COLS + ["m"], de_add("m", [I(5), I(9), I(4), I(7), I(6)]), ["gene_id"],
     notes="Parentheses around the higher-precedence product are not required and are dropped. "
           "3+2 = 5, 9, 4, 7, 6. BRCA1.m = 5: holds.")

case("derive_abs", ["de"], "de", [derive("a", "abs(fc)")],
     a_value([S("MDM2")], "a", "eq", D("1.5")), "holds",
     [(5, "Add column a = abs(fc); 5 rows.")],
     DE_COLS + ["a"], de_add("a", [D("0.5"), D("3.0"), M, D("1.5"), D("2.5")]), ["gene_id"],
     notes="abs keeps dec and scale: 0.5, 3.0, missing, 1.5, 2.5. MDM2.a = 1.5: holds.")

case("derive_unary_minus", ["de"], "de", [derive("neg", "-n")],
     a_value([S("TP53")], "neg", "eq", I(-4)), "holds",
     [(5, "Add column neg = -n; 5 rows.")],
     DE_COLS + ["neg"], de_add("neg", [I(-3), I(-7), I(-2), I(-5), I(-4)]), ["gene_id"],
     notes="Unary minus keeps int, rendered with no space: -3, -7, -2, -5, -4. TP53.neg = -4: holds.")

case("derive_sub_int_dec", ["de"], "de", [derive("d", "n - fc")],
     a_value([S("MDM2")], "d", "eq", D("6.5")), "holds",
     [(5, "Add column d = n - fc; 5 rows.")],
     DE_COLS + ["d"], de_add("d", [D("2.5"), D("4.0"), M, D("6.5"), D("1.5")]), ["gene_id"],
     notes="int with dec gives dec at the larger scale (1): 3-0.5 = 2.5, 7-3.0 = 4.0, missing, "
           "5-(-1.5) = 6.5, 4-2.5 = 1.5. MDM2.d = 6.5: holds.")

case("derive_str_eq_bool", ["de"], "de", [derive("liv", 'tissue = "liver"')],
     a_value([S("TP53")], "liv", "eq", B(True)), "holds",
     [(5, 'Add column liv = tissue = "liver"; 5 rows.')],
     DE_COLS + ["liv"], de_add("liv", [B(False), B(False), B(False), B(True), B(True)]), ["gene_id"],
     notes="String equality gives bool: false, false, false, true, true. TP53.liv eq true: holds.")

case("derive_bool_missing", ["de"], "de", [derive("big", "fc > 1.0")],
     a_value([S("KRAS")], "big", "eq", B(True)), "fails",
     [(5, "Add column big = fc > 1.0; 5 rows.")],
     DE_COLS + ["big"], de_add("big", [B(False), B(True), M, B(False), B(True)]), ["gene_id"],
     notes="Comparison with a missing operand is missing: BRCA1 false, EGFR true, KRAS missing, MDM2 "
           "false, TP53 true. KRAS.big is missing: fails.")

case("derive_coalesce_widen", ["de"], "de", [derive("c", "coalesce(fc, n)")],
     a_value([S("KRAS")], "c", "eq", I(2)), "holds",
     [(5, "Add column c = coalesce(fc, n); 5 rows.")],
     DE_COLS + ["c"], de_add("c", [D("0.5"), D("3.0"), D("2"), D("-1.5"), D("2.5")]), ["gene_id"],
     notes="coalesce of dec and int gives dec; KRAS takes n = 2, written as dec '2' (an int widened to "
           "dec carries no fractional digits). Others keep fc. KRAS.c 2 = 2: holds.")

case("derive_log_domain", ["dom"], "dom", [derive("l", "log(x)")],
     a_value([S("d")], "l", "eq", D("4.60517"), tol("0.00001", "0")), "holds",
     [(5, "Add column l = log(x); 5 rows.")],
     cols_of("dom", add=["l"]),
     rows("dom", ["a", "b", "c", "d", "e"], add={"a": {"l": M}, "b": {"l": M}, "c": {"l": D("0")},
                                                 "d": {"l": D("4.60517018598809")}, "e": {"l": M}}), ["id"],
     notes="log is the natural logarithm through IEEE double, rounded to 15 significant digits. "
           "log(0.0) and log(-1.0) are at or below zero: missing. log(1.0) = 0. log(100.0) = "
           "4.605170185988092 -> 4.60517018598809. Missing stays missing. |4.60517018598809 - "
           "4.60517| = 0.00000018598809 <= 0.00001: holds.")

case("derive_log2", ["pw"], "pw", [derive("l", "log2(x)")],
     a_value([S("a")], "l", "eq", I(3)), "holds",
     [(4, "Add column l = log2(x); 4 rows.")],
     cols_of("pw", add=["l"]),
     rows("pw", ["a", "b", "c", "d"], add={"a": {"l": D("3")}, "b": {"l": D("-1")},
                                           "c": {"l": D("9.96578428466209")}, "d": {"l": M}}), ["id"],
     notes="log2(8.0) = 3 exactly; log2(0.5) = -1; log2(1000.0) = 9.965784284662087 -> "
           "9.96578428466209 (15 significant digits); log2(0.0): missing. a.l 3 = 3: holds.")

case("derive_log10", ["pw"], "pw", [derive("l", "log10(x)")],
     a_value([S("c")], "l", "eq", D("3.0")), "holds",
     [(4, "Add column l = log10(x); 4 rows.")],
     cols_of("pw", add=["l"]),
     rows("pw", ["a", "b", "c", "d"], add={"a": {"l": D("0.903089986991944")},
                                           "b": {"l": D("-0.301029995663981")},
                                           "c": {"l": D("3")}, "d": {"l": M}}), ["id"],
     notes="log10(8.0) = 0.9030899869919435 -> 0.903089986991944; log10(0.5) = -0.3010299956639812 -> "
           "-0.301029995663981; log10(1000.0) = 3; log10(0.0): missing. c.l 3 = 3.0: holds.")

case("derive_exp", ["ex"], "ex", [derive("y", "exp(x)")],
     a_value([S("c")], "y", "gt", I(1)), "fails",
     [(5, "Add column y = exp(x); 5 rows.")],
     cols_of("ex", add=["y"]),
     rows("ex", ["a", "b", "c", "d", "e"], add={"a": {"y": D("1")}, "b": {"y": D("2.71828182845905")},
                                                "c": {"y": M}, "d": {"y": D("0.367879441171442")},
                                                "e": {"y": M}}), ["id"],
     notes="exp(0) = 1; exp(1) = 2.718281828459045 -> 2.71828182845905; exp(1000) overflows the "
           "double: missing, not an error; exp(-1) = 0.36787944117144233 -> 0.367879441171442; "
           "missing stays missing. c.y is missing: fails.")

case("derive_rounds_at_34_digits", ["big"], "big", [derive("sq", "x * x")],
     a_value([S("a")], "sq", "eq", D("100000000000000010000000000000000.2")), "holds",
     [(1, "Add column sq = x * x; 1 rows.")],
     cols_of("big", add=["sq"]),
     rows("big", ["a"], add={"a": {"sq": D("100000000000000010000000000000000.2")}}), ["id"],
     notes="Re-derived from the revised text (was derive_overflow_error). x = 10000000000000000.5 "
           "(18 significant digits, scale 1). The exact product is 100000000000000010000000000000000.25: "
           "33 integer digits + 2 fraction digits = 35 significant digits, scale 2. 'dec results round "
           "to 34 significant digits, half-even' and 'Rounding at 34 significant digits is not an "
           "overflow: the result rounds and the check goes on'. Dropping the last digit 5 exactly "
           "half-way, the kept digit 2 is even, so it stays: 100000000000000010000000000000000.2, "
           "scale 1 (rounding overrides the sum-of-scales rule). Adjusted exponent 32, far below Emax "
           "6144, so no Overflow. a.sq equals the rounded value: holds.")

case("derive_coalesce_static_dec", ["cw"], "cw", [derive("d1", "coalesce(i, x) * coalesce(i, x)")],
     a_value([S("a")], "d1", "eq", D("10000000000000000200000000000000000")), "holds",
     [(4, "Add column d1 = coalesce(i, x) * coalesce(i, x); 4 rows.")],
     cols_of("cw", add=["d1"]),
     rows("cw", ["a", "b", "c", "d"], add={"a": {"d1": D("10000000000000000200000000000000000")},
                                           "b": {"d1": D("6.25")}, "c": {"d1": D("9")},
                                           "d": {"d1": M}}), ["id"],
     notes="Ruling D19 (2026-11-20): arithmetic is performed in the expression's static type. "
           "coalesce(i, x) is dec because x is a dec column, so the product is dec and rounds at 34 "
           "significant digits whichever branch coalesce took at run time. a: both operands are the "
           "int cell 100000000000000001 (18 digits), chosen by coalesce and widened to dec; the exact "
           "square 10000000000000000200000000000000001 has 35 significant digits and rounds half-even "
           "to 34, dropping the final 1: 10000000000000000200000000000000000 (an int product would "
           "have kept it). b: i is missing, so 2.5 * 2.5 = 6.25. c: 3 * 3 = 9, written as the integer. "
           "d: both missing, so missing. a.d1 equals the rounded value: holds.")

case("derive_date_arith_error", ["ev"], "ev", [derive("d3", "d + 1")],
     a_count("eq", I(4)), "error", error=("E6004", "/ops/0/expr"),
     notes="Arithmetic on a date is E6004 (Dates row), checked before anything runs.")

case("derive_missing_no_type_error", ["de"], "de", [derive("m", "missing")],
     a_count("eq", I(5)), "error", error=("E6004", "/ops/0/expr"),
     notes="'a derive whose result has no type is E6004': the bare literal missing has no type.")

case("derive_name_present_error", ["de"], "de", [derive("fc", "n + 1")],
     a_count("eq", I(5)), "error", error=("E6002", "/ops/0/as"),
     notes="'a step that would add a column whose name is already present' is E6002, field at the "
           "member that names it.")

case("derive_then_filter", ["de"], "de", [derive("m", "n * 2"), filt("m > 8")],
     a_value([S("EGFR")], "m", "eq", I(14)), "holds",
     [(5, "Add column m = n * 2; 5 rows."),
      (2, "Keep rows where m > 8 (missing values excluded); 2 of 5 rows remain.")],
     DE_COLS + ["m"], rows("de", ["EGFR", "MDM2"], add={"EGFR": {"m": I(14)}, "MDM2": {"m": I(10)}}), ["gene_id"],
     notes="m = n*2: 6, 14, 4, 10, 8. m > 8 keeps EGFR 14 and MDM2 10 (8 is not > 8). EGFR.m = 14: holds.")

# -- group ---------------------------------------------------------------------------------------
BREAST = [[S("BRCA1")], [S("EGFR")]]
LIVER = [[S("MDM2")], [S("TP53")]]
LUNG = [[S("KRAS")]]


def de_groups(aggs_by_tissue):
    """aggs_by_tissue: {tissue: {as: value}} in group order breast, liver, lung."""
    return [
        grp_rows({"tissue": S("breast")}, aggs_by_tissue["breast"], 2, BREAST),
        grp_rows({"tissue": S("liver")}, aggs_by_tissue["liver"], 2, LIVER),
        grp_rows({"tissue": S("lung")}, aggs_by_tissue["lung"], 1, LUNG),
    ]


def gcols(by, aggs):
    return by + aggs + ["member_count", "member_keys_hash"]


case("group_count", ["de"], "de", [group(["tissue"], [("cnt", "count", "fc")])],
     a_value([S("lung")], "cnt", "eq", I(1)), "holds",
     [(3, "Group by tissue and compute cnt = count(fc); 3 groups from 5 rows.")],
     gcols(["tissue"], ["cnt"]),
     de_groups({"breast": {"cnt": I(2)}, "liver": {"cnt": I(2)}, "lung": {"cnt": I(1)}}), ["tissue"],
     notes="Groups in tuple sort order: breast {BRCA1, EGFR}, liver {MDM2, TP53}, lung {KRAS}. count "
           "counts every member, missing included (KRAS.fc is missing): 2, 2, 1. member_count 2, 2, 1. "
           "member_keys_hash = sha256 of the canonical JSON list of key tuples sorted in binary order. "
           "lung.cnt = 1: holds.")

case("group_sum_int", ["de"], "de", [group(["tissue"], [("s", "sum", "n")])],
     a_value([S("breast")], "s", "eq", I(10)), "holds",
     [(3, "Group by tissue and compute s = sum(n); 3 groups from 5 rows.")],
     gcols(["tissue"], ["s"]),
     de_groups({"breast": {"s": I(10)}, "liver": {"s": I(9)}, "lung": {"s": I(2)}}), ["tissue"],
     notes="sum keeps int: breast 3+7 = 10, liver 5+4 = 9, lung 2. breast.s = 10: holds.")

case("group_sum_dec_all_missing", ["de"], "de", [group(["tissue"], [("s", "sum", "fc")])],
     a_value([S("lung")], "s", "eq", I(0)), "fails",
     [(3, "Group by tissue and compute s = sum(fc); 3 groups from 5 rows.")],
     gcols(["tissue"], ["s"]),
     de_groups({"breast": {"s": D("3.5")}, "liver": {"s": D("1.0")}, "lung": {"s": M}}), ["tissue"],
     notes="sum ignores missing and keeps dec at scale 1: breast 0.5+3.0 = 3.5, liver -1.5+2.5 = 1.0, "
           "lung has only a missing member so the sum is missing. lung.s missing: fails.")

case("group_min_max", ["de"], "de", [group(["tissue"], [("lo", "min", "p"), ("hi", "max", "p")])],
     a_cmp([S("liver")], "lo", [S("breast")], "hi", "lt"), "holds",
     [(3, "Group by tissue and compute lo = min(p) and hi = max(p); 3 groups from 5 rows.")],
     gcols(["tissue"], ["lo", "hi"]),
     de_groups({"breast": {"lo": D("0.200"), "hi": D("0.200")},
                "liver": {"lo": D("0.001"), "hi": D("0.040")},
                "lung": {"lo": D("0.010"), "hi": D("0.010")}}), ["tissue"],
     notes="Two aggregates render as 'lo = min(p) and hi = max(p)'. min and max ignore missing: breast "
           "{0.200, missing} -> 0.200 / 0.200; liver {0.040, 0.001} -> 0.001 / 0.040; lung 0.010. "
           "compare_rows liver.lo 0.001 < breast.hi 0.200: holds.")

case("group_min_str", ["de"], "de", [group(["tissue"], [("g", "min", "gene_id")])],
     a_value([S("liver")], "g", "eq", S("MDM2")), "holds",
     [(3, "Group by tissue and compute g = min(gene_id); 3 groups from 5 rows.")],
     gcols(["tissue"], ["g"]),
     de_groups({"breast": {"g": S("BRCA1")}, "liver": {"g": S("MDM2")}, "lung": {"g": S("KRAS")}}), ["tissue"],
     notes="min on str keeps str and uses binary order: BRCA1 < EGFR, MDM2 < TP53, KRAS. "
           "liver.g = MDM2: holds.")

case("group_mean_int", ["de"], "de", [group(["tissue"], [("m", "mean", "n")])],
     a_value([S("liver")], "m", "eq", D("4.5")), "holds",
     [(3, "Group by tissue and compute m = mean(n); 3 groups from 5 rows.")],
     gcols(["tissue"], ["m"]),
     de_groups({"breast": {"m": D("5")}, "liver": {"m": D("4.5")}, "lung": {"m": D("2")}}), ["tissue"],
     notes="mean is a DOUBLE rounded to 15 significant digits and written shortest: (3+7)/2 = 5 -> '5'; "
           "(5+4)/2 = 4.5; 2/1 = 2. liver.m = 4.5: holds.")

case("group_mean_inexact", ["sc"], "sc", [group(["grp"], [("m", "mean", "score")])],
     a_value([S("x")], "m", "eq", D("11.6667"), tol("0.0001", "0")), "holds",
     [(2, "Group by grp and compute m = mean(score); 2 groups from 5 rows.")],
     gcols(["grp"], ["m"]),
     [grp_rows({"grp": S("x")}, {"m": D("11.6666666666667")}, 3, [[S("a")], [S("b")], [S("e")]]),
      grp_rows({"grp": S("y")}, {"m": D("20")}, 2, [[S("c")], [S("d")]])], ["grp"],
     notes="x = {a 10, b 20, e 5}: 35/3 = 11.666666666666666 -> 11.6666666666667 (15 significant "
           "digits, half-even on the 16th). y = {c 20, d missing}: the missing member is ignored (as "
           "DuckDB avg does; checker.md states this only for sum, min, max), 20/1 = 20. member_count "
           "counts d: 3 and 2. |11.6666666666667 - 11.6667| = 0.0000333333333 <= 0.0001: holds.")

case("group_median_even", ["de"], "de", [group(["tissue"], [("md", "median", "n")])],
     a_value([S("breast")], "md", "eq", I(5)), "holds",
     [(3, "Group by tissue and compute md = median(n); 3 groups from 5 rows.")],
     gcols(["tissue"], ["md"]),
     de_groups({"breast": {"md": D("5")}, "liver": {"md": D("4.5")}, "lung": {"md": D("2")}}), ["tissue"],
     notes="median is a DOUBLE: an even member count takes the midpoint of the two middle values: "
           "breast {3, 7} -> 5; liver {4, 5} -> 4.5; lung {2} -> 2. breast.md 5 = 5: holds.")

case("group_median_odd", ["sc"], "sc", [group(["grp"], [("md", "median", "score")])],
     a_value([S("x")], "md", "eq", I(10)), "holds",
     [(2, "Group by grp and compute md = median(score); 2 groups from 5 rows.")],
     gcols(["grp"], ["md"]),
     [grp_rows({"grp": S("x")}, {"md": D("10")}, 3, [[S("a")], [S("b")], [S("e")]]),
      grp_rows({"grp": S("y")}, {"md": D("20")}, 2, [[S("c")], [S("d")]])], ["grp"],
     notes="x = {5, 10, 20} -> 10; y = {20, missing} -> 20 (missing ignored). x.md = 10: holds.")

case("group_two_by_columns", ["obs"], "obs", [group(["cond", "gene"], [("k", "count", "val")])],
     a_value([S("trt"), S("A")], "k", "eq", I(2)), "holds",
     [(4, "Group by cond and gene and compute k = count(val); 4 groups from 5 rows.")],
     gcols(["cond", "gene"], ["k"]),
     [grp_rows({"cond": S("ctl"), "gene": S("A")}, {"k": I(1)}, 1, [[S("s1"), S("A")]]),
      grp_rows({"cond": S("ctl"), "gene": S("B")}, {"k": I(1)}, 1, [[S("s1"), S("B")]]),
      grp_rows({"cond": S("trt"), "gene": S("A")}, {"k": I(2)}, 2, [[S("s2"), S("A")], [S("s3"), S("A")]]),
      grp_rows({"cond": S("trt"), "gene": S("B")}, {"k": I(1)}, 1, [[S("s2"), S("B")]])], ["cond", "gene"],
     notes="Tuples in sort order: (ctl,A), (ctl,B), (trt,A), (trt,B). (trt,A) has members (s2,A) and "
           "(s3,A); (trt,B) has (s2,B) whose val is missing but count counts it. The output key is "
           "(cond, gene), so the assertion row is [trt, A]. k = 2: holds.")

case("group_max_date", ["ev"], "ev", [group(["k"], [("mx", "max", "d")])],
     a_value([S("g2")], "mx", "eq", DA("2025-12-31")), "holds",
     [(2, "Group by k and compute mx = max(d); 2 groups from 4 rows.")],
     gcols(["k"], ["mx"]),
     [grp_rows({"k": S("g1")}, {"mx": DA("2026-03-01")}, 2, [[S("a")], [S("b")]]),
      grp_rows({"k": S("g2")}, {"mx": DA("2025-12-31")}, 2, [[S("c")], [S("d")]])], ["k"],
     notes="max takes a date and keeps it: g1 {2026-01-05, 2026-03-01} -> 2026-03-01; g2 {2025-12-31, "
           "missing} -> 2025-12-31. g2.mx = 2025-12-31: holds.")

case("group_three_aggregates", ["de"], "de",
     [group(["tissue"], [("cnt", "count", "n"), ("s", "sum", "n"), ("mx", "max", "n")])],
     a_value([S("breast")], "mx", "eq", I(7)), "holds",
     [(3, "Group by tissue and compute cnt = count(n), s = sum(n), and mx = max(n); 3 groups from 5 rows.")],
     gcols(["tissue"], ["cnt", "s", "mx"]),
     de_groups({"breast": {"cnt": I(2), "s": I(10), "mx": I(7)},
                "liver": {"cnt": I(2), "s": I(9), "mx": I(5)},
                "lung": {"cnt": I(1), "s": I(2), "mx": I(2)}}), ["tissue"],
     notes="Three aggregates render with ', and ' before the last. breast: 2, 10, 7; liver: 2, 9, 5; "
           "lung: 1, 2, 2. breast.mx = 7: holds.")

# -- join -------------------------------------------------------------------------------------------
JOIN_ANN_COLS = DE_COLS + ["ann.gene_id", "ann.chrom"]

case("join_inner", ["de", "ann"], "de", [join("ann", [("gene_id", "gene_id")], "inner")],
     a_value([S("TP53")], "ann.chrom", "eq", I(17)), "holds",
     [(3, "Inner join with ann on gene_id = gene_id; 3 rows.")],
     JOIN_ANN_COLS,
     joined("de", ["BRCA1", "KRAS", "TP53"], "ann", {"BRCA1": "BRCA1", "KRAS": "KRAS", "TP53": "TP53"}, "inner"),
     ["gene_id"],
     notes="ann has BRCA1, KRAS, TP53, ZZZ3. Inner join drops EGFR and MDM2 (unmatched left) and never "
           "adds ZZZ3. Every right column is added with the prefix 'ann.', key included. Left order "
           "kept. TP53.ann.chrom = 17: holds.")

case("join_left", ["de", "ann"], "de", [join("ann", [("gene_id", "gene_id")], "left")],
     a_value([S("MDM2")], "ann.chrom", "ne", I(17)), "fails",
     [(5, "Left join with ann on gene_id = gene_id; 5 rows.")],
     JOIN_ANN_COLS,
     joined("de", DE_ALL, "ann", {"BRCA1": "BRCA1", "EGFR": None, "KRAS": "KRAS", "MDM2": None, "TP53": "TP53"}, "left"),
     ["gene_id"],
     notes="Left join keeps EGFR and MDM2 with missing in ann.gene_id and ann.chrom. MDM2.ann.chrom is "
           "missing, and a missing cell never satisfies any comparison, ne included: fails.")

case("join_left_nonkey_column", ["de", "tiss"], "de", [join("tiss", [("tissue", "tissue")], "left")],
     a_value([S("TP53")], "tiss.organ", "eq", S("hepatic")), "holds",
     [(5, "Left join with tiss on tissue = tissue; 5 rows.")],
     DE_COLS + ["tiss.tissue", "tiss.organ"],
     joined("de", DE_ALL, "tiss", {"BRCA1": "breast", "EGFR": "breast", "KRAS": None, "MDM2": "liver", "TP53": "liver"}, "left"),
     ["gene_id"],
     notes="The on pair covers the right key (tissue); the left column need not be a key. breast and "
           "liver match; lung (KRAS) does not and gets missing. Output key stays gene_id. "
           "TP53.tiss.organ = hepatic: holds.")

case("join_composite_on", ["obs", "meta"], "obs",
     [join("meta", [("sample", "sample"), ("gene", "gene")], "inner")],
     a_value([S("s2"), S("B")], "meta.qc", "eq", S("fail")), "holds",
     [(3, "Inner join with meta on sample = sample and gene = gene; 3 rows.")],
     cols_of("obs") + ["meta.sample", "meta.gene", "meta.qc"],
     joined("obs", ["s1,A", "s2,B", "s3,A"], "meta", {"s1,A": "s1,A", "s2,B": "s2,B", "s3,A": "s3,A"}, "inner"),
     ["sample", "gene"],
     notes="Two on pairs render as a two-item list with ' and '. meta has (s1,A), (s2,B), (s3,A); "
           "(s1,B) and (s2,A) are dropped. Output key is the left key (sample, gene). "
           "(s2,B).meta.qc = fail: holds.")

case("join_then_filter_prefixed", ["de", "ann"], "de",
     [join("ann", [("gene_id", "gene_id")], "inner"), filt("`ann.chrom` = 17")],
     a_count("eq", I(2)), "holds",
     [(3, "Inner join with ann on gene_id = gene_id; 3 rows."),
      (2, "Keep rows where `ann.chrom` = 17 (missing values excluded); 2 of 3 rows remain.")],
     JOIN_ANN_COLS,
     joined("de", ["BRCA1", "TP53"], "ann", {"BRCA1": "BRCA1", "TP53": "TP53"}, "inner"),
     ["gene_id"],
     notes="A prefixed column has a dot, which IDENT does not allow, so the expression needs backticks "
           "and {expr} keeps them. chrom 17: BRCA1, TP53; KRAS 12 dropped. count 2 = 2: holds.")

# -- composite key ----------------------------------------------------------------------------------
case("composite_filter_value", ["obs"], "obs", [filt('cond = "trt"')],
     a_value([S("s2"), S("A")], "val", "eq", D("3.0")), "holds",
     [(3, 'Keep rows where cond = "trt" (missing values excluded); 3 of 5 rows remain.')],
     cols_of("obs"), rows("obs", ["s2,A", "s2,B", "s3,A"]), ["sample", "gene"],
     notes="Key is (sample, gene); the assertion row lists both in key order. trt rows: (s2,A), (s2,B), "
           "(s3,A). The key map holds two-element tuples. (s2,A).val = 3.0: holds.")

case("composite_sort", ["obs"], "obs", [sort([("val", "desc")], "first")],
     a_rank([S("s3"), S("A")], 2), "holds",
     [(5, "Sort by val descending (missing values placed first; ties broken by sample and gene); 5 rows.")],
     cols_of("obs"), rows("obs", ["s2,B", "s3,A", "s2,A", "s1,B", "s1,A"]), ["sample", "gene"],
     notes="{key} is the two-column key, rendered 'sample and gene'. Missing first: (s2,B); then 5.0 "
           "(s3,A), 3.0 (s2,A), 2.0 (s1,B), 1.0 (s1,A). (s3,A) at position 2: holds.")

case("composite_group_sum", ["obs"], "obs", [group(["sample"], [("s", "sum", "val")])],
     a_cmp([S("s1")], "s", [S("s2")], "s", "eq"), "holds",
     [(3, "Group by sample and compute s = sum(val); 3 groups from 5 rows.")],
     gcols(["sample"], ["s"]),
     [grp_rows({"sample": S("s1")}, {"s": D("3.0")}, 2, [[S("s1"), S("A")], [S("s1"), S("B")]]),
      grp_rows({"sample": S("s2")}, {"s": D("3.0")}, 2, [[S("s2"), S("A")], [S("s2"), S("B")]]),
      grp_rows({"sample": S("s3")}, {"s": D("5.0")}, 1, [[S("s3"), S("A")]])], ["sample"],
     notes="Member key tuples are two-element. s1: 1.0+2.0 = 3.0; s2: 3.0 + missing -> 3.0; s3: 5.0. "
           "Output key is sample alone. compare_rows s1.s = s2.s: 3.0 = 3.0: holds.")

# -- NaN and infinity -------------------------------------------------------------------------------
case("nan_inf_sort_missing_last", ["fl"], "fl", [sort([("x", "desc")], "last")],
     a_rank([S("f")], 2), "holds",
     [(6, "Sort by x descending (missing values placed last; ties broken by id); 6 rows.")],
     cols_of("fl"), rows("fl", ["a", "f", "b", "c", "d", "e"]), ["id"],
     notes="The spec maps NaN and Inf to values, and 'nan, +inf, -inf' are 'placed by the sort's "
           "missing rule', so b (nan), c (+inf), d (-inf) and e (missing) all go last, in key order. "
           "Before them x descending: a 1.5, f 0.25. float64 values are the shortest round-trip "
           "decimal. f at position 2: holds.")

case("nan_inf_filter_excluded", ["fl"], "fl", [filt("x > 0")],
     a_count("eq", I(2)), "holds",
     [(2, "Keep rows where x > 0 (missing values excluded); 2 of 6 rows remain.")],
     cols_of("fl"), rows("fl", ["a", "f"]), ["id"],
     notes="nan and inf are 'treated as missing by filters', so x > 0 is missing for b, c, d and e; "
           "a 1.5 and f 0.25 remain. count 2 = 2: holds.")

case("nan_cell_fails", ["fl"], "fl", [],
     a_value([S("b")], "x", "ne", I(1)), "fails",
     [], cols_of("fl"), rows("fl", ["a", "b", "c", "d", "e", "f"]), ["id"],
     notes="No step; the final table is the input in key order with the nan and inf typed values. "
           "b.x is nan: never satisfies any comparison: fails.")

case("inf_cell_fails", ["fl"], "fl", [],
     a_value([S("c")], "x", "gt", I(1)), "fails",
     [], cols_of("fl"), rows("fl", ["a", "b", "c", "d", "e", "f"]), ["id"],
     notes="c.x is +inf; an inf cell gives fails even for gt (11.18.3 Missing row).")

case("nan_inf_mapped_to_missing", ["flm"], "flm", [sort([("x", "asc")], "first")],
     a_rank([S("a")], 6), "holds",
     [(6, "Sort by x ascending (missing values placed first; ties broken by id); 6 rows.")],
     cols_of("flm"), rows("flm", ["b", "c", "d", "e", "f", "a"]), ["id"],
     notes="Same file, spec nan: missing and inf: missing, so b, c, d read as missing like e. Missing "
           "first in key order: b, c, d, e; then f 0.25, a 1.5. a at position 6: holds.")

# -- dates ----------------------------------------------------------------------------------------------
case("filter_date_lt_date", ["ev"], "ev", [filt("d < d2")],
     a_value([S("c")], "d", "eq", DA("2025-12-31")), "holds",
     [(1, "Keep rows where d < d2 (missing values excluded); 1 of 4 rows remain.")],
     cols_of("ev"), rows("ev", ["c"]), ["id"],
     notes="Dates compare chronologically: a 01-05 < 01-01 false; b equal false; c 2025-12-31 < "
           "2026-06-30 true; d missing. 1 of 4. c.d = 2025-12-31: holds.")

case("filter_date_vs_datetime_error", ["ev"], "ev", [filt("d < ts")],
     a_count("eq", I(4)), "error", error=("E6004", "/ops/0/where"),
     notes="'a date and a datetime do not compare': E6004 at the expression, before anything runs.")

case("value_datetime_lt", ["ev"], "ev", [],
     a_value([S("a")], "ts", "lt", DT("2026-01-05T12:00:00Z")), "holds",
     [], cols_of("ev"), rows("ev", ["a", "b", "c", "d"]), ["id"],
     notes="a.ts 2026-01-05T10:00:00Z is before 2026-01-05T12:00:00Z: holds.")

case("value_date_between", ["ev"], "ev", [],
     a_value([S("b")], "d", "between", [DA("2026-01-01"), DA("2026-12-31")]), "holds",
     [], cols_of("ev"), rows("ev", ["a", "b", "c", "d"]), ["id"],
     notes="2026-01-01 <= 2026-03-01 <= 2026-12-31, bounds inclusive, no tol: holds.")

case("value_date_tol_error", ["ev"], "ev", [],
     a_value([S("a")], "d", "eq", DA("2026-01-05"), tol("1", "0")), "error", error=("E6004", "/tol"),
     notes="'op on dates: tol is E6004'; the schema is known before running, so it is refused at /tol.")

# -- assertion kinds and operators ---------------------------------------------------------------------
FC_GT1 = [filt("fc > 1.0")]
FC_GT1_STEP = [(2, "Keep rows where fc > 1.0 (missing values excluded); 2 of 5 rows remain.")]
FC_GT1_ROWS = rows("de", ["EGFR", "TP53"])

case("count_ne", ["de"], "de", FC_GT1, a_count("ne", I(3)), "holds",
     FC_GT1_STEP, DE_COLS, FC_GT1_ROWS, ["gene_id"], notes="2 rows; 2 != 3 (|2 - 3| = 1 > 0): holds.")
case("count_lt", ["de"], "de", FC_GT1, a_count("lt", I(3)), "holds",
     FC_GT1_STEP, DE_COLS, FC_GT1_ROWS, ["gene_id"], notes="2 < 3: holds.")
case("count_le_fails", ["de"], "de", FC_GT1, a_count("le", I(1)), "fails",
     FC_GT1_STEP, DE_COLS, FC_GT1_ROWS, ["gene_id"], notes="2 <= 1 is false: fails.")
case("count_gt", ["de"], "de", [], a_count("gt", I(4)), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="5 > 4: holds.")
case("count_between", ["de"], "de", FC_GT1, a_count("between", [I(2), I(2)]), "holds",
     FC_GT1_STEP, DE_COLS, FC_GT1_ROWS, ["gene_id"], notes="2 <= 2 <= 2, inclusive: holds.")
case("count_eq_fails", ["de"], "de", FC_GT1, a_count("eq", I(3)), "fails",
     FC_GT1_STEP, DE_COLS, FC_GT1_ROWS, ["gene_id"], notes="2 = 3 is false: fails.")

case("value_ne_holds", ["de"], "de", [], a_value([S("TP53")], "fc", "ne", D("1.0")), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="|2.5 - 1.0| = 1.5 > 0: holds.")
case("value_lt_holds", ["de"], "de", [], a_value([S("TP53")], "fc", "lt", I(3)), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="2.5 < 3 (dec against int): holds.")
case("value_le_edge", ["de"], "de", [], a_value([S("TP53")], "fc", "le", D("2.5")), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="2.5 <= 2.5: holds.")
case("value_gt_fails", ["de"], "de", [], a_value([S("TP53")], "fc", "gt", D("2.5")), "fails",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="2.5 > 2.5 is false with m = 0: fails.")
case("value_ge_tol", ["de"], "de", [], a_value([S("TP53")], "fc", "ge", D("2.6"), tol("0.1", "0")), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="m = 0.1; 2.5 >= 2.6 - 0.1 = 2.5: holds.")
case("value_between_rel", ["de"], "de", [],
     a_value([S("TP53")], "fc", "between", [D("1.0"), D("2.0")], tol("0", "0.25")), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"],
     notes="lo - 0.25*1.0 = 0.75 <= 2.5 <= hi + 0.25*2.0 = 2.5: holds (each bound widened by its own |bound|).")
case("value_eq_abs_rel_sum", ["de"], "de", [],
     a_value([S("EGFR")], "fc", "eq", D("2.8"), tol("0.1", "0.05")), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"],
     notes="m = 0.1 + 0.05*2.8 = 0.1 + 0.14 = 0.24; |3.0 - 2.8| = 0.2 <= 0.24: holds.")
case("value_str_lt_error", ["de"], "de", [], a_value([S("TP53")], "tissue", "lt", S("m")), "error",
     error=("E6004", "/op"),
     notes="lt on a str expected is 'an operator that the type of expected does not allow': E6004 at /op.")
case("value_str_tol_error", ["de"], "de", [],
     a_value([S("TP53")], "tissue", "eq", S("liver"), tol("0", "0")), "error", error=("E6004", "/tol"),
     notes="tol on a string comparison is E6004 at /tol even when both parts are 0 (the member is present).")
case("value_bool_ne", ["de"], "de", [derive("liv", 'tissue = "liver"')],
     a_value([S("BRCA1")], "liv", "ne", B(True)), "holds",
     [(5, 'Add column liv = tissue = "liver"; 5 rows.')],
     DE_COLS + ["liv"], de_add("liv", [B(False), B(False), B(False), B(True), B(True)]), ["gene_id"],
     notes="ne on booleans is allowed: BRCA1.liv false != true: holds.")
case("value_absent_after_filter", ["de"], "de", FC_GT1, a_value([S("KRAS")], "n", "eq", I(2)), "fails",
     FC_GT1_STEP, DE_COLS, FC_GT1_ROWS, ["gene_id"],
     notes="KRAS is dropped by the filter; an absent row is fails, not an error.")

case("compare_rows_gt", ["de"], "de", [], a_cmp([S("TP53")], "fc", [S("MDM2")], "fc", "gt"), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="2.5 > -1.5: holds.")
case("compare_rows_le_cross_columns", ["de"], "de", [],
     a_cmp([S("BRCA1")], "fc", [S("BRCA1")], "n", "le"), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"],
     notes="Different columns of the same row, dec against int: 0.5 <= 3: holds.")
case("compare_rows_ne_tol", ["de"], "de", [],
     a_cmp([S("EGFR")], "fc", [S("TP53")], "fc", "ne", tol("0", "0.2")), "fails",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"],
     notes="m is taken from the right cell: 0.2*2.5 = 0.5. ne needs |a - b| > m; |3.0 - 2.5| = 0.5 is "
           "not > 0.5: fails (the edge belongs to eq, not ne).")
case("compare_rows_absent_left", ["de"], "de", [],
     a_cmp([S("ZZZ3")], "fc", [S("TP53")], "fc", "lt"), "fails",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="ZZZ3 is not in de: either absent is fails.")
case("compare_rows_lt_fails", ["de"], "de", [],
     a_cmp([S("TP53")], "fc", [S("MDM2")], "fc", "lt"), "fails",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="2.5 < -1.5 is false: fails.")
case("compare_rows_ge_eq", ["de"], "de", [],
     a_cmp([S("BRCA1")], "p", [S("BRCA1")], "p", "ge"), "holds",
     [], DE_COLS, rows("de", DE_ALL), ["gene_id"], notes="0.200 >= 0.200: holds.")

case("exists_after_join_inner", ["de", "ann"], "de", [join("ann", [("gene_id", "gene_id")], "inner")],
     a_exists([S("EGFR")]), "fails",
     [(3, "Inner join with ann on gene_id = gene_id; 3 rows.")],
     JOIN_ANN_COLS,
     joined("de", ["BRCA1", "KRAS", "TP53"], "ann", {"BRCA1": "BRCA1", "KRAS": "KRAS", "TP53": "TP53"}, "inner"),
     ["gene_id"], notes="EGFR has no ann row and the join is inner: absent, fails.")
case("exists_group_key", ["de"], "de", [group(["tissue"], [("cnt", "count", "n")])],
     a_exists([S("lung")]), "holds",
     [(3, "Group by tissue and compute cnt = count(n); 3 groups from 5 rows.")],
     gcols(["tissue"], ["cnt"]),
     de_groups({"breast": {"cnt": I(2)}, "liver": {"cnt": I(2)}, "lung": {"cnt": I(1)}}), ["tissue"],
     notes="After group the key is the by tuple, so exists takes [lung]: present, holds.")

case("rank_assert_tol", ["de"], "de", [sort([("fc", "asc")], "last")],
     a_rank([S("TP53")], 4, tol("1", "0")), "holds",
     [(5, "Sort by fc ascending (missing values placed last; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["MDM2", "BRCA1", "TP53", "EGFR", "KRAS"]), ["gene_id"],
     notes="TP53 is 3rd; |3 - 4| = 1 <= abs 1: holds.")
case("rank_assert_fails", ["de"], "de", [sort([("fc", "asc")], "last")],
     a_rank([S("TP53")], 1), "fails",
     [(5, "Sort by fc ascending (missing values placed last; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["MDM2", "BRCA1", "TP53", "EGFR", "KRAS"]), ["gene_id"],
     notes="TP53 is 3rd, expected 1, m = 0: fails.")
case("rank_assert_missing_row_first", ["de"], "de", [sort([("fc", "desc")], "first")],
     a_rank([S("KRAS")], 1), "holds",
     [(5, "Sort by fc descending (missing values placed first; ties broken by gene_id); 5 rows.")],
     DE_COLS, rows("de", ["KRAS", "EGFR", "TP53", "BRCA1", "MDM2"]), ["gene_id"],
     notes="The rank assertion is a position; the missing-fc row KRAS sorted first is at 1: holds.")


# -- exponent input (revised 11.6, 11.7, 11.18.1) ---------------------------------------------------
GW_COLS = cols_of("gw")

case("exp_literal_filter_lt", ["gw"], "gw", [filt("padj < 1e-10")],
     a_exists([S("b")]), "holds",
     [(2, "Keep rows where padj < 1e-10 (missing values excluded); 2 of 5 rows remain.")],
     GW_COLS, rows("gw", ["a", "b"]), ["gene_id"],
     notes="1e-10 is an EXPNUM literal, a dec: positionally 0.0000000001 (scale 10). padj is a scale-12 "
           "column, each cell written at scale 12: a 0.000000000001, b 0.000000000050 (CSV "
           "0.00000000005), c 0.000000000200 (CSV 0.0000000002), d 0.030000000000 (CSV 0.03), e missing. "
           "a 1e-12 < 1e-10 true; b 5e-11 < 1e-10 true; c 2e-10 false; d false; e missing, dropped. "
           "The description keeps the literal as written, '1e-10'. exists b: holds.")

case("exp_literal_filter_upper_e", ["gw"], "gw", [filt("padj<=2.0E-10")],
     a_exists([S("c")]), "holds",
     [(3, "Keep rows where padj <= 2.0E-10 (missing values excluded); 3 of 5 rows remain.")],
     GW_COLS, rows("gw", ["a", "b", "c"]), ["gene_id"],
     notes="Capital E and the mantissa's trailing zero are part of the literal as written, so {expr} "
           "keeps '2.0E-10' while adding the spaces around '<='. 2.0E-10 = 0.00000000020 (scale 11). "
           "a, b below; c 0.000000000200 equals it, so <= holds; d 0.03 false; e missing. 3 of 5. "
           "exists c: holds.")

case("derive_exp_literal_unary_minus", ["de"], "de", [derive("adj", "fc*-2.5e-1")],
     a_value([S("MDM2")], "adj", "eq", D("0.375")), "holds",
     [(5, "Add column adj = fc * -2.5e-1; 5 rows.")],
     DE_COLS + ["adj"], de_add("adj", [D("-0.125"), D("-0.750"), M, D("0.375"), D("-0.625")]), ["gene_id"],
     notes="-2.5e-1 is unary minus applied to the literal 2.5e-1; the minus inside the exponent belongs "
           "to the literal. 2.5e-1 = 0.25 (scale 2), negated -0.25. Rendered with no space after the "
           "unary minus and the literal as written: 'fc * -2.5e-1'. Products take scale 1 + 2 = 3: "
           "0.5*-0.25 = -0.125; 3.0*-0.25 = -0.750; KRAS missing; -1.5*-0.25 = 0.375; 2.5*-0.25 = "
           "-0.625. MDM2.adj = 0.375: holds.")

case("derive_exp_literal_binary_minus", ["de"], "de", [derive("adj", "fc -1e-1")],
     a_value([S("TP53")], "adj", "eq", D("2.4")), "holds",
     [(5, "Add column adj = fc - 1e-1; 5 rows.")],
     DE_COLS + ["adj"], de_add("adj", [D("0.4"), D("2.9"), M, D("-1.6"), D("2.4")]), ["gene_id"],
     notes="'a -1e-10 is a - 1e-10': the minus after a column is binary. 1e-1 = 0.1 (scale 1). "
           "Subtraction takes the larger scale, 1: 0.5-0.1 = 0.4; 3.0-0.1 = 2.9; missing; -1.5-0.1 = "
           "-1.6; 2.5-0.1 = 2.4. Rendered 'fc - 1e-1'. TP53.adj = 2.4: holds.")

case("exp_cells_dec_column", ["tiny"], "tiny", [filt("q < 0.001")],
     a_value([S("a")], "q", "eq", D("0.0000000000000032")), "holds",
     [(2, "Keep rows where q < 0.001 (missing values excluded); 2 of 5 rows remain.")],
     cols_of("tiny"), rows("tiny", ["a", "b"]), ["id"],
     notes="Cells written with exponents in a dec column are converted exactly to positional notation: "
           "3.2e-15 = 0.0000000000000032 (scale 16), 1.0e-12 = 0.0000000000010 (scale 13), 4.5E-3 = 0.0045 "
           "(scale 4), 0.02 (scale 2). The column scale is 16, the largest seen; every value is written "
           "at scale 16: a 0.0000000000000032, b 0.0000000000010000, c 0.0045000000000000, e "
           "0.0200000000000000. q < 0.001: a, b true; c 0.0045 false; d missing; e 0.02 false. 2 of 5. "
           "Typed values stay positional, so expected is written 0.0000000000000032: holds.")

case("exp_scale3_trailing_zero", ["s3"], "s3", [filt("v = 0.001")],
     a_cmp([S("a")], "v", [S("b")], "v", "eq"), "holds",
     [(3, "Keep rows where v = 0.001 (missing values excluded); 3 of 4 rows remain.")],
     cols_of("s3"), rows("s3", ["a", "b", "c"]), ["id"],
     notes="Scale-3 column. 1.0e-3 is 0.0010 positionally (scale 4) and 0.0010 is scale 4 as written; "
           "writing either at scale 3 gives 0.001 without changing its value (only trailing zeros go), "
           "so neither is E3102. 0.001 stays 0.001; 0.25 becomes 0.250. v = 0.001: a, b, c true; d "
           "false. 3 of 4. compare_rows a.v 0.001 = b.v 0.001: holds. At table.spec this file would show scale 4 "
           "(trailing zeros count), so the case reads the file as a later version under a scale-3 spec, "
           "where 'a later version's trailing zeros still fit'.")

case("derive_overflow_emax", ["one"], "one", [derive("y", "x * 9.9e6144 * 10")],
     a_count("eq", I(1)), "error", error=("E5002", "/ops/0/expr"),
     notes="Re-derived for the decimal128 context (was 9.9e999999 under Emax 999999). Context: precision "
           "34, Emax 6144. 9.9e6144 = 99E6143 has adjusted exponent 6143 + 2 - 1 = 6144, not above Emax, so "
           "the literal is accepted (11.18.1 names 9.9e6144 as accepted). It is made positional exactly, "
           "outside the context: 99 followed by 6143 zeros, 6,145 digits, scale 0 (11.18.1: quantizing in "
           "the precision-34 context would fail for this literal and must not be done). x = int 1. "
           "Left-associative: (x * 9.9e6144) * 10; int * dec is dec. 1 * 9.9e6144: the exact product has "
           "6,145 digits, all but the leading 99 zeros, so rounding to 34 significant digits drops only "
           "zeros: 9.900000000000000000000000000000000E6144 (coefficient 99 followed by 32 zeros, exponent "
           "6111), adjusted exponent 6144, not above Emax: no overflow. * 10: exact 9.9E6145, adjusted "
           "exponent 6145 > Emax 6144: the General Decimal Arithmetic Overflow condition, verdict error with "
           "E5002 (11.18.2 Domain). Row-level fields null. E5002 points at the member holding the expression, "
           "/ops/0/expr (11.18.2 Types). Cross-checked with Python decimal in the stated context.")

case("exp_literal_out_of_range", ["one"], "one", [filt("x < 1e1000000")],
     a_count("eq", I(1)), "error", error=("E6004", "/ops/0/where"),
     notes="1e1000000 has adjusted exponent 1000000, above Emax 6144: E6004 at validation, pointing "
           "at the where member. 11.18.1 states the rule; the refused literal it names is 1e6145 "
           "(adjusted 6145), and 1e1000000 is far above that.")


# -- grouping values, equal min/max, missing key components, float64 overflow (design review) ----------
case("group_missing_values_one_group", ["gm"], "gm", [group(["g"], [("s", "sum", "v")])],
     a_value([D("1.5")], "s", "eq", I(60)), "holds",
     [(2, "Group by g and compute s = sum(v); 2 groups from 6 rows.")],
     gcols(["g"], ["s"]),
     [grp_rows({"g": M}, {"s": I(150)}, 4, [[S("b")], [S("c")], [S("d")], [S("f")]]),
      grp_rows({"g": D("1.5")}, {"s": I(60)}, 2, [[S("a")], [S("e")]])], ["g"],
     notes="g is float64 with nan and inf mapped to values: a 1.5, b missing, c nan, d +inf, e 1.5, f -inf. "
           "'Every missing, nan, or inf value in a by component falls in one group ... and that group's value "
           "in the output is missing, whatever its members held'; 'Missing groups sort first'. Group 1: g "
           "missing, members b, c, d, f: sum 20+30+40+60 = 150, member_count 4. Group 2: g 1.5, members a, e: "
           "sum 10+50 = 60 (int, exact), member_count 2. The key map is [[missing], [1.5]]. The assertion "
           "locates the 1.5 group by value (a row holding missing is avoided, AMBIGUITIES.md 42): 60 = 60: holds.")

case("group_min_max_equal_values", ["mm"], "mm",
     [derive("m", "coalesce(x, n)"), group(["grp"], [("lo", "min", "m"), ("hi", "max", "m")])],
     a_cmp([S("g")], "lo", [S("h")], "lo", "eq"), "holds",
     [(5, "Add column m = coalesce(x, n); 5 rows."),
      (2, "Group by grp and compute lo = min(m) and hi = max(m); 2 groups from 5 rows.")],
     gcols(["grp"], ["lo", "hi"]),
     [grp_rows({"grp": S("g")}, {"lo": D("2.0"), "hi": D("3.5")}, 3, [[S("a")], [S("b")], [S("c")]]),
      grp_rows({"grp": S("h")}, {"lo": D("2"), "hi": D("2")}, 2, [[S("d")], [S("e")]])], ["grp"],
     notes="A dec column is written at one scale, so 2 and 2.0 can only differ in a derived column. "
           "m = coalesce(x, n): a 2.0 (x), b 2 (n widened, scale 0, written '2'), c 3.5, d 2, e 2.0. Input "
           "order to group is a, b, c, d, e. Group g {a 2.0, b 2, c 3.5}: the minimum value 2 is held by a and "
           "b, equal under =; the first in input order is a, so lo is '2.0'; hi 3.5. Group h {d 2, e 2.0}: "
           "both are minimum and maximum; the first is d, so lo and hi are '2'. compare_rows g.lo 2.0 = "
           "h.lo 2 as decimals: holds; the written forms differ in final_rows.")

case("sort_missing_key_component_first", ["pk"], "pk",
     [group(["a", "b"], [("k", "count", "v")]), sort([("k", "desc")], "last")],
     a_rank([S("x"), S("q")], 3), "holds",
     [(4, "Group by a and b and compute k = count(v); 4 groups from 5 rows."),
      (4, "Sort by k descending (missing values placed last; ties broken by a and b); 4 rows.")],
     gcols(["a", "b"], ["k"]),
     [grp_rows({"a": S("x"), "b": S("p")}, {"k": I(2)}, 2, [[S("r2")], [S("r3")]]),
      grp_rows({"a": S("x"), "b": M}, {"k": I(1)}, 1, [[S("r1")]]),
      grp_rows({"a": S("x"), "b": S("q")}, {"k": I(1)}, 1, [[S("r4")]]),
      grp_rows({"a": S("y"), "b": S("p")}, {"k": I(1)}, 1, [[S("r5")]])], ["a", "b"],
     notes="A key holds missing only after a group on a column with missing values. Group by (a, b): "
           "(x, missing) {r1}, (x, p) {r2, r3}, (x, q) {r4}, (y, p) {r5}, in that order (missing component "
           "first). k = count(v): 1, 2, 1, 1. The key is now (a, b). Sort by k descending: (x, p) k 2 first; "
           "(x, missing), (x, q), (y, p) tie at 1 and are broken by the key ascending, 'a missing key "
           "component sorting first': (x, missing) before (x, q) since a ties at x. 'missing values placed "
           "last' concerns k, which has none. Final order (x,p), (x,missing), (x,q), (y,p). (x, q) is at "
           "position 3; it would be 2 if the missing component sorted last: holds.")

case("float64_overflow_cell", ["ov"], "ov", [],
     a_count("eq", I(2)), "error", error=("E3102", None),
     notes="In a float64 column 'any other text is read as an IEEE double ... a finite number whose double "
           "overflows (1e400) being E3102, so only the declared infinity tokens produce infinity'. 1e400 "
           "exceeds the largest double (about 1.8e308): E3102, verdict error, row-level fields null. "
           "checker.md gives no field for E3102 on a cell; a CSV cell is not a JSON member, so field is null "
           "(errors.md allows null; AMBIGUITIES.md 41).")


case("assert_names_missing_group", ["sc2"], "sc2", [group(["score"], [("n", "count", "id")])],
     a_value([M], "n", "eq", I(2)), "holds",
     [(4, "Group by score and compute n = count(id); 4 groups from 6 rows.")],
     gcols(["score"], ["n"]),
     [grp_rows({"score": M}, {"n": I(2)}, 2, [[S("d")], [S("f")]]),
      grp_rows({"score": I(5)}, {"n": I(1)}, 1, [[S("e")]]),
      grp_rows({"score": I(10)}, {"n": I(1)}, 1, [[S("a")]]),
      grp_rows({"score": I(20)}, {"n": I(2)}, 2, [[S("b")], [S("c")]])], ["score"],
     notes="sc2 scores: a 10, b 20, c 20, d missing, e 5, f missing. Group by score: the two missing values "
           "fall in one group (11.18.2 group), sorted first, by value missing; then 5 {e}, 10 {a}, 20 {b, c}. "
           "n = count(id): 2, 1, 1, 2; member_count the same. The output key is (score), so the final key of "
           "the first group is [missing]. 11.9 lets a row component be missing 'to name a missing key "
           "component', and 11.18.3 'Locating a row' matches it by grouping identity, not by =. The row "
           "[missing] locates the first group; its n is 2 = 2: holds.")


def ranked_group(by_values, aggs, member_count, member_tuples, rank_value):
    row = grp_rows(by_values, aggs, member_count, member_tuples)
    row["rank"] = I(rank_value)
    return row


case("rank_group_by_missing_partition", ["gp"], "gp",
     [group(["g", "t"], [("s", "sum", "v")]),
      rank("s", "desc", "min", "last", "group", ["g"])],
     a_rank([M, S("p")], 3), "holds",
     [(4, "Group by g and t and compute s = sum(v); 4 groups from 6 rows."),
      (4, "Rank by s descending within each g (ties: min; missing values placed last); 4 rows.")],
     gcols(["g", "t"], ["s"]) + ["rank"],
     [ranked_group({"g": M, "t": S("q")}, {"s": I(90)}, 2, [[S("c")], [S("f")]], 1),
      ranked_group({"g": D("1.5"), "t": S("q")}, {"s": I(50)}, 1, [[S("e")]], 1),
      ranked_group({"g": M, "t": S("p")}, {"s": I(60)}, 2, [[S("b")], [S("d")]], 2),
      ranked_group({"g": D("1.5"), "t": S("p")}, {"s": I(10)}, 1, [[S("a")]], 2)], ["g", "t"],
     notes="g is float64 with nan and inf as values: a 1.5, b missing, c nan, d +inf, e 1.5, f -inf; t is p or "
           "q. Group by (g, t): every missing, nan, or inf g falls in one group per t, with g written missing: "
           "(missing, p) {b, d} s 60; (missing, q) {c, f} s 90; (1.5, p) {a} s 10; (1.5, q) {e} s 50. Missing "
           "groups sort first: that is the group order. The key is now (g, t). Rank by s descending within "
           "each g: the two missing-keyed groups form one partition (11.18.2 rank: every missing value in a "
           "group_by component is one partition): (missing, q) 90 -> 1, (missing, p) 60 -> 2. Partition 1.5: "
           "(1.5, q) 50 -> 1, (1.5, p) 10 -> 2. Table order is rank ascending then the key, 'a missing key "
           "component sorting first': rank 1: (missing, q) before (1.5, q); rank 2: (missing, p) before "
           "(1.5, p). Final order (missing,q), (1.5,q), (missing,p), (1.5,p). The row [missing, p] is located by "
           "grouping identity (11.18.3) and is at position 3; it would be 4 if the missing component sorted "
           "last: holds. The rank column follows member_keys_hash as the last column.")


# -- underflow below Emin, int beyond a double in the log family (11.18.2 Precision, Domain) -----------
table(
    "uf",
    "id,x\na,1.234567\n",
    spec([col("id", "str"), col("x", "dec", 6)], ["id"]),
    ["id", "x"], ["id"],
    {"a": [S("a"), D("1.234567")]},
)

# Etiny = Emin - (precision - 1) = -6143 - 33 = -6176; both values sit at that exponent.
UF_SUBNORMAL = "0." + "0" * 6172 + "1235"   # 1235E-6176 = 1.235E-6173
UF_ZERO = "0." + "0" * 6176                 # 0E-6176, scale kept

case("derive_underflow_subnormal_then_zero", ["uf"], "uf",
     [derive("y", "x * 1e-6143 * 1e-30"), derive("z", "y * 1e-5")],
     a_value([S("a")], "z", "eq", I(0)), "holds",
     [(1, "Add column y = x * 1e-6143 * 1e-30; 1 rows."),
      (1, "Add column z = y * 1e-5; 1 rows.")],
     cols_of("uf", add=["y", "z"]),
     rows("uf", ["a"], add={"a": {"y": D(UF_SUBNORMAL), "z": D(UF_ZERO)}}), ["id"],
     notes="Re-derived for the decimal128 context (was 1e-999999 under Emin -999999). Context: precision 34, "
           "Emin -6143, so Etiny = Emin - (34 - 1) = -6176, the smallest exponent a subnormal may have. "
           "Literals: 1e-6143 has adjusted exponent -6143, not below Emin, so it is accepted (11.18.1 names "
           "1e-6143 as accepted); 1e-30 and 1e-5 are ordinary. Each is made positional exactly first (scales "
           "6143, 30, 5). Step 1, left-associative (x * 1e-6143) * 1e-30. x = 1.234567 = 1234567E-6. "
           "1234567E-6 * 1E-6143 = 1234567E-6149, adjusted exponent -6149 + 6 = -6143 = Emin: normal, "
           "7 digits, exact. * 1E-30 = 1234567E-6179 exactly, adjusted -6179 + 6 = -6173 < Emin: subnormal. "
           "Its exponent -6179 is below Etiny, so General Decimal Arithmetic rounds the coefficient to exponent "
           "-6176: 1234567 / 10^3 = 1234.567, half-even -> 1235 (0.567 > 0.5). y = 1235E-6176 = 1.235E-6173 "
           "(Subnormal, Underflow, Inexact, Rounded: none is an error; 'Underflow below Emin -6143 gives a "
           "subnormal result, then zero, and is not an error'). Only 4 significant digits survive; an unbounded "
           "exponent would give 1.234567E-6173 and flushing would give 0. 11.18.2 Precision: 'An underflowed "
           "result keeps the exponent General Decimal Arithmetic gives it, so its scale is the negative of that "
           "exponent'. Written positionally at scale 6176: '0.' then 6172 zeros then 1235 (6,178 characters). "
           "Step 2: y * 1E-5 = 1235E-6181 exactly, adjusted -6178; rounded to exponent Etiny -6176: "
           "1235 / 10^5 = 0.01235 -> 0. z = 0E-6176 (Clamped as well), positive. 'a result that underflows to "
           "zero is written at that scale': '0.' then 6176 zeros (6,178 characters). The multiplication "
           "sum-of-scales rule would give scales 6179 and 6181; the subnormal rounding reduces both to 6176, as "
           "rounding at 34 digits does in derive_rounds_at_34_digits. No E5002: no adjusted exponent exceeds "
           "Emax. a.z = 0 against int 0 as decimals, |0 - 0| = 0 <= 0: holds. Cross-checked with Python "
           "decimal in the stated context.")

# -- clamp 0: an exponent above Emax - 33 shows in a later scale (11.18.2 Precision, 2026-11-24) ------
CLAMP_EXPR = ("(1.000000000000000000000000000000001e6133 * 1 - 1e6133 * 1) * "
              "(1.000000000000000000000000000000001e53 * 1 - 1e53 * 1) * 1e-6130")

case("clamp_zero_exponent_shows_in_scale", ["one"], "one", [derive("y", CLAMP_EXPR)],
     a_value([S("a")], "y", "eq", D("0.0000000001")), "holds",
     [(1, "Add column y = " + CLAMP_EXPR + "; 1 rows.")],
     cols_of("one", add=["y"]),
     rows("one", ["a"], add={"a": {"y": D("0.0000000001")}}), ["id"],
     notes="The example of the 11.18.2 Precision row, which states clamp 0: 'a result keeps any exponent up "
           "to Emax, never padded down to Emax - 33 = 6111'. Context: precision 34, Emax 6144, Emin -6143, "
           "half-even, clamp 0. Literals (11.18.1), all in range: A = 1.000000000000000000000000000000001e6133 "
           "(34 significant digits, adjusted 6133), 1e6133 (adjusted 6133), B = "
           "1.000000000000000000000000000000001e53 (adjusted 53), 1e53, 1e-6130 (adjusted -6130). Each is made "
           "positional exactly first: A is an integer of 6,134 digits, (10^33 + 1) * 10^6100, scale 0; 1e6133 "
           "is 10^6133, scale 0; B is (10^33 + 1) * 10^20, 54 digits, scale 0; 1e53 scale 0; 1e-6130 is "
           "1E-6130, scale 6130. The literal 1 is int; int * dec is dec, so every product applies the context. "
           "First factor: A * 1 exact is A with exponent 0 and 6,134 digits; rounding to 34 significant digits "
           "drops only zeros: coefficient 10^33 + 1, exponent 6100 (Rounded, not Inexact). 1e6133 * 1 likewise "
           "gives coefficient 10^33, exponent 6100. Their difference is exact: 1E6100 (coefficient 1, exponent "
           "6100, the smaller of the two exponents). Second factor: B * 1 rounds to coefficient 10^33 + 1, "
           "exponent 20; 1e53 * 1 to 10^33, exponent 20; difference 1E20. Left-associative: (1E6100 * 1E20) * "
           "1E-6130. 1E6100 * 1E20 = 1E6120: one digit, adjusted 6120, not above Emax, so no Overflow; its "
           "exponent 6120 is above Emax - 33 = 6111, and under clamp 0 it is kept, so the value stays 1E6120. "
           "* 1E-6130 = 1E-10, exact, adjusted -10, normal. Scale is the negative of the exponent: 10. Written "
           "'0.0000000001' (0. then 9 zeros then 1). Under clamp 1, 1E6120 would be padded to 1000000000E6111 "
           "and the final product would be 1000000000E-19, written '0.0000000001000000000' (scale 19): the "
           "same number with a different scale, so final_rows_hash differs between the readings. "
           "The assertion cannot tell the two apart: 11.9 compares numbers as decimals, and |a - b| = 0 under "
           "either reading. The written form is pinned by final_rows and final_rows_hash. a.y eq "
           "0.0000000001: holds. Description: both subtractions bind looser than *, so their parentheses are "
           "required; the outer (P * Q) * 1e-6130 is a left operand of equal precedence and drops its "
           "parentheses; literals as written, so the sentence repeats the expression. Cross-checked with "
           "Python decimal in the stated context, clamp 0 and clamp 1.")

table(
    "hx",
    "id,x\na,1\nb,1" + "0" * 37 + "\nc,1" + "0" * 34 + "\n",
    spec([col("id", "str"), col("x", "int")], ["id"]),
    ["id", "x"], ["id"],
    {
        "a": [S("a"), I(1)],
        "b": [S("b"), I(10 ** 37)],
        "c": [S("c"), I(10 ** 34)],
    },
)

case("log_of_int_beyond_double", ["hx"], "hx",
     [derive("p", "x * x * x * x * x * x * x * x * x"), derive("l", "log(p)"), filt("l >= 0")],
     a_exists([S("b")]), "fails",
     [(3, "Add column p = x * x * x * x * x * x * x * x * x; 3 rows."),
      (3, "Add column l = log(p); 3 rows."),
      (2, "Keep rows where l >= 0 (missing values excluded); 2 of 3 rows remain.")],
     cols_of("hx", add=["p", "l"]),
     rows("hx", ["a", "c"], add={"a": {"p": I(1), "l": D("0")},
                                 "c": {"p": I(10 ** 306), "l": D("704.591038456178")}}), ["id"],
     notes="Revised 11.7: an int cell is at most 38 digits, so the huge int is computed. int column x: a 1, "
           "b 10^37 (38 digits, the widest a cell may be), c 10^34 (35 digits); all three parse. Step 1: "
           "p = x^9 as nine factors, int * int giving int, exact and unbounded (11.18.2 Precision): a 1; b "
           "10^333 (334 digits); c 10^306 (307 digits). Step 2, 11.18.2 Domain: 'An int too large for a double "
           "(only a computed int can be ...) is infinity to the log family, so its logarithm is missing.' The "
           "largest double is about 1.797e308. a: log(1.0) = 0.0, an integral double, written '0' (11.6). b: "
           "10^333 exceeds it, so it is infinity and log gives missing, not an error and not a value. c: "
           "10^306 is below it and converts to the nearest double; ln(10^306) = 306 * ln 10 = 306 * "
           "2.302585092994045684 = 690.7755278982137052 + 13.815510557964274104 = 704.591038456177979...; to "
           "15 significant digits 704.591038456178 (the dropped digits 979... lie about 4 units of the "
           "double's last place from the rounding boundary, so libm last-bit differences cannot move it). "
           "Step 3, filter l >= 0: a 0 >= 0 true; b missing, dropped; c true. 2 of 3. p stays in the final "
           "rows as an int. exists b: the row whose logarithm is missing is absent: fails. An implementation "
           "that took a logarithm of 10^333 (766.76...) would keep b and hold; one that raised on the "
           "conversion would give error.")

table(
    "w39",
    "id,x\na,1\nb,1" + "0" * 38 + "\n",
    spec([col("id", "str"), col("x", "int")], ["id"]),
    ["id", "x"], ["id"],
    {"a": [S("a"), I(1)], "b": [S("b"), I(10 ** 38)]},
)

case("int_cell_wider_than_38_digits", ["w39"], "w39", [],
     a_count("eq", I(2)), "error", error=("E3102", None),
     notes="11.7: 'An int cell wider than 38 digits is E3102 (item 47 maps int to Decimal128(38,0))'. Row b holds "
           "10^38, written as 1 followed by 38 zeros: 39 digits, one more than 38 (the largest Decimal128(38,0) "
           "value is 10^38 - 1, 38 nines). The cell fails at parse: E3102, and 'An E3102 about a data file cell "
           "has field null'. Item 92: the harness parses every table before the steps and the assertion, so this "
           "is the case's error whatever they hold; verdict error, row-level fields null. A 38-digit cell parses "
           "(log_of_int_beyond_double holds 10^37).")

table(
    "sm",
    "id,x\na,9\nb,10\n",
    spec([col("id", "str"), col("x", "int")], ["id"]),
    ["id", "x"], ["id"],
    {"a": [S("a"), I(9)], "b": [S("b"), I(10)]},
)

INT_L = "1" + "0" * 3072   # 10^3072: 3,073 digits, adjusted exponent 3072

case("derive_int_overflow_past_emax", ["sm"], "sm", [derive("y", f"x * {INT_L} * {INT_L}")],
     a_count("eq", I(2)), "error", error=("E5002", "/ops/0/expr"),
     notes="11.6 int row and 11.18.2 Precision: 'int results are exact and never round; an int result whose "
           "adjusted exponent exceeds Emax 6144 (more than 6,145 digits) is Overflow, E5002, at the member "
           "holding the expression'. For a positive integer of d digits the adjusted exponent is d - 1, so "
           "d - 1 > 6144 is d > 6145: 6,145 digits are allowed and 6,146 are not. int column x with small "
           "cells: a 9, b 10. The literal L is 1 followed by 3,072 zeros, 10^3072: an INT literal (11.18.1), "
           "typed int, 3,073 digits, adjusted exponent 3072, in range, so it is not refused and is not near "
           "the literal bound; it is written twice so that no single literal has 6,145 digits. Left-associative: "
           "(x * L) * L; int * int gives int, exact up to the bound (11.18.2 Types). a: 9 * 10^3072 = 9 "
           "followed by 3,072 zeros (3,073 digits); * 10^3072 = 9 * 10^6144 = 9 followed by 6,144 zeros, "
           "6,145 digits, adjusted exponent 6144, not above Emax: exact, at the bound, no error. b: 10 * "
           "10^3072 = 10^3073, 1 followed by 3,073 zeros (3,074 digits, a computed int wider than a 38-digit "
           "cell, which 11.7 allows because it never leaves Rust or Python as Arrow); * 10^3072 = 10^6145 = 1 "
           "followed by 6,145 zeros, 6,146 digits, adjusted exponent 6145 > 6144: Overflow, verdict error with "
           "E5002, the same condition a dec result raises (11.18.2 Domain). Neither implementation's integers "
           "have this bound, so both check it explicitly (Precision row); an implementation that left int "
           "arithmetic unbounded would give y for both rows and count 2 = 2 holds. E5002 points at the member "
           "holding the expression, /ops/0/expr (11.18.2 Types). Row-level fields null (item 92). Digit counts "
           "cross-checked with Python (len of the decimal string) and the Overflow with Python decimal in the "
           "stated context.")


# ---- eval expectations (checker/assertions/eval/<name>/expected.json) ---------------------------

EVAL = {
    "absent_row": ("fails", None, "cell null: the row is absent; absent row is fails."),
    "between_reversed": ("fails", None, "20 <= 15 is false; bounds are not reordered, m = 0: fails."),
    "between_widened": ("holds", None, "lo - 0.1*10 = 9 <= 21.9 <= hi + 0.1*20 = 22: holds."),
    "compare_rows_missing_right": ("fails", None, "right cell missing: fails."),
    "compare_rows_rel": ("holds", None, "b is the right cell 10; m = 0.1*10 = 1; |10.9 - 10| = 0.9 <= 1: holds."),
    "compare_rows_str_tol": ("error", ("E6004", "/tol"), "tol on string cells is E6004 at /tol, whatever its value."),
    "count_ge": ("holds", None, "10 >= 10: holds."),
    "date_between": ("holds", None, "2026-01-01 <= 2026-12-31 <= 2026-12-31 inclusive: holds."),
    "date_vs_datetime": ("error", ("E6004", "/expected"), "a datetime cell against a date expected do not compare: E6004 at /expected."),
    "eq_abs_tol_edge": ("holds", None, "|1.1 - 1.0| = 0.1 <= 0.1 + 0: holds."),
    "eq_exact_fails": ("fails", None, "|0.0032 - 0.0031| = 0.0001 > 0: fails."),
    "eq_exact_holds": ("holds", None, "0.00310 - 0.0031 = 0 <= 0: holds."),
    "eq_rel_tol": ("holds", None, "m = 0.01*200 = 2; |201.9 - 200| = 1.9 <= 2: holds."),
    "exists_absent": ("fails", None, "present false: fails."),
    "exists_present": ("holds", None, "present true: holds."),
    "ge_fails": ("fails", None, "b - m = 1.0 - 0.1 = 0.9; 0.85 >= 0.9 is false: fails."),
    "gt_tol_loosens": ("holds", None, "0.95 > 1.0 - 0.1 = 0.9: holds."),
    "inf_cell": ("fails", None, "an inf cell never satisfies any comparison: fails."),
    "int_vs_dec": ("holds", None, "int 3 and dec 3.00 compare as decimals: equal: holds."),
    "le_edge": ("holds", None, "11 <= 10 + 1: holds."),
    "lt_tol_loosens": ("holds", None, "1.05 < 1.0 + 0.1 = 1.1: holds."),
    "missing_cell": ("fails", None, "a missing cell yields fails, not not checkable."),
    "nan_cell": ("fails", None, "a nan cell yields fails."),
    "ne_with_tol_inside": ("fails", None, "ne needs |a - b| > m: |1.05 - 1.0| = 0.05, not > 0.1: fails."),
    "rank_absent": ("fails", None, "position null: absent row is fails."),
    "rank_position": ("holds", None, "position 2 compared as eq with 3 under abs 1: |2 - 3| = 1 <= 1: holds."),
    "str_eq_nfc": ("holds", None, "identical NFC bytes: holds."),
    "str_vs_number": ("error", ("E6004", "/expected"), "a str cell whose type does not compare with a dec expected: E6004 at /expected."),
}


# ---- writing ----------------------------------------------------------------------------------------

def dump(path, value):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2, ensure_ascii=False)
        f.write("\n")


def resolve(v):
    if isinstance(v, MKH):
        return v.resolve()
    if isinstance(v, dict):
        return {k: resolve(x) for k, x in v.items()}
    if isinstance(v, list):
        return [resolve(x) for x in v]
    return v


def error_obj(err):
    return {"code": err[0], "field": err[1]}


def main():
    names = set()
    notes_out = ["# Arithmetic behind checker/cases\n",
                 "Written by tools/write_cases.py from the values worked by hand; one entry per case.\n"]
    for c in CASES:
        assert c["name"] not in names, c["name"]
        names.add(c["name"])
        d = os.path.join(CASES_DIR, c["name"])
        os.makedirs(os.path.join(d, "tables"), exist_ok=True)
        os.makedirs(os.path.join(d, "specs"), exist_ok=True)
        for t in c["tables"]:
            with open(os.path.join(d, "tables", t + ".csv"), "w", encoding="utf-8", newline="") as f:
                f.write(TABLES[t]["csv"])
            dump(os.path.join(d, "specs", t + ".json"), TABLES[t]["spec"])
        dump(os.path.join(d, "steps.json"), c["steps"])
        dump(os.path.join(d, "assertion.json"), c["assertion"])
        if c["error"] is not None:
            assert c["verdict"] == "error"
            exp = {"verdict": "error", "steps": None, "columns": None, "final_rows": None,
                   "final_rows_hash": None, "key_map_hash": None, "error": error_obj(c["error"])}
        else:
            assert len(c["steps_exp"]) == len(c["steps"]["ops"]), c["name"]
            final_rows = resolve(c["final_rows"])
            for r in final_rows:
                assert list(r.keys()) == c["columns"], (c["name"], list(r.keys()), c["columns"])
            if c["steps_exp"]:
                assert c["steps_exp"][-1][0] == len(final_rows), c["name"]
            key_map = [[r[k] for k in c["key_cols"]] for r in final_rows]
            exp = {
                "verdict": c["verdict"],
                "steps": [{"rows": n, "description": s} for n, s in c["steps_exp"]],
                "columns": c["columns"],
                "final_rows": final_rows,
                "final_rows_hash": id_of(final_rows),
                "key_map_hash": id_of(key_map),
            }
            exp["error"] = None
        dump(os.path.join(d, "expected.json"), exp)
        notes_out.append(f"\n## {c['name']}\n\n{c['notes']}\n")
    with open(os.path.join(CASES_DIR, "NOTES.md"), "w", encoding="utf-8") as f:
        f.write("".join(notes_out))

    for name, (verdict, err, note) in EVAL.items():
        d = os.path.join(EVAL_DIR, name)
        assert os.path.isfile(os.path.join(d, "case.json")), name
        exp = {"verdict": verdict, "error": error_obj(err) if err else None}
        dump(os.path.join(d, "expected.json"), exp)
    print(f"{len(CASES)} cases, {len(EVAL)} eval expectations")


if __name__ == "__main__":
    main()
