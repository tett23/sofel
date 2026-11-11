# 追加フォントの役割別計測（2026-11-11）

[ADR 0031](../adr/0031-diverse-font-corpus.md) の13ファイルについて、採用理由に対応する代表項目を追加して測った。
先の [cmapとadvanceの計測](2026-11-11-extra-fonts.md) を置き換えず、輪郭、縦書き、MATH、カラーとシェーピングを補う。
全64計測項目と、速度を計らない4件の期待する失敗をnativeとwasmで照合した。

## 入力と照合の範囲

すべてフェイス0と既定座標を使う。
TTCの残りのフェイスと非既定座標は、追加フォントの適合テストで別に検証する。
この計測から、非既定座標への対応や全グリフの一致は主張しない。

| 項目 | 入力と比較 |
| --- | --- |
| cmap | 13ファイルそれぞれでU+0041、0627、0915、1780、1000、1B05、5B57、2211、05D0、1F600のグリフID合計をFreeTypeとHarfBuzzに照合 |
| advance | 13ファイルそれぞれで先頭最大128グリフの横advance合計をFreeTypeとHarfBuzzに照合 |
| outline unscaled sampled | 同じ先頭最大128グリフの輪郭コマンド数を非ヒンティング・拡大縮小なしでFreeTypeに照合。Closeを数えない。座標列そのものは適合テストで別に比較 |
| vertical advance | Source Hanの先頭最大128グリフの縦advance合計をHarfBuzzに照合 |
| MATH constants | STIXの全56定数をUPEMスケール・ppem 0のHarfBuzzと個別に比較し、合計する処理を計測 |
| カラー | CFFのCOLRv1 paintとrecording、sbixのビットマップ取得、SVG文書取得、各形式の判定とextentsを既存のカラー項目へ追加。COLRv1は計測前に固定HarfBuzzの正規化したpaint列全体も比較 |
| シェーピング | 次表の9入力をHarfBuzzと比較。一致する5入力で、計画を再利用する処理と最初の1回を別に計測 |

シェーピングのscriptは入力から推定し、言語を指定しない（`HB_LANGUAGE_INVALID`）。
featureと座標は既定値で、方向だけを次表の値に固定する。
各入力の末尾に改行を付け、一行として扱う。
グリフID、UTF-8クラスタ、フラグと全位置を計測前に比較する。

| フォント | Unicodeコードポイント | 推定するscript | 方向 | 結果 |
| --- | --- | --- | --- | --- |
| arabic | 0628 0650 062A | Arab | RTL | 一致 |
| devanagari | 0915 094D 200D 0937 | Deva | LTR | Indic未実装による期待する失敗 |
| khmer | 1780 17D2 179A | Khmr | LTR | Khmer未実装による期待する失敗 |
| myanmar | 1004 103A 1039 1000 102D | Mymr | LTR | Myanmar未実装による期待する失敗 |
| balinese | 1B13 1B44 1B31 1B36 | Bali | LTR | USE未実装による期待する失敗 |
| hebrew | 05E9 05B8 05C1 05DC 05D5 05B9 05DD | Hebr | RTL | 一致 |
| source-serif | 006F 0066 0066 0069 0063 0065 | Latn | LTR | 一致 |
| source-han | 6F22 5B57 | Hani | TTB | 一致 |
| amstelvar | 006F 0066 0066 0069 0063 0065 | Latn | LTR | 一致 |

未実装の4入力は不一致を期待し、予期しない成功も失敗にする。
これらは速度の表に含めない。
短い代表入力の計測であり、既存の長文ベンチマークの代わりにはしない。

## 環境と実行

macOS 27.0（26A428）、arm64、Rust 1.99.0（b940084d7）、Apple clang 21.0.0を使用した。
Cは既存の `-O2 -DNDEBUG` でビルドしたFreeType 2.15.0とHarfBuzz 15.1.0を使い、新しいMATHラッパーを含むshimだけを同じフラグで再ビルドした。
固定コミットはそれぞれ `37515cf56ba3349656049edbed89ffc6f6466f2d` と `3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372` である。
wasmはwasi-sdk 34.0とwasmtime 50.0.1（c307ec81d）を使った。
sofelは `ec81c853ea109acfde4f1d726e5024177c520052` に今回の未コミット変更を加えた状態である。
Rustは `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_DEBUG=0` でreleaseビルドした。

```sh
bench/target/release/sofel-bench --once --filter extra
wasmtime run -W exceptions=y --dir target/testdata \
  bench/target/wasm32-wasip1/release/sofel-bench.wasm --once --filter extra
bench/target/release/sofel-bench --filter extra
wasmtime run -W exceptions=y --dir target/testdata \
  bench/target/wasm32-wasip1/release/sofel-bench.wasm --filter extra
```

