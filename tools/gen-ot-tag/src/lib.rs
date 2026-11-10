//! Read HarfBuzz's `src/hb-ot-tag-table.hh` and write it as Rust tables
//! for sofel-shape (ADR 0020).
//!
//! The header is the output of HarfBuzz's `gen-tag-table.py`: five
//! arrays and two functions made of a few fixed shapes of branches. The
//! parser accepts exactly those shapes, line by line after comments are
//! removed, and fails on anything else, so that a change of the
//! generator upstream is noticed when the pinned commit is raised.

use std::fmt::Write;

/// A four-byte tag as written by `HB_TAG ('a','b','c','d')`.
pub type Tag4 = [u8; 4];

/// The condition of a branch of `hb_ot_tags_from_complex_language`
/// inside the `switch (lang_str[0])`; the strings are compared from
/// `&lang_str[1]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    /// `lang_matches (&lang_str[1], limit, "<s>", <len>)`.
    LangMatches(String),
    /// `0 == strncmp (&lang_str[1], "<prefix>", <len>) &&
    /// subtag_matches (lang_str, limit, "<subtag>", <len>)`.
    PrefixSubtag(String, String),
    /// `0 == strcmp (&lang_str[1], "<s>")`.
    Exact(String),
}

/// One branch of the `switch (lang_str[0])`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// The `case` character.
    pub first: u8,
    /// The condition.
    pub cond: Cond,
    /// The tags, at most as many as the caller asks for are written.
    pub tags: Vec<Tag4>,
}

/// Everything in the header.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// `ot_languages2`: (language, tag), sorted by language.
    pub languages2: Vec<(Tag4, Tag4)>,
    /// `ot_languages3_blocked`, sorted.
    pub languages3_blocked: Vec<Tag4>,
    /// `ot_languages3`: (language, tag), sorted by language.
    pub languages3: Vec<(Tag4, Tag4)>,
    /// `ot_languages3_multi_values`.
    pub multi_values: Vec<Tag4>,
    /// `ot_languages3_multi`: (language, offset, count).
    pub multi: Vec<(Tag4, u16, u8)>,
    /// The `subtag_matches (p, limit, ...)` branches before the switch
    /// of `hb_ot_tags_from_complex_language`.
    pub complex_subtags: Vec<(String, Vec<Tag4>)>,
    /// The branches of its switch, in order.
    pub complex: Vec<Branch>,
    /// `hb_ot_ambiguous_tag_to_language`: (tag, language string).
    pub ambiguous: Vec<(Tag4, String)>,
}

/// A line of code with its line number (1-based) in the header.
#[derive(Clone, Debug)]
struct Line {
    no: usize,
    text: String,
}

/// The code lines of `text`: comments removed, whitespace outside
/// literals collapsed to one space, trimmed, empty lines dropped.
fn code_lines(text: &str) -> Result<Vec<Line>, String> {
    let mut out = Vec::new();
    let mut in_comment = false;
    for (i, raw) in text.lines().enumerate() {
        let no = i + 1;
        let b = raw.as_bytes();
        let mut s = String::new();
        let mut j = 0;
        let mut quote: Option<u8> = None;
        while j < b.len() {
            let c = b[j];
            if in_comment {
                if c == b'*' && b.get(j + 1) == Some(&b'/') {
                    in_comment = false;
                    if !s.ends_with(' ') {
                        s.push(' ');
                    }
                    j += 2;
                } else {
                    j += 1;
                }
                continue;
            }
            if let Some(q) = quote {
                if c == b'\\' {
                    return Err(format!("line {no}: an escape in a literal"));
                }
                if c == q {
                    quote = None;
                }
                s.push(char::from(c));
                j += 1;
                continue;
            }
            if c == b'/' && b.get(j + 1) == Some(&b'*') {
                in_comment = true;
                j += 2;
                continue;
            }
            if c == b'/' && b.get(j + 1) == Some(&b'/') {
                break;
            }
            if c == b'"' || c == b'\'' {
                quote = Some(c);
            }
            if !c.is_ascii() {
                return Err(format!("line {no}: a non-ASCII byte outside comments"));
            }
            if c.is_ascii_whitespace() {
                if !s.ends_with(' ') {
                    s.push(' ');
                }
            } else {
                s.push(char::from(c));
            }
            j += 1;
        }
        if quote.is_some() {
            return Err(format!("line {no}: an unterminated literal"));
        }
        let t = s.trim();
        if !t.is_empty() {
            out.push(Line {
                no,
                text: t.to_string(),
            });
        }
    }
    if in_comment {
        return Err("an unterminated comment".to_string());
    }
    Ok(out)
}

