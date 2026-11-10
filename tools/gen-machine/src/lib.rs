//! The generator of sofel-shape's syllable state machines (ADR 0024).
//!
//! It reads a grammar in the subset of Ragel's language that HarfBuzz's
//! `hb-ot-shaper-*-machine.rl` use, builds a nondeterministic automaton
//! for the longest-match scanner (`|* ... *|`), turns it into a
//! deterministic one, minimizes it, and writes the transition table as
//! Rust source.
//!
//! The grammar files (`grammars/*.machine`) are the category definitions
//! and the rules of HarfBuzz 15.1.0's machines (MIT); the actions are
//! replaced by numbers.
//!
//! Grammar language:
//! - `# ...` comments to the end of the line.
//! - `export NAME = NUMBER;` a category value.
//! - `NAME = EXPR;` a definition, usable after it.
//! - `main := |* EXPR => NUMBER; ... *|;` the scanner rules, in priority
//!   order, each with its action number.
//! - `EXPR`: union `a | b`; concatenation `a . b` or `a b`; postfix `?`,
//!   `*`, `+`, `{n}`, `{n,}`, `{,m}`, `{n,m}`; parentheses; names; `any`.
//!
//! Scanner semantics (as Ragel's): from the current position, take the
//! longest non-empty sequence one of the rules accepts, the first rule
//! on ties. Every grammar ends with `other = any`, so some rule always
//! matches one character.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write;

/// A regular expression over category values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Re {
    /// One category value.
    Value(u8),
    /// Any value.
    Any,
    /// Concatenation.
    Cat(Vec<Re>),
    /// Union.
    Alt(Vec<Re>),
    /// Zero or more.
    Star(Box<Re>),
    /// One or more.
    Plus(Box<Re>),
    /// Zero or one.
    Opt(Box<Re>),
    /// Between `min` and `max` (unbounded when `None`) repetitions.
    Repeat(Box<Re>, u32, Option<u32>),
}

/// A scanner rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The pattern.
    pub re: Re,
    /// The action number written for a match.
    pub action: u8,
    /// The pattern as written, for the generated file's comments.
    pub source: String,
}

/// A parsed grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grammar {
    /// The exported categories, in order of appearance.
    pub exports: Vec<(String, u8)>,
    /// The scanner rules in priority order.
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Num(u32),
    Sym(&'static str),
}

fn tokenize(text: &str) -> Result<Vec<(Tok, usize)>, String> {
    let mut out = Vec::new();
    for (ln, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("");
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if c.is_ascii_whitespace() {
                i += 1;
            } else if c.is_ascii_alphabetic() || c == b'_' {
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push((Tok::Ident(line[s..i].to_string()), ln + 1));
            } else if c.is_ascii_digit() {
                let s = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let n = line[s..i]
                    .parse()
                    .map_err(|e| format!("line {}: {e}", ln + 1))?;
                out.push((Tok::Num(n), ln + 1));
            } else {
                const SYMS: [&str; 16] = [
                    ":=", "|*", "*|", "=>", "=", ";", "|", ".", "?", "*", "+", "(", ")", "{", "}",
                    ",",
                ];
                let sym = SYMS
                    .iter()
                    .find(|s| line[i..].starts_with(**s))
                    .ok_or(format!("line {}: unexpected `{}`", ln + 1, c as char))?;
                out.push((Tok::Sym(sym), ln + 1));
                i += sym.len();
            }
        }
    }
    Ok(out)
}