`--once` の照合はnativeとwasmの両方で終了状態0だった。
Clippyの全target検査も `-D warnings` で成功した。
計測は各項目につき10 ms以上のバッチ、ウォームアップ3回、記録30回で行う。
表の値は1回の呼び出しに換算した中央値と中央絶対偏差で、単位はマイクロ秒である。
速度比が1未満ならsofelが速い。
共有マシンでの1回の計測であり、CIの完了時間への効果は測っていない。

nativeの初回は別作業の短いコンパイルと重なったため、再測定の値を載せる。
wasmの計測中には別作業の短いRust検査が重なっており、以下は性能の目安として扱う。
各計測は終了状態0で、出力の64行をそのまま掲載する。

## native（aarch64-apple-darwin）

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| cmap | extra/arabic | 0.1 ±0.0 | 0.1 ±0.0 | 1.04 | 0.1 ±0.0 | 0.94 |
| advance | extra/arabic | 0.2 ±0.0 | 1.4 ±0.0 | 0.17 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/arabic | 45.4 ±1.5 | 40.3 ±0.4 | 1.12 | — | — |
| cmap | extra/devanagari | 0.1 ±0.0 | 0.1 ±0.0 | 1.17 | 0.1 ±0.0 | 1.11 |
| advance | extra/devanagari | 0.2 ±0.0 | 1.5 ±0.0 | 0.15 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/devanagari | 50.7 ±0.5 | 43.7 ±0.6 | 1.16 | — | — |
| cmap | extra/khmer | 0.1 ±0.0 | 0.1 ±0.0 | 1.14 | 0.1 ±0.0 | 1.14 |
| advance | extra/khmer | 0.2 ±0.0 | 1.4 ±0.0 | 0.17 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/khmer | 47.2 ±0.6 | 40.5 ±0.4 | 1.17 | — | — |
| cmap | extra/myanmar | 0.1 ±0.0 | 0.1 ±0.0 | 1.08 | 0.1 ±0.0 | 1.12 |
| advance | extra/myanmar | 0.2 ±0.0 | 1.4 ±0.0 | 0.17 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/myanmar | 57.7 ±0.6 | 48.0 ±0.7 | 1.20 | — | — |
| cmap | extra/balinese | 0.1 ±0.0 | 0.1 ±0.0 | 1.17 | 0.1 ±0.0 | 1.10 |
| advance | extra/balinese | 0.2 ±0.0 | 1.4 ±0.0 | 0.17 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/balinese | 74.8 ±0.7 | 62.6 ±0.6 | 1.20 | — | — |
| cmap | extra/hebrew | 0.1 ±0.0 | 0.1 ±0.0 | 1.11 | 0.1 ±0.0 | 1.03 |
| advance | extra/hebrew | 0.2 ±0.0 | 1.4 ±0.0 | 0.17 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/hebrew | 28.8 ±0.2 | 30.2 ±0.2 | 0.95 | — | — |
| cmap | extra/source-serif | 0.1 ±0.0 | 0.1 ±0.0 | 1.05 | 0.1 ±0.0 | 0.96 |
| advance | extra/source-serif | 0.2 ±0.0 | 1.4 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/source-serif | 2218.3 ±18.5 | 775.3 ±4.1 | 2.86 | — | — |
| cmap | extra/stix-math | 0.1 ±0.0 | 0.1 ±0.0 | 1.07 | 0.1 ±0.0 | 1.06 |
| advance | extra/stix-math | 0.2 ±0.0 | 1.4 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/stix-math | 264.3 ±2.1 | 295.2 ±2.5 | 0.90 | — | — |
| MATH constants | extra/stix-math | 0.1 ±0.0 | — | — | 0.5 ±0.0 | 0.14 |
| cmap | extra/source-han | 0.1 ±0.0 | 0.2 ±0.0 | 0.84 | 0.1 ±0.0 | 0.99 |
| advance | extra/source-han | 0.2 ±0.0 | 1.4 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/source-han | 306.4 ±3.2 | 189.8 ±2.0 | 1.61 | — | — |
| vertical advance | extra/source-han | 0.5 ±0.0 | — | — | 1.0 ±0.0 | 0.52 |
| cmap | extra/amstelvar | 0.1 ±0.0 | 0.1 ±0.0 | 1.11 | 0.1 ±0.0 | 0.90 |
| advance | extra/amstelvar | 0.2 ±0.0 | 1.4 ±0.0 | 0.17 | 1.0 ±0.0 | 0.23 |
| outline unscaled sampled | extra/amstelvar | 54.1 ±0.4 | 55.9 ±0.3 | 0.97 | — | — |
| cmap | extra/color-cff | 0.1 ±0.0 | 0.1 ±0.0 | 1.20 | 0.1 ±0.0 | 1.11 |
| advance | extra/color-cff | 0.1 ±0.0 | 0.4 ±0.0 | 0.16 | 0.2 ±0.0 | 0.25 |
| outline unscaled sampled | extra/color-cff | 15.2 ±0.1 | 13.2 ±0.1 | 1.15 | — | — |
| cmap | extra/color-sbix | 0.1 ±0.0 | 0.1 ±0.0 | 1.19 | 0.1 ±0.0 | 1.11 |
| advance | extra/color-sbix | 0.1 ±0.0 | 0.3 ±0.0 | 0.17 | 0.2 ±0.0 | 0.25 |
| outline unscaled sampled | extra/color-sbix | 0.8 ±0.0 | 1.2 ±0.0 | 0.70 | — | — |
| cmap | extra/color-svg | 0.1 ±0.0 | 0.1 ±0.0 | 1.20 | 0.1 ±0.0 | 1.11 |
| advance | extra/color-svg | 0.1 ±0.0 | 0.3 ±0.0 | 0.17 | 0.2 ±0.0 | 0.25 |
| outline unscaled sampled | extra/color-svg | 0.9 ±0.0 | 1.3 ±0.0 | 0.70 | — | — |
| glyph color format | extra/color-cff | 1.4 ±0.0 | — | — | 0.3 ±0.0 | 4.36 |
| COLRv1 paint | extra/color-cff | 2.9 ±0.0 | 0.8 ±0.0 | 3.63 | 1.8 ±0.0 | 1.59 |
| COLRv1 recording | extra/color-cff | 2.9 ±0.0 | — | — | — | — |
| color glyph extents | extra/color-cff | 11.1 ±0.0 | — | — | 3.1 ±0.1 | 3.55 |
| glyph color format | extra/color-sbix | 1.3 ±0.0 | — | — | 0.2 ±0.0 | 6.88 |
| bitmap data 16 ppem | extra/color-sbix | 2.8 ±0.0 | — | — | 2.7 ±0.0 | 1.02 |
| bitmap data 109 ppem | extra/color-sbix | 2.8 ±0.0 | — | — | 2.7 ±0.0 | 1.02 |
| bitmap data 136 ppem | extra/color-sbix | 2.9 ±0.0 | — | — | 3.1 ±0.0 | 0.95 |
| bitmap data 0 ppem | extra/color-sbix | 2.9 ±0.0 | — | — | 3.0 ±0.0 | 0.98 |
| color glyph extents | extra/color-sbix | 1.3 ±0.0 | — | — | 1.5 ±0.0 | 0.89 |
| glyph color format | extra/color-svg | 1.0 ±0.0 | — | — | 0.2 ±0.0 | 5.05 |
| SVG documents | extra/color-svg | 0.4 ±0.0 | — | — | 0.6 ±0.0 | 0.76 |
| color glyph extents | extra/color-svg | 0.8 ±0.0 | — | — | 0.6 ±0.0 | 1.31 |
| shaping role-default-und | extra/arabic | 1.6 ±0.0 | — | — | 1.0 ±0.0 | 1.58 |
| first shaping role-default-und | extra/arabic | 13.2 ±0.1 | — | — | 17.7 ±0.1 | 0.75 |
| shaping role-default-und | extra/hebrew | 1.4 ±0.0 | — | — | 0.8 ±0.0 | 1.66 |
| first shaping role-default-und | extra/hebrew | 6.4 ±0.0 | — | — | 9.1 ±0.1 | 0.70 |
| shaping role-default-und | extra/source-serif | 1.2 ±0.0 | — | — | 0.5 ±0.0 | 2.55 |
| first shaping role-default-und | extra/source-serif | 19.7 ±0.3 | — | — | 20.9 ±0.2 | 0.95 |
| shaping role-default-und TTB | extra/source-han | 0.3 ±0.0 | — | — | 0.2 ±0.0 | 1.39 |
| first shaping role-default-und TTB | extra/source-han | 115.8 ±1.2 | — | — | 23.0 ±0.2 | 5.04 |
| shaping role-default-und | extra/amstelvar | 0.5 ±0.0 | — | — | 0.3 ±0.0 | 1.53 |
| first shaping role-default-und | extra/amstelvar | 2.4 ±0.0 | — | — | 10.0 ±0.1 | 0.24 |

