# APIの手引き

sofelのライブラリのクレートを使うための手引き。
各項目の詳しい説明はrustdocにある。手元では次で作って開く。

```sh
cargo doc --no-deps -p sofel-geom -p sofel-unicode -p sofel-read -p sofel-font -p sofel-outline -p sofel-raster -p sofel-cli --open
```

ここに書くのは、2段階目（[ADR 0009](adr/0009-geom-fixed-point-and-pen.md)〜[ADR 0012](adr/0012-antialiased-rasterizer.md)）までと、Unicodeの文字の性質（[ADR 0019](adr/0019-unicode-character-data.md)）の公開しているAPIである。
仕様は [specifications.md](specifications.md) にある。

## クレートの構成

| クレート | 役割 | 依存 |
|---|---|---|
| `sofel-geom` | 固定小数点の演算（`FT_MulFix` などと同じ結果）と、輪郭を受け渡す `Pen`、HarfBuzzの丸めを写す浮動小数点の補助（`float`）、カラーグリフの塗りを受け渡す `Painter`（[ADR 0023](adr/0023-color-glyphs.md)） | なし |
| `sofel-unicode` | Unicode 18.0.0の文字の性質。HarfBuzzの既定のUnicodeの関数と同じ値を返す | なし |
| `sofel-read` | sfntの表の読み取りと検証。コピーも確保もしない | なし |
| `sofel-font` | フォントを一度だけ読み、グリフの数、文字からグリフへの対応、送り幅を返す | `sofel-read`、`sofel-geom` |
| `sofel-outline` | glyfとGLYFの輪郭を、ヒンティングなしでFreeTypeと同じ座標で `Pen` に渡す。HarfBuzzと同じグリフの箱（glyf、CFF2、CFF）を求める | `sofel-font`、`sofel-geom` |
| `sofel-shape` | HarfBuzz 15.1.0のOTシェーパーと同じシェーピング（バッファ、計画、`GSUB` と `GPOS`）と、機能の文字列、直列化、タグ | `sofel-font`、`sofel-unicode`、`sofel-geom` |
| `sofel-bitmap` | 埋め込みビットマップのストライク、その選び方、グリフの画像のデータとメトリクス（[ADR 0023](adr/0023-color-glyphs.md)） | `sofel-font` |
| `sofel-color` | カラーグリフの塗りの命令、パレット、カラーグリフを含むグリフの箱（[ADR 0023](adr/0023-color-glyphs.md)） | `sofel-outline`、`sofel-bitmap` |
| `sofel-write` | sfntの組み立て（`build_sfnt`）と、HarfBuzzと同じ詰め方のオブジェクトの直列化（`Serializer`）と再配置（`Packed::repack`、`serialize_objects`。[ADR 0022](adr/0022-subsetting.md)） | `sofel-read` |
| `sofel-subset` | HarfBuzzの `hb-subset` と同じ出力のサブセット化（`Input`、`Plan`、`subset`、前処理の `Prepared`。[ADR 0022](adr/0022-subsetting.md)） | `sofel-font`、`sofel-write`、`sofel-unicode`、`sofel-geom` |
| `sofel-raster` | FreeTypeのsmoothのラスタライザと同じ計算で、8bitのアンチエイリアスの画像を作る | `sofel-geom` |
| `sofel-cli` | 小さな道具（`sofel-validate`、`sofel-render`、`sofel-paint`、`sofel-subset`）と、1グリフを描く `glyph::render` | すべて |
| `sofel-capi` | C ABI（[ADR 0015](adr/0015-c-abi.md)） | すべて |

`sofel-cli` と `sofel-capi` 以外は `no_std`（必要なものは `alloc`）で、`wasm32-unknown-unknown` にビルドできる（[ADR 0004](adr/0004-wasm-and-no-std.md)）。
`unsafe` は使わない（`sofel-capi` を除く）。どの入力でもpanicしない（clippyのlintで添字、検査なしの算術、情報の落ちるキャストを禁じ、ファジングで確かめている）。

## 共通の約束

- **借用で読む。** `Font<'a>` やsfntの表のビューは、呼び出し側が持つ `&'a [u8]` を借りる。フォントのデータは、ビューより長く生かしておく。
- **値は不変で、共有できる。** `Font` は `Send + Sync` で、複数のスレッドから同時に使える。
- **作業領域は呼び出し側が持つ。** `sofel_outline::Scratch` と `sofel_raster::Rasterizer` は、グリフごとに作らずに使い回す。中の `Vec` の容量が残るので、2つ目からは確保がほとんど起きない。スレッドごとに1つ持つ。
- **単位。** フォントの座標はフォントの単位（`unitsPerEm` 分の1 em）。拡大縮小した座標は26.6の固定小数点（1画素 = 64）。倍率は16.16の固定小数点（`Fixed`）。
- **y軸は上向き。** 輪郭の座標はy軸が上向き。画像（`Bitmap`）の行は上から下に並ぶ。
- **FreeTypeとの対応。** 輪郭とラスタライズはFreeType 2.15.0と、文字からグリフと送り幅はHarfBuzz 15.1.0と同じ結果を返すことを、適合テストで確かめている（[ADR 0005](adr/0005-conformance-tests.md)）。輪郭は、FreeTypeと同じく、幻の点のpp1.x（glyfのxMin − hmtxの左のサイドベアリング）だけ左にずらしてある（[ADR 0018](adr/0018-phantom-point-shift.md)）。

## フォントを開く（`sofel-font`）

```rust
use sofel_font::Font;
use sofel_font::read::GlyphId;

let data = std::fs::read("font.ttf")?;
let font = Font::new(&data, 0)?; // TTC/OTCでは2つ目の引数がフォントの番号

let upem = font.units_per_em();
let n = font.num_glyphs();
let gid: Option<GlyphId> = font.nominal_glyph('A' as u32);
let vs = font.variation_glyph(0x845B, 0xE0100); // 異体字セレクタ（cmapのフォーマット14）
let advance: u16 = font.default_instance().advance(gid.unwrap_or(GlyphId(0))); // フォントの単位
```

位置を持つ問い合わせは `Instance` の関数として公開する。`font.default_instance()` は既定位置の `Instance` を返す。

CFF2の係数列は `Instance::blend_scalars::<Ft>(vsindex)` または `::<HbF64>` で取り出す。返り値は既定成分を含まず、係数が0の列も元の順で含む。ストアの欠落、読めないストア、軸数の不一致、範囲外の番号、FreeTypeの位置のエラーは `BlendError` で区別する。C ABIの `sofel_cff2_blend_vector` はFreeTypeの16.16の係数列を返し、先頭に既定成分の65536を加える。必要な長さだけを調べるときは、出力ポインタにNULL、容量に0を渡す。

