# ADR 0023: カラーグリフと絵文字（`Painter`、`sofel-color`、`sofel-bitmap`）

ステータス: 採択→丸めは [ADR 0026](0026-harfbuzz-rounding.md)

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の5段階目は、カラーグリフと絵文字である（COLR、CPAL、CBDT、sbix、EBDT、SVGのグリフと、PNGとDEFLATEの展開器）。
[ADR 0003](0003-crate-layers.md) は、塗りの命令を `sofel-geom` の `Painter` で受け渡し、`sofel-color` がカラーグリフを `Painter` に渡し、`sofel-bitmap` がビットマップのグリフを扱うと決めた。
[ADR 0009](0009-geom-fixed-point-and-pen.md) は `Pen` だけを定め、`Painter` をこの段階のADRに回した。

sofelは、HarfBuzzとFreeTypeを置きかえる汎用のライブラリである。
カラーグリフについても、両者が提供する機能を覆い、その上で今の描画の仕組みに合う形にする。
この節では、両者が提供するものと、利用者の必要をまとめる。

### 利用者の必要

カラーグリフの利用者には、次の種類がある。

- 画面に描く利用者：グリフを画素（RGBA）にする。FreeTypeの `FT_LOAD_COLOR` の利用者や、ブラウザ、UIのツールキットである。描画の仕組み（Skia、Cairo、Direct2D、Core Graphics、GPUのシェーダー）は、変換、クリップ、グラデーション、合成のグループという共通の命令を持つ。
- ベクトルで書き出す利用者：PDFやSVGに塗りの命令として書く。最初の利用者のGolem（PDF/X-4を書く組版の道具）がこれに当たる。
- パレットを選ぶ利用者：CSSの `font-palette` と `@font-palette-values` のように、明るい背景向けと暗い背景向けのパレットを選び、パレットの一部の色を差しかえる。
- フォントを選ぶ利用者：文字を持たないフォントの代わりを探す（フォールバック）。フォントの選択はsofelの範囲の外である（[ADR 0001](0001-project-goal-and-scope.md)）が、選ぶための問い合わせ（文字を持つか、グリフがカラーか）は安く返せる必要がある。Golemのロードマップにも「絵文字のレンダリング」と「フォントのフォールバック」がある（Golemの `docs/roadmap.md`）。
- 古いフォントの利用者：輪郭を持たず、埋め込みビットマップだけを持つsfnt（日本語の古い画面用のフォントなど）を、FreeTypeと同じく扱う。

どの利用者にも、結果がx86-64、aarch64、wasm32-wasiで同じになることを保証する（Golemの要件でもある）。

### HarfBuzz 15.1.0

HarfBuzzは、カラーグリフを `hb_font_paint_glyph` で塗りの関数の組（`hb_paint_funcs_t`、`src/hb-paint.h`）に渡す。
関数は次のとおりである（`src/hb-paint.hh` の `HB_PAINT_FUNCS_IMPLEMENT_CALLBACKS`）。

- 変換：`push_transform(xx, yx, xy, yy, dx, dy)`、`pop_transform`。平行移動、拡大、回転、傾斜とその中心つきの形は、`src/hb-paint.hh` で1つの変換の行列に直してから `push_transform` に渡す。回転は `sincosf`、傾斜は `tanf` で求める（`src/hb-geometry.hh`、`src/hb-algs.hh`）。
- クリップ：`push_clip_glyph(gid)`、`push_clip_rectangle(xmin, ymin, xmax, ymax)`、`pop_clip`。ほかに `push_clip_path_start` と `push_clip_path_end` があるが、COLRの塗りはこれを呼ばない。
- 塗り：`color(is_foreground, color)`、`fill_glyph(gid, is_foreground, color)`（14.3.0から。無ければクリップ、`color`、クリップの解除に分けて呼ぶ）、`image(blob, width, height, format, slant, extents)`、`linear_gradient(color_line, p0, p1, p2)`、`radial_gradient(color_line, c0, r0, c1, r1)`、`sweep_gradient(color_line, center, start_angle, end_angle)`。角度は弧度で、COLRの値に1を足してπを掛ける。
- グループ：`push_group`、`push_group_for(mode)`（14.2.0から）、`pop_group(mode)`。合成の方法は28種類（Porter-Duffの12種類と、分離と非分離のブレンドの16種類）。
- そのほか：`color_glyph(gid)`（`PaintColrGlyph` の先を呼び出し側が描けば真を返す）、`custom_palette_color(index)`（パレットの1色を差しかえる）、計算の量の上限（`set_budget` など）。
- 色の線（`hb_color_line_t`）は、色の停止点（位置、色、前景色か）と延長の方法（`PAD`、`REPEAT`、`REFLECT`）を返す。停止点はフォントに書かれた順のまま返し、並べかえは受け取る側が `hb_paint_normalize_color_line` で行う。
- 色は、乗算済みでないsRGBの8bitのBGRAで、パレットの色にアルファを掛けて丸めた値である（`hb_color_t`）。

`hb_ot_paint_glyph_or_fail`（`src/hb-ot-font.cc`）は、COLR、SVG、CBDT、sbixの順に試し、最初に塗れたもので終える。どれでもなければ `hb_font_paint_glyph` は輪郭を前景色で `fill_glyph` する（`src/hb-font.cc`）。

- COLRv1（`src/OT/Color/COLR/COLR.hh`）：根に `push_font_transform`（`x_scale / upem` の拡大）を置く。クリップの箱（ClipList）があれば `push_clip_rectangle` で囲む。無ければ、先に塗りの木を `hb_paint_bounded`（`src/hb-paint-bounded.cc`）でたどり、範囲が有限でなければ何も塗らない。入れ子の深さ64（`HB_MAX_NESTING_LEVEL`）、辺の数16384（`HB_MAX_GRAPH_EDGE_COUNT`）、グリフごとの計算の量2^24（`HB_BUDGET_GLYPH`）で打ち切り、`PaintColrGlyph` と層の循環を `hb_decycler_t` で断つ。`PaintColrGlyph` では、逆の拡大の変換の中で `color_glyph` を呼び、偽なら先のグリフのクリップの箱で囲んで塗りをたどる。
- 色：パレットの番号がCPALのパレットの数以上なら0番を使う。色の番号0xFFFFは前景色とする。アルファは `roundf(色のアルファ × clamp(alpha, 0, 1))` とする。
- COLRv0：層ごとに `fill_glyph` を呼ぶ。拡大の変換は置かない。
- SVG（`src/OT/Color/svg/svg.hh`）：グリフを含む文書（gzipなら展開したもの）を、形式 `svg ` の `image` で渡す。HarfBuzzはSVGを描かない。
- CBDT（`src/OT/Color/CBDT/CBDT.hh`）とsbix（`src/OT/Color/sbix/sbix.hh`）：PNGのデータを展開せずに、形式 `png ` の `image` で渡す。CBDTは画像の形式17、18、19（PNG）だけを扱う。ストライクは、`max(x_ppem, y_ppem)` 以上のppemのうち最小のもの、無ければ最大のものを選ぶ。ppemが0なら最大のものを選ぶ。同じppemのストライクは先のものを使う。EBDTは扱わない。
- グリフの箱（`hb_ot_get_glyph_extents`）は、sbix、CBDT、COLR、VARC、glyf、CFF2、CFFの順に探す。COLRは、クリップの箱があればそれを、無ければ塗りの木を `hb_paint_extents`（`src/hb-paint-extents.cc`）でたどって求める。COLRv0のグリフも、層のグリフの箱の和になる。
- `hb-ot-color.h` は、パレットの数、名前のID、フラグ、色、色の名前のID、`has_layers`、`glyph_get_layers`、`has_paint`、`glyph_has_paint`、`has_svg`、`glyph_reference_svg`、`has_png`、`glyph_reference_png` を持つ。

HarfBuzz 15は、塗りの関数をSVGとPDFに書く実装（`src/hb-vector-paint-svg.cc`、`src/hb-vector-paint-pdf.cc`）と、画素にする実装（`src/hb-raster-paint.cc`、`src/hb-raster-image.cc`）、GPU向けの実装（`src/hb-gpu-paint.cc`）を持つ。
画素の実装は浮動小数点と `libm` で計算し、乗算済みのBGRA32を出し、PNGの展開は `libpng` に任せる。`test/api/test-raster.cc` は単体のテストだけで、カラーグリフの画像の期待値は持たない。
PDFの実装は、Porter-Duffの合成をPDFで表せないため、`PLUS` を `Screen`、`XOR` を `Difference` に近似し、ほかは `Normal` とする。