struct Parser<'a> {
    toks: &'a [(Tok, usize)],
    pos: usize,
    names: BTreeMap<String, Re>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }
    fn line(&self) -> usize {
        self.toks
            .get(self.pos)
            .or(self.toks.last())
            .map_or(0, |t| t.1)
    }
    fn err<T>(&self, msg: &str) -> Result<T, String> {
        Err(format!("line {}: {msg}", self.line()))
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.peek().cloned();
        self.pos += 1;
        t
    }
    fn expect(&mut self, s: &str) -> Result<(), String> {
        match self.next() {
            Some(Tok::Sym(x)) if x == s => Ok(()),
            _ => {
                self.pos -= 1;
                self.err(&format!("expected `{s}`"))
            }
        }
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if *x == s)
    }
    fn num(&mut self) -> Result<u32, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(n),
            _ => {
                self.pos -= 1;
                self.err("expected a number")
            }
        }
    }

    fn union(&mut self) -> Result<Re, String> {
        let mut alts = vec![self.concat()?];
        while self.is_sym("|") {
            self.pos += 1;
            alts.push(self.concat()?);
        }
        Ok(if alts.len() == 1 {
            alts.remove(0)
        } else {
            Re::Alt(alts)
        })
    }

    fn starts_factor(&self) -> bool {
        matches!(self.peek(), Some(Tok::Ident(_)) | Some(Tok::Sym("(")))
    }

    fn concat(&mut self) -> Result<Re, String> {
        let mut parts = vec![self.postfix()?];
        loop {
            if self.is_sym(".") {
                self.pos += 1;
                parts.push(self.postfix()?);
            } else if self.starts_factor() {
                parts.push(self.postfix()?);
            } else {
                break;
            }
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Re::Cat(parts)
        })
    }

    fn postfix(&mut self) -> Result<Re, String> {
        let mut re = self.atom()?;
        loop {
            re = match self.peek() {
                Some(Tok::Sym("?")) => Re::Opt(Box::new(re)),
                Some(Tok::Sym("*")) => Re::Star(Box::new(re)),
                Some(Tok::Sym("+")) => Re::Plus(Box::new(re)),
                Some(Tok::Sym("{")) => {
                    self.pos += 1;
                    let min = if self.is_sym(",") { 0 } else { self.num()? };
                    let max = if self.is_sym(",") {
                        self.pos += 1;
                        if self.is_sym("}") {
                            None
                        } else {
                            Some(self.num()?)
                        }
                    } else {
                        Some(min)
                    };
                    if self.is_sym("}") {
                        if max.is_some_and(|m| m < min) {
                            return self.err("repetition with max < min");
                        }
                        Re::Repeat(Box::new(re), min, max)
                    } else {
                        return self.err("expected `}`");
                    }
                }
                _ => return Ok(re),
            };
            self.pos += 1;
        }
    }

    fn atom(&mut self) -> Result<Re, String> {
        match self.next() {
            Some(Tok::Sym("(")) => {
                let re = self.union()?;
                self.expect(")")?;
                Ok(re)
            }
            Some(Tok::Ident(n)) if n == "any" => Ok(Re::Any),
            Some(Tok::Ident(n)) => match self.names.get(&n) {
                Some(re) => Ok(re.clone()),
                None => {
                    self.pos -= 1;
                    self.err(&format!("undefined name `{n}`"))
                }
            },
            _ => {
                self.pos -= 1;
                self.err("expected an expression")
            }
        }
    }
}

fn source_of(toks: &[(Tok, usize)]) -> String {
    let mut s = String::new();
    for (t, _) in toks {
        let piece = match t {
            Tok::Ident(n) => n.clone(),
            Tok::Num(n) => n.to_string(),
            Tok::Sym(x) => x.to_string(),
        };
        let glue = matches!(t, Tok::Sym("?" | "*" | "+" | ")" | "}" | "," | "."))
            || s.ends_with(['(', '{', '.', ','])
            || s.is_empty();
        if !glue {
            s.push(' ');
        }
        s.push_str(&piece);
    }
    s
}

