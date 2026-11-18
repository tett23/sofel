# ADR 0097: `atan2f` の差しかえを HarfBuzz raster の参照のビルドで行う（[ADR 0095](0095-glyph-render-math-and-colrv1.md)の修正）

Status: accepted

## 文脈

[ADR 0095](0095-glyph-render-math-and-colrv1.md) は、COLRv1 の画素の照合の参照を、HarfBuzz が呼ぶ `atan2f` を `sofel_geom::float::atan2f` に差しかえてビルドしたものとした。
差しかえる場所として、[ADR 0095](0095-glyph-render-math-and-colrv1.md) は `tools/hb-dump/` のビルドを挙げた。
しかし、COLRv1 の画素の参照を作るのは `tools/hb-dump/` ではない。
実装（PR #372、2026-11-18）は、参照を作る `tools/hb-raster-reference/` のビルドで差しかえた。
この ADR は、差しかえる場所の記述を、実装のとおりに直す。

## 調査した事項

- [ADR 0095](0095-glyph-render-math-and-colrv1.md) は、差しかえる場所を `tools/hb-dump/` と書く（`docs/adr/0095-glyph-render-math-and-colrv1.md:48` の実装すること、`:88` のテスト設計）。
- `tools/hb-dump/hb-dump.cc` に、`raster` を含む行は無い（`grep -c 'hb_raster\|raster' tools/hb-dump/hb-dump.cc` は `0`）。
- `tools/hb-raster-reference/pixels.cc:30` は `hb_raster_paint_create_or_fail()` を呼び、HarfBuzz raster で画素を作る。このツールは、コミット `a2260393`（Rasterize COLRv1 gradient fills and clip glyphs against HarfBuzz）で足された。
- `tools/hb-raster-reference/build.sh:20-27` は、既定で `tools/hb-raster-reference/atan2f/` のクレートを `cargo rustc ... --emit=obj` で1つのオブジェクト `atan2f.o` にし、参照のリンクに渡す。`SOFEL_HB_RASTER_ATAN2F=libm` のときは渡さず、プラットフォームの `atan2f` のままビルドする。
- `tools/hb-raster-reference/atan2f/lib.rs:14-15` は、`#[unsafe(no_mangle)] pub extern "C" fn atan2f(y: f32, x: f32) -> f32` を出す。
- `tools/hb-raster-reference/atan2f/Cargo.toml` は、release のプロファイルを `lto = "fat"`、`codegen-units = 1`、`panic = "abort"` とし、その理由を「sofel-geom と core を含めた1つのオブジェクトにし、compiler_builtins の部品をリンクに入れないため」とコメントに書く。
- aarch64-apple-darwin で `build.sh` と同じ `cargo rustc --release ... -- --emit=obj=<dir>/atan2f.o` を動かし、`nm -g` を取ると、出力は `T _atan2f` の1行だけだった（定義は `atan2f` の1つ、未定義の記号は無い。macOS の記号の名前には `_` が付く）。
- `.github/workflows/ci.yml:258-261` は、`SOFEL_HB_RASTER_ATAN2F=libm` で作った参照と既定の参照を `tools/hb-raster-reference/atan2f-changes.sh` で比べ、既定の参照で `gallery_colr_pixels` を走らせる。
- `docs/specifications.md:788-790` は、すでに `tools/hb-raster-reference/build.sh` での差しかえを書いている。

## 実装すること

### 差しかえる場所（[ADR 0095](0095-glyph-render-math-and-colrv1.md) の修正）

- [ADR 0095](0095-glyph-render-math-and-colrv1.md) の「照合の基準」の、`tools/hb-dump/` のビルドで差しかえるという記述を、次のとおり読みかえる。
  - HarfBuzz raster の参照（`tools/hb-raster-reference/build.sh`）のビルドで、`sofel_geom::float::atan2f` を C の関数 `atan2f` として出すクレート（`tools/hb-raster-reference/atan2f/`）を、C のライブラリより先にリンクする。
  - そのクレートは、静的ライブラリとしてではなく、fat LTO で1つのオブジェクトにしてリンクする。このオブジェクトが定義する外向きの記号は `atan2f` だけとし、ほかの libm の関数を置きかえない。
  - `SOFEL_HB_RASTER_ATAN2F=libm` のときは、差しかえずにビルドする。差しかえで画素が変わるグリフの一覧は、この2つの参照を `tools/hb-raster-reference/atan2f-changes.sh` で比べて出す。
- [ADR 0095](0095-glyph-render-math-and-colrv1.md) のテスト設計の I4 の `tools/hb-dump/` も、`tools/hb-raster-reference/` と読みかえる。
- `docs/specifications.md` は、いまのまま（すでにこの ADR のとおり）とする。

### ADRのステータス

この ADR を採択したら、[ADR 0095](0095-glyph-render-math-and-colrv1.md) のステータスの行を `superseded [{"part":"照合の基準（差しかえる場所）","by":["docs/adr/0097-atan2f-substitution-in-raster-reference.md"]},{"part":"テスト設計（I4 のビルド）","by":["docs/adr/0097-atan2f-substitution-in-raster-reference.md"]}]` に改める。
差しかえの実装は PR #372 で終わっている。
この ADR の実装は、不変条件 I1 と I2 の検査を足すことと、上のステータスの行の変更である。

## 実装しないこと

- `tools/hb-dump/` に HarfBuzz raster の処理を足すこと。
- [ADR 0095](0095-glyph-render-math-and-colrv1.md) のほかの決定（`sqrtf` を差しかえないこと、不変条件 I1 から I5）の変更。

## 不変条件

- I1: 既定の `tools/hb-raster-reference/build.sh` がリンクに渡す `atan2f.o` が定義する外向きの記号は `atan2f` だけで、未定義の記号を持たない。
- I2: `SOFEL_HB_RASTER_ATAN2F=libm` のときの `tools/hb-raster-reference/build.sh` は、`atan2f.o` をリンクに渡さない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 既定の `build.sh` で作った `atan2f.o` に `nm -g` | 定義は `atan2f` の1つ、未定義は0 | I1 |
| `SOFEL_HB_RASTER_ATAN2F=libm` の `build.sh` | `atan2f.o` を作らず、リンクにも渡さない | I2 |

## テスト設計

- I1：`atan2f.o` を作り、`nm -g` の出力の定義の記号が `atan2f` だけであり、未定義の記号が無いことを確かめる検査を `scripts/tests/` に足す（名前に `ADR 0097 I1` を含む）。
- I2：`build.sh` の、`SOFEL_HB_RASTER_ATAN2F=libm` のときのリンクの引数を確かめる検査を `scripts/tests/` に足す（名前に `ADR 0097 I2` を含む）。
- 実装は PR #372 で終わっているので、テストを先に書いて失敗させる手順は踏めない。代わりに、I1 の検査で、`lto = "fat"` を外した写しのクレートでは失敗することを確かめる。

## トレードオフ

- 差しかえのためのクレートを、ワークスペースの外の別のクレートとして持つので、`sofel-geom` を変えたときに、このクレートのビルドも確かめる手間がある。代わりに、参照のビルドだけが使うものを、ワークスペースのクレートの一覧に入れずに済む。

## 参考文献

- [ADR 0095](0095-glyph-render-math-and-colrv1.md)、[ADR 0066](0066-color-glyph-pixels.md)
- PR #372（2026-11-18）
