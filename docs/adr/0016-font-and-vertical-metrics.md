# ADR 0016: フォント全体のメトリクスと縦書きのメトリクス（OS/2、name、post、vhea、vmtx、VORG、グリフの箱、拡大縮小）

ステータス: 採択→`roundf` と `x_mult` の写し方は [ADR 0026](0026-harfbuzz-rounding.md)

## 文脈

[ADR 0010](0010-font-foundation.md) は、`sofel-font` に横の送り幅までを入れ、縦書きのメトリクス（vhea、vmtx、VORG）と、OS/2、name、postの読み取りを後の段階に回した。

sofelはHarfBuzzとFreeTypeを置きかえる汎用のライブラリである。この ADR は、両者がフォント全体とグリフのメトリクスとして公開するもの（HarfBuzzの `hb_font_get_*_extents`、`hb_ot_metrics_*`、`hb_ot_name_*`、`hb_font_get_glyph_name`、拡大縮小、FreeTypeの `FT_Face` の値、`FT_Get_Advance`、`FT_Get_Glyph_Name`、`FT_Get_Sfnt_Name`、`FT_Get_Postscript_Name`、`FT_Get_FSType_Flags`）を網羅する。最初の利用者のGolemの使い方は、その一部である。

Golemは、PDFの出力のために、HarfBuzzとFreeTypeから次の値を取っている（`modules/golem-font/cbits/golem/golem_font.c`）。

| 値 | いまの取り方 |
|---|---|
| 埋め込みの許可 | FreeTypeの `FT_Get_FSType_Flags`（OS/2の `fsType`） |
| アセンダ、ディセンダ | FreeTypeの `face->ascender`、`face->descender` |
| キャップハイト | OS/2の `sCapHeight`（版2以上）。無ければアセンダ |
| PostScript名 | FreeTypeの `FT_Get_Postscript_Name` |
| グリフの横と縦の送り幅、箱 | FreeTypeの `FT_Load_Glyph`（`NO_SCALE`、`NO_HINTING`）の `horiAdvance`、`vertAdvance`、輪郭の箱 |
| 縦の原点 | HarfBuzzの `hb_font_get_glyph_v_origin` |

[ADR 0010](0010-font-foundation.md) は、メトリクスをHarfBuzzに揃える方針とした。シェーピングがHarfBuzzの値で位置を決めるため、ほかの値も同じ源から取ると、組版の中で値が食い違わない。

HarfBuzz 15.1.0の決め方は、次のとおりである。

- **フォント全体の横の値**（`hb_ot_get_font_h_extents`、`src/hb-ot-metrics.cc`）：OS/2の `fsSelection` の `USE_TYPO_METRICS`（ビット7）が立ち、OS/2があれば、`sTypoAscender`、`sTypoDescender`、`sTypoLineGap` を使う。そうでなければ `hhea`（または `HHEA`）の `ascender`、`descender`、`lineGap` を使う。アセンダは絶対値、ディセンダは絶対値の負にする（`_fix_ascender_descender`）。どれかが取れなければ、代わりにアセンダを `unitsPerEm × 0.8`、ディセンダをアセンダ − `unitsPerEm`、行間を0とする（`get_h_extents_with_fallback`）。
- **フォント全体の縦の値**：`vhea` の `ascender`、`descender`、`lineGap`。取れなければ、アセンダを `unitsPerEm / 2`、ディセンダをアセンダ − `unitsPerEm` とする。
- **縦の送り幅**（`hb_ot_get_glyph_v_advances`）：vmtxがあれば、その送り幅の負の値。無ければ、横の値（代わりの値を含む）のディセンダ − アセンダ。
- **縦の原点**（`hb_ot_get_glyph_v_origins`）：xは横の送り幅の半分（整数の割り算）。yは次の順で決める。
  1. VORGがあれば、VORGの値（グリフの記録が無ければ `defaultVertOriginY`）。
  2. vmtxとglyfがあれば、glyfの上の幻の点（phantom point）のy。可変でないときは、グリフの `yMax` と vmtxの上のサイドベアリングの和に当たる。点を読めなければ `unitsPerEm`。
  3. グリフの箱が取れれば、`y_bearing + (フォントの高さ + 箱の高さ) / 2`。フォントの高さは横の値のアセンダ − ディセンダ。取れなければアセンダ。
