# ADR 0004: wasmへのビルドを必須とし、ライブラリを `no_std` で書く

ステータス: 採択

## 文脈

sofelは、wasmへのビルドを必須とする（[ADR 0001](0001-project-goal-and-scope.md)）。
ブラウザで動く `wasm32-unknown-unknown` には、ファイルシステム、スレッド、時刻、動的リンクが無い。
FreeTypeはHarfBuzzを `dlopen` で読み込む方式を持つが（`src/autofit/ft-hb.c`）、wasmでは使えない。

`std` を使わずに書けば、wasmでも組み込みの環境でも同じコードが動く。
シェーピングのバッファや輪郭の命令列には確保が要るため、`alloc` は使う。

## 実装すること

- ライブラリのクレート（`sofel-cli` 以外）は `#![no_std]` とし、確保が要るクレートだけ `extern crate alloc` を使う。`sofel-read` は確保をしない（[ADR 0003](0003-crate-layers.md)）。
- `std` の機能を足すfeatureは作らない。`std` が無くて困る箇所が出たら、ADRで判断する。
- `core` に無い浮動小数点の関数（平方根、三角関数など）が要るときは、自分で書く。HarfBuzzとFreeTypeは多くの計算を固定小数点で行うため、まず固定小数点で書けないかを検討する。
- CIに、ライブラリのクレートを次のターゲットでビルドするジョブを足す（最初のクレートを作るADRの実装で）。
  - `wasm32-unknown-unknown`：`cargo build --release --target wasm32-unknown-unknown` で、ライブラリのクレートをビルドする。
  - `wasm32-wasip1`：ライブラリのクレートの単体テストを、wasmtimeで実行する。`usize` が32bitの環境での桁あふれを見つけるため。
- SIMDは、まずスカラーの実装を書き、計測で必要と示せた箇所だけ足す。wasmのsimd128を使う場合も、スカラーの実装を残す。

## 実装しないこと

- JavaScriptからの呼び出しの層（wasm-bindgenなど）とnpmのパッケージ。必要になったら別に判断する。
- wasmでのスレッド（SharedArrayBuffer）。
- `sofel-cli` のwasmへのビルド。ファイルを読む道具なので、ネイティブだけとする。

## テスト設計

- 最初のクレートを作るプルリクエストで、CIの2つのジョブ（`wasm32-unknown-unknown` のビルド、`wasm32-wasip1` の単体テスト）が成功することを確かめる。
- `no_std` は、ライブラリのクレートの `lib.rs` の `#![no_std]` と、`wasm32-unknown-unknown` のビルドの成功で確かめる。
- わざと `std::` を使う変更を手元で入れ、`wasm32-unknown-unknown` のビルドが失敗することを一度確かめる。

## トレードオフ

- `no_std` のため、`HashMap` などの `std` の集まりが使えない。必要なものは `alloc` の `BTreeMap` か自分の実装で置きかえる。速さのために、HarfBuzzのビット集合（`hb-bit-set.hh`）のような専用の構造を自分で書くことになる。
- `core` の浮動小数点の関数が足りないため、自分で書く関数の精度と速さを検査する手間が増える。
- wasmtimeでのテストは、CIの時間を延ばす。ネイティブの単体テストと同じテストを2度走らせるためである。

## 参考文献

- FreeType `src/autofit/ft-hb.c`（2.15.0）
- HarfBuzz `src/hb-bit-set.hh`（15.1.0）
- The Embedded Rust Book, "no_std" https://docs.rust-embedded.org/book/intro/no-std.html
- Wasmtime https://wasmtime.dev/