| sofel | HarfBuzz／FreeType |
|---|---|
| `Font::new(data, index)` | `hb_face_create` + `hb_font_create`／`FT_New_Memory_Face` |
| `Font::nominal_glyph(c)` | `hb_font_get_nominal_glyph`（グリフの数以上のIDは `None`。フォーマット2と8もFreeTypeと同じく引く） |
| `Font::variation_glyph(c, vs)` | `hb_font_get_variation_glyph`（同じく） |
| `Font::nominal_glyph_raw(c)`、`Font::variation_glyph_raw(c, vs)` | `hb_font_get_nominal_glyph`、`hb_font_get_variation_glyph` そのまま（シェーピングが使う） |
| `Instance::advance(g)` | `hb_font_get_glyph_h_advance`（拡大縮小なし） |
| `Instance::ft_left_side_bearing(g)` | FreeTypeの `tt_face_get_metrics` の左のサイドベアリング（輪郭のpp1.xに使う） |
| `Font::table(tag)` | `hb_face_reference_table` |
| `Instance::h_extents()`、`Instance::v_extents()` | `hb_font_get_extents_for_direction`（代わりの値つき） |
| `Instance::v_advance(g)` | `hb_font_get_glyph_v_advance`（下向きが負） |
| `Instance::v_origin(g, extents)` | `hb_font_get_glyph_v_origin` |
| `Instance::glyf_extents(g)` | `hb_font_get_glyph_extents`（glyfだけ。CFFとCFF2は `sofel_outline::HbExtents`） |
| `Instance::metrics_position(tag)`、`Instance::metrics_position_with_fallback(tag, extents)` | `hb_ot_metrics_get_position`、`hb_ot_metrics_get_position_with_fallback` |
| `Instance::baseline(tag, direction, script)`、`Instance::baseline_with_fallback(tag, direction, script, extents)` | `hb_ot_layout_get_baseline`、`hb_ot_layout_get_baseline_with_fallback` |
| `Instance::layout_font_extents(direction, script, language)` | `hb_ot_layout_get_font_extents` |
| `Font::os2()`、`Font::fs_type()` | `FT_Get_Sfnt_Table(FT_SFNT_OS2)`、`FT_Get_FSType_Flags` |
| `HbScale::for_font(font, x, y)` | `hb_font_set_scale` |
| `Font::names()`、`Font::name_utf8(id, lang, out)` など | `hb_ot_name_list_names`、`hb_ot_name_get_utf8` など |
| `Font::sfnt_name(i)`、`Font::postscript_name(out)` | `FT_Get_Sfnt_Name`、`FT_Get_Postscript_Name` |
| `Font::glyph_name(g)`、`GlyphNames::glyph_from_name(name)` | `hb_font_get_glyph_name`、`hb_font_get_glyph_from_name` |
| `Font::ft_glyph_name(g)`、`GlyphNames::ft_glyph_from_name(name)` | `FT_Get_Glyph_Name`、`FT_Get_Name_Index` |
| `Instance::ft_face_metrics()`、`Instance::ft_v_advance(g)` | `FT_Face` の値、`FT_Get_Advance`（縦） |

- cmapのサブテーブルはHarfBuzzと同じ順で選び、記号フォントとMacのエンコーディングではHarfBuzzと同じく文字を付けかえて引く（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。送り幅は、hmtxが欠けたり短かったりするときもHarfBuzzと同じ値を返す（[ADR 0010](adr/0010-font-foundation.md)）。
- `FontError` は、ファイルがsfntでもコレクションでもない、`head` が読めない、番号のフォントが無い、のいずれか。
- メトリクスの値はフォントの単位（HarfBuzzの既定の拡大縮小）で返す。ほかの拡大縮小の値は `HbScale` の同じ名前の関数で求める。HarfBuzzと同じく、整数の値は `em_scale_x`（`(v × mult + 32768) >> 16`）、小数で持つ値は `em_scalef_x`（HarfBuzzの `roundf`）、箱は `scale_glyph_extents` で拡大縮小し、ビット単位で一致する。代わりの値はその拡大縮小で求める（拡大縮小した値は、フォントの単位の代わりの値を拡大縮小したものと一致しないことがある）。
- `v_origin` と `metrics_position_with_fallback` は、グリフの箱を求める関数を受け取る。glyf、CFF2、CFFの箱は `sofel-outline` の `HbExtents`（[ADR 0021](adr/0021-cff-outlines.md)）が求めるので、`|g| extents.glyph_extents(g)` を渡す。glyfだけのフォントなら `|g| font.glyf_extents(g)` でもよい。カラーのグリフの箱は [ADR 0023](adr/0023-color-glyphs.md) で足す。

```rust
use sofel_font::{HbScale, MetricsTag};
use sofel_outline::HbExtents;

let ext = font.h_extents(); // ascender, descender, line_gap
let extents = HbExtents::new(&font); // 一度だけ作り、持ち続ける
let (x, y) = font.v_origin(GlyphId(1), |g| extents.glyph_extents(g));
let scale = HbScale::for_font(&font, 12 * 64, 12 * 64);
let advance = scale.v_advance(&font, GlyphId(1));
let x_height = scale.metrics_position(&font, MetricsTag::XHeight);
```

- 基準線（[ADR 0017](adr/0017-base-baselines.md)）は、`Direction`（横と縦）と、OpenTypeの文字体系のタグと、`sofel_font::baseline` の定数（`baseline::IDEO_EMBOX_CENTRAL` など）で引く。`baseline_with_fallback` は、BASEに無い基準線をHarfBuzzと同じく合成する。`math` と `hang` の代わりの値はグリフの箱を使うので、`v_origin` と同じく箱を求める関数を渡す。

```rust
use sofel_font::{Direction, baseline};
use sofel_font::read::Tag;

// 仮想ボディの中心（横書き、漢字）。
let center = font.baseline_with_fallback(
    baseline::IDEO_EMBOX_CENTRAL,
    Direction::Horizontal,
    Tag::new(b"hani"),
    |g| font.glyf_extents(g),
);
let (extents, from_base) = font.layout_font_extents(Direction::Horizontal, Tag::new(b"latn"), Tag::new(b"dflt"));
```

- CFFとCFF2（[ADR 0021](adr/0021-cff-outlines.md)）：`Cff::new(&font)` は、FreeTypeがフェイスを開けるときだけ成功する。一度作って持ち続ける。`is_cid_keyed`、`cid(gid)`、`ros()`（`FT_Get_CID_*`）、`charset_name`、`build_name_index` と `glyph_from_name`、`ft_glyph_name`、`ft_glyph_from_name`、`ps_font_info()`（`FT_Get_PS_Font_Info`）、`ps_fs_type()`、`top_dict()`、`font_dict(fd)`、`private_dict(fd)`（PDFの `StemV` などに使う生の値）を持つ。`Font::is_cff`、`is_cff2` は表の有無だけを見る。
- CFFのcharsetの索引（[ADR 0025](adr/0025-cff-charset-index.md)）：`cid`、`charset_name`、`ft_glyph_name` などは、索引が無いとcharsetの範囲を毎回前から読む。全グリフのように繰り返し引くときは、索引を一度作って `_with` の版に渡す。`let cs = cff.charset()?; let mut starts = vec![0u32; cs.num_ranges()]; let index = cs.index_into(&mut starts).ok()?;` のあと `cff.cid_with(&index, gid)`、`charset_name_with`、`ft_glyph_name_with`、`build_name_index_with`、`glyph_from_name_with` を使う。結果は索引の無い版と同じ。作る費用は範囲の数の配列を1回読むことで、配列は呼び出し側が持つ（`sofel-font` は確保しない）。1回だけ引くなら索引は要らない。C ABIでは `sofel_font_t` が内部に持つので、何もしなくてよい。
- グリフ名はHarfBuzzと同じく、postに無ければCFFのcharsetから引く（CIDキーのフォントには無い。[ADR 0021](adr/0021-cff-outlines.md)）。名前から引く索引は呼び出し側が持つ。`let mut buf = vec![0u32; font.glyph_names_len()]; let names = font.glyph_names(&mut buf)?;` で作り、`names.glyph_from_name(b"A")` で引く。CFFの名前が同じグリフが複数あれば、グリフIDの小さいものを返す。`glyph_name` は呼び出しごとに `HbCff1` を作るので、繰り返し引くときは作っておいた値を `glyph_name_with(g, cff)` に渡す。名前の文字列は呼び出し側の配列に書く（`name_utf8` など）か、フォントのデータを借りて返す（`glyph_name`、`sfnt_name`）。
- 下の層が要るときは `sofel_font::read`（`sofel-read` の再公開）と `Font::sfnt()`、`Font::head()` を使う。

## 輪郭を取り出す（`sofel-outline`）

