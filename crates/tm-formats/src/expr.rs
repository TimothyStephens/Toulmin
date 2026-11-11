//! The expression grammar of `filter.where` and `derive.expr` (design
//! 11.18.1, item 17): a hand-written recursive-descent parser to an AST,
//! the 11.18.4 `{expr}` serializer over it, and type checking per 11.18.2.
//!
//! Codes: `E6006` syntax error, `E6005` unknown column, `E6004` type error;
//! every error's `field` is the pointer of the expression string.

use std::fmt::Write as _;

use crate::errors::{Code, Error};
use crate::typed_value::Tag;

/// A literal, kept as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    /// `INT`, digits as written.
    Int(String),
    /// `DEC`, digits as written.
    Dec(String),
    /// `STRING`, decoded.
    Str(String),
    /// `true` or `false`.
    Bool(bool),
    /// `missing`.
    Missing,
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `or`
    Or,
    /// `and`
    And,
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
}

impl BinOp {
    fn text(self) -> &'static str {
        match self {
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Eq => "=",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        }
    }

    fn prec(self) -> u8 {
        match self {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div => 6,
        }
    }

    fn is_cmp(self) -> bool {
        self.prec() == 4
    }
}

/// A function of the `call` rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Func {
    /// `abs`
    Abs,
    /// `log` (natural)
    Log,
    /// `log2`
    Log2,
    /// `log10`
    Log10,
    /// `exp`
    Exp,
    /// `coalesce`
    Coalesce,
}

impl Func {
    fn parse(s: &str) -> Option<Func> {
        Some(match s {
            "abs" => Func::Abs,
            "log" => Func::Log,
            "log2" => Func::Log2,
            "log10" => Func::Log10,
            "exp" => Func::Exp,
            "coalesce" => Func::Coalesce,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Func::Abs => "abs",
            Func::Log => "log",
            Func::Log2 => "log2",
            Func::Log10 => "log10",
            Func::Exp => "exp",
            Func::Coalesce => "coalesce",
        }
    }
}

/// An expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A literal.
    Lit(Lit),
    /// A column reference.
    Col(String),
    /// Unary minus.
    Neg(Box<Expr>),
    /// `not`.
    Not(Box<Expr>),
    /// A binary operation.
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// A function call.
    Call(Func, Vec<Expr>),
}

const RESERVED: &[&str] = &["and", "or", "not", "true", "false", "missing"];

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Int(String),
    Dec(String),
    Str(String),
    Ident(String),
    Quoted(String),
    Op(&'static str),
}

fn syntax(at: &str, msg: impl Into<String>) -> Error {
    Error::at(Code::E6006, at, msg)
}

#[allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "one hand-written lexer over bytes reads best as one loop"
)]
fn lex(src: &str, at: &str) -> Result<Vec<Tok>, Error> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i < b.len() && b[i] == b'.' {
                let p = i;
                i += 1;
                let fs = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i == fs {
                    return Err(syntax(
                        at,
                        format!("a decimal needs digits after the point at offset {p}"),
                    ));
                }
                out.push(Tok::Dec(src[s..i].to_owned()));
            } else {
                out.push(Tok::Int(src[s..i].to_owned()));
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(src[s..i].to_owned()));
            continue;
        }
        match c {
            b'"' => {
                i += 1;
                let mut s = String::new();
                loop {
                    let Some(&ch) = b.get(i) else {
                        return Err(syntax(at, "unterminated string"));
                    };
                    if ch == b'"' {
                        i += 1;
                        break;
                    }
                    if ch == b'\\' {
                        match b.get(i + 1) {
                            Some(b'"') => s.push('"'),
                            Some(b'\\') => s.push('\\'),
                            _ => {
                                return Err(syntax(
                                    at,
                                    "a backslash in a string escapes only '\"' or '\\'",
                                ));
                            }
                        }
                        i += 2;
                        continue;
                    }
                    // Copy one UTF-8 character.
                    let len = src[i..].chars().next().map_or(1, char::len_utf8);
                    s.push_str(&src[i..i + len]);
                    i += len;
                }
                out.push(Tok::Str(s));
            }
            b'`' => {
                let s = i + 1;
                let Some(end) = src[s..].find('`') else {
                    return Err(syntax(at, "unterminated backtick name"));
                };
                if end == 0 {
                    return Err(syntax(at, "empty backtick name"));
                }
                out.push(Tok::Quoted(src[s..s + end].to_owned()));
                i = s + end + 1;
            }
            _ => {
                let two = src.get(i..i + 2);
                let op = match two {
                    Some("!=") => Some("!="),
                    Some("<=") => Some("<="),
                    Some(">=") => Some(">="),
                    _ => None,
                };
                if let Some(op) = op {
                    out.push(Tok::Op(op));
                    i += 2;
                    continue;
                }
                let op = match c {
                    b'=' => "=",
                    b'<' => "<",
                    b'>' => ">",
                    b'+' => "+",
                    b'-' => "-",
                    b'*' => "*",
                    b'/' => "/",
                    b'(' => "(",
                    b')' => ")",
                    b',' => ",",
                    _ => {
                        let ch = src[i..].chars().next().unwrap_or('?');
                        return Err(syntax(at, format!("unexpected character {ch:?}")));
                    }
                };
                out.push(Tok::Op(op));
                i += 1;
            }
        }
    }
    Ok(out)
}

