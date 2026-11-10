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
| `sofel-geom` | 固定小数点の演算（`FT_MulFix` などと同じ結果）と、輪郭を受け渡す `Pen` | なし |
| `sofel-unicode` | Unicode 18.0.0の文字の性質。HarfBuzzの既定のUnicodeの関数と同じ値を返す | なし |
| `sofel-read` | sfntの表の読み取りと検証。コピーも確保もしない | なし |
| `sofel-font` | フォントを一度だけ読み、グリフの数、文字からグリフへの対応、送り幅を返す | `sofel-read`、`sofel-geom` |
| `sofel-outline` | glyfとGLYFの輪郭を、ヒンティングなしでFreeTypeと同じ座標で `Pen` に渡す。HarfBuzzと同じグリフの箱（glyf、CFF2、CFF）を求める | `sofel-font`、`sofel-geom` |
| `sofel-raster` | FreeTypeのsmoothのラスタライザと同じ計算で、8bitのアンチエイリアスの画像を作る | `sofel-geom` |
| `sofel-cli` | 小さな道具（`sofel-validate`、`sofel-render`）と、1グリフを描く `glyph::render` | すべて |
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
let advance: u16 = font.advance(gid.unwrap_or(GlyphId(0))); // フォントの単位
```

| sofel | HarfBuzz／FreeType |
|---|---|
| `Font::new(data, index)` | `hb_face_create` + `hb_font_create`／`FT_New_Memory_Face` |
| `Font::nominal_glyph(c)` | `hb_font_get_nominal_glyph` |
| `Font::variation_glyph(c, vs)` | `hb_font_get_variation_glyph` |
| `Font::advance(g)` | `hb_font_get_glyph_h_advance`（拡大縮小なし） |
| `Font::ft_left_side_bearing(g)` | FreeTypeの `tt_face_get_metrics` の左のサイドベアリング（輪郭のpp1.xに使う） |
| `Font::table(tag)` | `hb_face_reference_table` |
| `Font::h_extents()`、`Font::v_extents()` | `hb_font_get_extents_for_direction`（代わりの値つき） |
| `Font::v_advance(g)` | `hb_font_get_glyph_v_advance`（下向きが負） |
| `Font::v_origin(g, extents)` | `hb_font_get_glyph_v_origin` |
| `Font::glyf_extents(g)` | `hb_font_get_glyph_extents`（glyfだけ。CFFとCFF2は `sofel_outline::HbExtents`） |
| `Font::metrics_position(tag)`、`Font::metrics_position_with_fallback(tag, extents)` | `hb_ot_metrics_get_position`、`hb_ot_metrics_get_position_with_fallback` |
| `Font::baseline(tag, direction, script)`、`Font::baseline_with_fallback(tag, direction, script, extents)` | `hb_ot_layout_get_baseline`、`hb_ot_layout_get_baseline_with_fallback` |
| `Font::layout_font_extents(direction, script, language)` | `hb_ot_layout_get_font_extents` |
| `Font::os2()`、`Font::fs_type()` | `FT_Get_Sfnt_Table(FT_SFNT_OS2)`、`FT_Get_FSType_Flags` |
| `HbScale::for_font(font, x, y)` | `hb_font_set_scale` |
| `Font::names()`、`Font::name_utf8(id, lang, out)` など | `hb_ot_name_list_names`、`hb_ot_name_get_utf8` など |
| `Font::sfnt_name(i)`、`Font::postscript_name(out)` | `FT_Get_Sfnt_Name`、`FT_Get_Postscript_Name` |
| `Font::glyph_name(g)`、`GlyphNames::glyph_from_name(name)` | `hb_font_get_glyph_name`、`hb_font_get_glyph_from_name` |
| `Font::ft_glyph_name(g)`、`GlyphNames::ft_glyph_from_name(name)` | `FT_Get_Glyph_Name`、`FT_Get_Name_Index` |
| `Font::ft_face_metrics()`、`Font::ft_v_advance(g)` | `FT_Face` の値、`FT_Get_Advance`（縦） |

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

- グリフ名はHarfBuzzと同じく、postに無ければCFFのcharsetから引く（CIDキーのフォントには無い。[ADR 0021](adr/0021-cff-outlines.md)）。名前から引く索引は呼び出し側が持つ。`let mut buf = vec![0u32; font.glyph_names_len()]; let names = font.glyph_names(&mut buf)?;` で作り、`names.glyph_from_name(b"A")` で引く。CFFの名前が同じグリフが複数あれば、グリフIDの小さいものを返す。`glyph_name` は呼び出しごとに `HbCff1` を作るので、繰り返し引くときは作っておいた値を `glyph_name_with(g, cff)` に渡す。名前の文字列は呼び出し側の配列に書く（`name_utf8` など）か、フォントのデータを借りて返す（`glyph_name`、`sfnt_name`）。
- 下の層が要るときは `sofel_font::read`（`sofel-read` の再公開）と `Font::sfnt()`、`Font::head()` を使う。

## 輪郭を取り出す（`sofel-outline`）

`Pen` を実装した値に、輪郭の命令を送る。
命令の並びはFreeTypeの `FT_Outline_Decompose` と同じで、各輪郭は `move_to` で始まり、始点に戻る線か曲線のあと `close` で終わる。

```rust
use sofel_geom::{Command, FnPen};
use sofel_outline::{Scale, Scratch, draw};