HarfBuzzのテストは、`test/api/test-paint.c` が塗りの関数の呼び出しを字下げしたテキスト（数は `%.3g`）に書き、`test/api/results-paint/` の26個の期待値と比べる。
フォントは `test_glyphs-glyf_colr_1.ttf`、その可変フォント、`noto_handwriting-cff2_colr_1.otf`、`bad_colrv1.ttf`（循環）、`RocherColorGX.abc.ttf`（COLRv0）で、`hb_font_set_synthetic_slant` で傾けたものを含む。
`test/api/test-ot-color.c` は、`cpal-v0.ttf`、`cpal-v1.ttf`、`chromacheck-{colr,cbdt,sbix,svg}.ttf` で問い合わせを確かめる。
`test/vector/` は `hb-vector` のSVGとPDFの出力を期待値とバイト単位で比べ、COLRv0、COLRv1、sbixのグリフを含む。

### FreeType 2.15.0

- パレット（`include/freetype/ftcolor.h`、`src/sfnt/ttcpal.c`）：`FT_Palette_Data_Get` はパレットの数、各パレットの名前のIDとフラグ（明るい背景向け、暗い背景向け）、各色の名前のIDを返す。`FT_Palette_Select` は番号のパレットを選び、書きかえられる色の配列を返す。`FT_Palette_Set_Foreground_Color` は前景色を設定する。
- COLRv0：`FT_Get_Color_Glyph_Layer` が層を1つずつ返す。
- COLRv1：`FT_Get_Color_Glyph_Paint`（根の変換を含めるか選べる。`FT_COLOR_INCLUDE_ROOT_TRANSFORM`）、`FT_Get_Paint`、`FT_Get_Paint_Layers`、`FT_Get_Colorline_Stops`、`FT_Get_Color_Glyph_ClipBox` が、塗りの木をたどらずに生の値で返す（`src/sfnt/ttcolr.c`）。座標は16.16で、`INT_TO_FIXED` で直す。放射の半径は符号つきの16bitとして読む（HarfBuzzは符号なし）。
- `FT_LOAD_COLOR` と描画：COLRv0の層を、それぞれ輪郭として描いてから、選んだパレットの色で `tt_face_colr_blend_layer` によりBGRAの画像に重ねる（`src/base/ftobjs.c` の `FT_Render_Glyph_Internal`）。整数の演算である。COLRv1は描かない。
- 埋め込みビットマップ（`src/sfnt/ttsbit.c`）：EBDT、CBDT、Appleの `bdat` の画像の形式1、2、5、6、7、8、9（1、2、4、8、32bit）と17、18、19（PNG）、sbixの `png `、`dupe`、`flip` を読む。sbixの `jpg `、`tiff`、`rgbl` は読まない。PNGは `src/sfnt/pngshim.c` で展開し、乗算済みのBGRAにする。EBSCは読まない。
- ビットマップだけのフォント（`src/sfnt/sfobjs.c`）：glyfもCFFも無いsfntを、`EBLC`、`bloc`（Appleの名前）、`CBLC`、sbixのストライクで開く。Appleの形式は `head` の代わりに `bhed` を読む。CBLCかCBDTがあれば輪郭を無視する。ストライクは `FT_Face` の `available_sizes` になる。
- ストライクは、`FT_Select_Size` で番号を選ぶか、`FT_Request_Size` で丸めたppemが等しいものを選ぶ（`FT_Match_Size`）。近いものは選ばない。読み込みは、要求の大きさのストライクがあれば、`FT_LOAD_NO_BITMAP` が無いかぎりビットマップを優先する。
- SVG（`src/svg/`、`include/freetype/otsvg.h`）：SVGの描画は利用者が登録する関数（`svg-hooks`）に任せ、FreeTypeは文書を渡すだけである。

### ほかの設計

fontationsのskrifa 0.49.0（`skrifa/src/color/mod.rs`）は、`ColorPainter` トレイトでCOLRの塗りを渡す。
単色とグラデーションを1つの `fill(Brush)` にまとめ、色はパレットの番号とアルファ（`f32`）のまま渡して、色への解決を受け取る側に任せる。
停止点の正規化と3点の線形のグラデーションの2点への変換は、渡すまえにskrifaが行う。
`paint_cached_color_glyph` で、`PaintColrGlyph` の先を受け取る側のキャッシュから描ける。
グループは `push_layer(mode)` と `pop_layer` である。

## 実装すること

### 段階

5段階目を次の順に分ける。各段階は、前の段階の検査が通ってから始める。
このADRで扱わない部分は、下の「後回しにしたもの」に、作るADRの内容とあわせて挙げる。

| 段階 | 内容 |
|---|---|
| 5a | `Painter` と塗りの型。CPAL（パレットの問い合わせのすべて）、COLRv0（層の列と塗り）。CBLC/CBDTとsbixのストライク、メトリクス、画像のデータ（展開しない）と、PNGの `image` での受け渡し。SVG表の文書の受け渡し。グリフの箱、フォールバックのための問い合わせ |
| 5b | COLRv1の塗りと、生の塗りの木を読む関数（FreeTypeの `FT_Get_Paint` 相当）。可変の塗りは既定のインスタンスの値で。クリップの箱、範囲の判定、塗りの範囲。補助の関数と、塗りの命令を値の列にする記録 |
| 5c | EBLC/EBDT、`bloc`/`bdat`/`bhed` の白黒、灰色、32bitのビットマップ。ビットマップだけのフォント。FreeTypeと同じストライクの選び方と、ビットマップを優先する読み込み |
| 5d | COLRv0をBGRAの画像にする（FreeTypeの `FT_LOAD_COLOR` と同じ）。`sofel-render` のカラーの出力 |

順序の理由は次のとおりである。

- 5aは表の読み取りだけで済み、広く使われる形式（Twemoji MozillaのCOLRv0、Noto Color EmojiのCBDT版、Apple Color Emojiのsbix）を覆う。ベクトルで書き出す利用者（Golem）は、5aでこれらをPDFに描ける。
- COLRv1（5b）は塗りの木、範囲の判定、補助の関数を要し、量が多い。
- 5cと5dは、FreeTypeの機能（ビットマップだけのフォント、`FT_LOAD_COLOR` のCOLRv0）を覆う。どちらもFreeTypeと整数の演算で完全に一致させられ、既存の `sofel-raster`（[ADR 0012](0012-antialiased-rasterizer.md)）の上に作れる。
- PNGの展開、COLRv1の画素への合成、SVGのグリフの描画は、フォントの外の大きな仕様と、浮動小数点の許容差の決め方を要する。それぞれ別のADRにする。

### クレート

- `sofel-geom` に `Painter` と塗りの型、変換、三角関数、補助の関数を足す（下記）。依存は増やさない。
- `crates/sofel-bitmap` を作る。`#![no_std]`、確保なし、`#![forbid(unsafe_code)]`。この段階の依存は `sofel-font` だけとし、`sofel-png` はPNGのADRで足す（[ADR 0003](0003-crate-layers.md) の表のうち、この段階で要るもの）。
- `crates/sofel-color` を作る。`#![no_std]` と `alloc`、`#![forbid(unsafe_code)]`。この段階の依存は `sofel-outline` と `sofel-bitmap` だけとし、`sofel-svg` はSVGのADRで足す。
- `sofel-color` は `sofel-font` と `sofel-geom` の型を使うが、[ADR 0003](0003-crate-layers.md) の表は `sofel-color` からの直接の依存を許さない。[ADR 0010](0010-font-foundation.md) が `sofel-read` を `sofel_font::read` として再公開したのと同じく、`sofel-outline` が `sofel-font` を `sofel_outline::font` として、`sofel-font` が `sofel-geom` を `sofel_font::geom` として再公開し、`sofel-color` はこれを通して使う。
- 表のビューと検証器は `sofel-read` に足す。CPAL、COLR（v0とv1）、CBLC、CBDT、EBLC、EBDT、`bloc`、`bdat`、`bhed`、sbix、SVGである（[ADR 0007](0007-sfnt-read-foundation.md)。読み取りと同じプルリクエストで足す）。CBLC、EBLC、`bloc` と、CBDT、EBDT、`bdat` は、それぞれ同じビューを使う。

### 塗りの型（`sofel-geom`）