`Pen` を実装した値に、輪郭の命令を送る。
命令の並びはFreeTypeの `FT_Outline_Decompose` と同じで、各輪郭は `move_to` で始まり、始点に戻る線か曲線のあと `close` で終わる。

```rust
use sofel_geom::{Command, FnPen};
use sofel_outline::{Outlines, Scale, Scratch};

let outlines = Outlines::new(&font); // フォントごとに一度作り、持ち続ける
let mut scratch = Scratch::default(); // 使い回す
let scale = Scale::pixels(&font, 48, 48); // FT_Set_Pixel_Sizes(face, 0, 48) と同じ倍率
let mut pen = FnPen(|c: Command| println!("{c:?}"));
let info = outlines.draw(gid, scale, &mut scratch, &mut pen)?;
// info.cbox: 制御点を含む箱（FT_Outline_Get_CBox）
// info.overlap: FT_OUTLINE_OVERLAP（重なりの印）
```

- `Outlines::new` は、FreeTypeと同じく、sfntの版が `OTTO` ならCFFかCFF2（`sofel_font::Cff`）、それ以外ならglyfを選ぶ（[ADR 0021](adr/0021-cff-outlines.md)）。glyfのフォントでは費用は無く、CFFのフォントでは開いた表を持つ。`sofel_outline::draw(&font, …)` と `load(&font, …)` は毎回 `Outlines::new` を呼ぶ。CFFの点はフォントの単位の整数に切り捨てられ、FontMatrixをかけてから拡大縮小される。`overlap` は常に偽。
- `Scale::Unscaled` はフォントの単位のまま（`FT_LOAD_NO_SCALE`）、`Scale::Scaled { x, y }` は16.16の倍率で26.6の座標にする。
- 点は、FreeTypeと同じく幻の点のpp1.x（見出しの `xMin` − `Instance::ft_left_side_bearing` を拡大縮小した値）だけ左に動かしてある。合成グリフでは、USE_MY_METRICSの部品があればその部品のpp1.xを使う（[ADR 0018](adr/0018-phantom-point-shift.md)）。
- `draw` は `load`（点を `Scratch` に読む）と `Scratch::decompose`（命令を送る）を続けたもの。輪郭を移動してから分解したいとき（ラスタライズ）は、2つを分けて呼ぶ。`decompose(offset, multiplier, pen)` は、`FT_Outline_Translate` で `offset` だけ動かし、各点を `multiplier` 倍してから分解する。暗黙のon-curveの点（2点の中点、0へ切り捨て）がこの順に依存するので、FreeTypeと一致させるには同じ順で行う。
- HarfBuzzのグリフの箱は `HbExtents`（[ADR 0021](adr/0021-cff-outlines.md)）：`HbExtents::new(&font)` でHarfBuzzのCFFとCFF2のアクセラレータ（`sofel_font::HbCff1`、`HbCff2`）を一度だけ作り、`glyph_extents(g)` で `hb_font_get_glyph_extents` と同じ箱（glyf、CFF2、CFFの順。拡大縮小なし）を、`scaled_glyph_extents(g, &scale)` で `HbScale` の拡大縮小をした箱を返す。CFFの箱はHarfBuzzの解釈器と同じく制御点を含み、FreeTypeの輪郭の `cbox` とは違うことがある。
- `OutlineError`：グリフIDが範囲外（`InvalidGlyphIndex`）、glyfとlocaが無い（`NoGlyphData`）、グリフのデータが壊れている（`InvalidOutline`）、複合グリフが壊れている・循環している・深すぎる（`InvalidComposite`）、`OTTO` でCFFもCFF2も無い（`UnsupportedFormat`）、FreeTypeが開けないCFF（`InvalidCff`）、解読できないcharstring（`InvalidCharString`）。エラーの前に送った命令は取り消されない。

## ラスタライズする（`sofel-raster`）

`Rasterizer` は `Pen` でもある。26.6の輪郭を記録し、`render` で8bitの被覆率の画像にする。

1グリフをFreeTypeの `FT_Render_Glyph`（`FT_RENDER_MODE_NORMAL`）と同じ画像にするなら、`sofel_cli::glyph::render`（フォントの `Outlines` を受け取る）を使う。

```rust
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

let mut scratch = Scratch::default();
let mut raster = Rasterizer::new();
let bitmap = sofel_cli::glyph::render(&font, gid, 48, &mut scratch, &mut raster)?;
// bitmap.left, bitmap.top: FreeTypeの bitmap_left, bitmap_top
// bitmap.width, bitmap.rows, bitmap.pixels（上の行から、1画素1バイト、0〜255）
```

COLRv0のグリフをFreeTypeの `FT_LOAD_COLOR` と同じ色の画像にするなら、`sofel_cli::glyph::render_color` を使う。層をそれぞれ `glyph::render` で描き、`sofel_raster::blend_layer` で乗算済みのBGRAの `BgraImage` に重ねる。層の無いグリフや層を描けないグリフは、FreeTypeと同じくグリフ自身の輪郭を灰色で描く。

```rust
use sofel_cli::glyph::{ColorGlyph, render_color};
use sofel_color::{PaintOptions, Palette};
use sofel_raster::BgraImage;

let options = PaintOptions { palette: Palette::Font(0), ..PaintOptions::default() };
let mut image = BgraImage::default();
match render_color(&outlines, gid, 48, &options, &mut scratch, &mut raster, &mut image)? {
    ColorGlyph::Color => { /* image.pixels: BGRA、4 × image.width バイトで1行 */ }
    ColorGlyph::Gray(bitmap) => { /* glyph::render と同じ */ }
}
```

`glyph::render` の中身は次のとおりで、自前で組むときの手本になる。

1. `sofel_outline::load` で、ppemの倍率の輪郭を読む。
2. `sofel_raster::pixel_box` で、制御点の箱を画素の境界へ外向きに丸め、画像の位置と大きさを決める（`ft_glyphslot_preset_bitmap`）。
3. `Rasterizer::clear` のあと、`Scratch::decompose(box.shift(), m, &mut raster)` で、画像の原点へ動かした輪郭を記録する。`m` は重なりの印があれば4、無ければ1。
4. `Rasterizer::render_placed(box, options)` で描く。`options.overlap` が真なら4×4の超標本化で描く。

輪郭を自分で作るときは、`Pen` で記録して `Rasterizer::render` を呼べばよい。画像の箱は記録した点から求める。

```rust
use sofel_geom::{Pen, Point};
use sofel_raster::{FillRule, Rasterizer, RenderOptions};

let p = |x: i32, y: i32| Point { x: x * 64, y: y * 64 }; // 画素を26.6に
let mut r = Rasterizer::new();
r.move_to(p(0, 0));
r.line_to(p(0, 10));
r.line_to(p(10, 10));
r.line_to(p(10, 0));
r.line_to(p(0, 0));
r.close();
let b = r.render(RenderOptions { fill: FillRule::NonZero, overlap: false, ppem: None })?;
assert_eq!((b.width, b.rows), (10, 10));
assert!(b.pixels.iter().all(|&v| v == 255));
```

- `FillRule` は `NonZero`（glyfの既定）と `EvenOdd`。
- `RenderOptions::ppem` を与えると、FreeTypeと同じく幅か高さがppemの16倍を超える画像を `RasterError::TooLarge` で断る。
- `Rasterizer::with_band_cells(n)` で、1つの帯のセルの数（既定8192、64KiB）を変えられる。小さくするとメモリは減り、輪郭を描き直す回数が増える。結果は変わらない。
- 返る `Bitmap` は `Rasterizer` の中の領域を借りる。次の描画の前に写すか使い終える。

## 表を読む・検証する（`sofel-read`）

`sofel-read` は、sfntの目録と各表のビュー、および検証器を持つ。
ビューの作成時にヘッダと配列の範囲を一度だけ確かめ、その後の読み取りは範囲外なら `None` を返す（既定値で埋めない、[ADR 0007](adr/0007-sfnt-read-foundation.md)）。