let mut scratch = Scratch::default(); // 使い回す
let scale = Scale::pixels(&font, 48, 48); // FT_Set_Pixel_Sizes(face, 0, 48) と同じ倍率
let mut pen = FnPen(|c: Command| println!("{c:?}"));
let info = draw(&font, gid, scale, &mut scratch, &mut pen)?;
// info.cbox: 制御点を含む箱（FT_Outline_Get_CBox）
// info.overlap: FT_OUTLINE_OVERLAP（重なりの印）
```

- `Scale::Unscaled` はフォントの単位のまま（`FT_LOAD_NO_SCALE`）、`Scale::Scaled { x, y }` は16.16の倍率で26.6の座標にする。
- 点は、FreeTypeと同じく幻の点のpp1.x（見出しの `xMin` − `Font::ft_left_side_bearing` を拡大縮小した値）だけ左に動かしてある。合成グリフでは、USE_MY_METRICSの部品があればその部品のpp1.xを使う（[ADR 0018](adr/0018-phantom-point-shift.md)）。
- `draw` は `load`（点を `Scratch` に読む）と `Scratch::decompose`（命令を送る）を続けたもの。輪郭を移動してから分解したいとき（ラスタライズ）は、2つを分けて呼ぶ。`decompose(offset, multiplier, pen)` は、`FT_Outline_Translate` で `offset` だけ動かし、各点を `multiplier` 倍してから分解する。暗黙のon-curveの点（2点の中点、0へ切り捨て）がこの順に依存するので、FreeTypeと一致させるには同じ順で行う。
- HarfBuzzのグリフの箱は `HbExtents`（[ADR 0021](adr/0021-cff-outlines.md)）：`HbExtents::new(&font)` でHarfBuzzのCFFとCFF2のアクセラレータ（`sofel_font::HbCff1`、`HbCff2`）を一度だけ作り、`glyph_extents(g)` で `hb_font_get_glyph_extents` と同じ箱（glyf、CFF2、CFFの順。拡大縮小なし）を、`scaled_glyph_extents(g, &scale)` で `HbScale` の拡大縮小をした箱を返す。CFFの箱はHarfBuzzの解釈器と同じく制御点を含み、FreeTypeの輪郭の `cbox` とは違うことがある。
- `OutlineError`：グリフIDが範囲外（`InvalidGlyphIndex`）、glyfとlocaが無い（`NoGlyphData`）、グリフのデータが壊れている（`InvalidOutline`）、複合グリフが壊れている・循環している・深すぎる（`InvalidComposite`）。エラーの前に送った命令は取り消されない。

## ラスタライズする（`sofel-raster`）

`Rasterizer` は `Pen` でもある。26.6の輪郭を記録し、`render` で8bitの被覆率の画像にする。

1グリフをFreeTypeの `FT_Render_Glyph`（`FT_RENDER_MODE_NORMAL`）と同じ画像にするなら、`sofel_cli::glyph::render` を使う。

```rust
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