- `PointF { x: f32, y: f32 }`、`Rect { x_min, y_min, x_max, y_max: f32 }`。[ADR 0009](0009-geom-fixed-point-and-pen.md) が後に回した浮動小数点の点の型は、この `PointF` とする。
- `Transform { xx, yx, xy, yy, dx, dy: f32 }`。並びはHarfBuzzと同じ。`translate`、`scale`、`scale_around`、`rotate`、`rotate_around`、`skew`、`skew_around` を、`src/hb-geometry.hh` と同じ式で作る関数を持つ。
- `Color { r, g, b, a: u8 }`：乗算済みでないsRGB。
- `ColorRef { index: Option<u16>, alpha: f32 }`：色の出どころ。`index` はパレットの番号（`None` は前景色、COLRの0xFFFF）、`alpha` はフォントの値（`F2Dot14` と可変の差分）を0〜1に止めたもの。
- `Solid { color: Color, source: ColorRef }`：`color` は、選んだパレットと前景色から、HarfBuzzと同じ丸めで解決した色である。`source` を持つのは、パレットを替えるたびに塗りの木をたどりなおさずに、受け取る側が色を解決しなおせるようにするためである（skrifaと同じ考え）。
- `ColorStop { offset: f32, color: Solid }`、`Extend { Pad, Repeat, Reflect }`、`ColorLine<'a> { stops: &'a [ColorStop], extend: Extend }`。停止点はフォントに書かれた順とする（HarfBuzzと同じ。正規化は下の補助の関数）。
- `Brush<'a>`：
  - `Solid(Solid)`
  - `Linear { line: ColorLine<'a>, p0, p1, p2: PointF }`
  - `Radial { line: ColorLine<'a>, c0: PointF, r0: f32, c1: PointF, r1: f32 }`
  - `Sweep { line: ColorLine<'a>, center: PointF, start: f32, end: f32 }`（弧度、フォントの向き。HarfBuzzと同じ）
- `CompositeMode`：HarfBuzzの `hb_paint_composite_mode_t` と同じ28個を同じ番号で持つ。
- `Image<'a> { format: ImageFormat, data: &'a [u8], width: u32, height: u32, extents: GlyphExtents }`。`ImageFormat` は `Png`、`Svg { gzip: bool }` とする。`GlyphExtents { x_bearing, y_bearing, width, height: i32 }` は、HarfBuzzの `hb_glyph_extents_t` と同じ意味とする（高さは負）。

数に `f32` を使う。HarfBuzz、skrifa、今の2Dの描画の仕組み（Skia、Direct2D、Core Graphicsの多く、GPU）が `f32` を使い、変換して渡す手間が無いためである。
決定性は次で保つ。

- 四則演算と平方根はIEEE 754で、x86-64、aarch64、wasm32で同じ結果になる。`mul_add` を使わない。
- 三角関数（`sin`、`cos`、`tan`）は `core` に無く、プラットフォームの `libm` にも任せない。`sofel-geom` に自分で書く。muslの `sincosf` と `tanf` の多項式を移植し、[ADR 0002](0002-license-and-reuse.md) に従って流用元とライセンスを表示する。

### `Painter`（`sofel-geom`）

```rust
pub trait Painter {
    fn push_transform(&mut self, t: Transform);
    fn pop_transform(&mut self);
    fn push_clip_glyph(&mut self, glyph: u32);
    fn push_clip_rectangle(&mut self, r: Rect);
    fn pop_clip(&mut self);
    /// 今のクリップの範囲を塗る。
    fn fill(&mut self, brush: &Brush<'_>);
    /// 既定は push_clip_glyph、fill、pop_clip の順に呼ぶ。
    fn fill_glyph(&mut self, glyph: u32, color: &Solid) { … }
    /// 描けなければ false。
    fn image(&mut self, image: &Image<'_>) -> bool;
    fn push_group(&mut self, mode: CompositeMode);
    fn pop_group(&mut self, mode: CompositeMode);
    /// PaintColrGlyph の先を自分で描いたら true（既定は false）。
    fn color_glyph(&mut self, glyph: u32) -> bool { false }
}
```

- HarfBuzzの `hb_paint_funcs_t` と同じ順で同じ命令を出す。違いは形だけである。
  - 単色と3種類のグラデーションを、1つの `fill(&Brush)` にまとめる。今の描画の仕組みの「塗りの種類（shaderやbrush）で範囲を塗る」形に合い、トレイトのメソッドが減る。
  - グループの合成の方法を、`push_group` と `pop_group` の両方で渡す。HarfBuzzの `push_group`、`push_group_for`、`pop_group` と、skrifaの `push_layer`、`pop_layer` のどちらの使い方にも合わせられる。
  - `fill_glyph` は単色だけとする（HarfBuzzと同じ）。グラデーションのグリフは、クリップ、`fill`、クリップの解除で出す。
- 単位はフォントの単位で、yは上に増える。HarfBuzzと違い、根に拡大の変換を置かない。拡大と配置は、呼び出し側が自分の変換に含める。FreeTypeの `FT_COLOR_NO_ROOT_TRANSFORM` と同じである。
- グリフはIDだけで渡す。輪郭が要る受け取る側は、`sofel-outline` で輪郭を得る。`sofel-geom` はフォントを知らない（[ADR 0003](0003-crate-layers.md)）。
- HarfBuzzの `custom_palette_color` と計算の量の関数は持たない。色の差しかえは下の `Palette::Custom` で、計算の量の上限はHarfBuzzと同じ値の定数とする。
- `push_clip_path_start` と `push_clip_path_end` は持たない。COLRの塗りが呼ばないためである。SVGのグリフの描画（後回し）で要れば、そのADRで足す。

### 補助の関数（`sofel-geom`）

塗りの命令を描画の仕組みや書き出しの形式に写すときに要る純粋な関数を、HarfBuzzの `src/hb-paint.cc` から移植する。

- `normalize_color_line(stops: &mut [ColorStop]) -> (f32, f32)`：`hb_paint_normalize_color_line`。停止点を位置の順に安定に並べ、位置を0〜1に直し、もとの範囲を返す。
- `reduce_linear_anchors(p0, p1, p2) -> (PointF, PointF)`：`hb_paint_reduce_linear_anchors`。COLRの3点の線形のグラデーションを、多くの描画の仕組み（PDFの軸のシェーディング、SVGの `linearGradient`、Skiaの線形のshader）が表す2点に直す。
- `sweep_gradient_tiles(stops, extend, start, end, emit)`：`hb_paint_sweep_gradient_tiles`。扇形のグラデーションを、0〜2πの角度の区間（両端の角度と色）に分けて `emit` に渡す。扇形を直接表せない形式（PDFのパッチのメッシュ、三角形で描くGPU）に使う。

### 色とパレット（CPAL）

`sofel-color` に次を置く。どれも確保をしない。名前のIDから名前の文字列を引くのは、name表を読む [ADR 0016](0016-font-and-vertical-metrics.md) の関数である。

| 関数 | HarfBuzz | FreeType |
|---|---|---|
| `palette_count(font)` | `hb_ot_color_palette_get_count` | `FT_Palette_Data_Get` の `num_palettes` |
| `palette_name_id(font, i)` | `hb_ot_color_palette_get_name_id` | `palette_name_ids` |
| `palette_flags(font, i)` | `hb_ot_color_palette_get_flags` | `palette_flags` |
| `palette_color_name_id(font, c)` | `hb_ot_color_palette_color_get_name_id` | `palette_entry_name_ids` |
| `palette_colors(font, i) -> impl Iterator<Item = Color>` | `hb_ot_color_palette_get_colors` | `FT_Palette_Select` の配列 |
| `find_palette(font, flags) -> Option<u16>` | なし | なし |

- `find_palette` は、フラグ（明るい背景向け、暗い背景向け）を持つ最初のパレットを返す。CSSの `font-palette: light` と `dark` が求める選び方である。
- FreeTypeの `FT_Palette_Select` の書きかえと `FT_Palette_Set_Foreground_Color` は、値として渡す `PaintOptions` の `palette` と `foreground` に当たる。

### グリフから塗りへ

```rust
pub struct PaintOptions<'a> {
    pub palette: Palette<'a>,   // Font(u16) | Custom(&'a [Color])
    pub foreground: Color,
    pub ppem: u16,              // ビットマップのストライクの選択。0は最大
}

pub fn paint(font: &Font, glyph: u32, options: &PaintOptions<'_>,
             scratch: &mut Scratch, painter: &mut impl Painter) -> Painted;
```