```rust
use sofel_read::sfnt::FontFile;
use sofel_read::validate;

let data = std::fs::read("font.ttf")?;
match FontFile::new(&data)? {
    FontFile::Sfnt(sfnt) => validate::sfnt(&sfnt, &mut |d| {
        println!("{} {:?} {} at {}", d.table, d.severity, d.kind.id(), d.offset);
    }),
    FontFile::Collection(c) => validate::collection(&c, &mut |d| println!("{d:?}")),
}
```

- 検証器は確保をせず、`Diagnostic`（表、オフセット、重大度、種類）をコールバックに渡す。`Kind::details` で種類ごとの値を取り出せる。`sofel-validate` コマンドの出力はこれを並べたもの。
- 表ごとのモジュール：`head`、`maxp`、`hhea`、`hmtx`、`cmap`、`loca`、`glyf`、`os2`、`name`、`post`、`vhea`、`vmtx`、`vorg`、`base`、`gdef`、`gsub`、`gpos`、`math`。`device` はBASE、GDEF、GPOS、MATHが共有するDeviceの表のビュー（`delta_pixels` で補正を読む）。`layout` はGDEF、GSUB、GPOSが共有する構造（Coverage、ClassDef、ScriptList、FeatureList、LookupList、Lookup、GSUBとGPOSの見出しの `LayoutTable`、文脈のサブテーブル）。`base` と `math` のオフセットをたどる関数は、空のオフセットに `Ok(None)` を、読めない先に `Err` を返す。それぞれにビューの型と `validate` 関数がある。大きい番号の版（`MAXP`、`HHEA`、`HMTX`、`VHEA`、`VMTX`、`GLYF`、`LOCA`）は `CountWidth` や `TAG_WIDE` で扱う。
- `vhea` と `vmtx` のビューは `hhea` と `hmtx` のビューそのもの（`Vhea`、`Vmtx` は別名）で、`ascender` は縦のアセンダ、`advance` は縦の送り幅、`left_side_bearing` は上のサイドベアリングを返す。
- OS/2の版より後の欄（例えば版1の `s_cap_height`）は `None` を返す。nameの文字列が表の外にある記録は `Name::string` が `None` を返す。postの `MAC_GLYPH_NAMES` はMacの標準の258の名前。
- `be` は、範囲を確かめるビッグエンディアンの読み取りの関数。
- `cff` は `CFF ` と `CFF2` の表（[ADR 0021](adr/0021-cff-outlines.md)）：`Cff1`、`Cff2`（ヘッダとINDEXの並び）、`Index`（`get` は厳密に、`ft_element` と `ft_pointer` はFreeTypeの読み方で要素を返す）、`dict`（`DictTokens`、FreeTypeと同じ数の変換をする `Operand`、`TopDict`、`FontDict`、`PrivateDict`。値はFreeTypeが `FT_Long` に持つ64bitの値と型のまま）、`Charset`、`Encoding`、`FdSelect`、`charstring`（`CharStringTokens` と、FreeTypeのAdobe engineをヒンティングなしで移植したType 2 charstringの解読器 `decode`）、`FtBlend`（FreeTypeのCFF2の `blend` の係数）。`cff::validate` と `cff::validate2` が検証器で、`validate::sfnt` は `maxp` のグリフの数と `fvar` の軸の数を渡して呼ぶ。
- `cff::hb` は、HarfBuzzが `CFF ` と `CFF2` を読む手順（[ADR 0021](adr/0021-cff-outlines.md)）：サニタイザ `Sanitizer`（範囲の確かめと操作の予算）、`HbIndex`、`HbCharset`、`HbFdSelect`（`fd_range` はHarfBuzzの `get_fd_range`）、encodingとCFF2のVariation Storeの確かめ、DICTの解釈器 `interpret_dict` と、DICTの字句を1つずつ読む `dict_token`。`sofel_font::HbCff1` と `HbCff2` がこれで作られ、`sofel-subset` はCFFとCFF2のサブセット化に使う。`HbCff2` の `var_store`、`fd_array`、`fd_select` は、CFF2のサブセット化が読むVariation Storeの位置、FDArray、FDSelectを返す。
- `fvar`、`avar`、`stat` は可変フォントの軸の表（[ADR 0022](adr/0022-subsetting.md)）：`Fvar`（軸の `VariationAxis` と名前つきのインスタンス。`find_axis` は最初に当たる軸）、`Avar`（軸ごとの `SegmentMap`。版2は変化量の表のオフセットを持つことだけを確かめる）、`Stat`（デザインの軸と、形式1から4の `AxisValue`。表に収まらない軸の値は `None`）。どれもHarfBuzzのサニタイザが受け付ける形だけを受け付け、`validate::sfnt` は `avar` の軸の数を `fvar` と比べる。
- `item_variation` はOpenTypeの共通のItem Variation Store（領域の一覧と `ItemVariationData` の領域の番号）のビューと検証器。CFF2の `vstore` が使い、3段階目の可変フォントでHVARなどと共有する。
- GSUBとGPOS（[ADR 0020](adr/0020-shaping-foundation.md)）は、`LayoutTable::new` から `script_list`、`feature_list`、`lookup_list` をたどり、lookupのサブテーブルを `gsub::subtable(&lookup, i)`（`SubstSubtable`）と `gpos::subtable(&lookup, i)`（`PosSubtable`）で読む。拡張はたどった先の型で返る。HarfBuzzと同じく、`Err` のサブテーブルは何も適用しないものとして扱う。

```rust
use sofel_read::gsub::{self, SubstSubtable};
use sofel_read::layout::LayoutTable;
use sofel_read::GlyphId;

fn single(gsub_table: &[u8], glyph: GlyphId) -> Option<u32> {
    let lookups = LayoutTable::new(gsub_table).ok()?.lookup_list().ok()??;
    let lookup = lookups.lookup(0).ok()??;
    match gsub::subtable(&lookup, 0).ok()? {
        SubstSubtable::SingleDelta { coverage, delta, mask } => {
            coverage.get(glyph).map(|_| glyph.0.wrapping_add(delta) & mask)
        }
        SubstSubtable::SingleList { coverage, substitutes } => {
            substitutes.get(usize::try_from(coverage.get(glyph)?).ok()?)
        }
        _ => None,
    }
}
```

- `gsub::validate` と `gpos::validate` は、GDEFのマークのグリフの集合の数を引数に取る（`None` なら調べない）。`gdef::validate` は表だけを取る。

## カラーグリフ（`sofel-color`、`sofel-bitmap`）

カラーグリフは `sofel_geom::Painter` に塗りの命令として渡す（[ADR 0023](adr/0023-color-glyphs.md)）。命令の順と値はHarfBuzzの `hb_font_paint_glyph` と同じで、単位はフォントの単位（yは上）。拡大と配置は呼び出し側の変換に含める。

```rust
use sofel_color::{PaintOptions, Palette, Painted, Scratch, paint};
use sofel_outline::font::Font;
use sofel_outline::font::geom::{Brush, CompositeMode, Image, Painter, Rect, Solid, Transform};

struct Log;
impl Painter for Log {
    fn push_transform(&mut self, t: Transform) { println!("transform {t:?}"); }
    fn pop_transform(&mut self) {}
    fn push_clip_glyph(&mut self, glyph: u32) { println!("clip {glyph}"); }
    fn push_clip_rectangle(&mut self, r: Rect) { println!("clip {r:?}"); }
    fn pop_clip(&mut self) {}
    fn fill(&mut self, brush: &Brush<'_>) { println!("fill {brush:?}"); }
    fn image(&mut self, image: &Image<'_>) -> bool { println!("{:?}", image.format); true }
    fn push_group(&mut self, _: CompositeMode) {}
    fn pop_group(&mut self, _: CompositeMode) {}
}

let data = std::fs::read("emoji.ttf")?;
let font = Font::new(&data, 0)?;
let glyph = font.nominal_glyph(0x1F600).ok_or("no glyph")?.0;
let options = PaintOptions { palette: Palette::Font(0), ..PaintOptions::default() };
let mut scratch = Scratch::default(); // グリフをまたいで使い回す
if paint(&font, glyph, &options, &mut scratch, &mut Log) == Painted::None {
    // カラーでないグリフ：輪郭を前景色で塗るかは呼び出し側が決める
}
```

