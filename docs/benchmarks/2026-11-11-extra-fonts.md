# 追加フォントの計測（2026-11-11）

[ADR 0031](../adr/0031-diverse-font-corpus.md) で追加した13ファイルの代表項目を測った記録。項目の意味と照合の方法は [ADR 0013](../adr/0013-benchmark-harness.md) と [ADR 0006](../adr/0006-performance-measurement.md) に従う。値は1回あたりの中央値（±中央絶対偏差）で、単位はマイクロ秒。比が1未満ならsofelが速い。

`cmap` は10個のUnicodeスカラー値から得るグリフIDの合計を比較する。`advance` は先頭から最大128グリフの横advanceの合計を比較する。どちらも各ファイルのフェイス0と既定座標を使う。TTCの残り44フェイスと輪郭の正しさ、および可変軸の未実装ケースの分類は [ADR 0031](../adr/0031-diverse-font-corpus.md) の適合テストで確かめる。

## 環境と実行

- 機械：Apple Silicon（arm64）。メモリ容量はこの実行環境で取得できず、未確認。
- OS：macOS 27.0（26A428）。
- Rust：rustc 1.99.0（b940084d7 2026-09-28）。
- C/C++：Apple clang 21.0.0、`-O2 -DNDEBUG`。
- 比較相手：FreeType 2.15.0（`37515cf56ba3349656049edbed89ffc6f6466f2d`）、HarfBuzz 15.1.0（`3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`）。
- sofel：`codex/implement-adr-0031` の未コミット変更、基点は `20e2e46`。この記録はmainへ入る前の測定である。

`sh bench/build-c.sh` でCライブラリを作り、`cargo run --release --manifest-path bench/Cargo.toml -- --once --filter extra` は26項目の照合に成功した。数値は `/usr/bin/time -l bench/target/release/sofel-bench --filter extra > /tmp/sofel-adr0031-bench.txt` の標準出力である。ベンチマーク本体は全26項目を終了したが、`/usr/bin/time -l` がsandbox内の `sysctl kern.clockrate` を読めず、ラッパーの終了状態は1だった。ラッパーが出した実時間は41.10秒であり、以下の項目別の値はベンチマーク本体の出力から転記した。負荷の高い共有環境での1回の測定なので、速度比は目安である。

## native（aarch64-apple-darwin）

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| cmap | extra/arabic | 0.1 ±0.0 | 0.1 ±0.0 | 1.01 | 0.1 ±0.0 | 0.89 |
| advance | extra/arabic | 0.2 ±0.0 | 1.4 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| cmap | extra/devanagari | 0.2 ±0.1 | 0.1 ±0.0 | 1.55 | 0.2 ±0.1 | 0.97 |
| advance | extra/devanagari | 0.3 ±0.0 | 1.6 ±0.1 | 0.17 | 1.6 ±0.6 | 0.16 |
| cmap | extra/khmer | 0.1 ±0.0 | 0.1 ±0.0 | 1.22 | 0.1 ±0.0 | 1.14 |
| advance | extra/khmer | 0.2 ±0.0 | 1.4 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| cmap | extra/myanmar | 0.2 ±0.1 | 0.1 ±0.0 | 2.02 | 0.2 ±0.1 | 1.18 |
| advance | extra/myanmar | 0.3 ±0.1 | 2.3 ±0.7 | 0.13 | 1.0 ±0.0 | 0.28 |
| cmap | extra/balinese | 0.1 ±0.0 | 0.1 ±0.0 | 0.78 | 0.2 ±0.1 | 0.49 |
| advance | extra/balinese | 0.2 ±0.0 | 1.5 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| cmap | extra/hebrew | 0.1 ±0.0 | 0.1 ±0.0 | 1.09 | 0.1 ±0.0 | 0.95 |
| advance | extra/hebrew | 0.2 ±0.0 | 1.4 ±0.0 | 0.16 | 1.1 ±0.0 | 0.22 |
| cmap | extra/source-serif | 0.1 ±0.0 | 0.1 ±0.0 | 1.06 | 0.1 ±0.0 | 0.91 |
| advance | extra/source-serif | 0.2 ±0.0 | 1.5 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| cmap | extra/stix-math | 0.1 ±0.0 | 0.1 ±0.0 | 1.08 | 0.1 ±0.0 | 1.02 |
| advance | extra/stix-math | 0.2 ±0.0 | 1.5 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| cmap | extra/source-han | 0.1 ±0.0 | 0.2 ±0.0 | 0.84 | 0.1 ±0.0 | 0.99 |
| advance | extra/source-han | 0.2 ±0.0 | 1.6 ±0.1 | 0.15 | 1.0 ±0.0 | 0.23 |
| cmap | extra/amstelvar | 0.1 ±0.0 | 0.1 ±0.0 | 1.11 | 0.1 ±0.0 | 0.84 |
| advance | extra/amstelvar | 0.2 ±0.0 | 1.4 ±0.0 | 0.16 | 1.0 ±0.0 | 0.23 |
| cmap | extra/color-cff | 0.1 ±0.0 | 0.1 ±0.0 | 1.19 | 0.1 ±0.0 | 1.01 |
| advance | extra/color-cff | 0.1 ±0.0 | 0.4 ±0.0 | 0.16 | 0.2 ±0.0 | 0.25 |
| cmap | extra/color-sbix | 0.1 ±0.0 | 0.1 ±0.0 | 1.19 | 0.1 ±0.0 | 1.00 |
| advance | extra/color-sbix | 0.1 ±0.0 | 0.3 ±0.0 | 0.17 | 0.2 ±0.0 | 0.25 |
| cmap | extra/color-svg | 0.1 ±0.0 | 0.1 ±0.0 | 1.19 | 0.1 ±0.0 | 1.02 |
| advance | extra/color-svg | 0.1 ±0.0 | 0.3 ±0.0 | 0.17 | 0.2 ±0.0 | 0.25 |