/// Parses a grammar.
pub fn parse(text: &str) -> Result<Grammar, String> {
    let toks = tokenize(text)?;
    let mut p = Parser {
        toks: &toks,
        pos: 0,
        names: BTreeMap::new(),
    };
    let mut exports = Vec::new();
    let mut rules = None;
    while let Some(t) = p.next() {
        match t {
            Tok::Ident(kw) if kw == "export" => {
                let Some(Tok::Ident(name)) = p.next() else {
                    return p.err("expected a name after `export`");
                };
                p.expect("=")?;
                let v = p.num()?;
                let v = u8::try_from(v).or(p.err("category value above 255"))?;
                p.expect(";")?;
                if p.names.insert(name.clone(), Re::Value(v)).is_some() {
                    return p.err(&format!("`{name}` defined twice"));
                }
                exports.push((name, v));
            }
            Tok::Ident(name) if name == "main" => {
                p.expect(":=")?;
                p.expect("|*")?;
                let mut rs = Vec::new();
                while !p.is_sym("*|") {
                    let start = p.pos;
                    let re = p.union()?;
                    let source = source_of(&toks[start..p.pos]);
                    p.expect("=>")?;
                    let action = p.num()?;
                    let action = u8::try_from(action).or(p.err("action above 255"))?;
                    if action == NO_ACTION {
                        return p.err("action 255 is reserved");
                    }
                    p.expect(";")?;
                    rs.push(Rule { re, action, source });
                }
                p.expect("*|")?;
                p.expect(";")?;
                if rs.is_empty() {
                    return p.err("no rules");
                }
                rules = Some(rs);
            }
            Tok::Ident(name) => {
                p.expect("=")?;
                let re = p.union()?;
                p.expect(";")?;
                if p.names.insert(name.clone(), re).is_some() {
                    return p.err(&format!("`{name}` defined twice"));
                }
            }
            _ => {
                p.pos -= 1;
                return p.err("expected a definition");
            }
        }
    }
    Ok(Grammar {
        exports,
        rules: rules.ok_or("no `main := |* ... *|;`")?,
    })
}

/// The input classes: every value a pattern names is its own class; the
/// values no pattern names share the last class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classes {
    /// The class of every value 0..=255.
    pub of: Vec<u8>,
    /// The number of classes.
    pub count: usize,
}

fn values(re: &Re, out: &mut BTreeSet<u8>) {
    match re {
        Re::Value(v) => {
            out.insert(*v);
        }
        Re::Any => {}
        Re::Cat(v) | Re::Alt(v) => v.iter().for_each(|r| values(r, out)),
        Re::Star(r) | Re::Plus(r) | Re::Opt(r) | Re::Repeat(r, _, _) => values(r, out),
    }
}

/// The input classes of a grammar's rules.
pub fn classes(rules: &[Rule]) -> Classes {
    let mut named = BTreeSet::new();
    rules.iter().for_each(|r| values(&r.re, &mut named));
    let other = named.len();
    let of = (0..=255u8)
        .map(|v| named.iter().position(|&n| n == v).map_or(other, |i| i) as u8)
        .collect();
    Classes {
        of,
        count: other + 1,
    }
}

/// A nondeterministic automaton with ε-moves.
#[derive(Clone, Debug, Default)]
pub struct Nfa {
    /// Per state: moves on a class.
    pub moves: Vec<Vec<(usize, usize)>>,
    /// Per state: ε-moves.
    pub eps: Vec<Vec<usize>>,
    /// Per state: the rule it accepts, if any.
    pub accept: Vec<Option<usize>>,
}

impl Nfa {
    fn state(&mut self) -> usize {
        self.moves.push(Vec::new());
        self.eps.push(Vec::new());
        self.accept.push(None);
        self.moves.len() - 1
    }

