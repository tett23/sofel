//! What the shaping comparisons share (ADR 0020, 適合テスト): the
//! categories of expected failures and the checks that a category fits
//! a failing case, and the judging of a list.
//!
//! The categories are those of ADR 0020: `shaper:<name>` (the shaper of
//! ADR 0024 HarfBuzz's plan picks), `variations`, `aat`, `synthetic`,
//! `cff`, `color`, `out-of-scope:<name>`; and, for the cases in ADR 0044, `dfont`
//! and `sanitize` (see conformance/expected-failures.txt).

use std::collections::BTreeMap;

use sofel_font::Font;
use sofel_font::read::{Severity, Tag, gpos, gsub, sfnt::Sfnt};
use sofel_shape::{SegmentProperties, ShapePlan, Shaper, UnicodeBuffer};

use crate::expect;

/// The shapers of ADR 0024.
pub const COMPLEX: [&str; 8] = [
    "arabic", "hebrew", "hangul", "thai", "indic", "khmer", "myanmar", "use",
];

/// The category name of a shaper (`myanmar` for both Myanmar shapers).
pub fn shaper_name(s: Shaper) -> &'static str {
    match s {
        Shaper::Default => "default",
        Shaper::Dumber => "dumber",
        Shaper::Arabic => "arabic",
        Shaper::Hangul => "hangul",
        Shaper::Hebrew => "hebrew",
        Shaper::Indic => "indic",
        Shaper::Khmer => "khmer",
        Shaper::Myanmar | Shaper::MyanmarZawgyi => "myanmar",
        Shaper::Thai => "thai",
        Shaper::Use => "use",
    }
}

/// An option's value in an hb-shape command line, `--name=value` or
/// `--name value`.
pub fn option<'a>(options: &'a [String], name: &str) -> Option<&'a str> {
    let mut it = options.iter();
    while let Some(o) = it.next() {
        if o == name {
            return it.next().map(String::as_str);
        }
        if let Some(v) = o.strip_prefix(name).and_then(|v| v.strip_prefix('=')) {
            return Some(v);
        }
    }
    None
}

/// What a failing case is shaped with, for checking its category.
pub struct Subject<'a> {
    /// The font file.
    pub data: &'a [u8],
    /// The face index.
    pub face: u32,
    /// The hb-shape options of the case (for `variations`, `synthetic`).
    pub options: &'a [String],
    /// The shaper the case runs with (`ot`, `fallback`, or another
    /// shaper's name).
    pub shaper: &'a str,
    /// The characters.
    pub text: &'a [u32],
    /// The segment properties given; the rest is guessed.
    pub props: SegmentProperties,
}

impl Subject<'_> {
    /// The shaper HarfBuzz's plan picks (`hb_ot_shaper_categorize`).
    pub fn plan_shaper(&self) -> Option<&'static str> {
        let font = Font::new(self.data, self.face).ok()?;
        let mut buffer = UnicodeBuffer::new();
        for (&u, i) in self.text.iter().zip(0u32..) {
            buffer.push(u, i);
        }
        let props = self.props.clone().guess(&buffer, None);
        Some(shaper_name(
            ShapePlan::new(font.default_instance(), props, &[]).shaper(),
        ))
    }

    fn has_table(&self, tag: &[u8; 4]) -> bool {
        if let Ok(f) = Font::new(self.data, self.face) {
            return f.table(Tag(*tag)).is_some();
        }
        // The table directory of fonts sofel cannot open (no `head`).
        Sfnt::new(self.data).is_ok_and(|s| s.table(Tag(*tag)).is_some())
    }

    fn has_option(&self, names: &[&str]) -> bool {
        names.iter().any(|n| option(self.options, n).is_some())
    }

    /// Why `category` does not fit the case, or `None` when it does.
    pub fn misfit(&self, category: &str) -> Option<String> {
        let has = |t: &[u8; 4]| self.has_table(t);
        let fits = if let Some(name) = category.strip_prefix("out-of-scope:") {
            name == self.shaper && !["ot", "fallback"].contains(&name)
        } else if let Some(name) = category.strip_prefix("shaper:") {
            let actual = self.plan_shaper();
            if actual != Some(name) || !COMPLEX.contains(&name) || self.shaper != "ot" {
                return Some(format!("the plan's shaper is {actual:?}"));
            }
            true
        } else {
            match category {
                "variations" => {
                    self.has_option(&["--variations", "--named-instance"]) || has(b"fvar")
                }
                "aat" => ["morx", "mort", "kerx", "kern", "trak", "ankr"]
                    .iter()
                    .any(|t| has(&tag_bytes(t))),
                "synthetic" => self.has_option(&["--font-bold", "--font-slant", "--font-grade"]),
                "cff" => has(b"CFF ") || has(b"CFF2"),
                "color" => has(b"COLR") || has(b"CBDT") || has(b"sbix") || has(b"SVG "),
                "dfont" => self.data.starts_with(&[0, 0, 1, 0]),
                "sanitize" => Font::new(self.data, self.face).is_ok_and(|f| layout_errors(&f)),
                _ => return Some("unknown category".into()),
            }
        };
        (!fits).then(|| "the font or the options do not have what the category names".into())
    }

    /// A category to start from for an unlisted failure (a developer
    /// aid); `?` marks guesses to check by hand.
    pub fn suggest(&self) -> String {
        if !["ot", "fallback"].contains(&self.shaper) {
            return format!("out-of-scope:{}", self.shaper);
        }
        if self.misfit("synthetic").is_none() {
            return "synthetic".into();
        }
        if self.has_option(&["--variations", "--named-instance"]) {
            return "variations".into();
        }
        if Font::new(self.data, self.face).is_err() {
            return "UNKNOWN-badfont".into();
        }
        if let Some(s) = self.plan_shaper()
            && COMPLEX.contains(&s)
            && self.shaper == "ot"
        {
            return format!("shaper:{s}");
        }
        for (c, guess) in [
            ("aat", "aat?"),
            ("variations", "variations?"),
            ("color", "color?"),
            ("cff", "cff?"),
        ] {
            if self.misfit(c).is_none() {
                return guess.into();
            }
        }
        "UNKNOWN".into()
    }
}

