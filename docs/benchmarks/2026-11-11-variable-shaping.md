# 可変位置でのシェーピング時間

[sofel ADR 0027](../adr/0027-variable-fonts.md) に従い、SourceSerifVariable-Romanの `wght=500` で `react-dom.txt` をシェーピングした。改行で終わる29,869行について、計測前にsofelとHarfBuzz 15.1.0のグリフID、クラスタ、フラグ、4つの位置の値を照合し、nativeとwasm32-wasip1の両方で全行が一致した。

計測では、行ごとに区間の性質を推定する。sofelは性質ごとの `ShapePlan` を事前に作って使いまわし、HarfBuzzは同じフォントに `hb_shape` を繰り返し呼ぶ。フォントと `wght=500` の位置の作成、計画の作成、事前照合は計測に含めない。1回の標本は全文の全行を処理し、ウォームアップ3標本の後に30標本を取った。表は1標本の中央値と中央絶対偏差を示す。FreeTypeにはシェーピングの比較項目がない。

`cargo build --release --manifest-path bench/Cargo.toml` の後、`bench/target/release/sofel-bench --once --filter 'shaping react-dom.txt wght=500' --testdata target/testdata` で照合し、`--once` を外してnativeの時間を測った。wasm32-wasip1は `cargo build --release --manifest-path bench/Cargo.toml --target wasm32-wasip1` の後、Wasmtime 50.0.1で `wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm --filter 'shaping react-dom.txt wght=500' --testdata target/testdata` を実行した。両方ともmacOS arm64上で、Rust 1.99.0を使った。

| 環境 | 項目 | sofel (µs) | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|
| native | shaping react-dom.txt wght=500 | 269021.9 ±3242.3 | 76672.4 ±816.0 | 3.51 |
| wasm32-wasip1 | shaping react-dom.txt wght=500 | 494094.9 ±9841.0 | 117318.3 ±769.2 | 4.21 |

この記録は `codex/adr0027-variable-shaping-bench` ブランチでの測定である。計測時点ではgvarの幻の点による箱と送り幅をshapeの作業領域へ接続する作業が進行中である。接続後の値を測る場合は、新しい記録を作る。