- **グリフの箱**（`hb_ot_get_glyph_extents`）：sbix、CBDT、COLR、VARC、glyf、CFF2、CFFの順に探す。glyfでは、`x_bearing` をhmtxの左のサイドベアリング（無ければ `min(xMin, xMax)`）、`y_bearing` を `max(yMin, yMax)`、幅を `|xMax − xMin|`、高さを `−|yMax − yMin|` とする。空のグリフは全部0。`x_bearing` をサイドベアリングにするのは、FreeTypeが輪郭をpp1.xだけずらすこと（[ADR 0018](0018-phantom-point-shift.md)）と同じ結果を出すためである。

FreeTypeの `FT_Face` の値（`src/sfnt/sfobjs.c` の `sfnt_load_face`）は、HarfBuzzと決め方が違う。

- アセンダ、ディセンダ、高さ：OS/2があり `USE_TYPO_METRICS` が立てば、`sTypo*`（高さはアセンダ − ディセンダ + `sTypoLineGap`）。そうでなければhheaの値。hheaのアセンダとディセンダがどちらも0で、OS/2があれば、`sTypo*` のどちらかが0でなければ `sTypo*`、そうでなければ `usWinAscent` と `−usWinDescent`（高さはその差）。符号は補正しない。
- `max_advance_width` はhheaの `advanceWidthMax`、`max_advance_height` はvheaがあればその `advanceHeightMax`、無ければ高さ。
- `underline_position` はpostの `underlinePosition − underlineThickness / 2`（線の中心へ直す）、`underline_thickness` はpostの値。
- vmtxの無いフォントの縦の送り幅と上のサイドベアリングは、OS/2の `sTypo*`、無ければhheaから作る（`src/truetype/ttgload.c` の `compute_glyph_metrics`）。

PostScript名はHarfBuzzに無く、FreeTypeの手順（`src/sfnt/sfdriver.c` の `sfnt_get_ps_name`）に従う。

- nameの記録のうち、名前ID 6で長さが正のものを探す。Windows（プラットフォーム3、エンコーディング0か1）は、言語0x409を優先し、無ければ最初のもの。Apple（プラットフォーム1、エンコーディング0）は、言語0を優先し、無ければ最初のもの。
- Windowsの記録を優先する。UTF-16BEの2バイトずつ見て、上位が0で下位がPostScript名に使える文字（ASCIIの印字可能な文字のうち、空白と `[]{}()<>/%` を除くもの、`sfnt_ps_map`）なら残し、ほかは捨てる。1文字も残らなければ、Appleの記録を同じく1バイトずつ見る。どちらも無ければ、名前は無い。
- CFFのフォントでも、sfntに包まれていればnameの表を使う（`src/cff/cffdrivr.c` の `cff_get_ps_name`）。
- 可変フォントの名前つきインスタンスの名前（`sfnt_get_var_ps_name`）は、可変フォントの段階で足す。既定のインスタンス（Golemの使い方）では、上の手順の名前になる。

`fsType` は、FreeTypeの `FT_Get_FSType_Flags` がOS/2の版が0xFFFFでなければその値を、OS/2が無ければ0を返す。

## 実装すること

### `sofel-read`

次の表のビューと検証器を足す（[ADR 0007](0007-sfnt-read-foundation.md)）。

