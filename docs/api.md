# APIの手引き

sofelのライブラリのクレートを使うための手引き。
各項目の詳しい説明はrustdocにある。手元では次で作って開く。

```sh
cargo doc --no-deps -p sofel-geom -p sofel-read -p sofel-font -p sofel-outline -p sofel-raster -p sofel-cli --open
```

ここに書くのは、2段階目（[ADR 0009](adr/0009-geom-fixed-point-and-pen.md)〜[ADR 0012](adr/0012-antialiased-rasterizer.md)）までに公開しているAPIである。
仕様は [specifications.md](specifications.md) にある。

## クレートの構成

| クレート | 役割 | 依存 |
|---|---|---|
| `sofel-geom` | 固定小数点の演算（`FT_MulFix` などと同じ結果）と、輪郭を受け渡す `Pen` | なし |
| `sofel-read` | sfntの表の読み取りと検証。コピーも確保もしない | なし |
| `sofel-font` | フォントを一度だけ読み、グリフの数、文字からグリフへの対応、送り幅を返す | `sofel-read`、`sofel-geom` |
| `sofel-outline` | glyfとGLYFの輪郭を、ヒンティングなしでFreeTypeと同じ座標で `Pen` に渡す | `sofel-font`、`sofel-geom` |
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

- cmapのサブテーブルはHarfBuzzと同じ順で選ぶ。送り幅は、hmtxが欠けたり短かったりするときもHarfBuzzと同じ値を返す（[ADR 0010](adr/0010-font-foundation.md)）。
- `FontError` は、ファイルがsfntでもコレクションでもない、`head` が読めない、番号のフォントが無い、のいずれか。
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
- 表ごとのモジュール：`head`、`maxp`、`hhea`、`hmtx`、`cmap`、`loca`、`glyf`。それぞれにビューの型と `validate` 関数がある。大きい番号の版（`MAXP`、`HHEA`、`HMTX`、`GLYF`、`LOCA`）は `CountWidth` や `TAG_WIDE` で扱う。
- `be` は、範囲を確かめるビッグエンディアンの読み取りの関数。

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
| `sofel_font_table` | `Font::table` | `hb_face_reference_table`／`FT_Load_Sfnt_Table` |
| `sofel_outline_draw`、`sofel_outline_load`、`sofel_outline_decompose` | `draw`、`load`、`Scratch::decompose` | `FT_Load_Glyph` + `FT_Outline_Decompose` |
| `sofel_rasterizer_*`、`sofel_pixel_box` | `Rasterizer`、`pixel_box` | `FT_Outline_Render` |
| `sofel_render_glyph` | `sofel_cli::glyph::render` | `FT_Render_Glyph` |
| `sofel_validate`、`sofel_diagnostic_*` | `validate::sfnt`、`validate::collection`、`Kind` | なし |

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
