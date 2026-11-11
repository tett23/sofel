# 幻の点による輪郭の移動を含むラスタライズの照合

2026年11月11日、コミット `a045574357e3d9bba9a77a28713bf5b2befcc14d` で [ADR 0057](../adr/0057-record-unexcluded-phantom-point-check.md) の照合を行った。[以前の計測](2026-11-10-stage2.md)で除外したグリフも含め、sofel と FreeType の画素値の合計を比較した。

## 入力と実行環境

macOS 27.0（26A428、arm64）上で、Rust 1.99.0（b940084d7）、WASI SDK 34、Wasmtime 50.0.1 を使った。ネイティブのターゲットは `aarch64-apple-darwin`、wasm は `wasm32-wasip1` である。

参照ライブラリとテストデータは [testdata.lock](../../testdata.lock) の HarfBuzz `3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372` と FreeType `37515cf56ba3349656049edbed89ffc6f6466f2d` を使った。追加フォントは [fonts.lock](../../fonts.lock)（SHA-256 `72216620f254d81f4b852a2190fb9d6b1f8e043d9fb06d4c1a04db0c4152bf06`）で検証した。リポジトリのルートから、両環境でフィルターを指定せずに実行した。

```sh
sh scripts/fetch-testdata.sh
sh scripts/fetch-fonts.sh
sh scripts/build-bench-c.sh
cargo run --release --manifest-path bench/Cargo.toml -- --once
cargo build --release --manifest-path bench/Cargo.toml --target wasm32-wasip1
wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm --once
```

## 照合結果

標準エラーに出たグリフ数は両環境で同じだった。[ラスタライズの照合処理](../../bench/src/main.rs)は各サイズでグリフ番号 `0..n` を両実装に渡し、全グリフの画素値の合計を比較する。表の「一致」はこの合計が一致したことを示す。

| フォント | 環境 | 対象グリフ数 | 以前の除外数 | 今回の除外数 | 12 ppem | 24 ppem | 48 ppem | 96 ppem |
| --- | --- | ---: | ---: | ---: | --- | --- | --- | --- |
| Roboto-Regular | native | 3,359 | 52 | 0 | 一致 | 一致 | 一致 | 一致 |
| Roboto-Regular | wasm | 3,359 | 52 | 0 | 一致 | 一致 | 一致 | 一致 |
| Comfortaa-Regular-new | native | 879 | 6 | 0 | 一致 | 一致 | 一致 | 一致 |
| Comfortaa-Regular-new | wasm | 879 | 6 | 0 | 一致 | 一致 | 一致 | 一致 |

フィルターなしの `--once` は native、wasm ともに終了コード0で完了した。wasm の実行には Wasmtime 50.0.1 の実体を指定した。
