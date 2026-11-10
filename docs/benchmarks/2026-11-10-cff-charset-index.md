# CFFのcharsetの索引の計測（2026-11-10）

[ADR 0025](../adr/0025-cff-charset-index.md) の「計測」の記録。
項目、照合、計測の方法は [ADR 0013](../adr/0013-benchmark-harness.md) と [ADR 0006](../adr/0006-performance-measurement.md) のとおり。
時間は1回あたりの中央値（±中央絶対偏差）、単位はマイクロ秒。比が1未満ならsofelが速い。
変わった項目（`cid`、`cid (no index)`、`open (FreeType)`）の行だけを載せる。ほかの行は [CFFとCFF2の記録](2026-11-10-cff.md) を参照。

## 項目

- `cid`：CIDキーのフォントの全グリフのCID。sofelはcharsetの索引を使う `Cff::cid_with`、FreeTypeは `FT_Get_CID_From_Glyph_Index`。
- `cid (no index)`：同じ値を索引の無い `Cff::cid` で求める。1回だけ引く呼び出し側の費用の目安。
- `open (FreeType)`：sofelは `Cff::new` に加えて、charsetの索引の配列の確保と `Charset::index_into` を含む（FreeTypeの `cff_charset_load` がSIDの配列を作るのに相当）。FreeTypeは `FT_New_Memory_Face`。

## 環境

- 機械：Apple M1 Max、メモリ64GB
- OS：macOS 27.0（26A428）
- Rust：rustc 1.99.1（58cf8f9c3 2026-10-13）
- Cのコンパイラ（ネイティブ）：Apple clang、`-O2 -DNDEBUG`
- wasm：wasi-sdk 34、wasmtime 50.0.1（`-W exceptions=y`）
- 比較の相手：FreeType 2.15.0、HarfBuzz 15.1.0（`testdata.lock`）
- sofel：[ADR 0025](../adr/0025-cff-charset-index.md) の実装のプルリクエスト（main 482511e に対する変更）

**注意**：計測中の機械は、ほかの作業で負荷が高かった（計測の前後のload averageは、1分の値で17〜34、15分の値で85〜95）。比は目安にとどまる。
照合（`--once` と同じ結果の比べ合わせ）は、ネイティブとwasmですべて通った。

## ネイティブ（aarch64-apple-darwin）

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| cid | SourceHanSans-Regular_subset | 144.3 ±5.7 | 246.4 ±1.7 | 0.59 | — | — |
| cid (no index) | SourceHanSans-Regular_subset | 201186.9 ±4709.0 | 254.5 ±6.3 | 790.54 | — | — |
| open (FreeType) | SourceSansPro-Regular | 7.7 ±0.0 | 22.6 ±0.2 | 0.34 | — | — |
| open (FreeType) | NotoSerifMyanmar-Regular | 4.4 ±0.0 | 10.9 ±0.1 | 0.41 | — | — |
| open (FreeType) | AdobeVFPrototype | 6.8 ±0.1 | 10.5 ±0.1 | 0.64 | — | — |
| open (FreeType) | SourceHanSans-Regular_subset | 56.4 ±0.4 | 86.5 ±1.7 | 0.65 | — | — |

## wasm（wasm32-wasip1、wasmtime）

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| cid | SourceHanSans-Regular_subset | 379.6 ±3.9 | 431.1 ±18.0 | 0.88 | — | — |
| cid (no index) | SourceHanSans-Regular_subset | 489595.3 ±40940.1 | 467.3 ±50.4 | 1047.79 | — | — |
| open (FreeType) | SourceSansPro-Regular | 13.2 ±0.0 | 48.6 ±0.5 | 0.27 | — | — |
| open (FreeType) | NotoSerifMyanmar-Regular | 8.1 ±0.1 | 21.9 ±1.5 | 0.37 | — | — |
| open (FreeType) | AdobeVFPrototype | 13.5 ±0.2 | 18.6 ±0.5 | 0.73 | — | — |
| open (FreeType) | SourceHanSans-Regular_subset | 111.1 ±5.4 | 180.1 ±2.8 | 0.62 | — | — |

## 所見

- 索引を使う `cid` は、FreeTypeと同じ桁になった（ネイティブで0.59、wasmで0.88）。[前の記録](2026-11-10-cff.md) の約885倍から、ADRの目安（比が1前後）に届いた。
- 索引の無い `cid` は、前の記録と同じくグリフの数の2乗に比例し、全グリフを引くとFreeTypeの約800〜1000倍かかる。1回だけ引くときの費用は、グリフの前の範囲を読む分にとどまる。
- 開く時間に索引の確保と作成を足しても、FreeTypeの `FT_New_Memory_Face` より速い。前の記録の `open (FreeType)` とは負荷が違うため、索引の分の増減はこの記録からは言えない。