struct P<'a> {
    toks: Vec<Tok>,
    i: usize,
    at: &'a str,
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }

    fn peek_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }

    fn peek_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == kw)
    }

    fn err(&self, msg: &str) -> Error {
        let near = match self.peek() {
            None => "end of expression".to_owned(),
            Some(t) => format!("{t:?}"),
        };
        syntax(self.at, format!("{msg} (at {near})"))
    }

    fn or(&mut self) -> Result<Expr, Error> {
        let mut l = self.and()?;
        while self.peek_kw("or") {
            self.i += 1;
            let r = self.and()?;
            l = Expr::Bin(BinOp::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<Expr, Error> {
        let mut l = self.not()?;
        while self.peek_kw("and") {
            self.i += 1;
            let r = self.not()?;
            l = Expr::Bin(BinOp::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn not(&mut self) -> Result<Expr, Error> {
        if self.peek_kw("not") {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.cmp()
    }

    fn cmp_op(&self) -> Option<BinOp> {
        match self.peek() {
            Some(Tok::Op("=")) => Some(BinOp::Eq),
            Some(Tok::Op("!=")) => Some(BinOp::Ne),
            Some(Tok::Op("<")) => Some(BinOp::Lt),
            Some(Tok::Op("<=")) => Some(BinOp::Le),
            Some(Tok::Op(">")) => Some(BinOp::Gt),
            Some(Tok::Op(">=")) => Some(BinOp::Ge),
            _ => None,
        }
    }

    fn cmp(&mut self) -> Result<Expr, Error> {
        let l = self.add()?;
        if let Some(op) = self.cmp_op() {
            self.i += 1;
            let r = self.add()?;
            if self.cmp_op().is_some() {
                return Err(self.err("comparisons do not chain"));
            }
            return Ok(Expr::Bin(op, Box::new(l), Box::new(r)));
        }
        Ok(l)
    }

    fn add(&mut self) -> Result<Expr, Error> {
        let mut l = self.mul()?;
        loop {
            let op = if self.peek_op("+") {
                BinOp::Add
            } else if self.peek_op("-") {
                BinOp::Sub
            } else {
                return Ok(l);
            };
            self.i += 1;
            let r = self.mul()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn mul(&mut self) -> Result<Expr, Error> {
        let mut l = self.unary()?;
        loop {
            let op = if self.peek_op("*") {
                BinOp::Mul
            } else if self.peek_op("/") {
                BinOp::Div
            } else {
                return Ok(l);
            };
            self.i += 1;
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if self.peek_op("-") {
            self.i += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        let Some(t) = self.peek().cloned() else {
            return Err(self.err("expected a value"));
        };
        self.i += 1;
        match t {
            Tok::Int(s) => Ok(Expr::Lit(Lit::Int(s))),
            Tok::Dec(s) => Ok(Expr::Lit(Lit::Dec(s))),
            Tok::Str(s) => Ok(Expr::Lit(Lit::Str(s))),
            Tok::Quoted(s) => Ok(Expr::Col(s)),
            Tok::Op("(") => {
                let e = self.or()?;
                if !self.peek_op(")") {
                    return Err(self.err("expected ')'"));
                }
                self.i += 1;
                Ok(e)
            }
            Tok::Ident(s) => match s.as_str() {
                "true" => Ok(Expr::Lit(Lit::Bool(true))),
                "false" => Ok(Expr::Lit(Lit::Bool(false))),
                "missing" => Ok(Expr::Lit(Lit::Missing)),
                "and" | "or" | "not" => {
                    self.i -= 1;
                    Err(self.err("reserved word where a value belongs"))
                }
                _ if self.peek_op("(") => self.call(&s),
                _ => Ok(Expr::Col(s)),
            },
            Tok::Op(_) => {
                self.i -= 1;
                Err(self.err("expected a value"))
            }
        }
    }

    fn call(&mut self, name: &str) -> Result<Expr, Error> {
        let Some(f) = Func::parse(name) else {
            return Err(syntax(self.at, format!("unknown function {name:?}")));
        };
        self.i += 1; // "("
        let mut args = vec![self.or()?];
        while self.peek_op(",") {
            self.i += 1;
            args.push(self.or()?);
        }
        if !self.peek_op(")") {
            return Err(self.err("expected ',' or ')'"));
        }
        self.i += 1;
        let ok = if f == Func::Coalesce {
            args.len() >= 2
        } else {
            args.len() == 1
        };
        if !ok {
            return Err(syntax(
                self.at,
                format!(
                    "{} takes {}, got {}",
                    f.name(),
                    if f == Func::Coalesce {
                        "two or more arguments"
                    } else {
                        "one argument"
                    },
                    args.len()
                ),
            ));
        }
        Ok(Expr::Call(f, args))
    }
}

/// Parse an expression. `at` is the pointer of the expression string.
///
/// # Errors
/// `E6006` for anything 11.18.1 does not derive.
pub fn parse(src: &str, at: &str) -> Result<Expr, Error> {
    let toks = lex(src, at)?;
    let mut p = P { toks, i: 0, at };
    let e = p.or()?;
    if p.i != p.toks.len() {
        return Err(p.err("unexpected token after the expression"));
    }
    Ok(e)
}

// ---------------------------------------------------------------- {expr}

fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Bin(op, ..) => op.prec(),
        Expr::Not(_) => 3,
        Expr::Neg(_) => 7,
        Expr::Lit(_) | Expr::Col(_) | Expr::Call(..) => 8,
    }
}

fn is_ident(s: &str) -> bool {
    let mut b = s.bytes();
    matches!(b.next(), Some(c) if c.is_ascii_alphabetic() || c == b'_')
        && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn write_child(out: &mut String, e: &Expr, parens: bool) {
    if parens {
        out.push('(');
    }
    write(out, e);
    if parens {
        out.push(')');
    }
}

fn write(out: &mut String, e: &Expr) {
    match e {
        Expr::Lit(Lit::Int(s) | Lit::Dec(s)) => out.push_str(s),
        Expr::Lit(Lit::Str(s)) => {
            out.push('"');
            for c in s.chars() {
                if c == '"' || c == '\\' {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push('"');
        }
        Expr::Lit(Lit::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
        Expr::Lit(Lit::Missing) => out.push_str("missing"),
        Expr::Col(c) => {
            if is_ident(c) && !RESERVED.contains(&c.as_str()) {
                out.push_str(c);
            } else {
                let _ = write!(out, "`{c}`");
            }
        }
        Expr::Neg(x) => {
            out.push('-');
            write_child(out, x, prec(x) < 7);
        }
        Expr::Not(x) => {
            out.push_str("not ");
            write_child(out, x, prec(x) < 3);
        }
        Expr::Bin(op, l, r) => {
            let p = op.prec();
            let lp = prec(l);
            write_child(out, l, lp < p || (op.is_cmp() && lp == p));
            let _ = write!(out, " {} ", op.text());
            write_child(out, r, prec(r) <= p);
        }
        Expr::Call(f, args) => {
            out.push_str(f.name());
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write(out, a);
            }
            out.push(')');
        }
    }
}

impl Expr {
    /// The 11.18.4 `{expr}` rendering, re-serialized from the tree.
    #[must_use]
    pub fn render(&self) -> String {
        let mut s = String::new();
        write(&mut s, self);
        s
    }

    /// Every column the expression names, in order of appearance.
    #[must_use]
    pub fn columns(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_columns(&mut out);
        out
    }

    fn collect_columns<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Expr::Col(c) => out.push(c),
            Expr::Lit(_) => {}
            Expr::Neg(x) | Expr::Not(x) => x.collect_columns(out),
            Expr::Bin(_, l, r) => {
                l.collect_columns(out);
                r.collect_columns(out);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.collect_columns(out)),
        }
    }

    /// The result type under 11.18.2, given the type of each column.
    /// `Ok(None)` is the untyped `missing` literal (or an expression made
    /// only of it). `at` is the pointer of the expression string.
    ///
    /// # Errors
    /// `E6005` for a column `column_type` does not know, `E6004` for an
    /// operand type error.
    pub fn type_of(
        &self,
        column_type: &dyn Fn(&str) -> Option<Tag>,
        at: &str,
    ) -> Result<Option<Tag>, Error> {
        let mismatch = |msg: String| Error::at(Code::E6004, at, msg);
        let numeric = |t: Option<Tag>| matches!(t, None | Some(Tag::Int | Tag::Dec));
        Ok(match self {
            Expr::Lit(Lit::Int(_)) => Some(Tag::Int),
            Expr::Lit(Lit::Dec(_)) => Some(Tag::Dec),
            Expr::Lit(Lit::Str(_)) => Some(Tag::Str),
            Expr::Lit(Lit::Bool(_)) => Some(Tag::Bool),
            Expr::Lit(Lit::Missing) => None,
            Expr::Col(c) => Some(
                column_type(c)
                    .ok_or_else(|| Error::at(Code::E6005, at, format!("unknown column {c:?}")))?,
            ),
            Expr::Neg(x) => {
                let t = x.type_of(column_type, at)?;
                if !numeric(t) {
                    return Err(mismatch(format!("unary minus on {t:?}")));
                }
                t
            }
            Expr::Not(x) => {
                let t = x.type_of(column_type, at)?;
                if !matches!(t, None | Some(Tag::Bool)) {
                    return Err(mismatch(format!("not on {t:?}")));
                }
                Some(Tag::Bool)
            }
            Expr::Bin(op, l, r) => {
                let lt = l.type_of(column_type, at)?;
                let rt = r.type_of(column_type, at)?;
                match op {
                    BinOp::And | BinOp::Or => {
                        if !matches!(lt, None | Some(Tag::Bool))
                            || !matches!(rt, None | Some(Tag::Bool))
                        {
                            return Err(mismatch(format!("{} on {lt:?} and {rt:?}", op.text())));
                        }
                        Some(Tag::Bool)
                    }
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        if !numeric(lt) || !numeric(rt) {
                            return Err(mismatch(format!("{} on {lt:?} and {rt:?}", op.text())));
                        }
                        if *op == BinOp::Div {
                            Some(Tag::Dec)
                        } else {
                            match (lt, rt) {
                                (None, None) => None,
                                (Some(Tag::Int), Some(Tag::Int) | None)
                                | (None, Some(Tag::Int)) => Some(Tag::Int),
                                _ => Some(Tag::Dec),
                            }
                        }
                    }
                    _ => {
                        if !comparable(lt, rt, matches!(op, BinOp::Eq | BinOp::Ne)) {
                            return Err(mismatch(format!("{} on {lt:?} and {rt:?}", op.text())));
                        }
                        Some(Tag::Bool)
                    }
                }
            }
            Expr::Call(f, args) => {
                let mut types = Vec::with_capacity(args.len());
                for a in args {
                    types.push(a.type_of(column_type, at)?);
                }
                match f {
                    Func::Abs => {
                        if !numeric(types[0]) {
                            return Err(mismatch(format!("abs of {:?}", types[0])));
                        }
                        types[0]
                    }
                    Func::Log | Func::Log2 | Func::Log10 | Func::Exp => {
                        if !numeric(types[0]) {
                            return Err(mismatch(format!("{} of {:?}", f.name(), types[0])));
                        }
                        Some(Tag::Dec)
                    }
                    Func::Coalesce => {
                        let mut common: Option<Tag> = None;
                        for t in types.into_iter().flatten() {
                            common = Some(match (common, t) {
                                (None, t) => t,
                                (Some(a), b) if a == b => a,
                                (Some(Tag::Int | Tag::Dec), Tag::Int | Tag::Dec) => Tag::Dec,
                                (Some(a), b) => {
                                    return Err(mismatch(format!("coalesce of {a:?} and {b:?}")));
                                }
                            });
                        }
                        common
                    }
                }
            }
        })
    }
}

/// Whether two operand types compare under 11.18.2 (`eq_ne` for `=` and
/// `!=`, which also admit `bool`).
#[must_use]
pub fn comparable(a: Option<Tag>, b: Option<Tag>, eq_ne: bool) -> bool {
    match (a, b) {
        (None, _) | (_, None) => true,
        (Some(a), Some(b)) => match (a, b) {
            (Tag::Int | Tag::Dec, Tag::Int | Tag::Dec)
            | (Tag::Str, Tag::Str)
            | (Tag::Date, Tag::Date)
            | (Tag::DateTime, Tag::DateTime) => true,
            (Tag::Bool, Tag::Bool) => eq_ne,
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> String {
        parse(s, "/x").unwrap().render()
    }

    fn code(s: &str) -> Code {
        parse(s, "/x").unwrap_err().code
    }

    #[test]
    fn renders() {
        assert_eq!(r("a+b*c"), "a + b * c");
        assert_eq!(r("(a+b)*c"), "(a + b) * c");
        assert_eq!(r("a-(b-c)"), "a - (b - c)");
        assert_eq!(r("(a-b)-c"), "a - b - c");
        assert_eq!(r("a -5"), "a - 5");
        assert_eq!(r("- - 5"), "--5");
        assert_eq!(r("-(a*b)"), "-(a * b)");
        assert_eq!(r("not   a=b"), "not a = b");
        assert_eq!(r("(not a) = b"), "(not a) = b");
        assert_eq!(r("a or b and c"), "a or b and c");
        assert_eq!(r("(a or b) and c"), "(a or b) and c");
        assert_eq!(r("coalesce( a ,b,1.50 )"), "coalesce(a, b, 1.50)");
        assert_eq!(r("`gene_id` = \"a\\\"b\""), "gene_id = \"a\\\"b\"");
        assert_eq!(
            r("`log2 FC` > 1 and `and` = true"),
            "`log2 FC` > 1 and `and` = true"
        );
        assert_eq!(r("log > 1"), "log > 1");
        assert_eq!(r("(a < b) = c"), "(a < b) = c");
    }

    #[test]
    fn syntax_errors() {
        for s in [
            "a < b < c",
            "foo(a)",
            "abs(a, b)",
            "coalesce(a)",
            "\"x",
            "`x",
            "``",
            "\"\\n\"",
            "1.",
            "a b",
            "",
            "and",
            "(a",
            "a !b",
            "a = = b",
        ] {
            assert_eq!(code(s), Code::E6006, "{s}");
        }
    }

    #[test]
    fn types() {
        let cols = |c: &str| match c {
            "i" => Some(Tag::Int),
            "d" => Some(Tag::Dec),
            "s" => Some(Tag::Str),
            "b" => Some(Tag::Bool),
            "t" => Some(Tag::Date),
            "u" => Some(Tag::DateTime),
            _ => None,
        };
        let ty = |s: &str| parse(s, "/x").unwrap().type_of(&cols, "/x");
        assert_eq!(ty("i + i").unwrap(), Some(Tag::Int));
        assert_eq!(ty("i + d").unwrap(), Some(Tag::Dec));
        assert_eq!(ty("i / i").unwrap(), Some(Tag::Dec));
        assert_eq!(ty("log(i)").unwrap(), Some(Tag::Dec));
        assert_eq!(ty("coalesce(i, missing, d)").unwrap(), Some(Tag::Dec));
        assert_eq!(ty("missing").unwrap(), None);
        assert_eq!(ty("b = true and not missing").unwrap(), Some(Tag::Bool));
        assert_eq!(ty("s < \"x\"").unwrap(), Some(Tag::Bool));
        assert_eq!(ty("s = 1").unwrap_err().code, Code::E6004);
        assert_eq!(ty("b < true").unwrap_err().code, Code::E6004);
        assert_eq!(ty("t = u").unwrap_err().code, Code::E6004);
        assert_eq!(ty("t + 1").unwrap_err().code, Code::E6004);
        assert_eq!(ty("i and b").unwrap_err().code, Code::E6004);
        assert_eq!(ty("coalesce(s, i)").unwrap_err().code, Code::E6004);
        assert_eq!(ty("nope = 1").unwrap_err().code, Code::E6005);
    }
}
