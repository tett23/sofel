# 前処理つきのサブセット化の計測（2026-11-10）

[ADR 0022](../adr/0022-subsetting.md) の「計測」の記録（11段階目）。[5段階目の記録](2026-11-10-subset.md) の項目に、前処理の項目を足した。
項目、照合、計測の方法は [ADR 0013](../adr/0013-benchmark-harness.md) と [ADR 0006](../adr/0006-performance-measurement.md) のとおり。
時間は1回あたりの中央値（±中央絶対偏差）、単位はマイクロ秒。比が1未満ならsofelが速い。

## 項目

- `subset (N glyphs, retain-gids)` と `subset (N characters)`：5段階目の記録と同じ。
- `subset prepared (...)`：同じ2つの入力を、前処理したフォントでサブセット化する。sofelは計測の前に一度だけ `Prepared::new` を呼び、計測の中で `Plan::with_prepared` と `Plan::execute` を呼ぶ。HarfBuzzは計測の前に一度だけ `hb_subset_preprocess` を呼び、計測の中でその面に `hb_subset_or_fail` と `hb_face_reference_blob` を呼ぶ。出力のFNV-1aのハッシュを、計測の前に比べる。
- `preprocess`：前処理そのもの。sofelは `Prepared::new`、HarfBuzzは `hb_subset_preprocess`。結果の形が違う（HarfBuzzは `keep_everything` でサブセット化した面、sofelは前処理のデータ）ので、結果は比べない。
- フォントは5段階目と同じ Roboto-Regular.ttf、SourceHanSans-Regular_subset.otf、NotoNastaliqUrdu-Bold.ttf。

## 環境

- 機械：Apple M1 Max、メモリ64GB
- OS：macOS 27.0（26A428）
- Rust：rustc 1.99.1（58cf8f9c3 2026-10-13）
- Cのコンパイラ（ネイティブ）：Apple clang、`-O2 -DNDEBUG`
- wasm：wasi-sdk 34、wasmtime 50.0.1（`-W exceptions=y`）
- 比較の相手：HarfBuzz 15.1.0（`testdata.lock`。`harfbuzz-subset.cc` の単一の翻訳単位）
- sofel：[ADR 0022](../adr/0022-subsetting.md) の11段階目のブランチ（`implement-adr-0022-subset-11-prepared`、b3132c7 に対する変更）

**注意**：計測中の機械は、ほかの作業で負荷が高かった（load averageは、1分の値で10〜20、15分の値で12）。比は目安にとどまる。
照合（`--once` と同じ結果の比べ合わせ）は、ネイティブとwasmですべて通った。

## ネイティブ（aarch64-apple-darwin）

