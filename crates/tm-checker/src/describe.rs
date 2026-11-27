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
/// its input row count `m`, and its output row count `n`: the template's
/// head, `; `, its count clause, and the period.
#[must_use]
pub fn describe(op: &Op, key: &[String], m: u64, n: u64) -> String {
    format!("{}; {}.", head(op, key), counts(op, m, n))
}

/// The description of `op` without its count clause, for
/// `checker.describe_steps` (section 10), which reads no table: the
/// template's head closed with the period, never a count that was not
/// computed.
#[must_use]
pub fn describe_without_counts(op: &Op, key: &[String]) -> String {
    format!("{}.", head(op, key))
}

/// The count clause of `op`'s template (`{n} of {m} rows remain`, `{n}
/// rows`, `{n} rows remain`, `{n} groups from {m} rows`).
fn counts(op: &Op, m: u64, n: u64) -> String {
    let (n, m) = (count(n), count(m));
    match op {
        Op::Filter { .. } => format!("{n} of {m} rows remain"),
        Op::Take { .. } => format!("{n} rows remain"),
        Op::Group { .. } => format!("{n} groups from {m} rows"),
        Op::Select { .. }
        | Op::Sort { .. }
        | Op::Rank { .. }
        | Op::Derive { .. }
        | Op::Join { .. } => {
            format!("{n} rows")
        }
    }
}

/// The text of `op`'s template before the `;` that precedes the counts.
#[allow(
    clippy::many_single_char_names,
    reason = "k, d, t are 11.18.4's placeholders"
)]
fn head(op: &Op, key: &[String]) -> String {
    match op {
        Op::Filter { where_ } => format!(
            "Keep rows where {} (missing values excluded)",
            where_.render()
        ),
        Op::Select { columns: c } => format!("Keep columns {}", columns(c)),
        Op::Sort { by, missing: mp } => {
            let parts: Vec<String> = by
                .iter()
                .map(|(c, d)| format!("{} {}", column(c), dir(*d)))
                .collect();
            format!(
                "Sort by {} (missing values {}; ties broken by {})",
                parts.join(", then "),
                missing(*mp),
                columns(key)
            )
        }
        Op::Take { n: k } => format!("Take the first {} rows", count(*k)),
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
                "Rank by {} {} within {scope} (ties: {}; missing values {})",
                column(by),
                dir(*d),
                ties(*t),
                missing(*mp)
            )
        }
        Op::Derive { as_, expr } => {
            format!("Add column {} = {}", column(as_), expr.render())
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
            format!("Group by {} and compute {}", columns(by), list(&aggs))
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
                "{} join with {right} on {}",
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

    #[test]
    fn with_and_without_counts() {
        let op = Op::Take { n: 1500 };
        assert_eq!(
            describe(&op, &[], 2000, 1500),
            "Take the first 1,500 rows; 1,500 rows remain."
        );
        assert_eq!(
            describe_without_counts(&op, &[]),
            "Take the first 1,500 rows."
        );
        let op = Op::Select {
            columns: vec!["gene_id".to_owned(), "p-value".to_owned()],
        };
        assert_eq!(
            describe(&op, &[], 7, 7),
            "Keep columns gene_id and `p-value`; 7 rows."
        );
        assert_eq!(
            describe_without_counts(&op, &[]),
            "Keep columns gene_id and `p-value`."
        );
    }
}