- HarfBuzzの `hb_ot_paint_glyph_or_fail` と同じく、COLR、SVG、CBDT、sbixの順に試し、使った形式を `Painted`（`ColrV1`、`ColrV0`、`Svg`、`Cbdt`、`Sbix`、`None`）で返す。`None` なら何も呼ばない。カラーでないグリフを前景色で塗るかは、呼び出し側が決める（HarfBuzzの `hb_font_paint_glyph` は輪郭を前景色で塗る）。
- `Palette::Font(i)` は、`i` がパレットの数以上なら0番を使う。`Palette::Custom` は、呼び出し側が `palette_colors` から作って一部を変えたパレットで、HarfBuzzの `custom_palette_color`、FreeTypeの `FT_Palette_Select` の書きかえ、CSSの `override-colors` に当たる。範囲外の色の番号は、HarfBuzzと同じく透明の黒とする。
- `Scratch` は、色の停止点を並べる作業領域と、たどったグリフと層の記録（循環の検出）を持つ。呼び出し側が持ち、グリフをまたいで再利用する（[ADR 0003](0003-crate-layers.md)）。
- 数は、HarfBuzzが `float` で行う計算と同じ順で同じ演算をする。

#### 記録

- `sofel-color` に、`Painter` を実装して命令を値の列（`Vec<PaintCommand>`。`PaintCommand` は `Painter` の各メソッドに当たる列挙）に記録する `Recording` と、列を別の `Painter` に流しなおす `replay` を置く。
- 記録は、命令を値として持ち回りたい利用者（キャッシュ、スレッドをまたぐ受け渡し、C ABIやほかの言語への受け渡し）のためである。たどる処理は1つで、記録はその受け取る側の1つにすぎない。
- `PaintCommand` は、`Brush` と `Image` の借用を持つ（停止点は `Recording` が自分の配列に写す。画像のデータはフォントのバイト列を借りる）。

#### COLRv0（5a）

- `layers(font, glyph) -> impl Iterator<Item = (u32, Option<u16>)>`：層のグリフと色の番号を返す（`hb_ot_color_glyph_get_layers`、`FT_Get_Color_Glyph_Layer`）。
- `paint` は、層ごとに `fill_glyph(層のグリフ, 色)` を呼ぶ。HarfBuzzと同じく、COLRv1の塗りの記録があるグリフではCOLRv0の記録を見ない。

#### COLRv1（5b）

- 塗りの形式1〜32をすべて読む。可変の形式（`PaintVar*`、`VarColorStop` など）は、この段階では差分を0として、可変でない形式と同じ値で塗る。既定のインスタンスでは、HarfBuzzもこの値を出す。
- 平行移動、拡大、回転、傾斜とその中心つきの形は、`Transform` の関数で1つの変換にして `push_transform` に渡す。`PaintTransform` の16.16の値は、HarfBuzzと同じく `f32` に直す。
- クリップの箱（ClipBoxの形式1と2）があれば `push_clip_rectangle` で囲む。無ければ、HarfBuzzの `hb_paint_bounded` と同じ判定で塗りの木をたどり、範囲が有限でなければ何も塗らない。
- `PaintColrGlyph` は `color_glyph` を呼び、偽なら先のグリフのクリップの箱で囲んで塗りをたどる。
- 打ち切りの条件（深さ64、辺の数16384、計算の量2^24）と循環の断ち方は、HarfBuzzと同じにする。計算の量の数え方は `src/hb-paint.hh` と `COLR.hh` に合わせる。
- アルファは `round(色のアルファ × clamp(alpha, 0, 1))` とする。`round` は0.5を0から遠い方へ丸める（`roundf` と同じ）。
- 生の塗りの木：`base_paint(font, glyph) -> Option<PaintRef<'_>>`、`clip_box(font, glyph) -> Option<ClipBox>`、`PaintRef` の形式ごとの値と子、`PaintColrLayers` の層の列、色の線の停止点を、生の値（`F2Dot14`、`Fixed`、`i16`、`u16` と可変の番号）で返す。FreeTypeの `FT_Get_Color_Glyph_Paint`、`FT_Get_Paint`、`FT_Get_Paint_Layers`、`FT_Get_Colorline_Stops`、`FT_Get_Color_Glyph_ClipBox` に当たる。共有された層や部分木を自分で使い回したい利用者（GPUの符号化、書き出しの形式でのオブジェクトの共有）のためである。

#### CBDTとsbix（5a、`sofel-bitmap`）

- `strikes(font) -> impl Iterator<Item = Strike>`：ストライクの一覧（番号、x方向とy方向のppem、表の種類、sbixの解像度）。FreeTypeの `available_sizes` に当たる。
- `choose_strike(font, ppem)`：HarfBuzzと同じ選び方。`match_strike(font, x_ppem, y_ppem)`：FreeTypeの `FT_Match_Size` と同じ、丸めたppemが等しいものの選び方。
- `glyph_bitmap(font, strike, glyph) -> Option<GlyphBitmap<'_>>`：画像の形式（EBDT/CBDTの形式番号、sbixの図の種類のタグ）、データ（展開しない）、ビットマップのメトリクス（横書きと縦書きのベアリング、送り幅、幅、高さ）を返す。PNGの幅と高さはIHDRの固定の位置から読む。sbixの `dupe` と `flip` はFreeTypeと同じくたどる（深さ4まで）。sbixの `jpg `、`tiff` など、sofelが展開しない形式も、タグとデータをそのまま返す。
- `sofel-color` の `paint` は、HarfBuzzと同じく、CBDTの形式17、18、19と、sbixの `png ` を `Image { format: Png }` で渡す。`extents` と `width`、`height` はHarfBuzzと同じ値にする。ほかの形式は `paint` では塗らない（HarfBuzzと同じ）。

#### EBLC/EBDTとビットマップだけのフォント（5c、`sofel-bitmap`、`sofel-font`）

- EBDT、CBDT、`bdat` の画像の形式1、2、5、6、7、8、9と、1、2、4、8、32bitの深さを読み、FreeTypeの `FT_LOAD_DEFAULT` と同じ画素とメトリクスのビットマップを、呼び出し側の領域に書く関数を置く。32bitはFreeTypeと同じBGRAとする。形式8と9の成分の合成もFreeTypeに合わせる。
- `sofel-font` の `Font::new` が、glyfもCFFも無いsfntを、FreeTypeと同じ条件（`EBLC`、`bloc`、`CBLC`、sbixのストライクがある。Appleの形式は `bhed`）で開けるようにする。`Font` に、輪郭を持つか、ビットマップを持つかを返す問い合わせを足す。
- `sofel-render` は、FreeTypeと同じく、`--ppem` に等しいストライクがあれば、`--no-bitmap` が無いかぎりビットマップを出す。白黒と灰色はPGM、32bitは下の5dと同じPAMで書く。

#### COLRv0の画像（5d）

- `sofel-raster` に、8bitの被覆率の画像を色で重ねてBGRAの画像に足す `blend_layer` を置く。FreeTypeの `tt_face_colr_blend_layer` と同じ整数の演算と、同じ画像の箱の広げ方とする。フォントは知らない。
- フォントを読んで層ごとに輪郭を描き、`blend_layer` で重ねる手順は、[ADR 0012](0012-antialiased-rasterizer.md) の `sofel_cli::glyph::render` と同じく `sofel_cli::glyph::render_color` にまとめる。パレットと前景色は `PaintOptions` と同じ値で受け取る。
- `sofel-render` に `--color` を足し、COLRv0のグリフをPAM（`P7`、`TUPLTYPE RGB_ALPHA`、乗算済みでない値に直す）で書く。位置はPGMと同じ注釈の行に書く。
- FreeTypeとの完全一致には、層の輪郭がFreeTypeと同じ位置に来る必要があり、[ADR 0018](0018-phantom-point-shift.md) のpp1.xの平行移動に依存する。

#### SVG（5a）

- `svg_document(font, glyph) -> Option<SvgDocument<'_>>`：グリフを含む文書、文書の番号、文書が覆うグリフの範囲を返す（`hb_ot_color_glyph_reference_svg`、`hb_ot_color_glyph_get_svg_document_index`、`hb_ot_color_get_svg_document_glyph_range`）。
- `paint` は文書を `Image { format: Svg { gzip } }` で渡す。gzipの文書は、展開器のADRまで展開せずに `gzip: true` で渡す。受け取る側は、文書の中の `id="glyph<グリフID>"` の要素を描く（OpenType仕様のSVG表の約束）。FreeTypeの `svg-hooks` と同じく、描画は受け取る側に任せる形である。