| 項目 | フォント | sofel (µs) | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|
| subset (42 glyphs, retain-gids) | Roboto-Regular | 241.8 ±3.5 | 121.1 ±1.4 | 2.00 |
| subset (2322 characters) | Roboto-Regular | 330.3 ±4.2 | 134.7 ±2.3 | 2.45 |
| subset prepared (42 glyphs, retain-gids) | Roboto-Regular | 135.6 ±1.0 | 87.3 ±1.2 | 1.55 |
| subset prepared (2322 characters) | Roboto-Regular | 257.0 ±3.1 | 108.4 ±1.1 | 2.37 |
| preprocess | Roboto-Regular | 227.9 ±2.6 | 672.1 ±11.4 | 0.34 |
| subset (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 59598.2 ±324.2 | 13937.7 ±290.0 | 4.28 |
| subset (2322 characters) | SourceHanSans-Regular_subset | 37352.2 ±344.1 | 13373.3 ±150.1 | 2.79 |
| subset prepared (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 42693.6 ±191.3 | 5928.0 ±86.5 | 7.20 |
| subset prepared (2322 characters) | SourceHanSans-Regular_subset | 21536.1 ±330.9 | 5764.5 ±179.9 | 3.74 |
| preprocess | SourceHanSans-Regular_subset | 52045.1 ±457.5 | 46714.9 ±402.3 | 1.11 |
| subset (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 90.2 ±1.4 | 126.8 ±1.7 | 0.71 |
| subset (2322 characters) | NotoNastaliqUrdu-Bold | 189.6 ±3.6 | 141.1 ±2.4 | 1.34 |
| subset prepared (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 70.0 ±0.3 | 59.5 ±0.7 | 1.18 |
| subset prepared (2322 characters) | NotoNastaliqUrdu-Bold | 173.6 ±2.3 | 74.2 ±0.8 | 2.34 |
| preprocess | NotoNastaliqUrdu-Bold | 55.3 ±0.4 | 13668.1 ±149.1 | 0.00 |

## wasm（wasm32-wasip1、wasmtime）

| 項目 | フォント | sofel (µs) | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|
| subset (42 glyphs, retain-gids) | Roboto-Regular | 295.4 ±1.9 | 171.9 ±15.9 | 1.72 |
| subset (2322 characters) | Roboto-Regular | 401.4 ±4.5 | 170.9 ±3.3 | 2.35 |
| subset prepared (42 glyphs, retain-gids) | Roboto-Regular | 158.0 ±2.8 | 99.2 ±1.2 | 1.59 |
| subset prepared (2322 characters) | Roboto-Regular | 309.4 ±3.6 | 116.6 ±1.3 | 2.65 |
| preprocess | Roboto-Regular | 283.3 ±2.9 | 864.7 ±13.5 | 0.33 |
| subset (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 73295.4 ±481.6 | 18902.5 ±262.7 | 3.88 |
| subset (2322 characters) | SourceHanSans-Regular_subset | 45872.6 ±374.7 | 18215.9 ±222.2 | 2.52 |
| subset prepared (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 56422.0 ±1387.9 | 7685.8 ±157.8 | 7.34 |
| subset prepared (2322 characters) | SourceHanSans-Regular_subset | 27508.0 ±325.6 | 7837.7 ±633.8 | 3.51 |
| preprocess | SourceHanSans-Regular_subset | 71327.1 ±748.5 | 68156.6 ±753.8 | 1.05 |
| subset (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 112.1 ±1.7 | 178.5 ±3.3 | 0.63 |
| subset (2322 characters) | NotoNastaliqUrdu-Bold | 230.7 ±3.8 | 189.2 ±3.1 | 1.22 |
| subset prepared (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 82.2 ±0.8 | 68.6 ±0.9 | 1.20 |
| subset prepared (2322 characters) | NotoNastaliqUrdu-Bold | 206.0 ±2.4 | 84.1 ±0.8 | 2.45 |
| preprocess | NotoNastaliqUrdu-Bold | 76.1 ±2.4 | 17647.2 ±156.8 | 0.00 |

## 所見

- 出力はすべての項目でHarfBuzzとバイト単位で一致した。前処理つきの出力は、前処理の無い出力とも一致する。
- 前処理で、sofelのサブセット化は速くなった（ネイティブで、Roboto-Regularのグリフの項目は 241.8 から 135.6、Source Han Sansの文字の項目は 37352.2 から 21536.1）。ただし、HarfBuzzは前処理でさらに速くなる（Source Han Sansで約2.3倍）ので、比は前処理の無いときより悪い項目がある。最も遅いのは、Source Han Sansの1918グリフを `RETAIN_GIDS` で残す項目（ネイティブで7.20倍）で、CFFの解析を省いた後に残る費用（サブルーチンの閉包、charstringの符号化、直列化）を調べる必要がある。
- `preprocess` は、HarfBuzzが `keep_everything` でフォント全体をサブセット化するのに対し、sofelは計画が使うデータだけを作る。このため、GSUBとGPOSの大きなNotoNastaliqUrdu-Boldでは、sofelの前処理の時間はHarfBuzzの1%に満たない。Source Han Sansでは、全グリフのcharstringの解析が大部分を占め、HarfBuzzと同程度である。