let mut scratch = Scratch::default();
let mut raster = Rasterizer::new();
let bitmap = sofel_cli::glyph::render(&font, gid, 48, &mut scratch, &mut raster)?;
// bitmap.left, bitmap.top: FreeTypeの bitmap_left, bitmap_top
// bitmap.width, bitmap.rows, bitmap.pixels（上の行から、1画素1バイト、0〜255）
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
- 表ごとのモジュール：`head`、`maxp`、`hhea`、`hmtx`、`cmap`、`loca`、`glyf`、`os2`、`name`、`post`、`vhea`、`vmtx`、`vorg`、`base`、`gdef`、`gsub`、`gpos`。`device` はBASE、GDEF、GPOSが共有するDeviceの表のビュー（`delta_pixels` で補正を読む）。`layout` はGDEF、GSUB、GPOSが共有する構造（Coverage、ClassDef、ScriptList、FeatureList、LookupList、Lookup、GSUBとGPOSの見出しの `LayoutTable`、文脈のサブテーブル）。`base` のオフセットをたどる関数は、空のオフセットに `Ok(None)` を、読めない先に `Err` を返す。それぞれにビューの型と `validate` 関数がある。大きい番号の版（`MAXP`、`HHEA`、`HMTX`、`VHEA`、`VMTX`、`GLYF`、`LOCA`）は `CountWidth` や `TAG_WIDE` で扱う。
- `vhea` と `vmtx` のビューは `hhea` と `hmtx` のビューそのもの（`Vhea`、`Vmtx` は別名）で、`ascender` は縦のアセンダ、`advance` は縦の送り幅、`left_side_bearing` は上のサイドベアリングを返す。
- OS/2の版より後の欄（例えば版1の `s_cap_height`）は `None` を返す。nameの文字列が表の外にある記録は `Name::string` が `None` を返す。postの `MAC_GLYPH_NAMES` はMacの標準の258の名前。
- `be` は、範囲を確かめるビッグエンディアンの読み取りの関数。
- `cff` は `CFF ` と `CFF2` の表（[ADR 0021](adr/0021-cff-outlines.md)）：`Cff1`、`Cff2`（ヘッダとINDEXの並び）、`Index`（`get` は厳密に、`ft_element` と `ft_pointer` はFreeTypeの読み方で要素を返す）、`dict`（`DictTokens`、FreeTypeと同じ数の変換をする `Operand`、`TopDict`、`FontDict`、`PrivateDict`。値はFreeTypeが `FT_Long` に持つ64bitの値と型のまま）、`Charset`、`Encoding`、`FdSelect`、`charstring`（`CharStringTokens` と、FreeTypeのAdobe engineをヒンティングなしで移植したType 2 charstringの解読器 `decode`）、`FtBlend`（FreeTypeのCFF2の `blend` の係数）。`cff::validate` と `cff::validate2` が検証器で、`validate::sfnt` は `maxp` のグリフの数と `fvar` の軸の数を渡して呼ぶ。
- `cff::hb` は、HarfBuzzが `CFF ` と `CFF2` を読む手順（[ADR 0021](adr/0021-cff-outlines.md)）：サニタイザ `Sanitizer`（範囲の確かめと操作の予算）、`HbIndex`、`HbCharset`、`HbFdSelect`、encodingとCFF2のVariation Storeの確かめ、DICTの解釈器 `interpret_dict`。`sofel_font::HbCff1` と `HbCff2` がこれで作られる。
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

## Cから使う（`sofel-capi`）

`sofel-capi` は、上のAPIをCの呼び出し規約で公開する（[ADR 0015](adr/0015-c-abi.md)）。
宣言と約束は [`crates/sofel-capi/include/sofel.h`](../crates/sofel-capi/include/sofel.h) にあり、各関数の説明に、対応するRustの関数とHarfBuzzとFreeTypeの関数を書いている。
静的ライブラリは `cargo build --release -p sofel-capi` で `target/release/libsofel_capi.a` にできる。リンクに要るシステムのライブラリは `cargo rustc -p sofel-capi --lib --crate-type staticlib -- --print native-static-libs` で分かる。