- **OS/2**：版0（78バイト）、版1（86バイト）、版2〜4（96バイト）、版5（100バイト）。ビューは、版に応じた長さが無ければ誤りとし、版が5を超えれば版5の欄まで読む。欄ごとに読む関数を持つ。版より後の欄は `None` を返す。検証器は、版と長さの不一致、`usWeightClass` が1〜1000の外、`fsSelection` の矛盾（ITALICとREGULARが同時、など仕様が禁じる組）、`usFirstCharIndex` > `usLastCharIndex` を報告する。
- **name**：形式0と1。記録の配列、言語タグの記録（形式1）、文字列の領域。記録の文字列の範囲が領域の外なら、その記録の文字列は `None` とする（FreeTypeは長さを0にして読み飛ばす）。検証器は、領域の外の記録、記録の並び（プラットフォーム、エンコーディング、言語、名前IDの昇順）の乱れ、UTF-16BEの記録の奇数長を報告する。
- **vhea**、**vmtx**：hheaとhmtxと同じ形なので、[ADR 0007](0007-sfnt-read-foundation.md) の `hhea` と `hmtx` のビューを、表の名前だけを変えて使う（`VHEA`、`VMTX` の64Kを超える版も、`HHEA` と同じく `CountWidth` で扱う）。検証器も同じものを、表の名前を変えて使う。
- **post**：版1、2、2.5、3、4（AppleのTrueType）。版2の名前の文字列の配列と、Macの標準の258の名前。検証器は、版2の名前の番号が範囲の外であること、文字列の領域が足りないこと、版2.5（廃止）を報告する。
- **VORG**：版1（16bitのグリフID）と版2（24bitのグリフID）。記録の二分探索。検証器は、記録がグリフIDの昇順でないこと、重複、グリフの数以上のIDを報告する。

### `sofel-font`

`Font` に次の問い合わせを足す。値はすべてフォントの単位の整数とする。`Font` を作るときに表の位置を一度だけ見つけて持ち、問い合わせのたびに目録を探さない（[ADR 0010](0010-font-foundation.md) の速さの方針）。

| 問い合わせ | 方針 |
|---|---|
| `h_extents()` | HarfBuzzの `hb_font_get_h_extents` と代わりの値。アセンダ、ディセンダ、行間 |
| `v_extents()` | HarfBuzzの `hb_font_get_v_extents` と代わりの値 |
| `v_advance(gid)` | HarfBuzzの `hb_font_get_glyph_v_advance`。下向きを負とするHarfBuzzの符号に従う |
| `v_origin(gid, extents)` | HarfBuzzの `hb_font_get_glyph_v_origin`。上のVORG、幻の点、箱の順。3つ目の箱は、呼び出し側が渡す関数 `extents: impl Fn(GlyphId) -> Option<GlyphExtents>` で求める |
| `glyf_extents(gid)` | HarfBuzzの `hb_font_get_glyph_extents` のうちglyfの部分。見出しとhmtxだけで求める |
| `os2()` | OS/2のビュー（無ければ `None`） |
| `fs_type()` | FreeTypeの `FT_Get_FSType_Flags` |
| `postscript_name(out)` | FreeTypeの手順。呼び出し側のバッファ（最大127バイト）に書いて長さを返す。確保しない |
| `metrics_position(tag)` | HarfBuzzの `hb_ot_metrics_get_position` と `hb_ot_metrics_get_position_with_fallback`。`hb_ot_metrics_tag_t` のすべて（横と縦のアセンダ、ディセンダ、行間、クリップ、キャレットの傾きと位置、xHeight、キャップハイト、上付きと下付き、下線、取り消し線） |
| `names()`、`name_utf8(id, language, out)`、`name_utf16`、`name_utf32` | HarfBuzzの `hb_ot_name_list_names` と `hb_ot_name_get_utf*`（言語の照合、プラットフォームの優先、MacRomanなどの変換、置換の文字）。確保せず、呼び出し側のバッファに書く |
| `sfnt_name(index)` | FreeTypeの `FT_Get_Sfnt_Name` と同じ、生の記録 |
| `glyph_name(gid, out)`、`glyph_from_name(name)` | HarfBuzzの `hb_font_get_glyph_name` と `hb_font_get_glyph_from_name`（postの版1と2。CFFの名前は [ADR 0021](0021-cff-outlines.md)）。FreeTypeの `FT_Get_Glyph_Name`、`FT_Get_Name_Index` との違いは突き合わせで確かめ、違えば `ft_` の付いた関数を別に持つ。名前から引く索引は呼び出し側が持つ値とする（[ADR 0003](0003-crate-layers.md)） |
| `ft_face_metrics()` | FreeTypeの `FT_Face` のアセンダ、ディセンダ、高さ、最大の送り幅（横と縦）、下線の位置と太さ、`bbox`。上のFreeTypeの決め方 |
| `ft_v_advance(gid)` | FreeTypeの `FT_Get_Advance`（`FT_LOAD_VERTICAL_LAYOUT | FT_LOAD_NO_SCALE`）。vmtxが無ければ上の代わりの値 |

