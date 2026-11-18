# ADR 0087: グリフを描く関数を no_std のクレートに移し、C ABI から sofel-cli の依存を外す

Status: superseded [{"part":"移すコードと依存","by":["docs/adr/0095-glyph-render-math-and-colrv1.md"]},{"part":"不変条件 I3","by":["docs/adr/0095-glyph-render-math-and-colrv1.md"]}]

## 文脈

2026-11-17 の ADR 違反の監査は、低の指摘 L5 として、`sofel-capi` が `sofel-cli` に依存していることを挙げた。
この監査は会話で報告し、`docs/audits/` には記録していない（[ADR 0081](0081-crate-layout-corrections.md) と同じ扱いである）。
[ADR 0004](0004-wasm-and-no-std.md) は、`sofel-cli` を wasm にビルドしないと決めている。
ところが、CI は `sofel-capi` を `wasm32-unknown-unknown` にビルドするので、依存する `sofel-cli` のライブラリも wasm にビルドされる。

`sofel-capi` がライブラリのコードで `sofel-cli` から使うのは、グリフを描く関数だけである。
この ADR は、その関数を置く場所を決め、[ADR 0004](0004-wasm-and-no-std.md) に合わせる。

## 調査した事項

### sofel-capi が使うもの

- `crates/sofel-capi/Cargo.toml:24` は `sofel-cli = { path = "../sofel-cli" }` である。
- `sofel-capi` のライブラリのコードが使う `sofel-cli` の項目は次のとおりである（`grep -n sofel_cli`）。
  - `crates/sofel-capi/src/lib.rs:30` の `sofel_cli::glyph::RenderError`
  - `lib.rs:3815` の `sofel_cli::glyph::render`
  - `lib.rs:3900` の `sofel_cli::glyph::{ColorGlyph, render_color}`
- テストの `crates/sofel-capi/tests/fixture.rs` は、`:1047-1050` で `glyph::render` と `RenderError` を、`:1550` で `sofel_cli::paint::PaintText` を使う。

### 移すコード

- `crates/sofel-cli/src/glyph.rs` は 442 行である。`std::fmt` と `std::error::Error` を使い（`glyph.rs:7`、`:60`）、`sofel-color`、`sofel-outline`、`sofel-raster`、`sofel-type1`、`sofel-font` に依存する（`glyph.rs:9-18`）。`crate::color_raster::SvgImagePainter` も使う（`glyph.rs:40`）。
- `crates/sofel-cli/src/color_raster.rs` は 219 行で、`sofel-raster` と `sofel-svg` に依存する（`color_raster.rs:3-7`）。`SvgImagePainter` は、同じファイルの `render_glyph_scene`（`:80`）と `render_scene`（`:69`）と、子のモジュール `color_raster/clip.rs`（`:66` の `mod clip;`）を使う。
- `sofel-cli` の中で `glyph` と `color_raster` を使うのは、`render.rs`、`text_image.rs`、`glyph.rs` と、テストの `tests/svg_clip.rs`、`tests/svg_color_raster.rs` である（`rg -l`）。

### 層と検査

- [scripts/deps.txt](../../scripts/deps.txt) で、`sofel-color` は `sofel-outline`、`sofel-bitmap`、`sofel-svg`、`sofel-geom` に依存し、`sofel-raster` には依存しない。`sofel-capi` と `sofel-cli` は `*` で、何にでも依存してよい。
- [scripts/check-deps.sh](../../scripts/check-deps.sh) は、依存の種類（通常、開発用、ビルド用）を区別せずに検査する（`check-deps.sh:9-10`）。
- [ADR 0004](0004-wasm-and-no-std.md) の「実装しないこと」は、「`sofel-cli` のwasmへのビルド。ファイルを読む道具なので、ネイティブだけとする。」と書く。
- `.github/workflows/ci.yml:361` は `-p sofel-capi` を含めて `wasm32-unknown-unknown` にビルドする。続く `wasm32-wasip1` のテストの対象に、`sofel-capi` と `sofel-cli` は無い。
- `sofel-cli` には `sofel-render` という名前のバイナリがある（`crates/sofel-cli/Cargo.toml:34-36`）。

## 実装すること

### 新しいクレート

- 新しいクレート `sofel-glyph-render` を作り、`#![no_std]` と `extern crate alloc` で書く。名前は、既存のバイナリ `sofel-render` と区別するために選んだ。
- 次を `sofel-glyph-render` に移す。
  - `glyph.rs` の全体（`render`、`render_type1`、`render_bare_cff`、`render_cid`、`render_pfr`、`render_mono`、`render_color`、`render_colr_v0`、`color_layers`、`ColorGlyph`、`RenderError`）
  - `color_raster.rs` の全体と、子のモジュール `color_raster/clip.rs`