```c
#include "sofel.h"

int32_t err;
sofel_font_t *font = sofel_font_create(data, len, 0, &err); /* data は複製しない */
uint32_t gid;
uint16_t advance;
if (font != NULL && sofel_font_nominal_glyph(font, 'A', &gid) == SOFEL_OK) {
  sofel_font_h_advance(font, gid, &advance);

  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create(); /* スレッドごとに1つ */
  sofel_rasterizer_t *raster = sofel_rasterizer_create();
  uint8_t pixels[4096];
  size_t n;
  int32_t box[4]; /* left, top, width, rows */
  err = sofel_render_glyph(font, gid, 48, scratch, raster, pixels, sizeof pixels, &n, box);
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
| `sofel_font_h_advance` | `Font::advance` | `hb_font_get_glyph_h_advance` |
| `sofel_font_ft_left_side_bearing` | `Font::ft_left_side_bearing` | `FT_Get_Glyph_Metrics` |
| `sofel_font_table` | `Font::table` | `hb_face_reference_table`／`FT_Load_Sfnt_Table` |
| `sofel_font_h_extents`、`sofel_font_v_extents`、`sofel_font_v_advance`、`sofel_font_v_origin`、`sofel_font_glyph_extents` | `Font::h_extents` など | `hb_font_get_extents_for_direction`、`hb_font_get_glyph_v_advance`、`hb_font_get_glyph_v_origin`、`hb_font_get_glyph_extents` |
| `sofel_font_metrics_position`、`sofel_font_metrics_position_with_fallback` | `Font::metrics_position` など | `hb_ot_metrics_get_position` など |
| `sofel_font_baseline`、`sofel_font_baseline_with_fallback`、`sofel_font_layout_font_extents` | `Font::baseline` など | `hb_ot_layout_get_baseline`、`hb_ot_layout_get_baseline_with_fallback`、`hb_ot_layout_get_font_extents` |
| `sofel_font_os2_field`、`sofel_font_fs_type` | `Font::os2`、`Font::fs_type` | `FT_Get_Sfnt_Table`、`FT_Get_FSType_Flags` |
| `sofel_scale_*` | `HbScale` | `hb_font_set_scale` のあとの各関数 |
| `sofel_font_name_*`、`sofel_font_sfnt_name*`、`sofel_font_postscript_name` | `Font::names` など | `hb_ot_name_*`、`FT_Get_Sfnt_Name`、`FT_Get_Postscript_Name` |
| `sofel_font_glyph_name`、`sofel_font_ft_glyph_name`、`sofel_glyph_names_*` | `Font::glyph_name`、`GlyphNames` | `hb_font_get_glyph_name`、`FT_Get_Glyph_Name`、`FT_Get_Name_Index` |
| `sofel_font_ft_face_metrics`、`sofel_font_ft_v_advance` | `Font::ft_face_metrics`、`Font::ft_v_advance` | `FT_Face`、`FT_Get_Advance` |
| `sofel_outline_draw`、`sofel_outline_load`、`sofel_outline_decompose` | `draw`、`load`、`Scratch::decompose` | `FT_Load_Glyph` + `FT_Outline_Decompose` |
| `sofel_rasterizer_*`、`sofel_pixel_box` | `Rasterizer`、`pixel_box` | `FT_Outline_Render` |
| `sofel_render_glyph` | `sofel_cli::glyph::render` | `FT_Render_Glyph` |
| `sofel_validate`、`sofel_diagnostic_*` | `validate::sfnt`、`validate::collection`、`Kind` | なし |
| `sofel_unicode_*`、`sofel_script_from_iso15924` | `sofel_unicode` の関数 | `hb_unicode_*`、`hb_script_from_string` |

- 誤りうる関数は `int32_t` を返し、`SOFEL_OK`（0）以外は誤り。`sofel_error_string` で英語の説明を引ける。NULLのポインタは `SOFEL_ERROR_NULL` になり、panicもクラッシュもしない。
- 輪郭は5つの関数ポインタと `void *user` で受け取る。`sofel_rasterizer_move_to` などはその型を持つので、`sofel_outline_decompose` にそのまま渡してラスタライザに記録できる。
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