### グリフの箱

- `sofel_color::glyph_extents(font, glyph) -> Option<GlyphExtents>` を、HarfBuzzの `hb_ot_get_glyph_extents` と同じく、sbix、CBDT、COLRの順に探し、どれでもなければ `sofel_outline::Outlines::glyph_extents`（VARC、glyf、CFF2、CFF。[ADR 0016](0016-font-and-vertical-metrics.md)、[ADR 0021](0021-cff-outlines.md)）を返す関数とする。単位は拡大縮小しないフォントの単位で、ストライクはHarfBuzzの既定（ppem 0、最大のもの）とする。
- COLRは、クリップの箱があればそれを、無ければ `hb_paint_extents` と同じ計算で塗りの範囲を求める。層のグリフの箱は `sofel_font` から得る。
- グリフの箱の置き場所は、層ごとに分ける：glyfの見出しの箱は `sofel-font`（[ADR 0016](0016-font-and-vertical-metrics.md)）、charstringの解釈が要るCFFの箱は `sofel-outline`（[ADR 0021](0021-cff-outlines.md)）、塗りの木をたどるカラーの箱と全体の順はこの `sofel-color` とする。[ADR 0015](0015-c-abi.md) の `sofel_font_glyph_extents` は、この ADR の実装のあとはこの関数を呼ぶ。

### フォールバックのための問い合わせ

代わりのフォントを選ぶのは利用者である。sofelは次を返す。どれも表のビューを引くだけで、塗りの木をたどらない。

- 文字を持つか：`Font::nominal_glyph(c)`（[ADR 0010](0010-font-foundation.md)）。異体字セレクタ（絵文字の表示のU+FE0F、文字の表示のU+FE0E）つきは `Font::variation_glyph(c, vs)`。
- フォントの持つ形式：`color_formats(font) -> ColorFormats`（COLRv0、COLRv1、CPAL、SVG、CBDT、sbix、EBDTのビットの組）。`hb_ot_color_has_layers`、`has_paint`、`has_palettes`、`has_svg`、`has_png` と、FreeTypeの `FT_HAS_COLOR`、`FT_HAS_FIXED_SIZES` に当たる。
- グリフがカラーか：`glyph_color_format(font, glyph) -> Option<ColorFormat>`。`paint` と同じ順に、COLRv1の塗りの記録、COLRv0の記録、SVGの文書の範囲、CBDT（最大のストライク）、sbix（最大のストライク）を引き、最初に当たった形式を返す。`hb_ot_color_glyph_has_paint` を含む。
- ZWJでつないだ絵文字の並び、国旗、キーキャップのような並びをフォントが1つのグリフにするかは、GSUBの合字と文脈の置換で決まり、シェーピングをしないと分からない。安い必要条件（並びのすべての文字を `nominal_glyph` が返す）でまず候補を絞り、シェーピング（[ADR 0020](0020-shaping-foundation.md)）の結果に `.notdef`（グリフ0）が無いことで確かめる。この手順は利用者が行い、sofelは専用の関数を足さない。
- 文字の性質（`Emoji_Presentation`、`Extended_Pictographic`、`Emoji_Modifier` など）は、[ADR 0019](0019-unicode-character-data.md) のUnicodeの文字のデータから引く。

### 道具

- `sofel-cli` にバイナリ `sofel-paint` を足す。`sofel-paint [--face N] [--palette P] [--foreground RRGGBBAA] [--ppem P] (--glyph G | --char U+XXXX) FILE` は、`paint` の命令の列を、HarfBuzzの `test/api/test-paint.c` と同じ字下げのテキストで標準出力に書く。`fill` は、`Brush` の種類に応じてHarfBuzzの `solid`、`linear gradient`、`radial gradient`、`sweep gradient` の行にする。数は既定で `%.3g` と同じ形とし、`--exact` では `f32` のビットを16進で書く。カラーでないグリフは終了コード1、使い方の誤りは2とする。
- `sofel-render` の `--color` と、ビットマップの出力（5c、5d）。
- `sofel-validate` は、足した表の検証器を使う（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。

### C ABI

[ADR 0015](0015-c-abi.md) の決まりに従い、同じプルリクエストで `sofel-capi` に次を足す。

- `sofel_paint_glyph(font, glyph, palette, custom_colors, n_custom, foreground, ppem, out)`：`Recording` で記録した命令の列を、Cの構造体の配列（命令の種類と値の共用体）として `out` に返し、`Painted` を整数で返す。列は `sofel_paint_commands_free` で解放する。関数ポインタの組ではなく配列で返すのは、ほかの言語（GolemのHaskellなど）から呼び返しを登録するより、配列を読むほうが単純で速いためである。
- `sofel_color_layers`、`sofel_color_formats`、`sofel_glyph_color_format`、`sofel_palette_count`、`sofel_palette_name_id`、`sofel_palette_flags`、`sofel_palette_color_name_id`、`sofel_palette_colors`、`sofel_find_palette`、`sofel_svg_document`、`sofel_bitmap_strikes`、`sofel_glyph_bitmap`。
- 補助の関数 `sofel_normalize_color_line`、`sofel_reduce_linear_anchors`、`sofel_sweep_gradient_tiles`。

### 利用の例：PDFへの写し（参考）

この節は設計を決めるものではなく、`Painter` が書き出しの形式に写せることを確かめた記録である。PDFを書くのは12段階目か利用者で、このADRは書かない。HarfBuzzの `src/hb-vector-paint-pdf.cc` の方法で、Golemのような利用者は次のように写せる。

| 命令 | PDF |
|---|---|
| `push_transform`、`pop_transform` | `q`、`cm`、`Q`。根の変換に文字の大きさと位置を含める |
| `push_clip_glyph` | `sofel-outline` の輪郭（二次曲線は三次に直す）を `W n`。埋め込んだフォントの文字を描画のモード7で描く方法もある |
| `push_clip_rectangle`、`pop_clip` | `q`、`re W n`、`Q` |
| `fill(Solid)`、`fill_glyph` | 色を設定し、クリップの範囲か輪郭を塗る。アルファはExtGStateの `/ca` |
| `fill(Linear)` | `reduce_linear_anchors` で2点にし、ShadingType 2。停止点はType 2の関数をType 3でつないだもの |
| `fill(Radial)` | ShadingType 3 |
| `fill(Sweep)` | `sweep_gradient_tiles` の区間ごとのCoonsのパッチで、ShadingType 6 |
| 停止点のアルファ | DeviceGrayのシェーディングによる輝度のソフトマスク |
| `push_group`、`pop_group` | 透明のグループのForm XObjectとExtGStateの `/BM`。Porter-Duffの合成はPDFに無く、近似するか画素にする |
| `image`（PNG） | 画像のXObject。アルファの無い画像はIDATを `/FlateDecode` と `/Predictor 15` でそのまま入れられる。アルファのある画像は展開してソフトマスクに分ける |
| `image`（SVG） | 利用者のSVGの描画でPDFの命令にする |
| `color_glyph` | 同じグリフのForm XObjectを一度だけ作って使い回し、真を返す |

## データ構造

- `sofel-read` のビュー：`Cpal`（版0と1）、`Colr`（基本のグリフの記録、層の記録、BaseGlyphList、LayerList、ClipList、DeltaSetIndexMapとItemVariationStoreの位置）、`Paint`（形式ごとの値を読む列挙のビュー。子の塗りは24bitのオフセットのまま持つ）、`ColorLine`（`VarColorLine` を含む）、`Cblc`（CBLC、EBLC、`bloc`。BitmapSizeTable、IndexSubTableArray、IndexSubTableの形式1〜5）、`Cbdt`（CBDT、EBDT、`bdat`）、`Sbix`（ストライク、グリフのデータのオフセット）、`Svg`（文書の索引）、`bhed`（`head` と同じビュー）。値は生の型（`F2Dot14`、`Fixed`、`i16`、`u16`）で返す。
- `sofel-color` の `Scratch`：色の停止点の `Vec<ColorStop>`（色の線ごとに使い回す）、たどっている途中のグリフと層の記録（深さ64までの固定の大きさの配列で足りる）。
- `Recording`：`Vec<PaintCommand>` と、停止点を写す `Vec<ColorStop>`。
- ストライクの選択は、HarfBuzzのようにppemからの索引を前もって作らず、毎回BitmapSizeTableの列を走る。ストライクの数は小さい（実際のフォントで数個から十数個）ため、索引の値を呼び出し側に持たせる複雑さに見合わない。計測で遅ければ、呼び出し側が持つ索引の値を足す（[ADR 0003](0003-crate-layers.md)）。