struct Cursor {
    lines: Vec<Line>,
    pos: usize,
}

impl Cursor {
    fn peek(&self) -> Option<&Line> {
        self.lines.get(self.pos)
    }
    fn next(&mut self) -> Result<Line, String> {
        let l = self
            .lines
            .get(self.pos)
            .cloned()
            .ok_or("unexpected end of the header")?;
        self.pos += 1;
        Ok(l)
    }
    fn expect(&mut self, want: &str) -> Result<(), String> {
        let l = self.next()?;
        if l.text == want {
            Ok(())
        } else {
            Err(format!(
                "line {}: expected `{want}`, found `{}`",
                l.no, l.text
            ))
        }
    }
    /// Consume `want` if it is the next line.
    fn accept(&mut self, want: &str) -> bool {
        let yes = self.peek().is_some_and(|l| l.text == want);
        if yes {
            self.pos += 1;
        }
        yes
    }
}

fn unknown(l: &Line) -> String {
    format!("line {}: unknown shape `{}`", l.no, l.text)
}

/// One char literal `'x'` at the start of `s`.
fn char_literal(s: &str) -> Option<(u8, &str)> {
    let b = s.as_bytes();
    match b {
        [b'\'', c, b'\'', ..] if (0x20..0x7F).contains(c) && *c != b'\'' && *c != b'\\' => {
            Some((*c, &s[3..]))
        }
        _ => None,
    }
}

/// `HB_TAG('a','b','c','d')` at the start of `s` (spaces allowed after
/// `HB_TAG` and around commas).
fn hb_tag(s: &str) -> Option<(Tag4, &str)> {
    let mut rest = s.strip_prefix("HB_TAG")?.trim_start().strip_prefix('(')?;
    let mut tag = [0u8; 4];
    for (i, slot) in tag.iter_mut().enumerate() {
        rest = rest.trim_start();
        let (c, r) = char_literal(rest)?;
        *slot = c;
        rest = r.trim_start();
        rest = if i < 3 {
            rest.strip_prefix(',')?
        } else {
            rest.strip_prefix(')')?
        };
    }
    Some((tag, rest))
}

/// `s` is exactly `HB_TAG(...)` followed by `tail`.
fn hb_tag_then(s: &str, tail: &str) -> Option<Tag4> {
    let (t, rest) = hb_tag(s)?;
    (rest.trim() == tail).then_some(t)
}

/// A string literal with only printable ASCII.
fn string_literal(s: &str) -> Option<String> {
    let inner = s.strip_prefix('"')?.strip_suffix('"')?;
    inner
        .bytes()
        .all(|c| (0x20..0x7F).contains(&c) && c != b'"' && c != b'\\')
        .then(|| inner.to_string())
}

/// A call `name (arg, arg, ...)`: the name and the trimmed arguments.
/// Arguments are split at commas outside literals and parentheses.
fn call(s: &str) -> Option<(&str, Vec<String>)> {
    let open = s.find('(')?;
    let name = s[..open].trim();
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        return None;
    }
    let body = s[open + 1..].strip_suffix(')')?;
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in body.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '(' | '[' => {
                    depth += 1;
                    cur.push(c);
                }
                ')' | ']' => {
                    depth -= 1;
                    if depth < 0 {
                        return None;
                    }
                    cur.push(c);
                }
                ',' if depth == 0 => {
                    args.push(cur.trim().to_string());
                    cur.clear();
                }
                _ => cur.push(c),
            },
        }
    }
    if depth != 0 || quote.is_some() {
        return None;
    }
    args.push(cur.trim().to_string());
    Some((name, args))
}

