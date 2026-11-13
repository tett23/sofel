//! Generate the fixed font gallery from sofel-render and sofel-shape.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use font_gallery::inputs::{self, Sample, Status};
use font_gallery::pixels::{self, Glyph, Image};
use font_gallery::sheet;

struct Labels {
    shape: PathBuf,
    render: PathBuf,
    path: PathBuf,
    glyphs: HashMap<u32, Image>,
    labels: HashMap<String, Image>,
}

impl Labels {
    fn new(binaries: &Path, path: PathBuf) -> Self {
        Self {
            shape: binaries.join("sofel-shape"),
            render: binaries.join("sofel-render"),
            path,
            glyphs: HashMap::new(),
            labels: HashMap::new(),
        }
    }

    fn image(&mut self, text: &str) -> Result<Image, String> {
        if let Some(image) = self.labels.get(text) {
            return Ok(image.clone());
        }
        let out = Command::new(&self.shape)
            .args([
                "-O",
                "json",
                "--no-glyph-names",
                "--language=en",
                "--font-size=1024",
                "--font-ppem=16",
            ])
            .arg(&self.path)
            .arg(text)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }
        let records = inputs::shape_records(&out.stdout)?;
        let chars: Vec<_> = text.chars().collect();
        let mut glyphs = Vec::new();
        for record in records {
            if record.g == 0 {
                return Err(format!("label contains a missing glyph: {text}"));
            }
            let image = if chars.get(record.cl) == Some(&' ') {
                None
            } else {
                if !self.glyphs.contains_key(&record.g) {
                    let out = Command::new(&self.render)
                        .args(["--ppem", "16", "--glyph", &record.g.to_string()])
                        .arg(&self.path)
                        .output()
                        .map_err(|e| e.to_string())?;
                    if !out.status.success() {
                        return Err(String::from_utf8_lossy(&out.stderr).to_string());
                    }
                    self.glyphs.insert(record.g, pixels::parse(&out.stdout)?);
                }
                self.glyphs.get(&record.g).cloned()
            };
            glyphs.push(Glyph {
                image,
                dx: record.dx,
                dy: record.dy,
                ax: record.ax,
                ay: record.ay,
            });
        }
        let image = pixels::compose(&glyphs)?.ok_or("empty label")?;
        self.labels.insert(text.to_string(), image.clone());
        Ok(image)
    }
}

fn input_label(text: &str) -> String {
    if text.is_ascii() {
        text.to_string()
    } else {
        text.chars()
            .map(|c| format!("{:04X}", c as u32))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn escape_html(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn page(rows: &[[String; 5]]) -> String {
    let table = rows
        .iter()
        .map(|row| {
            format!(
                "<tr>{}</tr>",
                row.iter()
                    .map(|s| format!("<td>{}</td>", escape_html(s)))
                    .collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<!doctype html><html lang=\"ja\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\">\n\
<title>sofel フォント描画見本</title><style>body{{font:1rem system-ui;margin:2rem}}img{{max-width:100%;height:auto}}td,th{{text-align:start;padding:.35rem .6rem;border-bottom:1px solid #ddd}}table{{border-collapse:collapse}}</style>\n\
<h1>フォント描画見本</h1><p>左は単字、右は短い文字列です。画像を開くと等倍で確認できます。</p>\n\
<a href=\"all-samples.png\"><img src=\"all-samples.png\" alt=\"全フェイスの単字と短い文字列\"></a>\n\
<table><thead><tr><th>フォント / フェイス</th><th>単字</th><th>文字列</th><th>ppem</th><th>結果</th></tr></thead><tbody>{table}</tbody></table></html>\n"
    )
}

fn sample_card(
    sample: &Sample,
    binaries: &Path,
    labels: &mut Labels,
) -> Result<(Image, [String; 5]), String> {
    let rendered = inputs::render_case(
        sample,
        &binaries.join("sofel-render"),
        &binaries.join("sofel-shape"),
    )?;
    let run = pixels::compose(&rendered.glyphs)?;
    let heading = format!("{} / face {}", sample.id, sample.face);
    let single = format!("{} {}", sample.solo.kind(), sample.solo.value());
    let info = format!("{} | {}", single, input_label(sample.text));
    let (reason, note) = match rendered.status {
        Status::Empty => (
            Some("空のグリフ（単字19、文字列26）"),
            Some("Unsupported: glyphs 19 and 26 are empty"),
        ),
        Status::Grayscale => (
            Some("カラー未対応（グレーの輪郭を表示）"),
            Some("Unsupported: color (grayscale outline shown)"),
        ),
        Status::Drawn => (None, None),
    };
    let mut captions = vec![
        (heading.clone(), 9),
        (info, 170),
        (format!("{} ppem", sample.ppem), 189),
    ];
    if let Some(note) = note {
        captions.push((note.into(), 208));
    }
    let mut images = Vec::new();
    for (line, y) in captions {
        images.push((labels.image(&line)?, sheet::PADDING, y));
    }
    let card = sheet::card(rendered.solo.as_ref(), run.as_ref(), &images)?;
    let status = reason.map_or_else(|| "描画済み".to_string(), |why| format!("未対応: {why}"));
    Ok((
        card,
        [
            heading,
            single,
            sample.text.into(),
            sample.ppem.to_string(),
            status,
        ],
    ))
}

fn generate(output: &Path, binaries: &Path) -> Result<(), String> {
    if !output.is_dir() || fs::metadata(output).is_ok_and(|m| m.permissions().readonly()) {
        return Err("gallery output directory is not writable".into());
    }
    let repo = std::env::current_dir().map_err(|e| e.to_string())?;
    let samples = inputs::gallery_cases(&repo)?;
    let label_font = samples
        .iter()
        .find(|s| s.id == "source-serif")
        .ok_or("no source-serif sample")?
        .path
        .clone();
    let mut labels = Labels::new(binaries, label_font);
    let mut cards = Vec::new();
    let mut rows = Vec::new();
    for sample in &samples {
        let (card, row) = sample_card(sample, binaries, &mut labels)
            .map_err(|e| format!("{} face {}: {e}", sample.id, sample.face))?;
        cards.push(card);
        rows.push(row);
    }
    let png = pixels::png(&sheet::layout(&cards)?);
    fs::write(output.join("all-samples.png"), png).map_err(|e| e.to_string())?;
    fs::write(output.join("index.html"), page(&rows)).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: font-gallery OUTPUT-DIR BINARIES-DIR");
        return ExitCode::FAILURE;
    }
    match generate(Path::new(&args[0]), Path::new(&args[1])) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("font-gallery: {error}");
            ExitCode::FAILURE
        }
    }
}
