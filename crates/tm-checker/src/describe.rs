//! Step descriptions (design 11.18.4, item 56): one sentence per op from
//! its template, which the reference checker reproduces byte for byte.

use tm_formats::expr::Expr;
use tm_formats::steps::{AggFn, Dir, MissingPlace, Op, Ties};

/// An integer with `,` as the thousands separator (`{n}`, `{m}`, `{k}`).
#[must_use]
pub fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A column name as a placeholder writes it: backticks only where the
/// grammar requires them (11.18.4; runbook section 6).
#[must_use]
pub fn column(name: &str) -> String {
    Expr::Col(name.to_owned()).render()
}

/// The 11.18.4 list rule: ` and ` for two, `, and ` before the last of
/// three or more.
#[must_use]
pub fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [init @ .., last] => format!("{}, and {last}", init.join(", ")),
    }
}

fn columns(names: &[String]) -> String {
    list(&names.iter().map(|c| column(c)).collect::<Vec<_>>())
}

fn dir(d: Dir) -> &'static str {
    match d {
        Dir::Asc => "ascending",
        Dir::Desc => "descending",
    }
}

fn missing(m: MissingPlace) -> &'static str {
    match m {
        MissingPlace::First => "placed first",
        MissingPlace::Last => "placed last",
        MissingPlace::Exclude => "excluded",
    }
}

fn ties(t: Ties) -> &'static str {
    match t {
        Ties::Min => "min",
        Ties::Max => "max",
        Ties::Dense => "dense",
        Ties::Ordinal => "ordinal",
    }
}

fn func(f: AggFn) -> &'static str {
    match f {
        AggFn::Count => "count",
        AggFn::Sum => "sum",
        AggFn::Mean => "mean",
        AggFn::Min => "min",
        AggFn::Max => "max",
        AggFn::Median => "median",
    }
}

/// The description of `op`, given the key of its input table (`{key}`),
/// its input row count `m`, and its output row count `n`.
#[must_use]
#[allow(
    clippy::many_single_char_names,
    reason = "n, m, k, d, t are 11.18.4's placeholders"
)]
pub fn describe(op: &Op, key: &[String], m: u64, n: u64) -> String {
    let (n, m) = (count(n), count(m));
    match op {
        Op::Filter { where_ } => format!(
            "Keep rows where {} (missing values excluded); {n} of {m} rows remain.",
            where_.render()
        ),
        Op::Select { columns: c } => format!("Keep columns {}; {n} rows.", columns(c)),
        Op::Sort { by, missing: mp } => {
            let parts: Vec<String> = by
                .iter()
                .map(|(c, d)| format!("{} {}", column(c), dir(*d)))
                .collect();
            format!(
                "Sort by {} (missing values {}; ties broken by {}); {n} rows.",
                parts.join(", then "),
                missing(*mp),
                columns(key)
            )
        }
        Op::Take { n: k } => format!("Take the first {} rows; {n} rows remain.", count(*k)),
        Op::Rank {
            by,
            dir: d,
            ties: t,
            missing: mp,
            group_by,
        } => {
            let scope = match group_by {
                None => "all rows".to_owned(),
                Some(g) => format!("each {}", columns(g)),
            };
            format!(
                "Rank by {} {} within {scope} (ties: {}; missing values {}); {n} rows.",
                column(by),
                dir(*d),
                ties(*t),
                missing(*mp)
            )
        }
        Op::Derive { as_, expr } => {
            format!("Add column {} = {}; {n} rows.", column(as_), expr.render())
        }
        Op::Group { by, aggregates } => {
            let aggs: Vec<String> = aggregates
                .iter()
                .map(|a| {
                    format!(
                        "{} = {}({})",
                        column(&a.as_),
                        func(a.func),
                        column(&a.column)
                    )
                })
                .collect();
            format!(
                "Group by {} and compute {}; {n} groups from {m} rows.",
                columns(by),
                list(&aggs)
            )
        }
        Op::Join {
            right,
            on,
            left_kind,
        } => {
            let pairs: Vec<String> = on
                .iter()
                .map(|(l, r)| format!("{} = {}", column(l), column(r)))
                .collect();
            format!(
                "{} join with {right} on {}; {n} rows.",
                if *left_kind { "Left" } else { "Inner" },
                list(&pairs)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_lists() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1000), "1,000");
        assert_eq!(count(1_234_567), "1,234,567");
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert_eq!(list(&s(&["a"])), "a");
        assert_eq!(list(&s(&["a", "b"])), "a and b");
        assert_eq!(list(&s(&["a", "b", "c"])), "a, b, and c");
        assert_eq!(column("p-value"), "`p-value`");
        assert_eq!(column("or"), "`or`");
        assert_eq!(column("gene_id"), "gene_id");
    }
}
