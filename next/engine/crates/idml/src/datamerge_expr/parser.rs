//! Parses calculated-field expressions and binds their field names.

use super::lexer::{Lexed, Tok, describe, lex};
use super::{ExprError, FUNCS, Func, MANY, MAX_DEPTH, MAX_EXPR_CHARS, err};
use crate::datamerge::Field;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Op { Add, Sub, Mul, Div, Concat, Eq, Ne, Lt, Gt, Le, Ge }

#[derive(Debug)]
pub(super) struct Node { pub(super) kind: Kind, pub(super) pos: usize, pub(super) end: usize }

#[derive(Debug)]
pub(super) enum Kind { Num(f64), Text(String), Bool(bool), Field(usize), Neg(Box<Node>), Bin(Op, Box<Node>, Box<Node>), Call(Func, Vec<Node>) }

/// A parsed expression; `evaluate` it for a record.
#[derive(Debug)]
pub struct Compiled { pub(super) root: Node }

struct Parser<'a> {
    toks: Vec<Lexed>,
    i: usize,
    depth: usize,
    /// The fields an expression may name (`None`: only the syntax is checked).
    fields: Option<&'a [Field]>,
    /// The names of the computed fields defined below the one that is parsed (to say so when one is used).
    later: &'a [String],
}

/// The column `name` (`{name}`, `{@name}`, `{#name}`) is: the exact name first, then the same name in other case.
fn find_field(fields: &[Field], name: &str) -> Option<usize> {
    let (kind, base) = if let Some(b) = name.strip_prefix('@') { (Some("image"), b) } else if let Some(b) = name.strip_prefix('#') { (Some("qrcode"), b) } else { (None, name) };
    let base = base.trim();
    let kind_ok = |f: &Field| kind.is_none_or(|k| f.kind == k);
    fields.iter().position(|f| f.name == base && kind_ok(f)).or_else(|| {
        let low = base.to_lowercase();
        fields.iter().position(|f| f.name.to_lowercase() == low && kind_ok(f))
    })
}