| sofel | HarfBuzz | FreeType |
|---|---|---|
| `paint`、`paint_recording`（`Recording`） | `hb_font_paint_glyph_or_fail` | なし |
| `palette_count`、`palette_name_id`、`palette_flags`、`palette_color_name_id`、`palette_colors` | `hb_ot_color_palette_*` | `FT_Palette_Data_Get`、`FT_Palette_Select` |
| `find_palette`（CSSの `font-palette: light`、`dark`） | なし | なし |
| `layers` | `hb_ot_color_glyph_get_layers` | `FT_Get_Color_Glyph_Layer` |
| `svg_document` | `hb_ot_color_glyph_reference_svg` など | なし（`svg-hooks`） |
| `color_formats`、`glyph_color_format` | `hb_ot_color_has_*`、`hb_ot_color_glyph_has_paint` | `FT_HAS_COLOR` |
| `glyph_extents(&Outlines, glyph, &mut Scratch)` | `hb_font_get_glyph_extents` | なし |
| `base_paint`、`PaintRef`、`clip_box`、`layer_paint` | なし | `FT_Get_Color_Glyph_Paint`、`FT_Get_Paint`、`FT_Get_Color_Glyph_ClipBox`、`FT_Get_Paint_Layers` |
| `sofel_bitmap::strikes`、`match_strike` | なし | `available_sizes`、`FT_Match_Size` |
| `sofel_bitmap::choose_strike` | CBDTとsbixのストライクの選び方 | なし |
| `sofel_bitmap::glyph_bitmap` | なし | `FT_Load_Glyph`（展開しない） |
| `sofel_bitmap::bitmap_layout`、`draw_bitmap` | なし | `FT_Load_Glyph`（`FT_LOAD_COLOR`。EBDTの画素） |
| `Font::has_outlines`、`has_bitmaps` | なし | `FT_IS_SCALABLE`、`FT_HAS_FIXED_SIZES` |

- `Palette::Custom(&[Color])` で、`palette_colors` から作って一部を変えたパレット（CSSの `override-colors`）を使える。
- 単色（`Solid`）は、解決した色と出どころ（パレットの番号とアルファ）の両方を持つ。パレットを替えたとき、塗りの木をたどりなおさずに色を解決しなおせる。
- グラデーションの停止点はフォントに書かれた順で渡す。`sofel_geom::normalize_color_line`（並べかえと0〜1への直し）、`reduce_linear_anchors`（3点の線形を2点に）、`sweep_gradient_tiles`（扇形を角度の区間に）で、描画の仕組みの形に直す。
- PNGとSVGの画像は展開せずに渡す（`ImageFormat::Png`、`ImageFormat::Svg { gzip }`）。SVGは、文書の中の `id="glyph<グリフID>"` の要素を描く。
- 三角関数 `sofel_geom::sin_cos`、`tan` は、どの環境でも同じビットを返す。

## 固定小数点（`sofel-geom`）

| sofel | FreeType |
|---|---|
| `mul_fix(a, b)` | `FT_MulFix` |
| `div_fix(a, b)` | `FT_DivFix` |
| `mul_div(a, b, c)` | `FT_MulDiv` |
| `mul_div_no_round(a, b, c)` | `FT_MulDiv_No_Round` |
| `transform(p, m)` | `FT_Vector_Transform` |
| `vector_length(p)` | `FT_Vector_Length`（CORDIC） |

- 型：`F26Dot6`、`Fixed`（16.16）、`F2Dot14`、`Point`（`i32` の `x`、`y`）、`Matrix`。
- 結果が `i32` に収まらないときは飽和させる（FreeTypeは未定義の動作になる箇所）。

### HarfBuzzの丸め（`sofel_geom::float`）

HarfBuzzの値を写すときの浮動小数点の補助（[ADR 0026](adr/0026-harfbuzz-rounding.md)）。どれも `f32` と `f64` の四則演算だけで書き、FMAを使わない。

| sofel | HarfBuzz、C |
|---|---|
| `hb_roundf(v: f64) -> f64` | HarfBuzzの `roundf`（`_hb_roundf (double)`、`floor (x + .5)`） |
| `hb_roundf32(v: f32) -> f64` | HarfBuzzの `roundf`（`_hb_roundf (float)`、`floor ((double) x + .5)`） |
| `floor`、`floorf`、`ceilf`、`truncf` | Cの同じ名前の関数 |
| `hb_clamp_to_i32(v: f64) -> i32` | `hb_clamp_to<int> (double)` |
| `f32_to_i64`、`i64_to_f32`、`i32_to_f32` | Cの `(int64_t)`、`(float)` の変換 |

- HarfBuzzの `roundf` はCの `roundf` ではない。半分の値は負でも正の無限大の側へ丸まる（`hb_roundf(-2.5)` は −2.0、Cの `roundf` は −3.0）。一般の丸めには使わない。

## Unicodeの文字の性質（`sofel-unicode`）

```rust
use sofel_unicode::{GeneralCategory, Script, VerticalOrientation};

assert_eq!(sofel_unicode::general_category(0x41), GeneralCategory::UppercaseLetter);
assert_eq!(sofel_unicode::script(0x3042).tag(), *b"Hira");
assert_eq!(sofel_unicode::decompose(0xC5), Some((0x41, 0x30A)));
assert_eq!(sofel_unicode::compose(0x41, 0x30A), Some(0xC5));
assert_eq!(sofel_unicode::vertical_orientation(0x41), VerticalOrientation::R);
assert_eq!(Script::from_iso15924(b"latn").map(Script::tag), Some(*b"Latn"));
```

| sofel | HarfBuzz |
|---|---|
| `general_category(cp)` | `hb_unicode_general_category`（値も同じ順） |
| `combining_class(cp)` | `hb_unicode_combining_class` |
| `script(cp)`、`Script::tag` | `hb_unicode_script`（`hb_script_t` はタグの4バイト） |
| `Script::from_iso15924(tag)` | `hb_script_from_string`（`Qaai`、`Qaac` も読みかえる） |
| `mirroring(cp)` | `hb_unicode_mirroring` |
| `decompose(cp)`、`compose(a, b)` | `hb_unicode_decompose`、`hb_unicode_compose` |
| `is_extended_pictographic(cp)` | HarfBuzzの内部の `_hb_unicode_is_emoji_Extended_Pictographic` |
| `vertical_orientation(cp)` | なし（UAX #50） |

- どの関数も `const` の表を引くだけで、確保をしない。0x110000以上の値でもpanicせず、HarfBuzzと同じ既定の値を返す。
- `vertical_orientation` は、縦書きの文字列を、正立で組む区間（`U`、`Tu`）と、横に組んで90度回す区間（`R`、`Tr`）に分けるのに使う。分割そのものは呼び出し側の仕事である。
- 版は `UNICODE_VERSION`（`(18, 0, 0)`）。

## 言語と、文字体系と言語のタグ（`sofel-shape`）