## アルゴリズム

- COLRv1の塗りの木は、再帰でたどる。再帰の深さは打ち切りの条件で64以下に抑えられ、スタックの使用は有界である。
- 循環の検出は、たどっている途中のグリフ（`PaintColrGlyph`）と層（`PaintColrLayers`）の列を `Scratch` に持ち、入るときに同じものがあれば入らない（`hb_decycler_t` と同じ結果）。
- 範囲の判定（`hb_paint_bounded`）と塗りの範囲（`hb_paint_extents`）は、`Painter` を実装した内部の値として書き、同じたどり方を使い回す。
- 三角関数は、引数を `[-π/4, π/4]` に畳んでから多項式で求める（muslの `sincosf`、`tanf` と同じ手順）。
- `blend_layer` は、FreeTypeの `tt_face_colr_blend_layer` と同じく、最初の層で画像の箱を決め、以後の層で箱を広げて写しなおし、画素ごとに整数で重ねる。

## 後回しにしたもの

次は、このADRの実装のあとに、それぞれのADRを作る。各ADRが含める内容をここに決めておく。

1. **DEFLATEとPNGの展開（`sofel-inflate`、`sofel-png`）**
   - DEFLATE、zlib、gzipの展開（RFC 1950〜1952）。PNGの全ての色の型とビットの深さ、フィルタ、インターレース、PLTEとtRNS。CRCとAdler-32の検査。
   - `sofel-bitmap` に `sofel-png` の依存を足し、CBDTとsbixのPNGを、FreeTypeの `pngshim.c` と同じ乗算済みのBGRAにする関数。gAMAなどの補助のチャンクの扱いもFreeTypeに合わせる。
   - gzipのSVG文書を展開して渡す（`Svg { gzip: false }` にする）。
   - `ft-dump --sbit` に画素を書く動作を足し、PNGのグリフの画素をFreeTypeと完全に一致させる。展開器そのものは、仕様の参照実装が配布するテストのデータ（ライセンスを確かめて `testdata.lock` に加える）で確かめる。
   - 計測：FreeTypeの `FT_Load_Glyph`（`FT_LOAD_COLOR`）と、展開の速さ。
2. **塗りの命令の画素への合成（`sofel-raster`）**
   - `sofel-raster` に、フォントを知らない描画の面（`Painter` と同じ命令を、輪郭の命令の列でクリップを受け取る形で持つ）を置き、乗算済みのRGBAに合成する。線形、2点の円錐（放射）、扇形のグラデーション、3つの延長、28の合成、クリップのマスク、画像の双線形の標本化と変換。
   - グリフIDを輪郭に変えて描画の面に渡す、フォントを知る層の置き場所（`sofel-color` か新しいクレートか。[ADR 0003](0003-crate-layers.md) の表との関係）。
   - 数の型と許容差：基準をHarfBuzzの `libharfbuzz-raster`（`f32` と `libm`）とし、画素ごとの許容差を決める。`sqrt` と `atan2` も自分で書き、決定性を保つ。
   - COLRv1とPNGとSVG（展開と描画のADRのあと）の画像を `sofel-render --color` で書く。HarfBuzzの `hb-view` 相当の出力との比較の道具を `tools/hb-dump/` に足す。
   - 計測：`libharfbuzz-raster` の `hb_raster_paint_*` との比較。
3. **OpenTypeのSVGのグリフの描画（`sofel-svg`）**
   - OpenType仕様がSVG表に許す範囲（スクリプトと外部の参照を除く。`context-fill` などのOpenTypeの拡張を含む）の、XMLの解析、スタイル、パス、グラデーション、変換、`use` と `clipPath` の解釈。
   - SVGを `Painter` の命令に変える。要れば `Painter` に任意のパスのクリップ（HarfBuzzの `push_clip_path_start`、`push_clip_path_end`）を足す。
   - 比べる基準：HarfBuzzもFreeTypeもSVGを描かない（FreeTypeは `svg-hooks` に任せる）ため、別の基準（例えばresvgのテストの集まり）をライセンスを確かめて選ぶ。
4. **COLRv1の可変の塗りの差分**：3段階目（可変フォント）のADRで、ItemVariationStoreとDeltaSetIndexMapを読み、`VarIdxBase` の差分を塗りとクリップの箱に足す。HarfBuzzの `f32` の差分とFreeTypeの整数の差分の違いを、比べ方とあわせて決める。`results-paint` の `testvf-*` を既定でない座標でも比べる。
5. **書き出しとGPU**：12段階目のADRで、`Painter` の命令をSVGとPDFに書き（`sofel-vector`。HarfBuzzの `test/vector/` と比べる）、GPU向けに符号化する（`sofel-gpu`。HarfBuzzの `src/hb-gpu-paint.cc`）。
6. **可変のCOLRの固定**：COLRv1の変化量の固定（インスタンス化）は、[ADR 0022](0022-subsetting.md) の「後回しにしたもの」の可変フォントのインスタンス化のADRで扱う。COLR、CPAL、CBLC/CBDT、sbixのサブセット化そのものは [ADR 0022](0022-subsetting.md) の範囲で、この ADR のビューを使う（SVGの表は、HarfBuzzと同じくサブセット化しない）。
7. **合成の太字と斜体**：HarfBuzzの `synthetic_slant`（`image` の `slant` を含む）と `synthetic_bold`、FreeTypeの `FT_GlyphSlot_Embolden`、`FT_GlyphSlot_Oblique`、`FT_Bitmap_Embolden`、`FT_Bitmap_Convert` を、輪郭とビットマップの両方について1つのADRで扱う。

## 実装しないこと

- 上の「後回しにしたもの」。
- EBSC（ビットマップの拡大縮小の表）。HarfBuzzもFreeTypeも読まず、比べる基準が無い。
- sbixの `jpg `、`tiff`、`rgbl` の展開。FreeTypeもHarfBuzzも描かない。`glyph_bitmap` はタグとデータをそのまま返すので、利用者は自分で展開できる。
- 代わりのフォントの選択、書記素クラスタの区切り（UAX #29）、絵文字の並びの判定。利用者の範囲である。
- HarfBuzzの `custom_palette_color` のような1色ずつの差しかえの呼び返しと、計算の量を変える関数。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。表のバイト列は [ADR 0007](0007-sfnt-read-foundation.md) のテストの組み立て方で作る。

### 単体のテスト