/// A string argument whose stated length matches.
fn string_with_len(s: &str, len: &str) -> Option<String> {
    let v = string_literal(s)?;
    (len.parse::<usize>().ok()? == v.len()).then_some(v)
}

/// The condition `subtag_matches (<from>, limit, "<s>", <len>)`.
fn subtag_cond(s: &str, from: &str) -> Option<String> {
    match call(s)? {
        ("subtag_matches", a) => match &a[..] {
            [f, l, sub, len] if f == from && l == "limit" => string_with_len(sub, len),
            _ => None,
        },
        _ => None,
    }
}

/// A condition inside the switch.
fn switch_cond(s: &str) -> Option<Cond> {
    if let Some(rest) = s.strip_prefix("0 == ") {
        if let Some((left, right)) = rest.split_once(" && ") {
            let prefix = match call(left)? {
                ("strncmp", a) => match &a[..] {
                    [from, p, len] if from == "&lang_str[1]" => string_with_len(p, len)?,
                    _ => return None,
                },
                _ => return None,
            };
            let subtag = subtag_cond(right, "lang_str")?;
            return Some(Cond::PrefixSubtag(prefix, subtag));
        }
        return match call(rest)? {
            ("strcmp", a) => match &a[..] {
                [from, v] if from == "&lang_str[1]" => string_literal(v).map(Cond::Exact),
                _ => None,
            },
            _ => None,
        };
    }
    match call(s)? {
        ("lang_matches", a) => match &a[..] {
            [from, l, v, len] if from == "&lang_str[1]" && l == "limit" => {
                string_with_len(v, len).map(Cond::LangMatches)
            }
            _ => None,
        },
        _ => None,
    }
}

/// `if (...)`, joining continuation lines until the parentheses
/// balance; returns the condition.
fn if_line(c: &mut Cursor) -> Result<Option<(Line, String)>, String> {
    let Some(first) = c.peek().cloned() else {
        return Ok(None);
    };
    let Some(rest) = first.text.strip_prefix("if (") else {
        return Ok(None);
    };
    c.pos += 1;
    let balance = |s: &str| {
        s.chars().fold(0i32, |d, ch| match ch {
            '(' => d + 1,
            ')' => d - 1,
            _ => d,
        })
    };
    let mut text = rest.to_string();
    // `rest` lacks the opening parenthesis of the `if`.
    while balance(&text) != -1 {
        let l = c.next()?;
        if !l.text.starts_with("&& ") {
            return Err(unknown(&l));
        }
        text.push(' ');
        text.push_str(&l.text);
    }
    let cond = text
        .strip_suffix(')')
        .ok_or_else(|| unknown(&first))?
        .to_string();
    Ok(Some((first, cond)))
}

/// The body `{ ... return true; }` of a branch: its tags.
fn branch_body(c: &mut Cursor) -> Result<Vec<Tag4>, String> {
    c.expect("{")?;
    let l = c.next()?;
    let tags = if let Some(rest) = l.text.strip_prefix("tags[0] = ") {
        let t = hb_tag_then(rest, ";").ok_or_else(|| unknown(&l))?;
        c.expect("*count = 1;")?;
        vec![t]
    } else if l.text == "unsigned int i;" {
        c.expect("hb_tag_t possible_tags[] = {")?;
        let mut tags = Vec::new();
        while !c.accept("};") {
            let l = c.next()?;
            tags.push(hb_tag_then(&l.text, ",").ok_or_else(|| unknown(&l))?);
        }
        let l = c.next()?;
        let want = format!("for (i = 0; i < {} && i < *count; i++)", tags.len());
        if l.text != want || tags.is_empty() {
            return Err(unknown(&l));
        }
        c.expect("tags[i] = possible_tags[i];")?;
        c.expect("*count = i;")?;
        tags
    } else {
        return Err(unknown(&l));
    };
    c.expect("return true;")?;
    c.expect("}")?;
    Ok(tags)
}

const COMPLEX_HEAD: [&str; 10] = [
    "static inline bool",
    "hb_ot_tags_from_complex_language (const char *lang_str,",
    "const char *limit,",
    "unsigned int *count ,",
    "hb_tag_t *tags )",
    "{",
    "if (limit - lang_str >= 7)",
    "{",
    "const char *p = strchr (lang_str, '-');",
    "if (!p || p >= limit || limit - p < 5) goto out;",
];