```rust
use sofel_font::read::Tag;
use sofel_shape::{Language, Script, ot_tags_from_script_and_language, ot_tags_to_script_and_language};

let ja = Language::new(b"ja_JP.UTF-8").unwrap();
assert_eq!(ja.as_str(), "ja-jp");

let deva = Script::from_iso15924(b"Deva");
let mr = Language::new(b"mr");
let (scripts, languages) = ot_tags_from_script_and_language(deva, mr.as_ref());
assert_eq!(scripts, [Tag(*b"dev3"), Tag(*b"dev2"), Tag(*b"deva")]);
assert_eq!(languages, [Tag(*b"MAR ")]);

let (script, language) = ot_tags_to_script_and_language(Tag(*b"dev2"), Some(Tag(*b"MAR ")));
assert_eq!(script, deva);
assert_eq!(language.unwrap().as_str(), "mr-x-hbsc-64657632");
```

| sofel | HarfBuzz |
|---|---|
| `Language::new(bytes)`、`as_str` | `hb_language_from_string`（長さつき）、`hb_language_to_string` |
| `ot_tags_from_script_and_language(script, language)` | `hb_ot_tags_from_script_and_language`（上限は `MAX_TAGS_PER_SCRIPT`、`MAX_TAGS_PER_LANGUAGE` の3と3） |
| `ot_tags_from_script_and_language_max(script, language, n, m)` | 同じく、上限を指定する |
| `ot_tags_to_script_and_language(script_tag, language_tag)` | `hb_ot_tags_to_script_and_language` |
| `ot_tag_to_script(tag)`、`ot_tag_to_language(tag)` | `hb_ot_tag_to_script`、`hb_ot_tag_to_language` |
| `OT_TAG_DEFAULT_SCRIPT`、`OT_TAG_DEFAULT_LANGUAGE` | `HB_OT_TAG_DEFAULT_SCRIPT`（`DFLT`）、`HB_OT_TAG_DEFAULT_LANGUAGE`（`dflt`） |

- 文字体系の `None` は `HB_SCRIPT_INVALID`、言語の `None` は `HB_LANGUAGE_INVALID` に当たる。言語のタグの `None` は `dflt` と同じに扱う。
- ライブラリはプロセスのロケールを読まない。HarfBuzzの `hb_language_get_default()` と同じ言語が要るときは、呼び出し側が `setlocale(LC_CTYPE, NULL)` などで読んで `Language::new` に渡す。
- `ot_tag_to_script` は、ISO 15924の形でない値になるタグ（`LATN`、`abc2` など）に `Script::UNKNOWN` を返す。HarfBuzzはそのままの値を返す。
## 機能の文字列と直列化（`sofel-shape`）

```rust
use sofel_font::read::Tag;
use sofel_shape::{
    EmptyFont, Feature, GlyphInfo, GlyphPosition, SerializeFlags, SerializeFormat,
    deserialize_glyphs, serialize_glyphs,
};

let f = Feature::parse(b"aalt[3:5]=2").unwrap();
assert_eq!((f.tag, f.value, f.start, f.end), (Tag::new(b"aalt"), 2, 3, 5));
assert_eq!(f.format(), b"aalt[3:5]=2");

let infos = [GlyphInfo { glyph: 1, cluster: 0, ..Default::default() }];
let positions = [GlyphPosition { x_advance: 500, ..Default::default() }];
let mut out = Vec::new();
serialize_glyphs(&infos, Some(&positions), &EmptyFont, SerializeFormat::Text,
                 SerializeFlags::DEFAULT, &mut out);
assert_eq!(out, b"[gid1=0+500]");

let (mut back, mut back_positions) = (Vec::new(), Vec::new());
deserialize_glyphs(&out, SerializeFormat::Text, &EmptyFont, &mut back, &mut back_positions)
    .unwrap();
assert_eq!(back, infos);
```

| sofel | HarfBuzz |
|---|---|
| `Feature::parse`、`Feature::format`、`Feature::write` | `hb_feature_from_string`、`hb_feature_to_string` |
| `serialize_glyphs`、`serialize_glyphs_range` | `hb_buffer_serialize_glyphs`（`size` が `buf_size`） |
| `serialize_unicode`、`serialize_unicode_range` | `hb_buffer_serialize_unicode` |
| `serialize_invalid` | 内容の無い空のバッファの `hb_buffer_serialize` |
| `deserialize_glyphs`、`deserialize_unicode` | `hb_buffer_deserialize_glyphs`、`hb_buffer_deserialize_unicode`（`Err` の `consumed` が `end_ptr`） |
| `glyph_to_string`、`glyph_from_string` | `hb_font_glyph_to_string`、`hb_font_glyph_from_string` |
| `SerializeFormat::from_name`、`name` | `hb_buffer_serialize_format_from_string`、`_to_string` |
| `SerializeFont`、`EmptyFont` | 直列化に渡す `hb_font_t`、`hb_font_get_empty` |

- 書き出しも読み込みもバイト列で行う。グリフ名はバイト列だからである。
- フォントは `SerializeFont` を実装して渡す。グリフ名と名前からのグリフは `sofel_font::GlyphNames`、グリフの箱は `sofel_outline::HbExtents`、文字からのグリフは `Font::nominal_glyph` から得られる。
- 読み込みは結果を渡したベクタに追記し、失敗しても、それまでに読んだ項目を残す（HarfBuzzと同じ）。

## 文字列をシェーピングする（`sofel-shape`）

```rust
use sofel_font::Font;
use sofel_shape::{BufferFlags, Feature, Scale, SegmentProperties, ShapePlan, UnicodeBuffer, shape};

let font = Font::new(&data, 0)?;
let mut buffer = UnicodeBuffer::new();
buffer.set_flags(BufferFlags::BOT.union(BufferFlags::EOT));
buffer.push_utf8("Hello".as_bytes(), 0, None);

// 区間の性質を推定し、計画を作る。計画は覚えておいて使いまわす。
let props = SegmentProperties::default().guess(&buffer, None);
let features = [Feature::parse(b"-liga").unwrap()];
let plan = ShapePlan::new(&font, props, &features);

// 倍率がunitsPerEmなら、位置はフォントの単位になる。
let glyphs = shape(&font, &Scale::unscaled(&font), &plan, buffer);
for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
    println!("{} {} {}", info.glyph, info.cluster, pos.x_advance);
}
// 確保した領域を次の文字列に回す。
let buffer = glyphs.into_unicode();
```

| sofel | HarfBuzz |
|---|---|
| `UnicodeBuffer::push_utf8` などと設定の関数 | `hb_buffer_add_utf8` など、`hb_buffer_set_*` |
| `UnicodeBuffer::append`、`similar` | `hb_buffer_append`（Unicodeのバッファ）、`hb_buffer_create_similar` |
| `UnicodeBuffer::number_clusters` | `hb-shape` の、クラスタの値の文字の番号への付けなおし |
| `SegmentProperties::guess` | `hb_buffer_guess_segment_properties`（言語は引数。ロケールは読まない） |
| `ShapePlan::new` | `hb_shape_plan_create`（`ot` のシェーパー） |
| `shape`、`shape_with_extents` | `hb_shape_plan_execute` |
| `shape_fallback` | `fallback` のシェーパー |
| `GlyphBuffer::glyph_infos`、`glyph_positions` | `hb_buffer_get_glyph_infos`、`_positions` |
| `GlyphBuffer::normalize_glyphs`、`diff` | `hb_buffer_normalize_glyphs`、`hb_buffer_diff` |
| `Scale` | `hb_font_set_scale`、`hb_font_set_ppem` |