- グリフの箱は、層ごとに置く。glyfの見出しの箱は表を見るだけで求まるので `sofel-font` の `glyf_extents` に、charstringの解釈が要るCFFの箱は `sofel-outline`（[ADR 0021](0021-cff-outlines.md) の `Outlines::glyph_extents`。VARC、glyf、CFF2、CFFの順）に、塗りの木をたどるカラーの箱とHarfBuzzの全体の順（sbix、CBDT、COLR、…）は `sofel-color`（[ADR 0023](0023-color-glyphs.md) の `glyph_extents`）に置く。`sofel-font` は上の層に依存できない（[ADR 0003](0003-crate-layers.md)）ため、`v_origin` は箱を求める関数を引数で受け取る。この ADR の時点では、呼び出し側は `glyf_extents` を渡す。
- `v_origin` の幻の点は、`sofel-outline` に依存せずに求める。可変でないglyfでは、上の幻の点のyは `yMax + tsb`（`yMax` はグリフの見出しの値、`tsb` はvmtxの上のサイドベアリング）なので、`sofel-font` がglyfの見出しとvmtxから直接計算する。複合グリフでも見出しの `yMax` を使うことを、HarfBuzzの `get_points` の結果と突き合わせて確かめる。可変フォント（gvar）では幻の点が動くため、可変フォントの段階で `sofel-outline` に移す。
- [ADR 0010](0010-font-foundation.md) で後回しにしたcmapの付けかえは、OS/2を読めるようになるこの ADR で足す。HarfBuzzの `src/hb-ot-cmap-table.hh` の `cache_t` と同じく、記号フォント（(3,0)）ではOS/2のフォントのページ（`get_font_page`）に応じて `_hb_symbol_pua_map`、`_hb_arabic_pua_simp_map`、`_hb_arabic_pua_trad_map` で引きなおし、MacRomanの (1,0) ではUnicodeからMacRomanへ変換して引く。突き合わせは、下の `tools/hb-dump/` で全フォントの `hb_font_get_nominal_glyph` を書き出し、記号フォントとMacRomanのフォントを含めて完全一致を確かめる。

### 拡大縮小

- 値はフォントの単位で返し、拡大縮小は値の型 `HbScale`（`sofel-font`）で行う。HarfBuzzの `hb_font_t` と同じく、`x_scale` と `y_scale`（整数）から `x_mult = x_scale × 0x10000 / upem`（64bit、0へ切り捨て）と `x_multf = x_scale / upem`（`f32`）を作り、整数の値は `(v × mult + 32768) >> 16`（`em_mult`）、小数の値は `roundf(v × multf)`（`em_multf`）、箱は `scale_glyph_extents` の手順とする。HarfBuzzが値ごとにどちらを使うかも写し、拡大縮小した結果をHarfBuzzとビット単位で一致させる。
- FreeTypeの拡大縮小（`FT_Set_Char_Size` などの26.6と `FT_MulFix`）は、[ADR 0011](0011-glyf-outlines.md) の `Scale` が受け持つ。
- 浮動小数点は `f32` の四則演算だけとし、FMAを使わない（`mul_add` を呼ばない。Rustは明示しない限り融合しない）。`roundf` は自前で書く（[ADR 0004](0004-wasm-and-no-std.md)）。

### `sofel-capi`

[ADR 0015](0015-c-abi.md) の決まりに従い、`sofel_font_h_extents`、`sofel_font_v_extents`、`sofel_font_v_advance`、`sofel_font_v_origin`、`sofel_font_glyph_extents`（この ADR の時点ではglyfだけ。[ADR 0021](0021-cff-outlines.md) と [ADR 0023](0023-color-glyphs.md) の実装で、それぞれの層の箱を呼ぶように改める。`sofel_font_v_origin` も同じ箱を使う）、`sofel_font_fs_type`、`sofel_font_postscript_name`、`sofel_font_metrics_position`、`sofel_font_name_*`、`sofel_font_glyph_name`、`sofel_font_glyph_from_name`、`sofel_font_ft_face_metrics`、`sofel_scale_*`（`HbScale` の作成と、値の拡大縮小）と、OS/2の欄を読む `sofel_font_os2_*` を足す。