fn complex_function(c: &mut Cursor, t: &mut Table) -> Result<(), String> {
    COMPLEX_HEAD.iter().try_for_each(|w| c.expect(w))?;
    while let Some((l, cond)) = if_line(c)? {
        let s = subtag_cond(&cond, "p").ok_or_else(|| unknown(&l))?;
        let tags = branch_body(c)?;
        t.complex_subtags.push((s, tags));
    }
    c.expect("}")?;
    c.expect("out:")?;
    c.expect("switch (lang_str[0])")?;
    c.expect("{")?;
    while !c.accept("}") {
        let l = c.next()?;
        let first = l
            .text
            .strip_prefix("case ")
            .and_then(|r| r.strip_suffix(':'))
            .and_then(char_literal)
            .filter(|(_, rest)| rest.is_empty())
            .map(|(ch, _)| ch)
            .ok_or_else(|| unknown(&l))?;
        while let Some((l, cond)) = if_line(c)? {
            let cond = switch_cond(&cond).ok_or_else(|| unknown(&l))?;
            let tags = branch_body(c)?;
            t.complex.push(Branch { first, cond, tags });
        }
        c.expect("break;")?;
    }
    c.expect("return false;")?;
    c.expect("}")
}

fn ambiguous_function(c: &mut Cursor, t: &mut Table) -> Result<(), String> {
    c.expect("static inline hb_language_t")?;
    c.expect("hb_ot_ambiguous_tag_to_language (hb_tag_t tag)")?;
    c.expect("{")?;
    c.expect("switch (tag)")?;
    c.expect("{")?;
    while !c.accept("default:") {
        let l = c.next()?;
        let tag = l
            .text
            .strip_prefix("case ")
            .and_then(|r| hb_tag_then(r, ":"))
            .ok_or_else(|| unknown(&l))?;
        let l = c.next()?;
        let lang = l
            .text
            .strip_prefix("return ")
            .and_then(|r| r.strip_suffix(';'))
            .and_then(call)
            .and_then(|(name, a)| match (name, &a[..]) {
                ("hb_language_from_string", [s, n]) if n == "-1" => string_literal(s),
                _ => None,
            })
            .ok_or_else(|| unknown(&l))?;
        t.ambiguous.push((tag, lang));
    }
    c.expect("return HB_LANGUAGE_INVALID;")?;
    c.expect("}")?;
    c.expect("}")
}

/// The entries of an array up to `};`.
fn array<T>(c: &mut Cursor, entry: impl Fn(&str) -> Option<T>) -> Result<Vec<T>, String> {
    let mut v = Vec::new();
    while !c.accept("};") {
        let l = c.next()?;
        v.push(entry(&l.text).ok_or_else(|| unknown(&l))?);
    }
    Ok(v)
}

/// `{HB_TAG(...), HB_TAG(...)},`
fn lang_tag(s: &str) -> Option<(Tag4, Tag4)> {
    let rest = s.strip_prefix('{')?;
    let (a, rest) = hb_tag(rest)?;
    let rest = rest.trim_start().strip_prefix(',')?.trim_start();
    let b = hb_tag_then(rest, "},")?;
    Some((a, b))
}

/// `{HB_TAG(...), <offset>, <count>},`
fn lang_tag_range(s: &str) -> Option<(Tag4, u16, u8)> {
    let rest = s.strip_prefix('{')?;
    let (a, rest) = hb_tag(rest)?;
    let rest = rest.trim_start().strip_prefix(',')?.strip_suffix("},")?;
    let (off, count) = rest.split_once(',')?;
    Some((a, off.trim().parse().ok()?, count.trim().parse().ok()?))
}

fn check_sorted<T: Ord>(name: &str, v: &[T], strict: bool) -> Result<(), String> {
    v.windows(2)
        .position(|w| if strict { w[0] >= w[1] } else { w[0] > w[1] })
        .map_or(Ok(()), |i| {
            Err(format!("{name} is not sorted at index {}", i + 1))
        })
}