- 読み取りと検証器：CPAL（版0、版1、パレットの範囲外、色の範囲外）、COLRv0（記録の二分探索、層の範囲外）、COLRv1の塗りの形式1〜32と形式の範囲外、ClipBoxの形式1と2、CBLC/EBLC/`bloc` の索引の形式1〜5、CBDT/EBDT/`bdat` の画像の形式1、2、5、6、7、8、9、17、18、19、`bhed`、sbixの `png `、`dupe`、`flip`、`jpg `、SVGの文書の索引。それぞれ、わざと壊したバイト列で検証器が期待する場所と理由を報告することを確かめる。
- `Transform` の関数：恒等、各変換、中心つきの形を、HarfBuzzの式を `f64` で書いた参照の値と比べ、差が `f32` の1ulp以下であることを確かめる。
- 三角関数：0、±π/4の前後、±π、大きな引数（±1e6）と、自分で書く疑似乱数（xorshift）による100万個の引数で、`std` の `f64` の値を `f32` に丸めたものとの差が1ulp以下であることを確かめる。
- 決定性：三角関数と `Transform` の100万個の結果のビットのFNV-1aのハッシュを、テストに書いた定数と比べる。同じテストを `wasm32-wasip1` とCIのaarch64のランナーでも通し、どれでも同じビットになることを確かめる。
- 補助の関数：`normalize_color_line`（並んでいない停止点、同じ位置、1点、0〜1の外）、`reduce_linear_anchors`（p2がp0p1の上、直交、退化）、`sweep_gradient_tiles`（`Pad`、`Repeat`、`Reflect`、0〜2πをまたぐ角度、開始と終了の逆転）を、HarfBuzzの `test/api/test-paint.c` の `test_hb_paint_sweep_gradient_tiles_clipped` などの期待値を移したテストと、手で計算した値で確かめる。
- `Painter` の既定：`fill_glyph` の既定が `push_clip_glyph`、`fill`、`pop_clip` を呼ぶことを、命令を記録する `Painter` で確かめる。
- `Recording` と `replay`：テストのフォントの全カラーグリフで、直接 `paint` した命令の列と、記録して流しなおした列が等しいことを確かめる。
- `paint`：COLRv0の前景色（0xFFFF）、パレットの範囲外（0番に戻る）、`Palette::Custom`、`Solid` の `source` の番号とアルファ、アルファの丸めとclamp（負、1超）、COLRv1の循環（自分を指す `PaintColrGlyph`、層の循環）、深さ64と辺の数の打ち切り、クリップの箱の無い有界と無界の木、`color_glyph` が真を返す場合を確かめる。
- パレット：`find_palette` の明るい背景向け、暗い背景向け、該当なし、版0（フラグなし）。
- ストライクの選択：`choose_strike`（要求より大きいもののうち最小、すべて小さいとき最大、ppem 0、同じppemの2つ、ppemが0のストライク）と `match_strike`（丸めの境界、幅を無視する場合）。
- `glyph_color_format` と `color_formats`：各形式を1つずつ持つフォントと、COLRv1とCBDTを両方持つフォント（COLRv1が勝つ）。
- ビットマップだけのフォント：glyfの無いEBLCのフォント、`bhed` と `bloc` のフォント、CBLCと輪郭を両方持つフォント（輪郭を無視する）を `Font::new` で開けることを確かめる。
- `blend_layer`：重ならない2層、重なる2層、アルファ0と255、箱の広げ方を、FreeTypeの式を直接書いた参照の関数と比べる。
- `sofel-paint`：値の分かっている小さなフォントで、出力の字下げ、`%.3g` の形、`--exact`、終了コード0、1、2を確かめる。`%.3g` の書式は、`1e+03`、`0.977`、`-0`、`1.3e+03`、`100` など、Cの `printf` の結果を書いた表で確かめる。
- `sofel-render`：ビットマップのグリフのPGM、`--no-bitmap`、`--color` のPAMの見出しと画素を確かめる。
- C ABI：`sofel-capi` のCのテストで、`sofel_paint_glyph` の配列を `sofel-paint` と同じテキストにして比べる。

### 適合テスト（`sofel-conformance`）

- HarfBuzzの期待値：`test/api/results-paint/` の26個を、`test/api/test-paint.c` の `paint_tests` の表と同じフォント、グリフ、パレット、前景色で `paint` し、`sofel-paint` と同じテキストにして比べる。HarfBuzzが置く根の拡大の変換と `color_glyph` を囲む逆の変換（この拡大では恒等）と、`synthetic_slant` の変換は、テストの側で同じ行を足す。`push_group(mode)` は `push group` の行にする。`paint` が `None` を返したグリフには、HarfBuzzが代わりに出す前景色の `fill_glyph` の行を、テストの側で足す。COLRv1のレイヤーのグリフがCFF2の `noto_handwriting-cff2_colr_1.otf` も、塗りの列はグリフIDだけなので比べられる。項目は `results-paint/<名前>` とする。
- `hb-dump`：[ADR 0016](0016-font-and-vertical-metrics.md) が置く `tools/hb-dump/`（HarfBuzz 15.1.0の `src/harfbuzz.cc` を1つの翻訳単位でビルドし、FreeTypeとの統合なし）に、次の書き出しを足す。`--paint` は、フォントの全グリフについて `hb_font_paint_glyph` の呼び出しを `test/api/test-paint.c` と同じテキストで書く。パレットはすべての番号、ppemは0と各ストライクのppemとする。`--color` は `hb_ot_color_*` の問い合わせのすべての結果（パレット、名前のID、フラグ、色、層、SVGの文書の番号と範囲、PNGの長さ）を、`--extents` は `hb_font_get_glyph_extents`（拡大は `upem`）を書く。
  - 対象は、`target/testdata/harfbuzz` の中でCOLR、SVG、CBDT、sbixのどれかを持つすべてのフォントとする（`test/api/fonts/` と `test/fuzzing/fonts/` の `colr-*`、`chromacheck-*`、`NotoColorEmoji.*`、`TwemojiMozilla.*`、`sbix*`、`noto_handwriting-*`、`bad_colrv1.ttf` など）。
  - 項目は `<フォント>#<フェイス>:paint:<パレット>:<PPEM>:<グリフ>`、`<フォント>#<フェイス>:color`、`<フォント>#<フェイス>:extents:<グリフ>` とする。期待する失敗の一覧は `conformance/expected-failures-harfbuzz.txt` とする。
  - 数は `%.3g` で比べるため、三角関数の結果がHarfBuzzの `libm` と3桁目の丸めの境界で分かれる項目がありうる。その項目は、両方の値を `--exact` の形で並べた理由つきで一覧に載せる。
  - `hb-dump --paint` の正しさは、出力が `test/api/results-paint/` の26個と一致することで、CIで自動に確かめる。
- FreeTypeの生の値：`ft-dump` に `--colr` を足す。各グリフの `FT_Get_Color_Glyph_Layer` の層、`FT_Get_Color_Glyph_Paint`（`FT_COLOR_NO_ROOT_TRANSFORM`）から `FT_Get_Paint` と `FT_Get_Paint_Layers` でたどった塗りの木（16.16の値）、`FT_Get_Colorline_Stops`、`FT_Get_Color_Glyph_ClipBox`、`FT_Palette_Data_Get` と各パレットの色を書く。sofelの生の塗りの木とパレットの関数の値を同じ16.16に直して、完全に一致することを確かめる。項目は `<フォント>#<フェイス>:colr:<グリフ>` と `<フォント>#<フェイス>:cpal`。放射の半径が32767を超える場合（FreeTypeは符号つきで読む）は、理由つきで一覧に載せる。
- FreeTypeのビットマップ（5aと5c）：`ft-dump` に `--sbit <PPEM>` を足す。そのppemのストライクがあれば、`FT_Set_Pixel_Sizes` と `FT_LOAD_DEFAULT` で各グリフを読み、`bitmap_left`、`bitmap_top`、幅、行数、送り幅、縦書きのメトリクス、画素の形式と、白黒、灰色、32bitの画素を書く。PNGのグリフは `FT_LOAD_COLOR` で読み、画素は書かずにメトリクスだけを書く。ストライクの一覧（`available_sizes`）も書く。sofelの `strikes`、`match_strike`、`glyph_bitmap` の結果と完全に一致することを確かめる。項目は `<フォント>#<フェイス>:sbit:<PPEM>:<グリフ>` と `<フォント>#<フェイス>:strikes`。
  - 取得したテストのデータには、EBLCや `bloc` を持つフォントが無い。白黒、灰色、32bit、成分の形式、`bhed` のフォントは、テストの組み立て方で各形式と各索引の形式の小さなフォントを作り、そのバイト列を `ft-dump` にも読ませて比べる。実際のフォントを加える場合は、ライセンスを確かめて `testdata.lock` に加えるADRを作る（[ADR 0005](0005-conformance-tests.md)）。
- FreeTypeのCOLRv0の画像（5d）：`ft-dump --raster <PPEM>` に `--color` を足し、`FT_LOAD_COLOR` と `FT_RENDER_MODE_NORMAL` で、COLRv0のグリフのBGRAの画像を書く。パレットは0番と最後の番号とする。`sofel_cli::glyph::render_color` の画像と完全に一致することを、12、24、48画素で確かめる。項目は `<フォント>#<フェイス>:colr-raster:<PPEM>:<パレット>:<グリフ>`。層の輪郭が一致しないグリフ（[ADR 0018](0018-phantom-point-shift.md) の実装まえのpp1.x）は、[ADR 0012](0012-antialiased-rasterizer.md) と同じく理由つきで一覧に載せる。

自動テストにしにくいものは、次のとおり自動で確かめる。

- `tools/hb-dump/` の書き出しの確かめ：CIのLinuxのランナーでビルドし、上の `results-paint` との一致を走らせる。
- 「利用の例：PDFへの写し」：このADRは書き出しを実装しないので、検査しない。12段階目のADRで、HarfBuzzの `test/vector/` の期待値（`colrv0-large-scale.pdf` など）と比べる。

### 計測の項目

[ADR 0013](0013-benchmark-harness.md) の道具に、次の項目を足す。フォントは `TwemojiMozilla.subset.ttf`（COLRv0）、`test_glyphs-glyf_colr_1.ttf` と `noto_handwriting-glyf_colr_1.ttf`（COLRv1）、`NotoColorEmoji.subset.ttf`（CBDT）、`noto_handwriting-sbix.ttf`（sbix）、`chromacheck-svg.ttf`（SVG）とし、全グリフを対象とする。