impl Parser<'_> {
    fn peek(&self) -> &Lexed { &self.toks[self.i] }
    fn next(&mut self) -> &Lexed { let t = &self.toks[self.i]; if self.i + 1 < self.toks.len() { self.i += 1; } t }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { let t = self.peek(); return Err(err("tooDeep", t.pos, t.end, vec![])); }
        Ok(())
    }

    fn comparison(&mut self) -> Result<Node, ExprError> {
        let mut left = self.concat()?;
        loop {
            let op = match self.peek().tok { Tok::Eq => Op::Eq, Tok::Ne => Op::Ne, Tok::Lt => Op::Lt, Tok::Gt => Op::Gt, Tok::Le => Op::Le, Tok::Ge => Op::Ge, _ => return Ok(left) };
            self.next();
            let right = self.concat()?;
            left = Node { pos: left.pos, end: right.end, kind: Kind::Bin(op, Box::new(left), Box::new(right)) };
        }
    }

    fn concat(&mut self) -> Result<Node, ExprError> {
        let mut left = self.additive()?;
        while self.peek().tok == Tok::Amp {
            self.next();
            let right = self.additive()?;
            left = Node { pos: left.pos, end: right.end, kind: Kind::Bin(Op::Concat, Box::new(left), Box::new(right)) };
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Node, ExprError> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek().tok { Tok::Plus => Op::Add, Tok::Minus => Op::Sub, _ => return Ok(left) };
            self.next();
            let right = self.term()?;
            left = Node { pos: left.pos, end: right.end, kind: Kind::Bin(op, Box::new(left), Box::new(right)) };
        }
    }

    fn term(&mut self) -> Result<Node, ExprError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek().tok { Tok::Star => Op::Mul, Tok::Slash => Op::Div, _ => return Ok(left) };
            self.next();
            let right = self.unary()?;
            left = Node { pos: left.pos, end: right.end, kind: Kind::Bin(op, Box::new(left), Box::new(right)) };
        }
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        self.enter()?;
        let result = match self.peek().tok {
            Tok::Minus => {
                let pos = self.next().pos;
                let inner = self.unary()?;
                Ok(Node { pos, end: inner.end, kind: Kind::Neg(Box::new(inner)) })
            }
            Tok::Plus => { self.next(); self.unary() }
            _ => self.primary(),
        };
        self.depth -= 1;
        result
    }

    fn primary(&mut self) -> Result<Node, ExprError> {
        let (tok, pos, end) = { let t = self.next(); (t.tok.clone(), t.pos, t.end) };
        match tok {
            Tok::Num(x) => Ok(Node { kind: Kind::Num(x), pos, end }),
            Tok::Str(s) => Ok(Node { kind: Kind::Text(s), pos, end }),
            Tok::Field(name) => match self.fields {
                None => Ok(Node { kind: Kind::Field(usize::MAX), pos, end }),
                Some(fields) => match find_field(fields, &name) {
                    Some(i) => Ok(Node { kind: Kind::Field(i), pos, end }),
                    None => {
                        let base = name.trim_start_matches(['@', '#']).trim();
                        let code = if self.later.iter().any(|l| l == base) { "laterField" } else { "unknownField" };
                        Err(err(code, pos, end, vec![name]))
                    }
                },
            },
            Tok::LParen => {
                self.enter()?;
                let inner = self.comparison()?;
                self.depth -= 1;
                let t = self.peek();
                if t.tok != Tok::RParen { return Err(self.unexpected(")")); }
                self.next();
                Ok(inner)
            }
            Tok::Ident(name) => {
                if self.peek().tok == Tok::LParen {
                    let def = FUNCS.iter().find(|d| d.name.eq_ignore_ascii_case(&name)).ok_or_else(|| err("unknownFunction", pos, end, vec![name.clone()]))?;
                    self.next();
                    let mut args = Vec::new();
                    if self.peek().tok == Tok::RParen { self.next(); } else {
                        loop {
                            self.enter()?;
                            args.push(self.comparison()?);
                            self.depth -= 1;
                            match self.peek().tok {
                                Tok::Comma => { self.next(); }
                                Tok::RParen => { self.next(); break; }
                                _ => return Err(self.unexpected(")")),
                            }
                        }
                    }
                    let close = self.toks[self.i.saturating_sub(1)].end;
                    if args.len() < def.min || args.len() > def.max {
                        let wanted = if def.max == MANY { format!("{} or more", def.min) } else if def.min == def.max { def.min.to_string() } else { format!("{} to {}", def.min, def.max) };
                        return Err(err("argCount", pos, close, vec![def.name.into(), wanted, args.len().to_string()]));
                    }
                    Ok(Node { kind: Kind::Call(def.func, args), pos, end: close })
                } else if name.eq_ignore_ascii_case("TRUE") { Ok(Node { kind: Kind::Bool(true), pos, end }) }
                else if name.eq_ignore_ascii_case("FALSE") { Ok(Node { kind: Kind::Bool(false), pos, end }) }
                else { Err(err("unknownName", pos, end, vec![name])) }
            }
            Tok::End => Err(err("unexpectedEnd", pos, end, vec!["a value".into()])),
            other => Err(err("unexpectedToken", pos, end, vec![describe(&other)])),
        }
    }

    /// The current token is not what the grammar wants: at the end of the text something is missing, elsewhere it is in the way.
    fn unexpected(&self, wanted: &str) -> ExprError {
        let t = self.peek();
        if t.tok == Tok::End { err("unexpectedEnd", t.pos, t.end, vec![wanted.into()]) } else { err("unexpectedToken", t.pos, t.end, vec![describe(&t.tok)]) }
    }
}

/// Parses `expr`. With `fields` the names in braces are bound to them (an unknown one is an error); without, only the syntax is checked.
/// `later` are the names of computed fields defined after this one (only to explain `laterField`).
pub fn compile(expr: &str, fields: Option<&[Field]>, later: &[String]) -> Result<Compiled, ExprError> {
    let chars: Vec<char> = expr.chars().collect();
    if chars.len() > MAX_EXPR_CHARS { return Err(err("tooLong", MAX_EXPR_CHARS, chars.len(), vec![])); }
    if chars.iter().all(|c| c.is_whitespace()) { return Err(err("empty", 0, 0, vec![])); }
    let toks = lex(&chars)?;
    let mut p = Parser { toks, i: 0, depth: 0, fields, later };
    let root = p.comparison()?;
    if p.peek().tok != Tok::End { return Err(p.unexpected("an operator")); }
    Ok(Compiled { root })
}