    /// Thompson's construction of `re` from `from`; returns the end.
    fn build(&mut self, re: &Re, cls: &Classes, from: usize) -> usize {
        match re {
            Re::Value(v) => {
                let to = self.state();
                self.moves[from].push((cls.of[*v as usize] as usize, to));
                to
            }
            Re::Any => {
                let to = self.state();
                (0..cls.count).for_each(|c| self.moves[from].push((c, to)));
                to
            }
            Re::Cat(parts) => parts.iter().fold(from, |s, p| self.build(p, cls, s)),
            Re::Alt(alts) => {
                let to = self.state();
                for a in alts {
                    let s = self.state();
                    self.eps[from].push(s);
                    let e = self.build(a, cls, s);
                    self.eps[e].push(to);
                }
                to
            }
            Re::Star(r) => {
                let s = self.state();
                self.eps[from].push(s);
                let e = self.build(r, cls, s);
                self.eps[e].push(s);
                let to = self.state();
                self.eps[s].push(to);
                to
            }
            Re::Plus(r) => {
                let s = self.state();
                self.eps[from].push(s);
                let e = self.build(r, cls, s);
                self.eps[e].push(s);
                let to = self.state();
                self.eps[e].push(to);
                to
            }
            Re::Opt(r) => {
                let e = self.build(r, cls, from);
                self.eps[from].push(e);
                e
            }
            Re::Repeat(r, min, max) => {
                let mut s = (0..*min).fold(from, |s, _| self.build(r, cls, s));
                match max {
                    None => self.build(&Re::Star(r.clone()), cls, s),
                    Some(max) => {
                        let to = self.state();
                        for _ in *min..*max {
                            self.eps[s].push(to);
                            s = self.build(r, cls, s);
                        }
                        self.eps[s].push(to);
                        to
                    }
                }
            }
        }
    }

    /// The automaton of a scanner: the union of the rules, each end
    /// accepting its rule. State 0 is the start.
    pub fn scanner(rules: &[Rule], cls: &Classes) -> Nfa {
        let mut n = Nfa::default();
        let start = n.state();
        for (i, r) in rules.iter().enumerate() {
            let s = n.state();
            n.eps[start].push(s);
            let e = n.build(&r.re, cls, s);
            n.accept[e] = Some(n.accept[e].map_or(i, |a| a.min(i)));
        }
        n
    }

    fn closure(&self, set: &mut BTreeSet<usize>) {
        let mut stack: Vec<usize> = set.iter().copied().collect();
        while let Some(s) = stack.pop() {
            for &t in &self.eps[s] {
                if set.insert(t) {
                    stack.push(t);
                }
            }
        }
    }

    fn accept_of(&self, set: &BTreeSet<usize>) -> Option<usize> {
        set.iter().filter_map(|&s| self.accept[s]).min()
    }

    /// The rule the automaton accepts after `input`, if any (by
    /// simulation, for checks).
    pub fn run(&self, input: &[usize]) -> Option<usize> {
        let mut set = BTreeSet::from([0]);
        self.closure(&mut set);
        for &c in input {
            let mut next: BTreeSet<usize> = set
                .iter()
                .flat_map(|&s| self.moves[s].iter())
                .filter(|m| m.0 == c)
                .map(|m| m.1)
                .collect();
            if next.is_empty() {
                return None;
            }
            self.closure(&mut next);
            set = next;
        }
        self.accept_of(&set)
    }
}

/// A deterministic automaton. State 0 is dead (every move stays there,
/// nothing is accepted), state 1 is the start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dfa {
    /// The number of input classes.
    pub classes: usize,
    /// `next[state * classes + class]`.
    pub next: Vec<usize>,
    /// Per state: the rule accepted, if any.
    pub accept: Vec<Option<usize>>,
}

impl Dfa {
    /// The number of states, the dead state included.
    pub fn states(&self) -> usize {
        self.accept.len()
    }