- `std::fmt` は `core::fmt` に、`std::error::Error` は `core::error::Error` に置き換える。
- `sofel-glyph-render` の依存は、`sofel-font`、`sofel-outline`、`sofel-raster`、`sofel-color`、`sofel-svg`、`sofel-type1`、`sofel-geom` とし、[scripts/deps.txt](../../scripts/deps.txt) に書く。

### 使う側

- `sofel-cli` は `sofel-glyph-render` を使う。移した関数を `sofel_cli::glyph` のパスで再公開しない。
- `sofel-cli` の中とテストの `use` を、新しいクレートのパスに書き換える。
- `sofel-capi` は `sofel-glyph-render` を使い、通常の依存から `sofel-cli` を外す。
- `sofel-capi` のテストが使う `PaintText` は、開発用の依存（`dev-dependencies`）の `sofel-cli` から使う。テストはネイティブでだけ動かすためである。

### 検査と CI

- [scripts/deps.txt](../../scripts/deps.txt) の `sofel-capi *` を、通常の依存を一つずつ書く形に変える。
- [scripts/check-deps.sh](../../scripts/check-deps.sh) に、開発用の依存だけを許す指示を足す（例: `!dev sofel-capi sofel-cli`）。通常の依存とビルド用の依存は、これまでどおり `deps.txt` の行で検査する。
- CI の `wasm32-unknown-unknown` のビルドと、`wasm32-wasip1` のテストに、`-p sofel-glyph-render` を足す。
- [docs/specifications.md](../specifications.md) の「構成」の図とクレートの責務の表に `sofel-glyph-render` を足し、`sofel-cli` と `sofel-capi` の行を直す。

## 実装しないこと

- 関数の振る舞いの変更。移すだけとし、引数、戻り値、エラーの種類を変えない。
- `sofel-cli` の他のモジュール（`paint`、`render`、`text_image` など）の移動。`sofel-capi` のライブラリのコードが使わないためである。

## 不変条件

- I1: `sofel-capi` の通常の依存は、`sofel-cli` を推移的にも含まない。
- I2: `sofel-glyph-render` は `#![no_std]` で、`wasm32-unknown-unknown` にビルドできる。
- I3: 移した関数は、同じ入力に、移す前と同じビットマップとエラーを返す。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| `cargo tree -p sofel-capi -e normal` | `sofel-cli` が出ない | I1 |
| `sofel-capi` の `dev-dependencies` の `sofel-cli` | `check-deps.sh` が通る | I1 |
| `sofel-capi` の `dependencies` に `sofel-cli` を戻す | `check-deps.sh` が失敗する | I1 |
| `cargo build --target wasm32-unknown-unknown -p sofel-glyph-render` | 成功する | I2 |
| 空の輪郭のグリフ、1輪郭のグリフ、色のグリフ、SVG のグリフ、Type1 のグリフ | 移す前の結果と同じ | I3 |
| 描けない輪郭（`OutlineError`）、ラスタライザのエラー | 移す前と同じエラーの種類 | I3 |

## 受けいれ条件

- A1: [scripts/check-deps.sh](../../scripts/check-deps.sh) が、`sofel-capi` から `sofel-cli` への通常の依存を拒み、開発用の依存を許す。
- A2: 既存の `sofel-cli` と `sofel-capi` のテストが、`use` のパスを除いて変えずに通る。

## テスト設計

- `ADR 0087 I1: ...`: `scripts/tests/` のシェルのテストで、一時ディレクトリのワークスペースに境界の表の依存の行を作り、`check-deps.sh` の終了コードを確かめる。
- `adr0087_i2_...`: CI の wasm のビルドに `sofel-glyph-render` を加え、`check-deps.sh` の `!no_std` の行で `#![no_std]` を確かめる。
- `adr0087_i3_...`: 既存の `sofel-capi` の fixture のテストと、`sofel-cli` の glyph、`svg_clip`、`svg_color_raster` のテストを、移した後も `use` のパスのほかは変えずに動かす。

## トレードオフ

- クレートが一つ増える。一方、C ABI と CLI の両方が使う描画の関数を、wasm でも使えるライブラリとして置ける。
- `sofel-color` に `sofel-raster` の依存を足して関数を置く道もある。`sofel-color` は塗りの命令を作る層で、画素に描く層と分けておく方が、各クレートの責務が一つに保てるため、退けた。
- `sofel-cli` で同じパスに再公開すれば、`use` の書き換えは要らない。再公開しないのは、同じ関数を二つのパスで公開しないためである。

## 参考文献

- [ADR 0004: wasmへのビルドを必須とし、ライブラリを `no_std` で書く](0004-wasm-and-no-std.md)
- [ADR 0003: クレートの層と依存の向き](0003-crate-layers.md)（クレートの表は [ADR 0081](0081-crate-layout-corrections.md) が直した部分を含めて読む）
- [ADR 0081: クレートの置き場所を ADR 0003 の表に戻し、WOFF の依存を足す](0081-crate-layout-corrections.md)