fn tag_bytes(s: &str) -> [u8; 4] {
    let mut t = [b' '; 4];
    for (d, c) in t.iter_mut().zip(s.bytes()) {
        *d = c;
    }
    t
}

/// Whether sofel's validator reports an error in `GSUB` or `GPOS`.
fn layout_errors(font: &Font<'_>) -> bool {
    let mut error = false;
    let mut seen = |d: sofel_font::read::Diagnostic| error |= d.severity == Severity::Error;
    if let Some(t) = font.table(gsub::TAG) {
        gsub::validate(t, None, &mut seen);
    }
    if let Some(t) = font.table(gpos::TAG) {
        gpos::validate(t, None, &mut seen);
    }
    error
}

/// Judges the results of a comparison, `(item, passed)` for every case,
/// against the list `list` with its categories. Several cases may make
/// one item, which passes when all of them pass; the category of a
/// listed item must fit every failing case (`subject` gives the case of
/// a result by its index). Returns a report (empty when all is well)
/// and the number of items by category. With `suggest`, prints a
/// suggested category for each unlisted failing item.
pub fn judge<'a>(
    results: &[(String, bool)],
    subject: &dyn Fn(usize) -> Option<Subject<'a>>,
    list: &str,
    suggest: bool,
) -> (String, BTreeMap<String, usize>) {
    let reasons = expect::parse_reasons(
        &std::fs::read_to_string(crate::paths::conformance_file(list)).unwrap_or_default(),
    );
    let expected = reasons.keys().cloned().collect();
    let mut items: BTreeMap<&str, bool> = BTreeMap::new();
    let mut report = String::new();
    let mut suggested = std::collections::BTreeSet::new();
    for (i, (id, passed)) in results.iter().enumerate() {
        let all = items.entry(id.as_str()).or_insert(true);
        *all &= *passed;
        if *passed {
            continue;
        }
        match reasons.get(id) {
            Some(cat) => {
                if let Some(s) = subject(i)
                    && let Some(why) = s.misfit(cat)
                {
                    report.push_str(&format!("category does not fit: {id} # {cat}: {why}\n"));
                }
            }
            None if suggest && suggested.insert(id.as_str()) => {
                if let Some(s) = subject(i) {
                    println!("SUGGEST {id}  # {}", s.suggest());
                }
            }
            None => {}
        }
    }
    let mut by_category: BTreeMap<String, usize> = BTreeMap::new();
    for (id, passed) in &items {
        if let (false, Some(cat)) = (passed, reasons.get(*id)) {
            *by_category.entry(cat.clone()).or_default() += 1;
        }
    }
    let problems = expect::judge(items.iter().map(|(i, p)| (*i, *p)), &expected);
    report.push_str(&expect::report(&problems, list));
    for (id, cat) in &reasons {
        if cat.split_whitespace().count() != 1 {
            report.push_str(&format!(
                "{list}: {id} needs exactly one category, has `{cat}`\n"
            ));
        }
    }
    (report, by_category)
}