    /// The subset construction.
    pub fn from_nfa(nfa: &Nfa, classes: usize) -> Dfa {
        let mut start = BTreeSet::from([0]);
        nfa.closure(&mut start);
        let mut ids: BTreeMap<BTreeSet<usize>, usize> = BTreeMap::new();
        let mut sets = vec![BTreeSet::new(), start.clone()];
        ids.insert(BTreeSet::new(), 0);
        ids.insert(start, 1);
        let mut next = vec![0; 2 * classes];
        let mut queue = VecDeque::from([1usize]);
        while let Some(id) = queue.pop_front() {
            for c in 0..classes {
                let mut t: BTreeSet<usize> = sets[id]
                    .iter()
                    .flat_map(|&s| nfa.moves[s].iter())
                    .filter(|m| m.0 == c)
                    .map(|m| m.1)
                    .collect();
                nfa.closure(&mut t);
                let tid = match ids.get(&t) {
                    Some(&i) => i,
                    None => {
                        let i = sets.len();
                        ids.insert(t.clone(), i);
                        sets.push(t);
                        next.extend(std::iter::repeat_n(0, classes));
                        queue.push_back(i);
                        i
                    }
                };
                next[id * classes + c] = tid;
            }
        }
        let accept = sets.iter().map(|s| nfa.accept_of(s)).collect();
        Dfa {
            classes,
            next,
            accept,
        }
    }

    /// Moore's partition refinement: states with the same accepted rule
    /// and moves into the same blocks are merged. The dead state stays 0,
    /// the start 1; the others are numbered breadth-first from the start.
    pub fn minimize(&self) -> Dfa {
        let n = self.states();
        let mut block: Vec<usize> = {
            let mut keys: Vec<Option<usize>> = self.accept.clone();
            keys.sort();
            keys.dedup();
            self.accept
                .iter()
                .map(|a| keys.iter().position(|k| k == a).unwrap_or(0))
                .collect()
        };
        loop {
            let mut sig: BTreeMap<Vec<usize>, usize> = BTreeMap::new();
            let refined: Vec<usize> = (0..n)
                .map(|s| {
                    let mut key = vec![block[s]];
                    key.extend((0..self.classes).map(|c| block[self.next[s * self.classes + c]]));
                    let len = sig.len();
                    *sig.entry(key).or_insert(len)
                })
                .collect();
            let count = |b: &[usize]| b.iter().collect::<BTreeSet<_>>().len();
            let done = count(&refined) == count(&block);
            block = refined;
            if done {
                break;
            }
        }
        // Renumber: dead, start, then breadth-first.
        let mut order = vec![block[0]];
        let mut queue = VecDeque::from([1usize]);
        let mut seen = BTreeSet::from([block[0]]);
        while let Some(s) = queue.pop_front() {
            if !seen.insert(block[s]) {
                continue;
            }
            order.push(block[s]);
            for c in 0..self.classes {
                queue.push_back(self.next[s * self.classes + c]);
            }
        }
        let new_id = |b: usize| order.iter().position(|&x| x == b).unwrap_or(0);
        let rep: Vec<usize> = order
            .iter()
            .map(|&b| (0..n).find(|&s| block[s] == b).unwrap_or(0))
            .collect();
        let next = rep
            .iter()
            .flat_map(|&s| (0..self.classes).map(move |c| (s, c)))
            .map(|(s, c)| new_id(block[self.next[s * self.classes + c]]))
            .collect();
        let accept = rep.iter().map(|&s| self.accept[s]).collect();
        Dfa {
            classes: self.classes,
            next,
            accept,
        }
    }

    /// The rule accepted after `input`, if any.
    pub fn run(&self, input: &[usize]) -> Option<usize> {
        let s = input
            .iter()
            .fold(1, |s, &c| self.next[s * self.classes + c]);
        self.accept[s]
    }

    /// The scanner: the syllables of `input` as `(start, end, rule)`.
    pub fn scan(&self, input: &[usize]) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        let mut p = 0;
        while p < input.len() {
            let mut s = 1;
            let mut last = None;
            for (i, &c) in input[p..].iter().enumerate() {
                s = self.next[s * self.classes + c];
                if s == 0 {
                    break;
                }
                if let Some(r) = self.accept[s] {
                    last = Some((i + 1, r));
                }
            }
            let (len, rule) = last.unwrap_or((1, usize::MAX));
            out.push((p, p + len, rule));
            p += len;
        }
        out
    }
}