- 計画は不変で、スレッドの間で共有できる。区間の性質と機能の組ごとに作り、呼び出し側が覚える（`ShapePlan::key` で比べる）。
- CFFのフォントやカラーグリフでは、`sofel_outline` の箱を `shape_with_extents` に渡す（フォールバックのマークの配置と縦書きの原点に使う）。
- シェーピングは、cmapを `Font::nominal_glyph_raw` と `variation_glyph_raw`（HarfBuzzの `hb_font_get_nominal_glyph` と同じく、グリフの数以上のIDも返し、フォーマット2と8は引かない）で引く。
- `hb-shape` と同じ道具は `sofel-cli` の `sofel-shape` で、`sofel_cli::shape::run_with_input` をライブラリからも呼べる（[README](../README.md)）。
- [ADR 0024](adr/0024-complex-shapers.md) のシェーパー（アラビア文字、インド系文字など）は、まだdefaultの手順で処理する。`ShapePlan::shaper` はHarfBuzzが選ぶシェーパーを返す。

## サブセット化する（`sofel-subset`）

`Input` は不変の値で、`map_set` や `with_flags` が新しい値を返す。集合とフラグの意味はHarfBuzzの `hb_subset_input_t` と同じである（[ADR 0022](adr/0022-subsetting.md)）。

```rust
use sofel_font::Font;
use sofel_subset::{Flags, Input, SetKind, subset};

fn embed(data: &[u8], glyphs: &[u32]) -> Result<Vec<u8>, sofel_subset::SubsetError> {
    let font = Font::new(data, 0).map_err(|_| sofel_subset::SubsetError::NoGlyphs)?;
    let input = Input::new()
        .map_set(SetKind::Glyphs, |s| glyphs.iter().for_each(|&g| s.add(g)))
        .with_flags(Flags::RETAIN_GIDS);
    subset(&font, &input)
}
```

`Plan::new` で閉包とグリフの対応を求め、`Plan::execute` で表を書く。`subset` はこの2つを続けて呼ぶ。CFFのフォント（名前のキーとCIDのキー）は、HarfBuzzと同じく `CFF ` の表をサブセット化し、出力から取り出した `CFF ` の表はそれだけでCFFとして正しい（PDFの `FontFile3` に入れられる）。まだ書けない表を持つフォントは、その表を `SetKind::DropTables` に入れないかぎり `SubsetError::Unsupported` になる。HarfBuzzが失敗する入力では、sofelも失敗する。例えば、表の出力が元の表の256倍を超えてHarfBuzzの作業領域に入らないときは `SubsetError::Limit` になる。

可変フォントは、`with_all_axes_pinned_to_default` ですべての軸を既定の値に固定すると、HarfBuzzの `hb_subset_input_pin_all_axes_to_default` と同じく既定のインスタンスとして書く（fvarなどの変化量の表を除き、glyfの輪郭と送り幅から計る値を書きなおす）。軸の関数（`with_axis_pinned_to_default`、`with_axis_pinned`、`with_axis_range`）は、フォントに軸が無ければ `None` を返す。既定の値以外への固定と範囲の指定は、`Plan::new` が `SubsetError::Unsupported` を返す。

```rust
use sofel_font::Font;
use sofel_subset::{Input, SetKind, SubsetError, subset};

fn default_instance(data: &[u8], text: &str) -> Result<Vec<u8>, SubsetError> {
    let font = Font::new(data, 0).map_err(|_| SubsetError::NoGlyphs)?;
    let input = Input::new().map_set(SetKind::Unicodes, |s| text.chars().for_each(|c| s.add(u32::from(c))));
    let input = input.with_all_axes_pinned_to_default(&font).unwrap_or(input);
    subset(&font, &input)
}
```

同じフォントを何度もサブセット化するときは、`Prepared::new` で一度だけ前処理し（HarfBuzzの `hb_subset_preprocess`）、`Plan::with_prepared` で計画を作る。出力は前処理の無いときと同じで、計画が速くなる（cmapの対応の逆引き、CFFの解析済みのcharstringを使う）。`Prepared` は不変で、複製は安く（中身を共有する）、スレッドの間で共有できる。前処理したフォントと違うフォントに使うと `SubsetError::InvalidInput` になる。`cff_charstrings_index` と `cff_charstring_data` は、`CFF ` の表のCharStrings INDEXと各グリフのcharstringのバイト列を写さずに返す。

```rust
use sofel_font::Font;
use sofel_subset::{Input, Plan, Prepared, SetKind, SubsetError};

fn many(font: &Font<'_>, texts: &[&str]) -> Result<Vec<Vec<u8>>, SubsetError> {
    let prepared = Prepared::new(font);
    texts
        .iter()
        .map(|t| {
            let input = Input::new().map_set(SetKind::Unicodes, |s| t.chars().for_each(|c| s.add(u32::from(c))));
            Plan::with_prepared(font, &prepared, &input)?.execute(font)
        })
        .collect()
}
```

## Cから使う（`sofel-capi`）

`sofel-capi` は、上のAPIをCの呼び出し規約で公開する（[ADR 0015](adr/0015-c-abi.md)）。
宣言と約束は [`crates/sofel-capi/include/sofel.h`](../crates/sofel-capi/include/sofel.h) にあり、各関数の説明に、対応するRustの関数とHarfBuzzとFreeTypeの関数を書いている。
静的ライブラリは `cargo build --release -p sofel-capi` で `target/release/libsofel_capi.a` にできる。リンクに要るシステムのライブラリは `cargo rustc -p sofel-capi --lib --crate-type staticlib -- --print native-static-libs` で分かる。

```c
#include "sofel.h"

int32_t err;
sofel_font_t *font = sofel_font_create(data, len, 0, &err); /* data は複製しない */
uint32_t gid;
uint32_t advance;
if (font != NULL && sofel_font_nominal_glyph(font, 'A', &gid) == SOFEL_OK) {
  sofel_font_h_advance(font, NULL, gid, &advance);

  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create(); /* スレッドごとに1つ */
  sofel_rasterizer_t *raster = sofel_rasterizer_create();
  uint8_t pixels[4096];
  size_t n;
  int32_t box[4]; /* left, top, width, rows */
  err = sofel_render_glyph(font, NULL, gid, 48, scratch, raster, pixels, sizeof pixels, &n, box);
  /* SOFEL_ERROR_BUFFER_TOO_SMALL なら n バイトの配列で呼びなおす */
  sofel_rasterizer_destroy(raster);
  sofel_outline_scratch_destroy(scratch);
}
sofel_font_destroy(font);
```

