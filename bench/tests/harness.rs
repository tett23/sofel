use std::cell::Cell;
use std::rc::Rc;

use sofel_bench::harness::{
    Check, Clock, Config, Row, Stats, batch_size, mad, measure, median, report_checks, table,
};

#[derive(Clone)]
struct FakeClock(Rc<Cell<u64>>);

impl Clock for FakeClock {
    fn now_ns(&mut self) -> u64 {
        self.0.get()
    }
}

#[test]
fn medians_and_deviations() {
    assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
    assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
    assert_eq!(median(&[5.0; 7]), 5.0);
    assert_eq!(mad(&[5.0; 7]), 0.0);
    // |x - 2| = 1, 1, 0, 2, 98 -> median 1
    assert_eq!(mad(&[1.0, 3.0, 2.0, 4.0, 100.0]), 1.0);
}

#[test]
fn batches_last_at_least_the_minimum() {
    let t = Rc::new(Cell::new(0u64));
    let mut clock = FakeClock(t.clone());
    let mut body = || {
        t.set(t.get() + 3_000_000);
        1
    };
    assert_eq!(batch_size(&mut clock, &mut body, 10_000_000), 4);
    let t2 = Rc::new(Cell::new(0u64));
    let mut clock = FakeClock(t2.clone());
    let mut slow = || {
        t2.set(t2.get() + 50_000_000);
        1
    };
    assert_eq!(batch_size(&mut clock, &mut slow, 10_000_000), 1);
}

#[test]
fn measurements_report_per_iteration_times() {
    let t = Rc::new(Cell::new(0u64));
    let mut clock = FakeClock(t.clone());
    let calls = Rc::new(Cell::new(0u64));
    let c = calls.clone();
    let mut body = || {
        t.set(t.get() + 2_000_000);
        c.set(c.get() + 1);
        7
    };
    let config = Config {
        min_batch_ns: 10_000_000,
        warmup: 3,
        samples: 30,
    };
    let s = measure(&mut clock, &mut body, &config);
    assert_eq!(
        s,
        Stats {
            median_ns: 2_000_000.0,
            mad_ns: 0.0
        }
    );
    // Sizing runs batches of 1, 2, 4 and 8 calls (16 ms >= 10 ms); then
    // 3 + 30 batches of 8.
    assert_eq!(calls.get(), 15 + 8 * 33);
}

#[test]
fn tables() {
    let s = |m: f64, d: f64| {
        Some(Stats {
            median_ns: m,
            mad_ns: d,
        })
    };
    let rows = vec![
        Row {
            item: "cmap".into(),
            font: "Roboto".into(),
            sofel: s(1500.0, 20.0),
            freetype: s(3000.0, 10.0),
            harfbuzz: s(1000.0, 5.0),
        },
        Row {
            item: "raster 12px".into(),
            font: "Roboto".into(),
            sofel: s(2_000_000.0, 0.0),
            freetype: s(1_000_000.0, 0.0),
            harfbuzz: None,
        },
    ];
    let expected = "\
| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| cmap | Roboto | 1.5 ±0.0 | 3.0 ±0.0 | 0.50 | 1.0 ±0.0 | 1.50 |
| raster 12px | Roboto | 2000.0 ±0.0 | 1000.0 ±0.0 | 2.00 | — | — |
";
    assert_eq!(table(&rows), expected);
}

#[test]
fn failed_checks_are_reported() {
    let checks = vec![
        Check {
            item: "advance".into(),
            font: "A".into(),
            ok: true,
            detail: String::new(),
        },
        Check {
            item: "cmap".into(),
            font: "B".into(),
            ok: false,
            detail: "sofel 3, FreeType 4".into(),
        },
    ];
    let r = report_checks(&checks).unwrap_err();
    assert!(
        r.contains("cmap") && r.contains("B") && r.contains("sofel 3, FreeType 4"),
        "{r}"
    );
    assert!(report_checks(&checks[..1]).is_ok());
}