/// The action value of states that accept nothing.
pub const NO_ACTION: u8 = 0xFF;

/// The machine of a grammar: its classes and its minimized automaton.
pub fn compile(g: &Grammar) -> (Classes, Dfa) {
    let cls = classes(&g.rules);
    let nfa = Nfa::scanner(&g.rules, &cls);
    let dfa = Dfa::from_nfa(&nfa, cls.count).minimize();
    (cls, dfa)
}

/// Checks the machine against the automaton before determinization on
/// every input of up to `len` classes, and that every input gets a
/// rule for its first character.
pub fn verify(g: &Grammar, cls: &Classes, dfa: &Dfa, len: usize) -> Result<(), String> {
    let nfa = Nfa::scanner(&g.rules, cls);
    let mut input = Vec::new();
    fn walk(
        nfa: &Nfa,
        dfa: &Dfa,
        input: &mut Vec<usize>,
        len: usize,
        classes: usize,
    ) -> Result<(), String> {
        let (a, b) = (nfa.run(input), dfa.run(input));
        if a != b {
            return Err(format!("input {input:?}: NFA {a:?}, DFA {b:?}"));
        }
        if input.len() == 1 && b.is_none() {
            return Err(format!("input {input:?}: no rule matches one character"));
        }
        if input.len() < len {
            for c in 0..classes {
                input.push(c);
                walk(nfa, dfa, input, len, classes)?;
                input.pop();
            }
        }
        Ok(())
    }
    walk(&nfa, dfa, &mut input, len, cls.count)
}

