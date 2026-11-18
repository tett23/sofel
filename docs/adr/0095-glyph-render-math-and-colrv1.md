# ADR 0095: 描画のクレートにCOLRv1の描画を含め、数学の関数を自前で持つ（[ADR 0087](0087-glyph-render-crate.md)と[ADR 0066](0066-color-glyph-pixels.md)の修正）

Status: accepted-pending

## 文脈

[ADR 0087](0087-glyph-render-crate.md) は、グリフを描く関数を `sofel-cli` から `#![no_std]` の新しいクレート `sofel-glyph-render` に移し、`sofel-capi` から `sofel-cli` への依存を外すと決めた。
実装を始めたところ、[ADR 0087](0087-glyph-render-crate.md) のとおりには実装できないことが分かった（2026-11-18、実装は `target/wt/adr0087` のブランチ `claude/adr0087-glyph-render` で止めてある）。

- 移すコードの一覧に、COLRv1を描くモジュールが漏れている。
- そのモジュールは、[ADR 0087](0087-glyph-render-crate.md) の依存の一覧に無い `sofel-png` を使う。
- そのモジュールは `f32` の `sqrt`、`atan2`、`floor`、`ceil` を使うが、`#![no_std]` ではこれらが使えない。

`atan2` を自前の関数に置きかえると、扇形のグラデーションの画素が、いまと変わりうる。
そのため、[ADR 0087](0087-glyph-render-crate.md) の I3（移す前と同じビットマップ）と、[ADR 0066](0066-color-glyph-pixels.md) の不変条件2（COLRv1の画素がHarfBuzzのラスタと全チャンネルで一致する）にも関わる。

利用者は、自前の `atan2` を書く案を選んだ（2026-11-18）。
どのプラットフォームでも同じ画素を出すためである。

## 調査した事項

- `crates/sofel-cli/src/glyph.rs:361` の `render_color` が、`crate::colr_raster::render` を呼ぶ。`crates/sofel-cli/src/colr_raster.rs` は339行で、[ADR 0087](0087-glyph-render-crate.md) の「移すコード」（`docs/adr/0087-glyph-render-crate.md:26-29`）に無い。
- `colr_raster.rs:185` が `sofel_png::decode_for_paint` を、`:12` が `sofel_read::GlyphId` を使う。`sofel-png` は [ADR 0087](0087-glyph-render-crate.md) の依存の一覧（`0087-…md:49`）に無い。`sofel_read` は、`sofel_font::read`（`sofel-font` の再公開）で置きかえられる。
- `colr_raster.rs` は、`floor` を `:74`、`:84`、`:155`、`:158` で、`ceil` を `:79`、`:89`、`:156`、`:157` で、`f32::sqrt` を `:116`、`:250`、`:290` で関数として渡し、`f32::atan2` を `:278` で `ColorGradient::with_atan2` に渡す。
- rustc 1.99.1（`rustup run 1.99.1`）で、`#![no_std]` のクレートに `a.sqrt()` と `a.atan2(b)` を書くと、どちらも `error[E0599]: no method named ... found for type f32` になる。
- `sofel_geom::float`（[ADR 0026](0026-harfbuzz-rounding.md)）は、`floorf` と `ceilf` を持つ。`sqrt` と `atan2` は持たない。
- HarfBuzz 15.1.0 のラスタは、扇形のグラデーションの角度をlibmの `atan2f` で（`src/hb-raster-paint.cc:1781`、`:1794`、`:1824`、`:1845`）、放射のグラデーションと画像の標本化の平方根を `sqrtf` で（`hb-raster-paint.cc:1531`、`:1577`、`:1634`、`:1682`、`hb-raster-image.cc:257`）求める。`atan2f` の結果はプラットフォームのlibmによる。
- IEEE 754 の平方根は正しく丸められるので、自前の `sqrt` は、どのプラットフォームの `sqrtf` ともビット単位で同じ結果を出せる。`atan2` には同じ保証が無い。

## 実装すること

### 移すコード（[ADR 0087](0087-glyph-render-crate.md) の修正）

- `crates/sofel-cli/src/colr_raster.rs` も `sofel-glyph-render` に移す。`sofel_read::GlyphId` は `sofel_font::read::GlyphId` に置きかえる。
- `sofel-glyph-render` の依存に `sofel-png` を足す。依存は `sofel-font`、`sofel-outline`、`sofel-raster`、`sofel-color`、`sofel-svg`、`sofel-type1`、`sofel-geom`、`sofel-png` とし、[scripts/deps.txt](../../scripts/deps.txt) に書く。

### 数学の関数

- `sofel_geom::float` に、`sqrtf(f32) -> f32` と `atan2f(f32, f32) -> f32` を足す。
  - `sqrtf` は正しく丸めた平方根とする（IEEE 754 の `squareRoot` と同じ値）。
  - `atan2f` は、決まった算術の手順だけで値を求め、プラットフォームのlibmを呼ばない。手順は、ライセンスの合うもの（MITなど）を移植するか、自分で書く。FTLなど、`sofel-geom` のライセンス（MIT）に混ざるものは参照しない。移植するときは、版とライセンスを確かめ、`THIRD_PARTY_NOTICES.md` に書く。
  - どちらも、FMAを使わない（[ADR 0026](0026-harfbuzz-rounding.md) と同じ）。
- `sofel-glyph-render` は、`floor`、`ceil`、`sqrt`、`atan2` を、すべて `sofel_geom::float` のものから使う。`f32` のメソッドは使わない。

### 照合の基準（[ADR 0066](0066-color-glyph-pixels.md) の修正）

