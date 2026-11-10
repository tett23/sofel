//! A small measuring harness (ADR 0006, ADR 0013): batches of calls that
//! last at least a minimum time, warm-up batches, and the median and
//! median absolute deviation of the per-call time.

use std::fmt::Write;
use std::time::Instant;

/// A monotonic clock in nanoseconds.
pub trait Clock {
    fn now_ns(&mut self) -> u64;
}

/// The real clock.
pub struct SystemClock(Instant);

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now_ns(&mut self) -> u64 {
        u64::try_from(self.0.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// A batch lasts at least this long.
    pub min_batch_ns: u64,
    /// Batches run and thrown away first.
    pub warmup: usize,
    /// Batches measured.
    pub samples: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            min_batch_ns: 10_000_000,
            warmup: 3,
            samples: 30,
        }
    }
}

/// Per-call time.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Stats {
    pub median_ns: f64,
    pub mad_ns: f64,
}

pub fn median(xs: &[f64]) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// The median absolute deviation from the median.
pub fn mad(xs: &[f64]) -> f64 {
    let m = median(xs);
    let d: Vec<f64> = xs.iter().map(|x| (x - m).abs()).collect();
    median(&d)
}

fn run_batch(clock: &mut dyn Clock, body: &mut dyn FnMut() -> u64, n: u64) -> u64 {
    let start = clock.now_ns();
    for _ in 0..n {
        std::hint::black_box(body());
    }
    clock.now_ns().saturating_sub(start)
}

/// The smallest power of two of calls whose batch lasts `min_ns`.
pub fn batch_size(clock: &mut dyn Clock, body: &mut dyn FnMut() -> u64, min_ns: u64) -> u64 {
    let mut n = 1u64;
    loop {
        if run_batch(clock, body, n) >= min_ns || n >= 1 << 30 {
            return n;
        }
        n *= 2;
    }
}

pub fn measure(clock: &mut dyn Clock, body: &mut dyn FnMut() -> u64, config: &Config) -> Stats {
    let n = batch_size(clock, body, config.min_batch_ns);
    for _ in 0..config.warmup {
        run_batch(clock, body, n);
    }
    let times: Vec<f64> = (0..config.samples)
        .map(|_| run_batch(clock, body, n) as f64 / n as f64)
        .collect();
    Stats {
        median_ns: median(&times),
        mad_ns: mad(&times),
    }
}

/// One line of the result table.
#[derive(Clone, Debug)]
pub struct Row {
    pub item: String,
    pub font: String,
    pub sofel: Option<Stats>,
    pub freetype: Option<Stats>,
    pub harfbuzz: Option<Stats>,
}

fn us(s: &Option<Stats>) -> String {
    s.map_or("—".into(), |s| {
        format!("{:.1} ±{:.1}", s.median_ns / 1000.0, s.mad_ns / 1000.0)
    })
}

fn ratio(a: &Option<Stats>, b: &Option<Stats>) -> String {
    match (a, b) {
        (Some(a), Some(b)) if b.median_ns > 0.0 => format!("{:.2}", a.median_ns / b.median_ns),
        _ => "—".into(),
    }
}

/// The rows as a Markdown table, times in microseconds per call.
pub fn table(rows: &[Row]) -> String {
    let mut out = String::from(
        "| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |\n\
         |---|---|---:|---:|---:|---:|---:|\n",
    );
    for r in rows {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} |",
            r.item,
            r.font,
            us(&r.sofel),
            us(&r.freetype),
            ratio(&r.sofel, &r.freetype),
            us(&r.harfbuzz),
            ratio(&r.sofel, &r.harfbuzz)
        );
    }
    out
}

/// Whether sofel gave the same result as a counterpart for an item.
#[derive(Clone, Debug)]
pub struct Check {
    pub item: String,
    pub font: String,
    pub ok: bool,
    pub detail: String,
}

/// `Err` with a report of the failed checks, if any.
pub fn report_checks(checks: &[Check]) -> Result<(), String> {
    let failed: Vec<String> = checks
        .iter()
        .filter(|c| !c.ok)
        .map(|c| format!("{} / {}: {}", c.item, c.font, c.detail))
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("results differ:\n  {}", failed.join("\n  ")))
    }
}