| sofel（C） | Rust | HarfBuzz／FreeType |
|---|---|---|
| `sofel_font_create` | `Font::new` | `hb_face_create`／`FT_New_Memory_Face` |
| `sofel_font_nominal_glyph`、`sofel_font_variation_glyph` | `Font::nominal_glyph`、`Font::variation_glyph` | `hb_font_get_nominal_glyph`、`hb_font_get_variation_glyph` |
| `sofel_font_h_advance` | `Instance::advance` | `hb_font_get_glyph_h_advance` |
| `sofel_font_ft_left_side_bearing` | `Instance::ft_left_side_bearing` | `FT_Get_Glyph_Metrics` |
| `sofel_font_table` | `Font::table` | `hb_face_reference_table`／`FT_Load_Sfnt_Table` |
| `sofel_font_h_extents`、`sofel_font_v_extents`、`sofel_font_v_advance`、`sofel_font_v_origin`、`sofel_font_glyph_extents` | `Instance::h_extents` など | `hb_font_get_extents_for_direction`、`hb_font_get_glyph_v_advance`、`hb_font_get_glyph_v_origin`、`hb_font_get_glyph_extents` |
| `sofel_font_metrics_position`、`sofel_font_metrics_position_with_fallback` | `Instance::metrics_position` など | `hb_ot_metrics_get_position` など |
| `sofel_font_baseline`、`sofel_font_baseline_with_fallback`、`sofel_font_layout_font_extents` | `Instance::baseline` など | `hb_ot_layout_get_baseline`、`hb_ot_layout_get_baseline_with_fallback`、`hb_ot_layout_get_font_extents` |
| `sofel_font_os2_field`、`sofel_font_fs_type` | `Font::os2`、`Font::fs_type` | `FT_Get_Sfnt_Table`、`FT_Get_FSType_Flags` |
| `sofel_scale_*` | `HbScale` | `hb_font_set_scale` のあとの各関数 |
| `sofel_font_name_*`、`sofel_font_sfnt_name*`、`sofel_font_postscript_name` | `Font::names` など | `hb_ot_name_*`、`FT_Get_Sfnt_Name`、`FT_Get_Postscript_Name` |
| `sofel_font_glyph_name`、`sofel_font_ft_glyph_name`、`sofel_glyph_names_*` | `Font::glyph_name`、`GlyphNames` | `hb_font_get_glyph_name`、`FT_Get_Glyph_Name`、`FT_Get_Name_Index` |
| `sofel_font_ft_face_metrics`、`sofel_font_ft_v_advance` | `Instance::ft_face_metrics`、`Instance::ft_v_advance` | `FT_Face`、`FT_Get_Advance` |
| `sofel_outline_draw`、`sofel_outline_load`、`sofel_outline_decompose` | `draw`、`load`、`Scratch::decompose` | `FT_Load_Glyph` + `FT_Outline_Decompose` |
| `sofel_rasterizer_*`、`sofel_pixel_box` | `Rasterizer`、`pixel_box` | `FT_Outline_Render` |
| `sofel_render_glyph` | `sofel_cli::glyph::render` | `FT_Render_Glyph` |
| `sofel_render_color_glyph`、`sofel_blend_layer` | `sofel_cli::glyph::render_color`、`sofel_raster::blend_layer` | `FT_Render_Glyph`（`FT_LOAD_COLOR`）、`tt_face_colr_blend_layer` |
| `sofel_validate`、`sofel_diagnostic_*` | `validate::sfnt`、`validate::collection`、`Kind` | なし |
| `sofel_unicode_*`、`sofel_script_from_iso15924` | `sofel_unicode` の関数 | `hb_unicode_*`、`hb_script_from_string` |
| `sofel_feature_parse`、`sofel_feature_format` | `Feature::parse`、`Feature::format` | `hb_feature_from_string`、`hb_feature_to_string` |
| `sofel_script_from_string`、`sofel_language_*`、`sofel_ot_tag*` | `Script::from_iso15924`、`Language`、`ot_tags_from_script_and_language` など | `hb_script_from_string`、`hb_language_from_string`、`hb_ot_tags_from_script_and_language` など |
| `sofel_segment_guess` | `SegmentProperties::guess` | `hb_buffer_guess_segment_properties` |
| `sofel_shape_plan_*` | `ShapePlan` | `hb_shape_plan_create_cached` |
| `sofel_buffer_*` | `UnicodeBuffer`、`GlyphBuffer` | `hb_buffer_*` |
| `sofel_shape`、`sofel_shape_fallback` | `shape_with_extents`、`shape_fallback` | `hb_shape_plan_execute` |
| `sofel_buffer_serialize`、`sofel_buffer_deserialize_*`、`sofel_buffer_diff` | `serialize_glyphs` など、`diff` | `hb_buffer_serialize`、`hb_buffer_deserialize_*`、`hb_buffer_diff` |
| `sofel_subset_input_*` | `Input`、`RangeSet` | `hb_subset_input_*`、`hb_set_*` |
| `sofel_subset_prepared_*` | `Prepared` | `hb_subset_preprocess` |
| `sofel_subset_plan_*` | `Plan` | `hb_subset_plan_*` |
| `sofel_subset_cff_charstrings_index`、`sofel_subset_cff_charstring_data` | `cff_charstrings_index`、`cff_charstring_data` | `hb_subset_cff_get_charstrings_index`、`hb_subset_cff_get_charstring_data` |
| `sofel_subset`、`sofel_bytes_destroy` | `subset` | `hb_subset_or_fail`、`hb_face_reference_blob` |
| `sofel_serialize_objects` | `sofel_write::serialize_objects` | `hb_subset_serialize_or_fail` |

- 誤りうる関数は `int32_t` を返し、`SOFEL_OK`（0）以外は誤り。`sofel_error_string` で英語の説明を引ける。NULLのポインタは `SOFEL_ERROR_NULL` になり、panicもクラッシュもしない。
- 輪郭は5つの関数ポインタと `void *user` で受け取る。`sofel_rasterizer_move_to` などはその型を持つので、`sofel_outline_decompose` にそのまま渡してラスタライザに記録できる。
- シェーピングは、HarfBuzzの `hb_shape` と違い、呼び出し側が計画を作って持つ。計画は区間の性質と機能の組ごとに1つ作り、スレッドの間で共有してよい。バッファはスレッドごとに持ち、`sofel_buffer_clear` で使いまわす。

```c
sofel_buffer_t *buf = sofel_buffer_create();
sofel_buffer_add_utf8(buf, text, len, 0, len);
sofel_segment_t in = {SOFEL_DIRECTION_TTB, 0, NULL}, props; /* 文字体系と言語は不明 */
sofel_segment_guess(text, len, &in, NULL, &props);
sofel_feature_t kern;
sofel_feature_parse("vkrn", 4, &kern);
sofel_shape_plan_t *plan = sofel_shape_plan_create(font, &props, &kern, 1, &err); /* 覚えて使いまわす */
sofel_shape(font, NULL, plan, buf); /* scale が NULL ならフォントの単位 */
uint32_t gids[64], clusters[64];
int32_t pos[4 * 64]; /* x_advance, y_advance, x_offset, y_offset */
int64_t n = sofel_buffer_glyphs(buf, gids, clusters, NULL, pos, 64);
/* n > 64 なら何も書かれていない。n 個の配列で呼びなおす */
sofel_buffer_clear(buf); /* 次の文字列へ */
```

- サブセット化は、入力を作って集合とフラグを入れ、`sofel_subset` を呼ぶ。出力はsofelが確保するので、`sofel_bytes_destroy` で解放する。入力は値を変える関数で変わるので、スレッドの間で共有しながら変えない。

```c
sofel_subset_input_t *input;
sofel_subset_input_create(&input); /* HarfBuzz の既定の値 */
for (size_t i = 0; i < n; i++)
  sofel_subset_input_set_add(input, SOFEL_SUBSET_SET_GLYPHS, gids[i]);
sofel_subset_input_set_flags(input, SOFEL_SUBSET_FLAGS_RETAIN_GIDS);
/* 可変フォントなら既定のインスタンスにする（軸が無ければ SOFEL_ERROR_NOT_FOUND） */
sofel_subset_input_pin_all_axes_to_default(input, font);
sofel_bytes_t out;
if (sofel_subset(font, input, &out) == SOFEL_OK) {
  /* out.data, out.len: hb-subset と同じバイト列 */
  sofel_bytes_destroy(&out);
}
sofel_subset_input_destroy(input);
```

- `sofel-read` の表のビューは公開しない。表のバイト列を `sofel_font_table` で取り出して読む。公開しない関数とその理由は `crates/sofel-capi/unmapped.txt` にある。
- `wasm32-wasip1` では、同じ静的ライブラリをwasi-sdkのclangでCのプログラムとリンクできる。例外処理の提案もC++の実行時ライブラリも要らない。

## wasmで使う

ライブラリのクレートは追加の設定なしで `wasm32-unknown-unknown` と `wasm32-wasip1` にビルドできる。
ファイルの読み込みは呼び出し側の仕事で、ライブラリは `&[u8]` を受け取るだけである。

```sh
cargo build --release --target wasm32-unknown-unknown -p sofel-font -p sofel-outline -p sofel-raster
```

## 性能

FreeTypeとHarfBuzzとの比較は [benchmarks/](benchmarks/) にある。
`Scratch` と `Rasterizer` を使い回すこと、`Font` を一度だけ作ることが、速さの前提である。
