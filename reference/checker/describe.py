"""Step descriptions (checker.md 11.18.4), byte for byte.

Each function takes the op as written, the parsed expression where one
exists, the table key before the step, and the counts the step produced.
"""

from exprs import render, render_column


def number(n):
    """{n}: an integer with `,` as the thousands separator."""
    return format(n, ",")


def join_list(items):
    """The list rule: `, and ` before the last of three or more, ` and ` for two."""
    items = list(items)
    if len(items) <= 1:
        return "".join(items)
    if len(items) == 2:
        return f"{items[0]} and {items[1]}"
    return ", ".join(items[:-1]) + ", and " + items[-1]


def columns(names):
    return join_list(render_column(n) for n in names)


DIR = {"asc": "ascending", "desc": "descending"}
MISSING = {"first": "placed first", "last": "placed last", "exclude": "excluded"}


def filter_(expr, n, m):
    return f"Keep rows where {render(expr)} (missing values excluded); {number(n)} of {number(m)} rows remain."


def select(op, n):
    return f"Keep columns {columns(op['columns'])}; {number(n)} rows."


def sort(op, key, n):
    by = ", then ".join(f"{render_column(e['column'])} {DIR[e['dir']]}" for e in op["by"])
    return (
        f"Sort by {by} (missing values {MISSING[op['missing']]}; "
        f"ties broken by {columns(key)}); {number(n)} rows."
    )


def take(op, n):
    return f"Take the first {number(op['n'])} rows; {number(n)} rows remain."


def rank(op, n):
    scope = "all rows" if op["scope"] == "all" else f"each {columns(op['group_by'])}"
    return (
        f"Rank by {render_column(op['by'])} {DIR[op['dir']]} within {scope} "
        f"(ties: {op['ties']}; missing values {MISSING[op['missing']]}); {number(n)} rows."
    )


def derive(op, expr, n):
    return f"Add column {render_column(op['as'])} = {render(expr)}; {number(n)} rows."


def group(op, n, m):
    aggs = join_list(
        f"{render_column(a['as'])} = {a['fn']}({render_column(a['column'])})" for a in op["aggregates"]
    )
    return f"Group by {columns(op['by'])} and compute {aggs}; {number(n)} groups from {number(m)} rows."


def join(op, n):
    kind = "Inner" if op["kind"] == "inner" else "Left"
    pairs = join_list(f"{render_column(p['left'])} = {render_column(p['right'])}" for p in op["on"])
    return f"{kind} join with {op['right']} on {pairs}; {number(n)} rows."
