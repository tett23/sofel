//! Unfixed-axis outline and advance preservation (root ADR 0022).
#![allow(clippy::unwrap_used)]
use sofel_conformance::paths;
use sofel_font::{Font, var::Location};
use sofel_geom::{Command, Pen, Point};
use sofel_outline::{Outlines, Scale, Scratch};
use sofel_read::{GlyphId, Tag, sfnt::Collection};
use sofel_subset::{Flags, Input, Plan, RangeSet, SetKind, SubsetError};

#[derive(Default)]
struct Record(Vec<Command>);
impl Pen for Record {
    fn move_to(&mut self, p: Point) {
        self.0.push(Command::MoveTo(p));
    }
    fn line_to(&mut self, p: Point) {
        self.0.push(Command::LineTo(p));
    }
    fn quad_to(&mut self, c: Point, p: Point) {
        self.0.push(Command::QuadTo(c, p));
    }
    fn cubic_to(&mut self, a: Point, b: Point, p: Point) {
        self.0.push(Command::CubicTo(a, b, p));
    }
    fn close(&mut self) {
        self.0.push(Command::Close);
    }
}
fn outline(font: &Font<'_>, values: &[f32], gid: u32) -> Result<Vec<Command>, String> {
    let location = if values.is_empty() {
        Location::new(font, None, &[])
    } else {
        Location::from_design(font, values)
    };
    let outlines = Outlines::new(font.instance(&location));
    let mut scratch = Scratch::default();
    let mut pen = Record::default();
    outlines
        .draw(GlyphId(gid), Scale::Unscaled, &mut scratch, &mut pen)
        .map(|_| pen.0)
        .map_err(|e| format!("{e:?}"))
}

fn assert_harfbuzz_rejects(path: &std::path::Path, glyph_count: u32, reduced: bool) {
    let selected = if reduced {
        (0..glyph_count)
            .step_by(2)
            .map(|gid| gid.to_string())
            .collect::<Vec<_>>()
            .join(",")
    } else {
        format!("0-{}", glyph_count.saturating_sub(1))
    };
    let output = std::env::temp_dir().join(format!(
        "sofel-0022-rejected-{}-{}.ttf",
        std::process::id(),
        reduced
    ));
    let _ = std::fs::remove_file(&output);
    let result = std::process::Command::new(sofel_conformance::paths::hb_dump())
        .arg("--subset")
        .arg(format!("--font-file={}", path.display()))
        .arg(format!("--output-file={}", output.display()))
        .arg("--notdef-outline")
        .arg(format!("--gids={selected}"))
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(1),
        "HarfBuzz did not reject {} with reduced={reduced}: {}",
        path.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("subsetting failed"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}

#[test]
fn adr0022_unfixed_outlines_and_advances() {
    let dir = paths::testdata().join("harfbuzz/test/subset/data/fonts");
    paths::require(&dir, "HarfBuzz subset fonts").unwrap();
    paths::require(&paths::hb_dump(), "pinned HarfBuzz subset reference").unwrap();
    let mut files = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    files.sort();
    let (mut faces, mut outputs, mut glyphs) = (0, 0, 0);
    let mut failures = Vec::new();
    let mut rejected = 0;
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let count = Collection::new(&bytes).map_or(1, |c| c.num_fonts());
        for face in 0..count {
            let Ok(source) = Font::new(&bytes, face) else {
                continue;
            };
            faces += 1;
            let axes = source.var_axes().collect::<Vec<_>>();
            let mut locations = vec![Vec::new()];
            if !axes.is_empty() {
                locations.push(axes.iter().map(|a| a.min).collect());
                locations.push(axes.iter().map(|a| a.max).collect());
            }
            for reduced in [false, true] {
                let selected = if reduced {
                    RangeSet::from_values((0..source.num_glyphs()).step_by(2))
                } else {
                    RangeSet::all()
                };
                let input = Input::new()
                    .with_set(SetKind::Glyphs, selected)
                    .with_flags(Flags::NOTDEF_OUTLINE);
                let result = Plan::new(&source, &input)
                    .and_then(|p| p.execute(&source).map(|bytes| (p, bytes)));
                let (plan, subset) = match result {
                    Ok(result) => result,
                    // Fixed HarfBuzz 15.1 rejects these same inputs: the
                    // reduced AdobeBlank selection and both selections of
                    // NotoColrEmojiGlyf-Regular.subset.ttf have no output.
                    // They have no output whose outline could be compared.
                    Err(SubsetError::Limit { tag })
                        if path.file_name().unwrap() == "AdobeBlank-Regular.ttf"
                            && reduced
                            && tag == Tag::new(b"cmap") =>
                    {
                        assert_harfbuzz_rejects(&path, source.num_glyphs(), reduced);
                        rejected += 1;
                        continue;
                    }
                    Err(SubsetError::Malformed { tag })
                        if path.file_name().unwrap() == "NotoColrEmojiGlyf-Regular.subset.ttf"
                            && tag == Tag::new(b"COLR") =>
                    {
                        assert_harfbuzz_rejects(&path, source.num_glyphs(), reduced);
                        rejected += 1;
                        continue;
                    }
                    Err(error) => {
                        failures.push(format!(
                            "{} face {face} reduced {reduced}: subset {error}",
                            path.display()
                        ));
                        continue;
                    }
                };
                let subset = Font::new(&subset, 0).unwrap();
                outputs += 1;
                for values in &locations {
                    let original_location = if values.is_empty() {
                        Location::new(&source, None, &[])
                    } else {
                        Location::from_design(&source, values)
                    };
                    let subset_location = if values.is_empty() {
                        Location::new(&subset, None, &[])
                    } else {
                        Location::from_design(&subset, values)
                    };
                    for &(new, old) in plan.new_to_old_glyphs() {
                        glyphs += 1;
                        let original = outline(&source, values, old);
                        let actual = outline(&subset, values, new);
                        if actual != original {
                            failures.push(format!("{} face {face} reduced {reduced} location {values:?} glyph {old}->{new}: outline", path.display()));
                        }
                        if source.instance(&original_location).advance(GlyphId(old))
                            != subset.instance(&subset_location).advance(GlyphId(new))
                        {
                            failures.push(format!("{} face {face} reduced {reduced} location {values:?} glyph {old}->{new}: advance", path.display()));
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "unfixed subset: {faces} faces, {outputs} outputs, {glyphs} glyph comparisons, {rejected} matching HarfBuzz rejections, {} failures",
        failures.len()
    );
    for failure in failures.iter().take(40) {
        eprintln!("{failure}");
    }
    assert_eq!(
        rejected, 3,
        "the fixed reference rejects exactly these three input selections"
    );
    assert!(faces > 0 && outputs > 0 && glyphs > 0);
    assert!(
        failures.is_empty(),
        "{} preservation failures",
        failures.len()
    );
}