/// The Rust source of a machine.
pub fn emit(name: &str, header: &[String], g: &Grammar, cls: &Classes, dfa: &Dfa) -> String {
    let mut o = String::new();
    header.iter().for_each(|l| {
        let sep = if l.is_empty() { "" } else { " " };
        writeln!(o, "//{sep}{l}").unwrap();
    });
    writeln!(o, "//").unwrap();
    writeln!(
        o,
        "// {} states (state 0 is dead, 1 the start), {} classes.",
        dfa.states(),
        cls.count
    )
    .unwrap();
    writeln!(o, "//").unwrap();
    writeln!(o, "// Rules (action: pattern), in priority order:").unwrap();
    g.rules
        .iter()
        .for_each(|r| writeln!(o, "//   {}: {}", r.action, r.source).unwrap());
    writeln!(o).unwrap();
    // The classes table stops after the last named value; values past it
    // are in the last class.
    let other = cls.count - 1;
    let used = cls
        .of
        .iter()
        .rposition(|&c| c as usize != other)
        .map_or(0, |i| i + 1);
    writeln!(
        o,
        "/// The input class of each category value of the {name} machine;"
    )
    .unwrap();
    writeln!(o, "/// values past the end are in class [`OTHER`].").unwrap();
    writeln!(o, "pub(crate) const CLASSES: [u8; {used}] = [").unwrap();
    cls.of[..used].chunks(16).for_each(|c| {
        let cells: Vec<String> = c.iter().map(|v| format!("{v},")).collect();
        writeln!(o, "    {}", cells.join(" ")).unwrap();
    });
    writeln!(o, "];").unwrap();
    writeln!(o).unwrap();
    writeln!(o, "/// The class of the values no rule names.").unwrap();
    writeln!(o, "pub(crate) const OTHER: u8 = {other};").unwrap();
    writeln!(o).unwrap();
    writeln!(o, "/// The number of input classes.").unwrap();
    writeln!(o, "pub(crate) const NUM_CLASSES: usize = {};", cls.count).unwrap();
    writeln!(o).unwrap();
    let ty = if dfa.states() <= 256 { "u8" } else { "u16" };
    writeln!(
        o,
        "/// The next state, `TRANSITIONS[state * NUM_CLASSES + class]`."
    )
    .unwrap();
    writeln!(
        o,
        "pub(crate) const TRANSITIONS: [{ty}; {}] = [",
        dfa.next.len()
    )
    .unwrap();
    dfa.next.chunks(cls.count).for_each(|c| {
        let cells: Vec<String> = c.iter().map(|v| format!("{v},")).collect();
        writeln!(o, "    {}", cells.join(" ")).unwrap();
    });
    writeln!(o, "];").unwrap();
    writeln!(o).unwrap();
    writeln!(
        o,
        "/// The action of the rule each state accepts, {NO_ACTION} for none."
    )
    .unwrap();
    writeln!(o, "pub(crate) const ACCEPT: [u8; {}] = [", dfa.states()).unwrap();
    dfa.accept.chunks(16).for_each(|c| {
        let cells: Vec<String> = c
            .iter()
            .map(|a| format!("{},", a.map_or(NO_ACTION, |r| g.rules[r].action)))
            .collect();
        writeln!(o, "    {}", cells.join(" ")).unwrap();
    });
    writeln!(o, "];").unwrap();
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classes of `g` for the given values, in order.
    fn input(cls: &Classes, vals: &[u8]) -> Vec<usize> {
        vals.iter().map(|&v| cls.of[v as usize] as usize).collect()
    }

    /// Whether the single rule of the grammar `main := |* EXPR => 0;
    /// other => 1; *|` accepts `vals` as a whole.
    fn accepts(expr: &str, vals: &[u8]) -> bool {
        let g = parse(&format!(
            "export A = 1; export B = 2; export C = 3;\nmain := |* {expr} => 0; any => 1; *|;"
        ))
        .unwrap();
        let (cls, dfa) = compile(&g);
        dfa.run(&input(&cls, vals)) == Some(0)
    }

    #[test]
    fn constructs() {
        // Union, concatenation (both spellings), ?, *, +.
        assert!(accepts("A | B", &[1]) && accepts("A | B", &[2]) && !accepts("A | B", &[3]));
        assert!(accepts("A . B", &[1, 2]) && accepts("A B", &[1, 2]) && !accepts("A B", &[2, 1]));
        assert!(accepts("A B?", &[1]) && accepts("A B?", &[1, 2]) && !accepts("A B?", &[1, 2, 2]));
        assert!(accepts("A B*", &[1]) && accepts("A B*", &[1, 2, 2, 2]) && !accepts("A B*", &[2]));
        assert!(!accepts("A B+", &[1]) && accepts("A B+", &[1, 2, 2]));
        // Parentheses and precedence: postfix binds tighter than
        // concatenation, which binds tighter than union.
        assert!(accepts("(A B)+", &[1, 2, 1, 2]) && !accepts("(A B)+", &[1, 2, 1]));
        assert!(accepts("A B | C", &[3]) && !accepts("A B | C", &[1, 3]));
        assert!(accepts("A (B | C)", &[1, 3]));
        // Repetition counts.
        assert!(accepts("A{2}", &[1, 1]) && !accepts("A{2}", &[1]) && !accepts("A{2}", &[1, 1, 1]));
        assert!(!accepts("A{2,}", &[1]) && accepts("A{2,}", &[1, 1, 1, 1]));
        assert!(
            accepts("B A{,2}", &[2])
                && accepts("B A{,2}", &[2, 1, 1])
                && !accepts("B A{,2}", &[2, 1, 1, 1])
        );
        assert!(
            !accepts("A{1,3}", &[])
                && accepts("A{1,3}", &[1, 1, 1])
                && !accepts("A{1,3}", &[1, 1, 1, 1])
        );
        // any.
        assert!(accepts("A any", &[1, 200]));
    }

    #[test]
    fn names() {
        let g = parse(
            "export A = 1; export B = 2;\nab = A B; # a comment\nx = ab+;\nmain := |* x => 7; any => 9; *|;",
        )
        .unwrap();
        assert_eq!(g.exports, vec![("A".to_string(), 1), ("B".to_string(), 2)]);
        assert_eq!(g.rules[0].action, 7);
        assert_eq!(g.rules[0].source, "x");
        let (cls, dfa) = compile(&g);
        assert_eq!(dfa.run(&input(&cls, &[1, 2, 1, 2])), Some(0));
        assert!(
            parse("main := |* y => 0; *|;")
                .unwrap_err()
                .contains("undefined name `y`")
        );
        assert!(parse("export A = 1; export A = 2;").is_err());
        assert!(parse("export A = 300;").is_err());
        assert!(parse("export A = 1; x = A").is_err());
    }

    #[test]
    fn longest_match_and_priority() {
        let g = parse(
            "export A = 1; export B = 2;\nmain := |* A A => 0; A => 1; A B B => 2; any => 3; *|;",
        )
        .unwrap();
        let (cls, dfa) = compile(&g);
        let rules = |v: &[u8]| -> Vec<(usize, usize, u8)> {
            dfa.scan(&input(&cls, v))
                .into_iter()
                .map(|(s, e, r)| (s, e, g.rules[r].action))
                .collect()
        };
        // Longest: A A beats A; A B B beats A then B B.
        assert_eq!(rules(&[1, 1, 1]), vec![(0, 2, 0), (2, 3, 1)]);
        assert_eq!(rules(&[1, 2, 2]), vec![(0, 3, 2)]);
        // A B (no second B): back up to the last accepted, A.
        assert_eq!(rules(&[1, 2, 5]), vec![(0, 1, 1), (1, 2, 3), (2, 3, 3)]);
        // Equal length: the first rule wins (A over any).
        let g = parse("export A = 1;\nmain := |* any => 5; A => 6; *|;").unwrap();
        let (cls, dfa) = compile(&g);
        assert_eq!(dfa.scan(&input(&cls, &[1])), vec![(0, 1, 0)]);
    }

    #[test]
    fn minimization_keeps_every_result() {
        // Every input of up to 6 classes gives the same rule before and
        // after minimization, and minimization shrinks the automaton.
        let g = parse(
            "export A = 1; export B = 2; export C = 3;\n\
             x = (A | B) (A | B)* C?; y = A B{1,2} | C+;\n\
             main := |* x => 0; y => 1; (A C)+ B => 2; any => 3; *|;",
        )
        .unwrap();
        let cls = classes(&g.rules);
        let raw = Dfa::from_nfa(&Nfa::scanner(&g.rules, &cls), cls.count);
        let min = raw.minimize();
        assert!(min.states() < raw.states());
        fn walk(a: &Dfa, b: &Dfa, input: &mut Vec<usize>, classes: usize) {
            assert_eq!(a.run(input), b.run(input), "{input:?}");
            if input.len() < 6 {
                for c in 0..classes {
                    input.push(c);
                    walk(a, b, input, classes);
                    input.pop();
                }
            }
        }
        walk(&raw, &min, &mut Vec::new(), cls.count);
        verify(&g, &cls, &min, 6).unwrap();
    }

    #[test]
    fn classes_and_emit() {
        let g = parse("export A = 4; export B = 2;\nmain := |* A B => 0; any => 1; *|;").unwrap();
        let (cls, dfa) = compile(&g);
        assert_eq!(cls.count, 3);
        assert_eq!((cls.of[2], cls.of[4], cls.of[0], cls.of[255]), (0, 1, 2, 2));
        let src = emit("test", &["header".to_string()], &g, &cls, &dfa);
        assert!(src.starts_with("// header\n"));
        assert!(src.contains("pub(crate) const CLASSES: [u8; 5] = [\n    2, 2, 0, 2, 1,\n];"));
        assert!(src.contains("pub(crate) const OTHER: u8 = 2;"));
        assert!(src.contains("//   0: A B\n"));
    }
}