### `sofel-validate`

新しい表の検証器を `sofel-validate` の検証に加える。誤りの種類を `Kind` に足し、`--json` の出力にも出す（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。

## 実装しないこと

- BASEの基準線（[ADR 0017](0017-base-baselines.md)）。

## 後回しにしたもの

この ADR の実装が済んだら、次のADRを起こす。

- **可変フォントのメトリクス**（3段階目）：MVAR、VVAR、HVARによる値の変化、gvarで動く幻の点を使う縦の原点、可変フォントの名前つきインスタンスのPostScript名（`sfnt_get_var_ps_name`）。
- **合成の太字と斜体**：HarfBuzzの `hb_font_set_synthetic_bold` と `hb_font_set_synthetic_slant`（送り幅、箱、輪郭、原点への影響）と、FreeTypeの `FT_GlyphSlot_Embolden` と `FT_GlyphSlot_Oblique`（`src/base/ftsynth.c`）。
- **フォントの関数の差しかえ**：HarfBuzzの `hb_font_funcs_t` と親子のフォント（`hb_font_create_sub_font`）に当たる、呼び出し側がメトリクスを差しかえる仕組み。sofelでは不変の値と関数の引数で表す方針を決める。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。フォントは、[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方で作る。

- **表の読み取りと検証器**：OS/2の各版と短すぎる長さ、nameの形式0と1、領域の外の記録、VORGの版1と2と並びの乱れ、vhea/vmtxとVHEA/VMTXで、値と報告される誤りを確かめる。ファジングの対象（`fuzz/`）にOS/2、name、VORGを足す。
- **方針の単体テスト**：
  - `h_extents`：`USE_TYPO_METRICS` の有無、OS/2が無い、hheaが無い（代わりの値）、負のアセンダと正のディセンダ（符号の補正）。
  - `v_advance`：vmtxの有無（無ければ横の値から）。
  - `v_origin`：VORGあり（記録あり、記録なしで既定の値）、VORGなしでvmtxとglyfあり（単純と複合のグリフ）、vmtxなし（箱から）、空のグリフ。
  - `glyf_extents`：`xMin` とサイドベアリングが違うグリフで、`x_bearing` がサイドベアリングになること。空のグリフ。
  - `v_origin` が、渡した箱の関数を3つ目の手段でだけ呼ぶこと。
  - `postscript_name`：Windowsの言語0x409とそれ以外、Appleだけ、使えない文字を含む名前（捨てられる）、全部が使えない文字（Appleに移る、または無し）、名前ID 6が無い。
  - `fs_type`：OS/2なし、版0xFFFF。
- **HarfBuzzとの突き合わせ**：`tools/hb-dump/`（[ADR 0008](0008-validate-command-and-freetype-comparison.md) の `ft-dump` に倣う小さな道具。HarfBuzz 15.1.0の `src/harfbuzz.cc` を取りこむC++の1つの翻訳単位でビルドし、公開のAPIに無い内部の関数も呼べるようにする。FreeTypeとの統合はしない）を足し、HarfBuzz 15.1.0のテストのフォントすべてについて、フォント全体の横と縦の値、全グリフの縦の送り幅、縦の原点、グリフの箱を書き出す。`sofel-conformance` で完全一致を確かめる（[ADR 0005](0005-conformance-tests.md)）。まだ扱えない輪郭（CFFなど）のフォントの箱は、期待する失敗の一覧に理由を添えて載せる。
- **HarfBuzzとの突き合わせ（続き）**：`hb-dump` で、`hb_ot_metrics_tag_t` のすべての値（代わりの値つきと無し）、`hb_ot_name_list_names` と全記録の `hb_ot_name_get_utf8/16/32`（言語は記録に現れるものと `HB_LANGUAGE_INVALID`）、全グリフの `hb_font_get_glyph_name` と、その名前からの `hb_font_get_glyph_from_name` を書き出し、完全一致を確かめる。
- **拡大縮小の突き合わせ**：`hb-dump` に拡大の指定（`x_scale`、`y_scale`。`upem`、`upem` の2倍と半分、12×64、`−upem` などの負の値）を足し、上の値すべてと送り幅と箱の拡大縮小した値が、`HbScale` を通したsofelの値と一致することを確かめる。
- **FreeTypeとの突き合わせ**：`ft-dump` に、`fsType`、PostScript名、`FT_Face` の値（アセンダ、ディセンダ、高さ、最大の送り幅、下線、`bbox`）、全グリフの `FT_Get_Advance`（縦、拡大縮小なし）、`FT_Get_Glyph_Name`、`FT_Get_Sfnt_Name` の全記録を書き出す形を足し、全フォントで一致を確かめる。
- **C ABI**：[ADR 0015](0015-c-abi.md) のCのテストに、足した関数を加える。
- **計測**：[ADR 0013](0013-benchmark-harness.md) の項目に、全グリフの縦の送り幅と縦の原点（sofelとHarfBuzz）を足す。

## トレードオフ

- 縦の原点の幻の点を `sofel-font` で直接求めるため、可変フォントの段階で求め方を `sofel-outline` に移す手間が残る。いま `sofel-outline` に依存させると、[ADR 0003](0003-crate-layers.md) の依存の向き（`sofel-outline` が `sofel-font` に依存する）に逆らう。
- メトリクスをHarfBuzzに揃えるため、FreeTypeから値を取っているGolemの出力は変わりうる。例えば、vmtxの無いフォントの縦の送り幅は、FreeTypeでは `face->height` 系の値、HarfBuzzではアセンダ − ディセンダになる。Golemは置きかえの前後で出力を比べる必要がある。
- PostScript名だけはFreeTypeに従う。HarfBuzzに同じ問い合わせが無いためで、PDFに書く名前としてFreeTypeの手順が広く使われている。
- PostScript名を呼び出し側のバッファに書くため、長さの上限（127バイト、FreeTypeの `MAX_PS_NAME_LEN`）を超える名前は切りつめる。FreeTypeの既定のインスタンスの名前は上限を設けないため、128バイト以上の名前で食い違う。突き合わせで見つかれば、確保する版を足す。

## 参考文献

- HarfBuzz `src/hb-ot-name.cc`、`src/hb-ot-post-table.hh`、`src/hb-ot-metrics.h`、`src/hb-font.hh`（`em_mult`、`em_multf`、`scale_glyph_extents`）（15.1.0）
- FreeType `src/sfnt/sfobjs.c`（`sfnt_load_face`）、`src/truetype/ttgload.c`（`compute_glyph_metrics`）、`src/sfnt/ttpost.c`、`src/base/ftsnames.c`（`FT_Get_Sfnt_Name`）（2.15.0）
- HarfBuzz `src/hb-ot-font.cc`（`hb_ot_get_font_h_extents`、`hb_ot_get_glyph_v_advances`、`hb_ot_get_glyph_v_origins`、`hb_ot_get_glyph_extents`）、`src/hb-ot-metrics.cc`、`src/hb-font.hh`（`get_h_extents_with_fallback`）、`src/hb-ot-vorg-table.hh`、`src/hb-ot-os2-table.hh`、`src/hb-ot-name-table.hh`、`src/OT/glyf/glyf.hh`（`get_v_origin_with_var_unscaled`）、`src/OT/glyf/GlyphHeader.hh`（15.1.0）
- FreeType `src/sfnt/sfdriver.c`（`sfnt_get_ps_name`、`sfnt_get_name_id`、`get_win_string`、`sfnt_ps_map`）、`src/cff/cffdrivr.c`（`cff_get_ps_name`）、`src/base/ftfstype.c`、`src/sfnt/ttload.c`（OS/2、name）（2.15.0）
- OpenType仕様 OS/2 https://learn.microsoft.com/typography/opentype/spec/os2 、name https://learn.microsoft.com/typography/opentype/spec/name 、vhea https://learn.microsoft.com/typography/opentype/spec/vhea 、vmtx https://learn.microsoft.com/typography/opentype/spec/vmtx 、VORG https://learn.microsoft.com/typography/opentype/spec/vorg
- Golem `modules/golem-font/cbits/golem/golem_font.c`、`docs/golem-font/adr/0001-shaping-and-metrics.md`