/// Parse the whole header.
pub fn parse(text: &str) -> Result<Table, String> {
    let mut c = Cursor {
        lines: code_lines(text)?,
        pos: 0,
    };
    let mut t = Table::default();
    c.expect("#ifndef HB_OT_TAG_TABLE_HH")?;
    c.expect("#define HB_OT_TAG_TABLE_HH")?;
    c.expect("static const LangTag ot_languages2[] = {")?;
    t.languages2 = array(&mut c, lang_tag)?;
    c.expect("#ifndef HB_NO_LANGUAGE_LONG")?;
    c.expect("static const hb_tag_t ot_languages3_blocked[] = {")?;
    t.languages3_blocked = array(&mut c, |s| hb_tag_then(s, ","))?;
    c.expect("static const LangTag ot_languages3[] = {")?;
    t.languages3 = array(&mut c, lang_tag)?;
    c.expect("static const hb_tag_t ot_languages3_multi_values[] = {")?;
    t.multi_values = array(&mut c, |s| hb_tag_then(s, ","))?;
    c.expect("static const LangTagRange ot_languages3_multi[] = {")?;
    t.multi = array(&mut c, lang_tag_range)?;
    c.expect("#endif")?;
    complex_function(&mut c, &mut t)?;
    ambiguous_function(&mut c, &mut t)?;
    c.expect("#endif")?;
    if let Some(l) = c.peek() {
        return Err(unknown(l));
    }
    check(&t)?;
    Ok(t)
}

/// The invariants the lookups rely on.
fn check(t: &Table) -> Result<(), String> {
    let langs = |v: &[(Tag4, Tag4)]| v.iter().map(|x| x.0).collect::<Vec<_>>();
    // ot_languages2 lists one language several times for several tags.
    check_sorted("ot_languages2", &langs(&t.languages2), false)?;
    check_sorted("ot_languages3", &langs(&t.languages3), true)?;
    check_sorted("ot_languages3_blocked", &t.languages3_blocked, true)?;
    let multi: Vec<Tag4> = t.multi.iter().map(|x| x.0).collect();
    check_sorted("ot_languages3_multi", &multi, true)?;
    t.multi.iter().try_for_each(|&(l, off, n)| {
        let end = usize::from(off) + usize::from(n);
        if n == 0 || end > t.multi_values.len() {
            Err(format!(
                "ot_languages3_multi: {} is out of range",
                String::from_utf8_lossy(&l)
            ))
        } else {
            Ok(())
        }
    })
}

fn tag_lit(t: &Tag4) -> String {
    format!("t(b\"{}\")", String::from_utf8_lossy(t))
}

fn tags_lit(v: &[Tag4]) -> String {
    let inner: Vec<String> = v.iter().map(tag_lit).collect();
    format!("&[{}]", inner.join(", "))
}