## wasm（wasm32-wasip1）

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| cmap | extra/arabic | 0.2 ±0.0 | 0.3 ±0.0 | 0.70 | 0.4 ±0.0 | 0.57 |
| advance | extra/arabic | 0.2 ±0.0 | 4.7 ±0.0 | 0.05 | 2.7 ±0.1 | 0.08 |
| outline unscaled sampled | extra/arabic | 68.2 ±1.0 | 71.5 ±0.8 | 0.95 | — | — |
| cmap | extra/devanagari | 0.2 ±0.0 | 0.3 ±0.0 | 0.74 | 0.4 ±0.0 | 0.52 |
| advance | extra/devanagari | 0.2 ±0.0 | 4.7 ±0.0 | 0.05 | 2.8 ±0.1 | 0.08 |
| outline unscaled sampled | extra/devanagari | 75.1 ±0.7 | 79.7 ±1.0 | 0.94 | — | — |
| cmap | extra/khmer | 0.2 ±0.0 | 0.3 ±0.0 | 0.71 | 0.3 ±0.0 | 0.72 |
| advance | extra/khmer | 0.2 ±0.0 | 4.8 ±0.0 | 0.05 | 2.8 ±0.1 | 0.08 |
| outline unscaled sampled | extra/khmer | 73.9 ±3.4 | 72.2 ±1.1 | 1.02 | — | — |
| cmap | extra/myanmar | 0.2 ±0.0 | 0.3 ±0.0 | 0.72 | 0.3 ±0.0 | 0.73 |
| advance | extra/myanmar | 0.2 ±0.0 | 4.7 ±0.0 | 0.05 | 2.7 ±0.1 | 0.08 |
| outline unscaled sampled | extra/myanmar | 83.1 ±0.4 | 84.1 ±0.8 | 0.99 | — | — |
| cmap | extra/balinese | 0.2 ±0.0 | 0.3 ±0.0 | 0.71 | 0.3 ±0.0 | 0.70 |
| advance | extra/balinese | 0.2 ±0.0 | 4.8 ±0.0 | 0.05 | 2.8 ±0.1 | 0.08 |
| outline unscaled sampled | extra/balinese | 109.7 ±1.6 | 107.4 ±1.2 | 1.02 | — | — |
| cmap | extra/hebrew | 0.2 ±0.0 | 0.3 ±0.0 | 0.70 | 0.3 ±0.0 | 0.71 |
| advance | extra/hebrew | 0.2 ±0.0 | 4.8 ±0.0 | 0.05 | 2.7 ±0.1 | 0.08 |
| outline unscaled sampled | extra/hebrew | 42.3 ±0.3 | 52.9 ±0.4 | 0.80 | — | — |
| cmap | extra/source-serif | 0.2 ±0.0 | 0.3 ±0.0 | 0.70 | 0.4 ±0.0 | 0.58 |
| advance | extra/source-serif | 0.2 ±0.0 | 4.8 ±0.1 | 0.05 | 2.7 ±0.1 | 0.08 |
| outline unscaled sampled | extra/source-serif | 3358.3 ±38.6 | 1124.7 ±6.1 | 2.99 | — | — |
| cmap | extra/stix-math | 0.2 ±0.0 | 0.4 ±0.0 | 0.71 | 0.4 ±0.0 | 0.56 |
| advance | extra/stix-math | 0.2 ±0.0 | 4.8 ±0.1 | 0.05 | 2.8 ±0.2 | 0.08 |
| outline unscaled sampled | extra/stix-math | 341.7 ±2.8 | 439.3 ±6.5 | 0.78 | — | — |
| MATH constants | extra/stix-math | 0.2 ±0.0 | — | — | 0.8 ±0.0 | 0.29 |
| cmap | extra/source-han | 0.3 ±0.0 | 0.4 ±0.0 | 0.73 | 0.4 ±0.0 | 0.67 |
| advance | extra/source-han | 0.2 ±0.0 | 4.7 ±0.0 | 0.05 | 2.7 ±0.1 | 0.08 |
| outline unscaled sampled | extra/source-han | 351.3 ±5.0 | 267.9 ±2.2 | 1.31 | — | — |
| vertical advance | extra/source-han | 0.9 ±0.0 | — | — | 2.8 ±0.0 | 0.32 |
| cmap | extra/amstelvar | 0.2 ±0.0 | 0.3 ±0.0 | 0.69 | 0.3 ±0.0 | 0.64 |
| advance | extra/amstelvar | 0.2 ±0.0 | 4.7 ±0.0 | 0.05 | 2.7 ±0.0 | 0.08 |
| outline unscaled sampled | extra/amstelvar | 81.4 ±0.6 | 98.9 ±0.6 | 0.82 | — | — |
| cmap | extra/color-cff | 0.1 ±0.0 | 0.2 ±0.0 | 0.76 | 0.2 ±0.0 | 0.68 |
| advance | extra/color-cff | 0.1 ±0.0 | 1.2 ±0.0 | 0.05 | 0.6 ±0.0 | 0.09 |
| outline unscaled sampled | extra/color-cff | 14.6 ±0.1 | 13.3 ±0.1 | 1.10 | — | — |
| cmap | extra/color-sbix | 0.1 ±0.0 | 0.2 ±0.0 | 0.76 | 0.2 ±0.0 | 0.67 |
| advance | extra/color-sbix | 0.1 ±0.0 | 1.1 ±0.0 | 0.05 | 0.6 ±0.0 | 0.09 |
| outline unscaled sampled | extra/color-sbix | 1.3 ±0.0 | 2.5 ±0.0 | 0.49 | — | — |
| cmap | extra/color-svg | 0.1 ±0.0 | 0.2 ±0.0 | 0.75 | 0.2 ±0.0 | 0.66 |
| advance | extra/color-svg | 0.1 ±0.0 | 1.1 ±0.0 | 0.05 | 0.6 ±0.0 | 0.09 |
| outline unscaled sampled | extra/color-svg | 1.3 ±0.0 | 2.6 ±0.0 | 0.49 | — | — |
| glyph color format | extra/color-cff | 3.0 ±0.0 | — | — | 0.5 ±0.0 | 5.64 |
| COLRv1 paint | extra/color-cff | 6.8 ±0.1 | 1.9 ±0.0 | 3.62 | 3.6 ±0.0 | 1.88 |
| COLRv1 recording | extra/color-cff | 6.7 ±0.1 | — | — | — | — |
| color glyph extents | extra/color-cff | 13.5 ±0.1 | — | — | 5.4 ±0.1 | 2.53 |
| glyph color format | extra/color-sbix | 2.4 ±0.0 | — | — | 0.4 ±0.0 | 5.90 |
| bitmap data 16 ppem | extra/color-sbix | 5.0 ±0.0 | — | — | 5.0 ±0.0 | 0.99 |
| bitmap data 109 ppem | extra/color-sbix | 5.0 ±0.0 | — | — | 5.0 ±0.1 | 0.99 |
| bitmap data 136 ppem | extra/color-sbix | 5.1 ±0.0 | — | — | 5.5 ±0.1 | 0.93 |
| bitmap data 0 ppem | extra/color-sbix | 5.2 ±0.0 | — | — | 5.4 ±0.0 | 0.96 |
| color glyph extents | extra/color-sbix | 2.3 ±0.0 | — | — | 2.8 ±0.0 | 0.83 |
| glyph color format | extra/color-svg | 1.8 ±0.0 | — | — | 0.4 ±0.0 | 4.44 |
| SVG documents | extra/color-svg | 0.9 ±0.0 | — | — | 0.6 ±0.0 | 1.49 |
| color glyph extents | extra/color-svg | 1.6 ±0.0 | — | — | 1.5 ±0.0 | 1.10 |
| shaping role-default-und | extra/arabic | 2.9 ±0.0 | — | — | 1.7 ±0.0 | 1.73 |
| first shaping role-default-und | extra/arabic | 18.4 ±0.2 | — | — | 24.1 ±0.3 | 0.76 |
| shaping role-default-und | extra/hebrew | 2.5 ±0.0 | — | — | 1.4 ±0.0 | 1.79 |
| first shaping role-default-und | extra/hebrew | 9.2 ±0.1 | — | — | 12.3 ±0.2 | 0.75 |
| shaping role-default-und | extra/source-serif | 2.3 ±0.0 | — | — | 0.8 ±0.0 | 3.07 |
| first shaping role-default-und | extra/source-serif | 29.1 ±0.3 | — | — | 24.6 ±0.2 | 1.18 |
| shaping role-default-und TTB | extra/source-han | 0.5 ±0.0 | — | — | 0.4 ±0.0 | 1.23 |
| first shaping role-default-und TTB | extra/source-han | 186.3 ±2.0 | — | — | 35.6 ±0.3 | 5.23 |
| shaping role-default-und | extra/amstelvar | 0.8 ±0.0 | — | — | 0.5 ±0.0 | 1.65 |
| first shaping role-default-und | extra/amstelvar | 3.4 ±0.1 | — | — | 12.4 ±0.2 | 0.27 |