## wasm（wasm32-wasip1、wasmtime）

wasm版のCライブラリを `sh bench/build-c.sh wasm32-wasip1` で作り直した。`wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm --once --filter extra` は26項目の照合に成功した。続いて同じ実行ファイルを `--filter extra` で測定し、終了状態0、実時間41.724秒だった。ローカルのmise shimはwasmtimeの既定版が未設定だったため、実測にはインストール済みのwasmtime 50.0.1の実行ファイルを直接指定した。

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| cmap | extra/arabic | 0.2 ±0.0 | 0.3 ±0.0 | 0.70 | 0.4 ±0.0 | 0.55 |
| advance | extra/arabic | 0.2 ±0.0 | 4.8 ±0.0 | 0.05 | 2.8 ±0.1 | 0.08 |
| cmap | extra/devanagari | 0.2 ±0.0 | 0.3 ±0.0 | 0.74 | 0.4 ±0.0 | 0.54 |
| advance | extra/devanagari | 0.2 ±0.0 | 4.9 ±0.0 | 0.05 | 2.8 ±0.1 | 0.08 |
| cmap | extra/khmer | 0.2 ±0.0 | 0.3 ±0.0 | 0.71 | 0.3 ±0.0 | 0.73 |
| advance | extra/khmer | 0.2 ±0.0 | 4.9 ±0.0 | 0.05 | 2.9 ±0.1 | 0.08 |
| cmap | extra/myanmar | 0.2 ±0.0 | 0.3 ±0.0 | 0.72 | 0.3 ±0.0 | 0.73 |
| advance | extra/myanmar | 0.2 ±0.0 | 4.9 ±0.1 | 0.05 | 2.9 ±0.1 | 0.08 |
| cmap | extra/balinese | 0.2 ±0.0 | 0.3 ±0.0 | 0.69 | 0.3 ±0.0 | 0.70 |
| advance | extra/balinese | 0.2 ±0.0 | 4.9 ±0.1 | 0.05 | 2.8 ±0.0 | 0.08 |
| cmap | extra/hebrew | 0.2 ±0.0 | 0.3 ±0.0 | 0.70 | 0.3 ±0.0 | 0.72 |
| advance | extra/hebrew | 0.2 ±0.0 | 4.8 ±0.1 | 0.05 | 2.8 ±0.1 | 0.08 |
| cmap | extra/source-serif | 0.2 ±0.0 | 0.3 ±0.0 | 0.71 | 0.4 ±0.0 | 0.57 |
| advance | extra/source-serif | 0.2 ±0.0 | 4.9 ±0.1 | 0.05 | 2.7 ±0.0 | 0.08 |
| cmap | extra/stix-math | 0.3 ±0.0 | 0.4 ±0.0 | 0.78 | 0.4 ±0.0 | 0.63 |
| advance | extra/stix-math | 0.2 ±0.0 | 5.0 ±0.1 | 0.05 | 2.9 ±0.2 | 0.08 |
| cmap | extra/source-han | 0.3 ±0.0 | 0.4 ±0.0 | 0.73 | 0.4 ±0.0 | 0.66 |
| advance | extra/source-han | 0.2 ±0.0 | 4.9 ±0.1 | 0.05 | 2.8 ±0.1 | 0.08 |
| cmap | extra/amstelvar | 0.2 ±0.0 | 0.3 ±0.0 | 0.69 | 0.3 ±0.0 | 0.64 |
| advance | extra/amstelvar | 0.2 ±0.0 | 4.9 ±0.1 | 0.05 | 2.8 ±0.1 | 0.08 |
| cmap | extra/color-cff | 0.1 ±0.0 | 0.2 ±0.0 | 0.76 | 0.2 ±0.0 | 0.65 |
| advance | extra/color-cff | 0.1 ±0.0 | 1.2 ±0.0 | 0.05 | 0.7 ±0.0 | 0.09 |
| cmap | extra/color-sbix | 0.1 ±0.0 | 0.2 ±0.0 | 0.75 | 0.2 ±0.0 | 0.65 |
| advance | extra/color-sbix | 0.1 ±0.0 | 1.1 ±0.0 | 0.05 | 0.6 ±0.0 | 0.09 |
| cmap | extra/color-svg | 0.2 ±0.0 | 0.2 ±0.0 | 0.82 | 0.2 ±0.0 | 0.72 |
| advance | extra/color-svg | 0.1 ±0.0 | 1.2 ±0.1 | 0.05 | 0.6 ±0.0 | 0.09 |