- [ADR 0066](0066-color-glyph-pixels.md) の不変条件2の「HarfBuzz raster の BGRA」は、HarfBuzzが呼ぶ `atan2f` を `sofel_geom::float::atan2f` に差しかえてビルドした参照の出力とする。
  - `tools/hb-dump/` のビルドで、libmの `atan2f` より先に、`sofel_geom::float::atan2f` をCの関数 `atan2f` として出す小さな静的ライブラリをリンクする。
  - `sqrtf` は差しかえない（正しく丸められるので同じ値になる）。
- この差しかえにより、参照の画素とsofelの画素は、どのプラットフォームでも同じになる。

### ADRのステータス

この ADR の実装のプルリクエストで、次のステータスの行を改める。

- [ADR 0087](0087-glyph-render-crate.md)：`superseded-pending [{"part":"移すコードと依存","by":["docs/adr/0095-glyph-render-math-and-colrv1.md"]},{"part":"不変条件 I3","by":["docs/adr/0095-glyph-render-math-and-colrv1.md"]}]`（[ADR 0087](0087-glyph-render-crate.md) の残りと一緒に実装を終えたら `superseded [...]`）。
- [ADR 0066](0066-color-glyph-pixels.md)：`superseded [{"part":"不変条件 2（COLRv1 の参照）","by":["docs/adr/0095-glyph-render-math-and-colrv1.md"]}]`。

## 実装しないこと

- `sofel-raster` の `ColorGradient::with_atan2` の引数の形の変更。`sofel-glyph-render` は、`sofel_geom::float::atan2f` を渡す。
- プラットフォームのlibmと同じ `atan2f` の値を出すこと。
- HarfBuzzのソースの変更。差しかえは、参照のビルドのリンクで行う。

## 不変条件

- I1: `sofel_geom::float::sqrtf` は、すべての `f32` で、IEEE 754 の正しく丸めた平方根と等しい（NaN、負の数、±0、無限大、非正規化数を含む）。
- I2: `sofel_geom::float::atan2f` は、x86-64、aarch64、wasm32 で、同じ入力にビット単位で同じ値を返す。
- I3: `sofel-glyph-render` の関数は、`atan2` を使わない入力（扇形のグラデーションを含まないグリフ）では、移す前と同じビットマップとエラーを返す。
- I4: COLRv1 の画素は、`atan2f` を差しかえた参照の出力と、全チャンネルで一致する（[ADR 0066](0066-color-glyph-pixels.md) の不変条件2をこの参照で読む）。
- I5: `sofel-glyph-render` は `#![no_std]` で、`wasm32-unknown-unknown` にビルドできる（[ADR 0087](0087-glyph-render-crate.md) I2）。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| `sqrtf` に、すべての `f32` のビットの並び | 正しく丸めた値（基準は `f32::sqrt`、ネイティブで比べる） | I1 |
| `atan2f` に、±0、±無限大、NaN、x軸とy軸の上の値、決まった種の乱数100万組 | 3つの環境で同じハッシュ | I2 |
| 単色、線形、放射のグラデーションのCOLRv1のグリフ、COLRv0のグリフ、SVGのグリフ、Type1のグリフ、空の輪郭 | 移す前と同じ | I3 |
| 扇形のグラデーションを含むCOLRv1のグリフ | 差しかえた参照と全チャンネルで一致 | I4 |
| `cargo build --target wasm32-unknown-unknown -p sofel-glyph-render` | 成功 | I5 |

## テスト設計

実装のまえに、次のテストを書いて失敗させる。テストの名前は `adr0095_i1_...` の形とする。

- I1：`f32` のすべてのビットの並び（約43億）を、`f32::sqrt` と比べる。CIでは時間がかかるので、ネイティブのリリースのビルドで、区間を分けて走らせる。
- I2：決まった入力の列の `atan2f` の結果のハッシュを、ネイティブ（x86-64 と aarch64）とwasm32-wasip1 で出し、等しいことをCIで確かめる。
- I3：移す前のコミットの `sofel-cli` で作った出力（扇形のグラデーションを含まないもの）を期待の値として置き、移したあとの出力と比べる。
- I4：`tools/hb-dump/` を差しかえた `atan2f` でビルドし、[ADR 0066](0066-color-glyph-pixels.md) の COLRv1 の画素の照合（`gallery_colr_pixels` など）を、その参照で走らせる。差しかえの前の参照と比べて、どのグリフの画素が変わったかを記録に残す。
- I5：CIの wasm のジョブに `sofel-glyph-render` を足す。

## トレードオフ

- 参照のビルドで `atan2f` を差しかえるので、照合はHarfBuzzの「そのまま」の出力ではなく、角度の計算だけをsofelに揃えた出力と比べることになる。角度の値の違い（libmの丸め）は照合から外れ、それ以外の補間、丸め、合成の順の違いは、これまでどおり見つかる。
- 扇形のグラデーションの画素は、プラットフォームのlibmに頼っていたこれまでと比べて変わりうる。代わりに、どのプラットフォームでも同じ画素になる。
- `sqrtf` の全数の確かめは時間がかかる。CIの時間を見て、区間の分け方を決める。

## 参考文献

- [ADR 0087](0087-glyph-render-crate.md)、[ADR 0066](0066-color-glyph-pixels.md)、[ADR 0026](0026-harfbuzz-rounding.md)
- HarfBuzz `src/hb-raster-paint.cc`、`src/hb-raster-image.cc`（15.1.0）
- IEEE 754-2019 5.4.1（`squareRoot`）