| 項目 | sofel | HarfBuzz | FreeType |
|---|---|---|---|
| グリフの色の形式 | `glyph_color_format` | `hb_ot_color_glyph_has_paint` と `hb_ot_color_glyph_get_layers`（数だけ） | なし |
| COLRv0の層 | `layers` | `hb_ot_color_glyph_get_layers` | `FT_Get_Color_Glyph_Layer` の繰り返し |
| COLRv0の塗り | `paint`（何もしない `Painter`） | `hb_font_paint_glyph`（何もしない関数） | なし |
| COLRv1の塗り | `paint`（何もしない `Painter`） | `hb_font_paint_glyph`（何もしない関数） | `FT_Get_Color_Glyph_Paint` と `FT_Get_Paint` で木をたどる |
| COLRv1の記録 | `paint` と `Recording` | なし | なし |
| ビットマップのデータ（16、109、136画素とppem 0） | `paint`（`image` を数える） | `hb_font_paint_glyph` | なし（FreeTypeはPNGを展開するため、展開器のADRで比べる） |
| SVGの文書 | `svg_document` | `hb_ot_color_glyph_reference_svg` | なし |
| カラーのグリフの箱 | `glyph_extents` | `hb_font_get_glyph_extents` | なし |
| EBDT（5c、組み立てたフォント） | `glyph_bitmap` と画素の書き込み | なし | `FT_Load_Glyph`（`FT_LOAD_DEFAULT`、そのストライクのppem） |
| COLRv0の画像（5d、12、24、48画素） | `render_color` | なし | `FT_Load_Glyph`（`FT_LOAD_COLOR`）と `FT_Render_Glyph` |

- 結果の照合は、命令の数、層の数、`image` のデータの長さの合計、箱の値の合計、画素の合計で行う。
- FreeTypeの塗りの木は、たどる処理を `bench/` の側で書く。HarfBuzzとsofelはライブラリがたどる。違いは記録に書く。

## トレードオフ

- 数に `f32` を使うため、sofelの固定小数点の方針（[ADR 0009](0009-geom-fixed-point-and-pen.md)）から外れる。16.16の値は24bitの仮数に収まらず、丸めが生じる。代わりに、HarfBuzz、skrifa、今の描画の仕組みと同じ型になる。固定小数点にすると、放射の半径（符号なし16bit）や変換の合成が16.16の範囲を超え、型を増やす必要がある。`f64` にすると、描画の仕組みに渡すたびに変換が要り、HarfBuzzとの比較の数も揃わない。生の値の正しさは、FreeTypeの16.16の値との完全一致で別に確かめる。
- 三角関数を自分で書くため、HarfBuzzの `sincosf` と最後の1ビットが違いうる。決定性を優先し、適合テストでは3桁目の境界の項目を一覧で管理する。
- `fill(&Brush)` にまとめ、グループの合成の方法を両端で渡すため、HarfBuzzの塗りの関数とメソッドが1対1に対応しない。HarfBuzzから移る利用者は、`Brush` の種類で分ける必要がある。代わりに、トレイトが小さくなり、描画の仕組みの形に合う。命令の順と値はHarfBuzzと同じなので、比べることはできる。
- 停止点の正規化と線形の2点への変換を、skrifaと違って渡すまえに行わない。HarfBuzzと同じ値を出して比べられることと、フォントの値を失わずに渡すことを優先する。要る利用者は補助の関数を呼ぶ。
- 単色に、解決した色と出どころ（番号とアルファ）の両方を持たせるため、命令が少し大きくなる。代わりに、HarfBuzzと同じ色をそのまま使う利用者と、パレットを替えて色を解決しなおす利用者（skrifaの形）の両方に合う。
- 根に拡大の変換を置かないため、HarfBuzzの塗りの関数を移した利用者は、拡大を自分で足す必要がある。代わりに、塗りの値がフォントの単位のまま決まり、拡大の違う描画で同じ命令の列を使い回せる。
- PNGとSVGのデータを展開せずに渡すため、展開器のADRまでは、sofelだけでは絵文字のビットマップを画素にできない。HarfBuzzもFreeTypeの `svg-hooks` も同じ形で、ベクトルで書き出す利用者はPNGのデータをそのまま使える。
- C ABIで呼び返しではなく記録した配列を返すため、命令の列の確保が1回増える。代わりに、ほかの言語からの呼び出しが単純になり、呼び返しの境界を何度も越える費用が無い。
- 再公開（`sofel_outline::font`、`sofel_font::geom`）で依存の表を保つため、`sofel-color` の `Cargo.toml` からは実際の依存が読み取りにくい。依存の表を変えるには [ADR 0003](0003-crate-layers.md) を改めるADRが要り、表の意図（フォントを扱うクレートは下の層を通して使う）にも合うので、再公開を選ぶ。
- グリフの箱を `sofel-color` に置くため、HarfBuzzと同じ箱を得るには `sofel-color` まで要る。シェーピングだけを使う利用者にも、カラーのフォントでは `sofel-color` が要る。
- COLRv0の画像の手順を `sofel-cli` のライブラリに置くため、ライブラリの利用者には、フォントを知る描画の層がまだ無い。画素への合成のADRで、その層の置き場所を決める。
- ストライクの索引を作らないため、ストライクの多いフォントでは選択が線形の時間になる。
- Porter-Duffの合成はPDFで表せず、画素への合成のADRまでは、ベクトルで書き出す利用者は近似しかできない。実際の絵文字のフォントがどれだけ使うかは、5bの実装で `hb-dump` の出力から数え、そのADRの判断の材料にする。

## 参考文献

- HarfBuzz `src/hb-paint.h`、`src/hb-paint.hh`、`src/hb-paint.cc`（`hb_paint_normalize_color_line`、`hb_paint_reduce_linear_anchors`、`hb_paint_sweep_gradient_tiles`）、`src/hb-paint-bounded.cc`、`src/hb-paint-extents.cc`、`src/hb-geometry.hh`、`src/hb-algs.hh`（`hb_sincos`）、`src/hb-limits.hh`、`src/hb-font.cc`（`hb_font_t::paint_glyph`）、`src/hb-ot-color.h`、`src/hb-ot-color.cc`、`src/hb-ot-font.cc`（`hb_ot_paint_glyph_or_fail`、`hb_ot_get_glyph_extents`）、`src/OT/Color/COLR/COLR.hh`、`src/OT/Color/CPAL/CPAL.hh`、`src/OT/Color/CBDT/CBDT.hh`、`src/OT/Color/sbix/sbix.hh`、`src/OT/Color/svg/svg.hh`、`src/hb-vector-paint-pdf.cc`、`src/hb-raster-paint.cc`、`src/hb-raster-image.cc`、`test/api/test-paint.c`、`test/api/results-paint/`、`test/api/test-ot-color.c`、`test/api/test-raster.cc`、`test/vector/README.md`、`test/vector/data/tests/basic.tests`（15.1.0）
- FreeType `include/freetype/ftcolor.h`、`include/freetype/otsvg.h`、`src/sfnt/ttcolr.c`（`tt_face_colr_blend_layer`）、`src/sfnt/ttcpal.c`、`src/sfnt/ttsbit.c`、`src/sfnt/pngshim.c`、`src/sfnt/sfobjs.c`、`src/base/ftobjs.c`（`FT_Render_Glyph_Internal`、`FT_Match_Size`、`FT_Request_Size`）、`src/base/ftbitmap.c`（2.15.0）
- fontations skrifa 0.49.0 `skrifa/src/color/mod.rs`（`ColorPainter`、`Brush`）
- OpenType Specification: COLR、CPAL、CBDT、CBLC、EBDT、EBLC、sbix、SVG https://learn.microsoft.com/en-us/typography/opentype/spec/
- Apple TrueType Reference Manual: `bhed`、`bloc`、`bdat`、`sbix` https://developer.apple.com/fonts/TrueType-Reference-Manual/
- CSS Fonts Module Level 4: `font-palette`、`@font-palette-values` https://www.w3.org/TR/css-fonts-4/
- ISO 32000-2（PDF 2.0）のシェーディングと透明の章
- musl libc の `sincosf` と `tanf`（移植の候補。実装のまえに版とライセンスを確かめる） https://musl.libc.org/
- Golem `docs/roadmap.md`（「絵文字のレンダリング」「フォントのフォールバック」）