/// The Rust file. `header` lines become `//` comments at the top.
pub fn emit(t: &Table, header: &[String]) -> String {
    let mut o = String::new();
    header.iter().for_each(|l| {
        let sep = if l.is_empty() { "" } else { " " };
        writeln!(o, "//{sep}{l}").unwrap();
    });
    writeln!(o).unwrap();
    writeln!(o, "use crate::tag::{{Branch, Cond, t}};").unwrap();

    let pairs = |o: &mut String, name: &str, doc: &str, v: &[(Tag4, Tag4)]| {
        writeln!(o).unwrap();
        writeln!(o, "/// {doc}").unwrap();
        writeln!(o, "pub(crate) const {name}: [(u32, u32); {}] = [", v.len()).unwrap();
        v.iter().for_each(|(a, b)| {
            writeln!(o, "    ({}, {}),", tag_lit(a), tag_lit(b)).unwrap();
        });
        writeln!(o, "];").unwrap();
    };
    let list = |o: &mut String, name: &str, doc: &str, v: &[Tag4]| {
        writeln!(o).unwrap();
        writeln!(o, "/// {doc}").unwrap();
        writeln!(o, "pub(crate) const {name}: [u32; {}] = [", v.len()).unwrap();
        v.chunks(6).for_each(|c| {
            let cells: Vec<String> = c.iter().map(|x| format!("{},", tag_lit(x))).collect();
            writeln!(o, "    {}", cells.join(" ")).unwrap();
        });
        writeln!(o, "];").unwrap();
    };

    pairs(
        &mut o,
        "LANGUAGES2",
        "`ot_languages2`: (two-letter language, tag), sorted by language.",
        &t.languages2,
    );
    list(
        &mut o,
        "LANGUAGES3_BLOCKED",
        "`ot_languages3_blocked`: three-letter languages that have no tag, sorted.",
        &t.languages3_blocked,
    );
    pairs(
        &mut o,
        "LANGUAGES3",
        "`ot_languages3`: (three-letter language, tag), sorted by language.",
        &t.languages3,
    );
    list(
        &mut o,
        "LANGUAGES3_MULTI_VALUES",
        "`ot_languages3_multi_values`: the tags of [`LANGUAGES3_MULTI`].",
        &t.multi_values,
    );

    writeln!(o).unwrap();
    writeln!(
        o,
        "/// `ot_languages3_multi`: (three-letter language, offset in\n/// [`LANGUAGES3_MULTI_VALUES`], count), sorted by language."
    )
    .unwrap();
    writeln!(
        o,
        "pub(crate) const LANGUAGES3_MULTI: [(u32, u16, u8); {}] = [",
        t.multi.len()
    )
    .unwrap();
    t.multi.iter().for_each(|(l, off, n)| {
        writeln!(o, "    ({}, {off}, {n}),", tag_lit(l)).unwrap();
    });
    writeln!(o, "];").unwrap();

    writeln!(o).unwrap();
    writeln!(
        o,
        "/// The `subtag_matches (p, limit, ...)` branches of\n/// `hb_ot_tags_from_complex_language`, in order: (subtag, tags)."
    )
    .unwrap();
    writeln!(
        o,
        "pub(crate) const COMPLEX_SUBTAGS: [(&[u8], &[u32]); {}] = [",
        t.complex_subtags.len()
    )
    .unwrap();
    t.complex_subtags.iter().for_each(|(s, tags)| {
        writeln!(o, "    (b\"{s}\", {}),", tags_lit(tags)).unwrap();
    });
    writeln!(o, "];").unwrap();

    writeln!(o).unwrap();
    writeln!(
        o,
        "/// The branches of the `switch (lang_str[0])` of\n/// `hb_ot_tags_from_complex_language`, in order."
    )
    .unwrap();
    writeln!(
        o,
        "pub(crate) const COMPLEX: [Branch; {}] = [",
        t.complex.len()
    )
    .unwrap();
    t.complex.iter().for_each(|b| {
        let cond = match &b.cond {
            Cond::LangMatches(s) => format!("Cond::LangMatches(b\"{s}\")"),
            Cond::PrefixSubtag(p, s) => format!("Cond::PrefixSubtag(b\"{p}\", b\"{s}\")"),
            Cond::Exact(s) => format!("Cond::Exact(b\"{s}\")"),
        };
        writeln!(
            o,
            "    Branch {{ first: b'{}', cond: {cond}, tags: {} }},",
            char::from(b.first),
            tags_lit(&b.tags)
        )
        .unwrap();
    });
    writeln!(o, "];").unwrap();

    writeln!(o).unwrap();
    writeln!(
        o,
        "/// `hb_ot_ambiguous_tag_to_language`: (tag, language), in order."
    )
    .unwrap();
    writeln!(
        o,
        "pub(crate) const AMBIGUOUS: [(u32, &[u8]); {}] = [",
        t.ambiguous.len()
    )
    .unwrap();
    t.ambiguous.iter().for_each(|(tag, l)| {
        writeln!(o, "    ({}, b\"{l}\"),", tag_lit(tag)).unwrap();
    });
    writeln!(o, "];").unwrap();
    o
}

/// One `<name> <url> <commit>` line of `testdata.lock`.
pub fn lock_commit(text: &str, name: &str) -> Result<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .find_map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [n, _, commit] if n == name => Some(commit.to_string()),
            _ => None,
        })
        .ok_or(format!("{name} is not in testdata.lock"))
}

#[cfg(test)]
mod tests;
