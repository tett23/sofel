# COLRv0の画像の計測（2026-11-10）

[ADR 0023](../adr/0023-color-glyphs.md) の5d段階（COLRv0の画像）の「計測」の記録。
項目、照合、計測の方法は [ADR 0013](../adr/0013-benchmark-harness.md) と [ADR 0006](../adr/0006-performance-measurement.md) のとおり。
時間は1回あたりの中央値（±中央絶対偏差）、単位はマイクロ秒。比が1未満ならsofelが速い。

## 項目

- `COLRv0 raster <N>px`：フォントの全グリフを、パレット0、前景色は黒で、Nピクセルの色の画像にし、画素のバイトの和を求める。sofelは `sofel_cli::glyph::render_color`（層の無いグリフは灰色の輪郭）、FreeTypeは `FT_Load_Glyph`（`FT_LOAD_COLOR | FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP | FT_LOAD_NO_SVG`）と `FT_Render_Glyph`（`FT_RENDER_MODE_NORMAL`）。和はFreeTypeと照合する。

## 環境

- 機械：Apple M1 Max、メモリ64GB
- OS：macOS 27.0（26A428）
- Rust：rustc 1.99.1（58cf8f9c3 2026-10-13）
- Cのコンパイラ（ネイティブ）：Apple clang、`-O2 -DNDEBUG`
- wasm：wasi-sdk 34、wasmtime 50.0.1（`-W exceptions=y`）
- 比較の相手：FreeType 2.15.0（`testdata.lock`）
- sofel：[ADR 0023](../adr/0023-color-glyphs.md)の5d段階の実装のプルリクエスト（main 949f5a3と5c段階のプルリクエストに対する変更）

**注意**：計測中の機械は、ほかの作業で負荷が高かった（計測の前後のload averageは、1分の値で14〜15、15分の値で25〜26）。比は目安にとどまる。
照合（`--once` と同じ結果の比べ合わせ）は、ネイティブとwasmですべて通った。

## ネイティブ（aarch64-apple-darwin）

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| COLRv0 raster 12px | TwemojiMozilla.subset | 19.8 ±0.3 | 13.6 ±0.3 | 1.45 | — | — |
| COLRv0 raster 24px | TwemojiMozilla.subset | 26.1 ±0.3 | 20.6 ±0.7 | 1.27 | — | — |
| COLRv0 raster 48px | TwemojiMozilla.subset | 45.1 ±0.5 | 39.1 ±0.5 | 1.16 | — | — |
| COLRv0 raster 12px | test_glyphs-glyf_colr_1 | 90.1 ±0.8 | 90.2 ±1.2 | 1.00 | — | — |
| COLRv0 raster 24px | test_glyphs-glyf_colr_1 | 141.2 ±2.6 | 160.9 ±2.1 | 0.88 | — | — |
| COLRv0 raster 48px | test_glyphs-glyf_colr_1 | 264.3 ±5.0 | 371.6 ±6.0 | 0.71 | — | — |

## wasm（wasm32-wasip1、wasmtime）

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| COLRv0 raster 12px | TwemojiMozilla.subset | 36.6 ±2.7 | 24.6 ±1.5 | 1.49 | — | — |
| COLRv0 raster 24px | TwemojiMozilla.subset | 50.4 ±1.3 | 42.1 ±2.7 | 1.20 | — | — |
| COLRv0 raster 48px | TwemojiMozilla.subset | 110.5 ±5.2 | 90.3 ±4.0 | 1.22 | — | — |
| COLRv0 raster 12px | test_glyphs-glyf_colr_1 | 175.4 ±9.9 | 130.4 ±5.1 | 1.35 | — | — |
| COLRv0 raster 24px | test_glyphs-glyf_colr_1 | 270.0 ±6.1 | 221.3 ±7.3 | 1.22 | — | — |
| COLRv0 raster 48px | test_glyphs-glyf_colr_1 | 572.5 ±15.8 | 476.8 ±18.5 | 1.20 | — | — |

## 所見

- 層の多い `test_glyphs-glyf_colr_1` では、ネイティブで0.71〜1.00とFreeTypeと同じか速い。輪郭の描画（[ADR 0012](../adr/0012-antialiased-rasterizer.md)）の速さがそのまま出ている。
- 小さな層の多い `TwemojiMozilla.subset` の12画素では1.45と遅い。`render_color` はグリフごとに層の一覧とパレットの色の配列を確保し、COLRの表を開きなおすので、層が小さいほどこの固定の費用が目立つ。ADRの目安（比が1前後）からは外れており、確保をやめる余地がある。
- wasmでは比が1.2〜1.5で、ネイティブより一様に遅い。
