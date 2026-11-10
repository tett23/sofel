# ADR 0027: 可変フォント（座標の模型、変化量の読み取りと適用）

ステータス: 提案

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の3段階目は、可変フォント（fvar、avar、gvar、HVAR）である。
sofelはHarfBuzzとFreeTypeを置きかえる汎用のライブラリなので、この段階の目標は、両者が可変フォントについて計算する値のすべてを、既定でない座標でも同じに出すことである。
最初の利用者のGolemは、いまはすべての軸を既定の値に固定してサブセット化するだけで（[ADR 0022](0022-subsetting.md)）、CFF2を拒む。Golemの必要は、この段階の作業の順序を決めるが、範囲は決めない。

### 先のADRが3段階目に回したもの

先のADRは、次のものを「可変フォントの段階」に回した。この ADR は、各項目を扱うか、扱わずに行き先のADRを示す（「後回しにしたもの」）。

| 回したADR | 項目 | この ADR での扱い |
|---|---|---|
| [ADR 0010](0010-font-foundation.md) | 正規化した座標を `Font` に足すか、別の値とするか | 別の値 `Location` と、組の `Instance` とする（下の「座標の模型」） |
| [ADR 0011](0011-glyf-outlines.md) | gvarによる輪郭の補間 | 扱う |
| [ADR 0016](0016-font-and-vertical-metrics.md) | MVAR、HVAR、VVARによる値の変化、gvarで動く幻の点による縦の原点、名前つきインスタンスのPostScript名（`sfnt_get_var_ps_name`） | 扱う。縦の原点の幻の点を `sofel-outline` に移す案は採らない（下の「`sofel-font` の値」） |
| [ADR 0017](0017-base-baselines.md) | BASEのBaseCoord形式3のVariationIndexの差分と、Deviceの差分を1つのADRで決めること | VariationIndexの差分を扱う。ppemによるヒンティングのDeviceは [ADR 0020](0020-shaping-foundation.md) で実装済みで、BASEへの適用は「後回しにしたもの」 |
| [ADR 0018](0018-phantom-point-shift.md) | gvarで4つの幻の点を動かし、HVARとVVARが無いフォントの送り幅と縦の原点を幻の点から求めること | 扱う |
| [ADR 0020](0020-shaping-foundation.md) | FeatureVariations、GDEFのItemVariationStoreとVariationIndexの補正、HVARとVVARによる送り幅、付着点の変化、計画の鍵への座標、`--variations`、`--named-instance`、`--font-ptem` による `opsz` | 扱う。ただし `--font-ptem` による `opsz` は存在しない（下の段落） |
| [ADR 0021](0021-cff-outlines.md) | CFF2の座標つきの `blend`（輪郭はFreeType、箱はHarfBuzz）、箱の数の型、Private DICTの読みなおし、HVARとVVAR、`FT_Set_Named_Instance`、`ft-dump` の座標の指定、`ItemVariationStore` のビューの共有 | 扱う。Private DICTの座標つきの値はヒンティングでだけ使うので、読み方だけを決める |
| [ADR 0022](0022-subsetting.md) | gvar、HVAR、VVAR、MVAR、cvar、ItemVariationStore、DeltaSetIndexMapの読み取り（段階10の前提）、インスタンス化のADR | cvarを除く読み取りを扱う。段階10はこの ADR の読み取りの部分で始められる。インスタンス化は [ADR 0022](0022-subsetting.md) の「後回しにしたもの」のADRに残す |
| [ADR 0023](0023-color-glyphs.md) | COLRv1の可変の塗り（`VarIdxBase`）とクリップの箱、HarfBuzzの `f32` とFreeTypeの整数の差分の比べ方 | 扱う |

[ADR 0020](0020-shaping-foundation.md) は、`--font-ptem` による `opsz` を `variations` の分類に入れた。
しかし、HarfBuzz 15.1.0の `src/` には、`ptem` から `opsz` の座標を決めるコードが無い（`opsz` を検索して、CFFの直列化の変数名のほかに見つからないことを確かめた）。`hb-shape` の `--font-ptem` は `hb_font_set_ptem` を呼ぶだけで、その値を使うのはAATの `trak` である。
`conformance/expected-failures.txt` の `variations` の263行のうち、`in-house/aat-trak.tests` の7行は `--font-ptem` だけを使う `trak` の行であり、可変フォントの行ではない。

### いま main にあるもの

- `sofel-read` の `fvar`、`avar`（版1と2の区分の写像。版2の末尾の2つのオフセットは位置を確かめるだけで読まない）、`STAT` のビューと検証器（[ADR 0022](0022-subsetting.md) の段階6）。
- `sofel-read` の `item_variation`：ItemVariationStoreの領域の一覧と、各ItemVariationDataの見出しと領域の番号（[ADR 0021](0021-cff-outlines.md)）。差分の行は読まない。DeltaSetIndexMapのビューは無い。
- `sofel-read` の `device`：Deviceの見出しと、VariationIndex（`deltaFormat` 0x8000）の外と内の番号。差分は0を返す。
- GDEFとBASEのItemVariationStoreへのオフセット、COLRのDeltaSetIndexMapとItemVariationStoreへのオフセット、GSUBとGPOSのFeatureVariationsへのオフセット。どれも先のビューは無い。
- `sofel-outline` のCFF2の `blend`：FreeTypeの領域の係数 `BV` を、空の座標か0の座標で求める（[ADR 0021](0021-cff-outlines.md)）。
- `sofel-subset` の `instance`：HarfBuzzの `normalize_axes_location`（インスタンス化の入力の範囲の正規化。`f32` の `normalize_axis_value`、avar版1の `map_float`）と、既定の値への固定でのFeatureVariationsの畳み込み（条件の形式1だけ、`sofel-subset` の中でバイト列を直接読む）。

### HarfBuzzとFreeTypeの違い

座標の持ち方と計算の型が、2つのライブラリで違う。sofelは、シェーピングとメトリクスをHarfBuzzに、輪郭とラスタライズをFreeTypeに合わせるので（[ADR 0005](0005-conformance-tests.md)、[ADR 0010](0010-font-foundation.md)）、両方を持つ必要がある。

- **正規化**：HarfBuzz（`src/hb-ot-var.cc` の `hb_ot_var_normalize_coords`）は、設計の座標を `float` のまま軸の範囲に切り詰め、`(v − def) / (def − min)` などを `float` で求め、`roundf(x × 65536.f)` で16.16の `int` にする。avar版1は `map_coords_16_16` で、`map_float(c / 65536.f) × 65536.f` を `roundf` する。avar版2は、16.16を `roundf(c / 4.f)` で2.14にした座標でItemVariationStoreを評価し、`float` の差分の4倍を `roundf` して足し、±1に切り詰める。最後に `(c + 2) >> 2` で2.14にする。`roundf` は [ADR 0026](0026-harfbuzz-rounding.md) の `_hb_roundf` である。
- FreeType（`src/truetype/ttgxvar.c` の `ft_var_to_normalized`）は、設計の座標を16.16の `FT_Fixed` で受け取り、`FT_DivFix` で正規化し、2.14に丸めずに16.16のまま持つ。avar版1は、区分の各組を16.16に広げ、`normalized < from[j]` となる最初の `j` で `FT_MulDiv` による比例をとる（最後の `from` 以上の値は写さない）。HarfBuzzの `map_float` の端と同点の扱いとは違う。avarの軸の数がfvarと違えば、FreeTypeはavarを使わない（`ft_var_load_avar`）。HarfBuzzは少ないほうの数だけ写す。avar版2は、写した16.16の座標でItemVariationStoreを評価し、整数の差分の4倍を足して±1に切り詰める。
- **領域の係数**：HarfBuzz（`src/hb-ot-layout-common.hh` の `VarRegionAxis::evaluate`）は2.14の座標から `float` で求め、`start > peak`、`peak > end`、`start < 0 < end` の不正な領域を1とする。FreeType（`tt_calculate_scalar`）は16.16の座標から `FT_MulDiv` で求める。不正な領域は、表を読むときに `peak` を0にしておく（`tt_var_load_item_variation_store`、`src/cff/cffload.c` の `cff_vstore_load`）ので、係数は1になるが、座標が0のときの分岐の順がHarfBuzzと違う（[ADR 0029](0029-variation-deltas.md)）。
- **ItemVariationStoreの差分**：HarfBuzzは `float` で `Σ scalar × delta` を求め、使う側が `roundf` するか `float` のまま拡大縮小する。FreeType（`tt_var_get_item_delta`）は64bitで `Σ delta × scalar` を足し、`ft_round_and_shift16` で整数にする。
- **gvar**：HarfBuzz（`src/hb-ot-var-gvar-table.hh`、`src/OT/glyf/Glyph.hh`）は、点を `float` で持ち、組の係数（`TupleVariationHeader::calculate_scalar`）も `float` で求め、推定した点の差分（IUP）も `float` で計算する。FreeType（`TT_Vary_Apply_Glyph_Deltas`）は、差分を16.16で足し、IUPを `tt_interpolate_deltas` の16.16で行い、最後に `FT_fixedToInt` で整数の点に、`FT_fixedToFdot6` で丸めない26.6の点（`unrounded`）にする。
- **既定でないインスタンスでの輪郭の拡大縮小**：FreeTypeは、名前つきインスタンスを選んだフェイスか、0でない正規化した座標を持つフェイス（`FT_FACE_FLAG_VARIATION`）では、単純なグリフの点を `unrounded` から `(FT_MulFix(u, scale) + 32) >> 6` で拡大縮小する（`src/truetype/ttgload.c` の `TT_Process_Simple_Glyph`、`IS_DEFAULT_INSTANCE`）。既定のインスタンスの `FT_MulFix(x, scale)` とは、差分が0でも結果が違いうる。同じ条件で、埋め込みビットマップも使わない（`TT_Load_Glyph`）。
- **送り幅**：HarfBuzz（`src/hb-ot-font.cc`、`src/hb-ot-hmtx-table.hh`）は、HVARがあれば `advance + roundf(差分)` を `hb_clamp_to<unsigned>` し、無くてgvarがあればglyfの幻の点を `float` で求める。CFF2でHVARが無ければ、変化させない。FreeTypeは、HVARがあれば整数の差分を足し（`tt_hadvance_adjust`）、gvarの幻の点を動かさない。無ければ、幻の点の `unrounded` の差を `FT_PIX_ROUND` する。
- **座標の持ち主**：HarfBuzzは座標を `hb_font_t` に持ち（`hb_face_t` には持たない）、FreeTypeは可変の `FT_Face` に持つ。sofelの値は不変で、内部可変性を使わない（[ADR 0003](0003-crate-layers.md)）ので、どちらの形も写せない。

### 範囲の決め方

先のADRと同じく、HarfBuzzとFreeTypeのそれぞれが公開する値ごとに、基準の側の手順と型をそのまま写す。
2つの座標（HarfBuzzの2.14と、FreeTypeの16.16）を1つの値に持ち、問い合わせごとに基準の側の座標を使う。

## 実装すること

### 座標の模型

- `sofel_font::var::Location`：1つのフォントの1つの位置を表す不変の値。`Clone`、`Send`、`Sync`。持つものは次のとおり。
  - `hb_coords: Vec<i32>`：HarfBuzzの正規化した2.14の座標（avar適用後）。fvarの軸の数だけ。
  - `hb_design: Vec<f32>`：HarfBuzzの設計の座標（`hb_font_get_var_coords_design`。正規化した座標から作った位置ではNaN）。
  - `ft_coords: Vec<Fixed>`：FreeTypeの正規化した16.16の座標（avar適用後）。
  - `ft_design: Vec<Fixed>`：FreeTypeの設計の座標（`FT_Get_Var_Design_Coordinates`。正規化した座標から作った位置では `ft_var_to_design` で求める）。
  - `named_instance: Option<u16>`：名前つきインスタンスから作ったときの番号。
  - `hb_nonzero: bool`（HarfBuzzの `has_nonzero_coords`）と、`ft_default: bool`（FreeTypeの `IS_DEFAULT_INSTANCE`。名前つきインスタンスでなく、正規化した座標がすべて0）。
- 空の `Location`（軸が0、名前つきインスタンスなし）は既定のインスタンスを表す。`Location::default_ref()` は、静的に置いた空の値への参照を返す。
- `sofel_font::Instance<'f, 'a>`：`{ font: &'f Font<'a>, location: &'f Location }`。`Copy`、`Send`、`Sync`。座標によって値が変わる問い合わせは、`Instance` の関数とする。`Font::instance(&self, &Location)` と `Font::default_instance(&self)` で作る。
- `Font` は、座標によらない問い合わせ（グリフの数、文字の対応、名前、表のバイト列など）だけを持つ。いま `Font` にある、座標によって変わる問い合わせ（`advance`、`v_advance`、`v_origin`、`glyf_extents`、`h_extents`、`v_extents`、`metrics_position`、`baseline` とその代わりの値つきの版、`layout_font_extents`、`ft_left_side_bearing` など）は、`Instance` に移す。同じ問い合わせを `Font` と `Instance` の両方に置かない。
- `Location` を作った `Font` と違うフォントの `Instance` に使ってもよい。そのときは、HarfBuzzの `num_coords` と同じく、足りない軸を0とし、余る座標を使わない。誤りにはしない。
- `sofel-font` は、`Location` の `Vec` のために `alloc` を使う。[ADR 0010](0010-font-foundation.md) の「確保なし」を改める。`Font::new` と座標によらない問い合わせは、引き続き確保しない。
- `Font::new(data, index)` の `index` は、引き続きフェイスの番号だけとする。HarfBuzzとFreeTypeが番号の上位16bitで名前つきインスタンスを選ぶ形（`hb_font_create`、`FT_Open_Face`）は、`Location::named_instance` で表し、`sofel-cli` と突き合わせの道具が番号を分ける。

### 位置の作り方

| 関数 | HarfBuzz | FreeType |
|---|---|---|
| `Location::new(font, named_instance: Option<u16>, variations: &[Variation])` | `hb_font_set_var_named_instance` のあとの `hb_font_set_variations`（設計の座標を既定の値か名前つきインスタンスの値で埋め、タグの一致する軸を上書きする。同じタグが複数あれば後が勝つ） | `FT_Set_Named_Instance` のあとの `FT_Set_Var_Design_Coordinates`。設計の座標の16.16は、hb-ftと同じく `(FT_Fixed)(v × 65536.f)` とする |
| `Location::from_design(font, &[f32])` | `hb_font_set_var_coords_design` | `FT_Set_Var_Design_Coordinates`（同じ変換） |
| `Location::from_normalized_2_14(font, &[i32])` | `hb_font_set_var_coords_normalized`（設計の座標はNaN） | `FT_Set_Var_Blend_Coordinates` に4倍した値 |
| `Location::from_normalized_16_16(font, &[Fixed])` | `(c + 2) >> 2`（`src/hb-ft.cc` の `hb_ft_font_changed` の変換） | `FT_Set_Var_Blend_Coordinates`（`FT_Set_MM_Blend_Coordinates` と同じ）。±1を超える値は、FreeTypeと同じく誤りとする |
| `Location::named_instance(font, i)` | `hb_font_set_var_named_instance` | `FT_Set_Named_Instance`。FreeTypeが合成した既定の名前つきインスタンス（下）も選べる |

- `Variation { tag, value: f32 }` は `hb_variation_t` に当たる。`Variation::parse`、`Variation::format` は `hb_variation_from_string`、`hb_variation_to_string` と同じ文字列を読み書きする。
- 名前つきインスタンスの番号が範囲の外なら、HarfBuzzは設計の座標を既定の値のままにし、FreeTypeは誤りを返す。`Location::new` は、HarfBuzzの側を既定の値とし、FreeTypeの側の結果を `Location::ft_error()` で返す。`ft-dump` との突き合わせは、両方を比べる。

### 軸と名前つきインスタンスの問い合わせ

- `Font::var_axes()`：`hb_ot_var_get_axis_infos` と同じ値（タグ、名前ID、隠す印、最小、既定、最大。最小と最大は、HarfBuzzと同じく既定の値を含むように並べなおす）。`Font::find_var_axis(tag)` は `hb_ot_var_find_axis_info`。
- `Font::named_instances()`：`hb_ot_var_get_named_instance_count`、`hb_ot_var_named_instance_get_subfamily_name_id`、`_get_postscript_name_id`、`_get_design_coords`。
- `Font::ft_mm_var()`：`FT_Get_MM_Var` と同じ値（軸の16.16の値、`FT_Get_Var_Axis_Flags`、名前つきインスタンスの `strid` と `psid`）。FreeTypeは、既定の位置の名前つきインスタンスが無く、nameにサブファミリー名とPostScript名があれば、末尾に既定のインスタンスを合成する（`TT_Get_MM_Var`）。`FT_Get_Default_Named_Instance` も同じ値から返す。
- `Instance::postscript_name()`：FreeTypeの `FT_Get_Postscript_Name`。既定のインスタンスでは [ADR 0016](0016-font-and-vertical-metrics.md) の手順、名前つきインスタンスでは `psid` の名前か、`sfnt_get_var_ps_name` の手順で作る名前（接頭辞は名前ID 25かファミリー名、軸ごとの `_<値><タグ>`、127字を超えればMurmurHash3の128bitの値で縮める）、任意の位置でも `sfnt_get_var_ps_name` の名前とする。MurmurHash3は自分で書く。
- `Instance::coords_normalized()`、`coords_design()`、`ft_coords()`、`ft_design_coords()`：`hb_font_get_var_coords_normalized`、`hb_font_get_var_coords_design`、`FT_Get_Var_Blend_Coordinates`、`FT_Get_Var_Design_Coordinates`。

### `sofel-read`：ビューと検証器

[ADR 0007](0007-sfnt-read-foundation.md) と [ADR 0003](0003-crate-layers.md) に従い、各ビューと同じプルリクエストで検証器を足し、`sofel-validate` の検証に加える。確保はしない。

- `item_variation`：ItemVariationDataの `itemCount`、`wordDeltaCount`（`LONG_WORDS` の0x8000を含む）、差分の行を読む。`delta(outer, inner, region_k)` は生の整数を返す。検証器に、行の長さが表に収まること、`wordDeltaCount` が領域の数を超えないことを足す。
- `delta_set_index_map`：DeltaSetIndexMapの形式0と1（`entryFormat` の幅と内の番号のbit数）。`map(i)` は、範囲の外の番号を最後の項目で引く（HarfBuzzの `DeltaSetIndexMapFormat01::map` とFreeTypeの読み方は同じ）。項目が0なら、番号をそのまま返す。
- `hvar`：HVARとVVAR（送り幅、左右（上下）のサイドベアリング、VVARの縦の原点の写像と、ItemVariationStore）。
- `mvar`：MVARの値の記録（タグの昇順の二分探索）とItemVariationStore。
- `gvar`：`gvar` と、24bitのグリフの数を持つ `GVAR`（HarfBuzzの `gvar_GVAR`、FreeTypeの `ft_var_load_gvar` の `TTAG_GVAR`）。共有の組、グリフごとのデータのオフセット（短い形式と長い形式）、組の見出し（埋め込みのピーク、中間の範囲、私用の点の印、共有の組の番号）、圧縮した点の番号と差分の復号のイテレータ（`ft_var_readpackedpoints`、`ft_var_readpackeddeltas`、HarfBuzzの `decompile_points`、`decompile_deltas`。差分の形式の0、1、2と、「すべての点」）。表の選び方は、glyfと同じく `GLYF` があれば `GVAR` とする。
- `avar`：版2の末尾のDeltaSetIndexMapとItemVariationStoreを読む。
- `feature_variations`：GSUBとGPOSのFeatureVariations、ConditionSet、Conditionの形式1〜5（軸の範囲、値、AND、OR、否定。HarfBuzzの `Condition`）、FeatureTableSubstitution。Feature表は、いまのGSUBとGPOSのビューのものを使う。
- `colr`：BaseGlyphList、ClipList、塗りの `VarIdxBase` の値の位置は [ADR 0023](0023-color-glyphs.md) のビューのままとし、DeltaSetIndexMapとItemVariationStoreを上のビューで開く関数を足す。
- `gdef`、`base`：ItemVariationStoreを上のビューで開く関数を足す。
- `device`：VariationIndexの差分を、0ではなく、渡された評価の関数（下の「変化量の評価」）で求める関数を足す。ppemのDeviceの扱いは変えない。
- 検証器が報告するもの：範囲の外のオフセット、数の矛盾（gvarのグリフの数と `maxp`、組の軸の数とfvar、HVARの写像の幅と項目、avarの軸の数とfvar）、オフセットの昇順でないgvarのデータ、点の番号と差分の数の不一致、ConditionSetの軸の番号がfvarの軸の数以上、未知の条件の形式（Warning。HarfBuzzは偽とする）。誤りの種類を `Kind` に足し、`--json` にも出す（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。
- `sofel-subset` のFeatureVariationsの直接の読み取りは、このビューに置きかえる。結果のバイト列は変えない。

### 正規化と変化量の評価（`sofel-font`）

- 正規化は、上の「HarfBuzzとFreeTypeの違い」の2つの手順を、それぞれの型のまま写す。HarfBuzzの `float` の演算は [ADR 0026](0026-harfbuzz-rounding.md) の決まり（同じ精度の四則演算、`_hb_roundf`、FMAなし）に従う。FreeTypeの `FT_DivFix`、`FT_MulDiv`、`FT_MulFix` は `sofel-geom` のもの（[ADR 0009](0009-geom-fixed-point-and-pen.md)）を使う。
- `sofel-subset` の `normalize_axis_value` と `map_float` は `sofel-font` に移し、`sofel-subset` はそれを使う。インスタンス化の範囲の正規化（`normalize_axes_location`）は `sofel-subset` に残す。
- 変化量の評価は、[ADR 0029](0029-variation-deltas.md) の1つの核 `item_delta::<A>` を、算術の方針 `HbF32`（HarfBuzzの `f32` の係数と差分）、`HbF64`（HarfBuzzのCFF2）、`Ft`（FreeTypeの16.16の係数、64bitの和と `ft_round_and_shift16`）で呼ぶ。値を使う箇所が、基準の側の方針を選ぶ。
- 領域の係数（HarfBuzzの `hb_scalar_cache_t`、FreeTypeの `regionScalars`）は、`Location` を作るときにストアごとに求めて `Location` が持つ（[ADR 0029](0029-variation-deltas.md)）。

### `sofel-font` の値

HarfBuzzの方針（[ADR 0010](0010-font-foundation.md)、[ADR 0016](0016-font-and-vertical-metrics.md)）の値は、HarfBuzzの座標と `item_delta::<HbF32>` の評価で求める。

| 問い合わせ | HarfBuzzの手順 |
|---|---|
| `Instance::advance(glyph, scratch)`、`v_advance` | HVAR（VVAR）があれば `hb_clamp_to<unsigned>(advance + roundf(差分))`。無くてgvarがあれば、glyfの幻の点（下の `hb_glyf`）。どちらも無ければ変化させない。`hb_nonzero` が偽なら、いまの値 |
| `Instance::v_origin(glyph, scratch)` | VORGがあれば `VORG + VVARの縦の原点の差分`（`em_scalef_y`）。vmtxとglyfがあれば、上の幻の点（`get_v_origin_with_var_unscaled`）。ほかは [ADR 0016](0016-font-and-vertical-metrics.md) の箱からの値 |
| `Instance::glyf_extents(glyph, scratch)` | `hb_nonzero` なら、`glyf_accelerator_t::get_extents_at` の手順（点を `float` で求めて箱をとる）。そうでなければ、いまの見出しの箱 |
| `metrics_position`、`h_extents`、`v_extents` | `hb_ot_metrics_get_position` の `GET_VAR`（MVARの `float` の差分を足して `em_scalef`）。`hb_ot_metrics_get_variation`、`get_x_variation`、`get_y_variation` も足す |
| `baseline` と代わりの値つきの版、`layout_font_extents` | BaseCoord形式3のVariationIndexの差分（`em_scalef_y`）。代わりの値が使う箱と送り幅は上の値 |

- `hb_glyf`（`sofel-font` の内部）：HarfBuzzの `Glyph::get_points` を写す。単純なグリフと合成グリフの点と4つの幻の点を `f32` で作り、gvarの差分の適用とIUPは [ADR 0029](0029-variation-deltas.md) の核 `gvar::apply::<HbF32>` で行う。合成グリフでは、成分の位置の差分と、`USE_MY_METRICS` の幻の点を、HarfBuzzと同じ順で扱う。
- `GlyphScratch`：`hb_glyf` の点と、[ADR 0029](0029-variation-deltas.md) の `GvarScratch<HbF32>` を入れる作業領域。呼び出し側が持ち、グリフをまたいで再利用する（[ADR 0003](0003-crate-layers.md)）。点が要らない問い合わせ（HVARのある送り幅など）でも同じ引数をとり、使わない。
- [ADR 0016](0016-font-and-vertical-metrics.md) は、gvarで動く縦の原点の幻の点を `sofel-outline` に移すとした。この ADR はそれを採らず、`sofel-font` に置く。`sofel-shape` は `sofel-font` にだけ依存し（[ADR 0003](0003-crate-layers.md)）、送り幅と縦の原点をシェーピングの中で使うためである。HarfBuzzの `float` の点とFreeTypeの16.16の点は型が違うが、差分の適用とIUPの手順は [ADR 0029](0029-variation-deltas.md) の核を共有し、重複は点を作る部分と幻の点の扱いに限られる。
- FreeTypeの方針の値は、FreeTypeの座標と `item_delta::<Ft>` の評価で求める。
  - `Instance::ft_left_side_bearing`、`ft_top_side_bearing`、FreeTypeの送り幅：`tt_face_get_metrics` のHVARとVVARの補正（`hadvance_adjust`、`lsb_adjust`、`vadvance_adjust`、`tsb_adjust`）。
  - `Instance::ft_face_metrics`：`FtFaceMetrics` に、MVARの補正（`tt_apply_mvar`。アセンダ、ディセンダ、行の高さ、下線の位置と太さ）を足す。

### `sofel-outline`

- `Outlines::draw`、`load`、`glyph_extents` は、`&Font` の代わりに `Instance` をとる。
- **glyfとgvar**：`TT_Vary_Apply_Glyph_Deltas` の結果を、[ADR 0029](0029-variation-deltas.md) の核 `gvar::apply::<Ft>` で求める。組ごとの16.16の係数（`ft_var_apply_tuple`）、IUP（`tt_interpolate_deltas`、`tt_delta_shift`、`tt_delta_interpolate`）、点ごとの16.16の差分の和は、核の `Ft` の方針が行う。最後に、`FT_fixedToInt` の整数の点と、`FT_fixedToFdot6` の `unrounded` を `sofel-outline` が作る。
  - 幻の点：4つの幻の点を点の列の末尾に置いて同じ差分を足す。HVARがあれば `pp1`、`pp2` の差分を、VVARがあれば `pp3`、`pp4` の差分を0にする。[ADR 0018](0018-phantom-point-shift.md) の `pp1.x` のずらしは、差分を足した `pp1` で行う。HVARがあるときの `pp1` は、HVARで補正した左のサイドベアリングから作る。
  - 合成グリフ：成分を点として並べた輪郭にgvarの差分を足し、`ARGS_ARE_XY_VALUES` の成分の位置を置きかえる（`load_truetype_glyph`）。
  - 拡大縮小：`ft_default` が偽なら、単純なグリフの点を `unrounded` から `(mul_fix(u, scale) + 32) >> 6` で求める。真なら、いまの手順のまま。
  - 誤り：FreeTypeが誤りとするgvar（壊れたオフセット、足りない差分）は、FreeTypeと同じ誤りの扱いとし、`OutlineError` に種類を足す。
- **CFF2**：`BV` は、`Instance::blend_scalars::<Ft>`（[ADR 0029](0029-variation-deltas.md)。FreeTypeの16.16の座標から `cff_blend_build_vector` と同じ規則で求め、`Location` が持つ係数）から作る。fvarの軸の数と領域の軸の数が違うときなどの誤りは、[ADR 0021](0021-cff-outlines.md) のとおり。
  - Private DICTの `blend`：座標つきの値（BlueValuesなど）はヒンティングでだけ使う。この ADR では、FreeTypeと同じくPrivate DICTの `blend` を座標で評価して読み、値を [ADR 0021](0021-cff-outlines.md) の構造体に入れるが、輪郭には使わない。`cff_blend_check_vector` による読みなおしは、`Instance` ごとに読むことで同じ結果になる。
- **HarfBuzzのCFF2の箱**（`glyph_extents`）：HarfBuzzの `number_t` は `double` で、`blend` は `float` の係数と `double` の差分の積を足す（`src/hb-cff2-interp-cs.hh` の `blend_deltas`）。係数は `Instance::blend_scalars::<HbF64>`（[ADR 0029](0029-variation-deltas.md)）から取る。[ADR 0021](0021-cff-outlines.md) の `i64` の16.16の解釈器は、座標つきの `blend` を表せない。解釈器の数の型を、HarfBuzzと同じ `f64` に揃える。既定のインスタンスでは `blend` が乗算を生まず、`f64` と `i64` の結果は等しい（[ADR 0021](0021-cff-outlines.md) の理由）。下の計測で、既定のインスタンスの箱が5%以上遅くなれば、既定のインスタンスに限って `i64` の経路を残す。
- 埋め込みビットマップ：`sofel-render` は、`ft_default` が偽ならビットマップを使わない（`TT_Load_Glyph` の `IS_DEFAULT_INSTANCE`）。

### `sofel-shape`

- `ShapePlan::new` と `shape` は、`&Font` の代わりに `Instance` をとる。
- **FeatureVariations**：計画を作るときに、GSUBとGPOSのそれぞれで `find_variations_index`（HarfBuzzの2.14の座標で、記録を順に評価し、最初に成り立つもの）を求め、計画に持つ。条件の形式2（値）は、GDEFのItemVariationStoreで評価する（HarfBuzzの `ConditionValue` と同じ）。フィーチャの置きかえ（`find_substitute`）は、フィーチャの索引から表を引くときに使う。`rvrn` は、HarfBuzzと同じく計画の最初の段の機能として足す（[ADR 0020](0020-shaping-foundation.md) の計画の手順のまま）。
- **計画の鍵**：`ShapePlan::key()` に、2つの `variations_index` を足す（`hb_ot_shape_plan_key_t`）。呼び出し側は、鍵が同じなら位置の違う `Instance` でも計画を使いまわしてよい。`ShapePlan::variations_index_for(instance)` で、計画を作らずに鍵を求められるようにする。
- **VariationIndexの補正**：ValueRecordの4つのDeviceと、アンカーの形式3のDeviceのVariationIndexを、GDEFのItemVariationStoreで評価し、`em_scalef_x`、`em_scalef_y` で拡大縮小する（`VariationDevice::get_x_delta`）。`hb_nonzero` が偽なら0とする。
- 送り幅、縦の原点、箱は、上の `Instance` の問い合わせを使う。`shape_with_extents` の箱の関数は、座標つきの箱を渡せるようにそのまま残す。シェーピングの作業領域に `GlyphScratch` を持つ。

### `sofel-color`

- COLRv1の塗りとクリップの箱は `Instance` をとる。
- HarfBuzzの描画（`paint`、`glyph_extents`）：`VarIdxBase + i` をDeltaSetIndexMapで写し、`item_delta::<HbF32>` の `float` の差分を、HarfBuzzの `VarStoreInstancer` と同じ箇所で足す（`src/OT/Color/COLR/COLR.hh` の各塗りの `paint_glyph`、`ClipBox`）。`VarIdxBase` が0xFFFFFFFFなら差分は0。
- FreeTypeの生の値（`raw` のモジュール。`FT_Get_Paint`、`FT_Get_Color_Glyph_ClipBox` に当たる）：`get_deltas_for_var_index_base` と同じく、`item_delta::<Ft>` の整数の差分を足す。
- [ADR 0023](0023-color-glyphs.md) が残した比べ方の問いは、このように答える：塗りの命令の列はHarfBuzzと、生の値はFreeTypeと、それぞれの差分の型のまま完全一致で比べる。

### `sofel-capi`

[ADR 0015](0015-c-abi.md) の決まりに従い、Rustの関数と同じプルリクエストで足す。

- `sofel_location_t`（不透明な型、不変）と、`sofel_variation_t { uint32_t tag; float value; }`。
- `sofel_location_create(font, named_instance, variations, n, out)`（`named_instance` は `UINT32_MAX` で「なし」）、`sofel_location_create_design`、`_create_normalized_2_14`、`_create_normalized_16_16`、`_create_named_instance`、`sofel_location_destroy`、`sofel_location_coords_normalized`、`_coords_design`、`_ft_coords`、`_ft_design_coords`、`sofel_location_ft_error`。
- `sofel_variation_parse`、`sofel_variation_format`。
- 軸と名前つきインスタンス：`sofel_font_var_axis_count`、`sofel_font_var_axis`、`sofel_font_find_var_axis`、`sofel_font_named_instance_count`、`sofel_font_named_instance`、`sofel_font_ft_mm_var` に当たる関数群。
- 座標で値の変わる既存の関数（送り幅、縦の原点、箱、メトリクス、基準線、輪郭、シェーピングの計画と実行、塗り）は、`const sofel_location_t *location` の引数を `font` の直後にとる。NULLは既定のインスタンスを表し、[ADR 0015](0015-c-abi.md) の「NULLは誤り」の例外としてヘッダの冒頭に書く。1.0.0までABIの互換は約束しないので、関数の名前は変えない。
- `sofel_postscript_name(font, location, buf, cap, len)` は、既存の関数に `location` を足す。

### `sofel-cli`

- `sofel-shape`、`sofel-render`、`sofel-metrics` など座標で結果の変わる道具に、`--variations=<tag=value,...>`（`hb-shape` と同じ書式）と `--named-instance=<i>` を足す。解析は `sofel_cli` の純粋な関数とし、適合テストと共有する。
- `sofel-validate` は、上の検証器を呼ぶ。

### [ADR 0022](0022-subsetting.md) の段階10

段階10（軸を指定しないときの可変フォントの表のサブセット化）は、変化量の評価を要さず、表の構造の読み取りだけを要する。
HarfBuzzは、軸を指定しないとき、gvarのグリフごとのデータを写し、HVAR、VVAR、GDEF、BASE、COLRのItemVariationStoreとDeltaSetIndexMapを、使う番号だけに詰めて書きなおし（`ItemVariationStore::subset`）、CFF2の `blend` と `vsindex` を保ってVariationStoreを書きなおす。どれも、差分の値を読んで写すだけである。

そのため、段階10は、この ADR の作業の1（`sofel-read` のビュー）の後に始められる。作業の2以降（座標と適用）を待たない。
段階10が使うものは、ItemVariationStoreの行、DeltaSetIndexMap、gvarのグリフごとのデータの範囲と共有の組、HVARとVVARのビュー、COLR、GDEF、BASEのItemVariationStoreを開く関数である。MVAR、avar、fvarはそのまま写すので、検証器だけを使う。
`conformance/expected-failures-subset.txt` の `stage:var` の312行と、`expected-failures-subset-hb.txt` の95行は、段階10の実装のプルリクエストで消す。この ADR の実装では消さない。

任意の位置へのインスタンス化と軸の範囲の絞り込みは、[ADR 0022](0022-subsetting.md) の「後回しにしたもの」のADRに残す。そのADRは、この ADR の作業の4までが済んだ後に作る。正規化の関数（`sofel-font`）と、`item_delta::<HbF32>` の評価を使う。

### 作業の分け方

設計は1つとし、実装は次の順にプルリクエストを分ける。各プルリクエストに、その範囲の `sofel-capi` の関数と、テスト設計のテストを含める。

1. `sofel-read`：ItemVariationStoreの行、DeltaSetIndexMap、HVAR、VVAR、MVAR、gvarとGVAR、avar版2の末尾、FeatureVariationsのビューと検証器、`sofel-validate`、ファジングの対象。`sofel-subset` のFeatureVariationsの読み取りの置きかえ。ここで [ADR 0022](0022-subsetting.md) の段階10が始められる。
2. `sofel-font`：`Location`、`Instance`、`Variation`、正規化（2つの手順。avar版1と2）、名前つきインスタンス、軸の問い合わせ、`ft_mm_var`、PostScript名（MurmurHash3）、[ADR 0029](0029-variation-deltas.md) の評価の核と算術の方針と `StoreScalars`、`Font` の `font_key`、座標で変わる問い合わせの `Instance` への移動（値はまだ既定のインスタンスのまま）。`hb-dump --var` と `ft-dump --var`。
3. `sofel-font` の値：HVAR、VVAR、MVAR、VORGとVVAR、`hb_glyf` とgvarの幻の点、BASEのVariationIndex、FreeTypeのサイドベアリングと `FtFaceMetrics`。`hb-dump` と `ft-dump` の `--metrics`、`--extents`、`--baselines` の座標の指定。
4. `sofel-outline`：glyfとgvar（[ADR 0029](0029-variation-deltas.md) の核の `Ft` の方針と `gvar::apply`、幻の点、合成、`unrounded` の拡大縮小）、CFF2の座標つきの `blend` とPrivate DICT、HarfBuzzのCFF2の箱の `f64` 化。`ft-dump --outline`、`--raster` と `hb-dump --extents` の座標の指定。
5. `sofel-shape`：FeatureVariations、VariationIndexの補正、計画の鍵、`--variations` と `--named-instance`。`variations` の期待する失敗を消す。
6. `sofel-color`：COLRv1の可変の塗りとクリップの箱。
7. 計測の項目と記録、`docs/specifications.md` と各ADRのステータスの更新の残り。

### ADRのステータス

実装のプルリクエストで、次のステータスの行を改める（各ADRの決定を変える最初のプルリクエストで行う）。

- [ADR 0010](0010-font-foundation.md)：「→可変フォントの座標と `sofel-font` の `alloc` は [ADR 0027](0027-variable-fonts.md)」（作業の2）。
- [ADR 0016](0016-font-and-vertical-metrics.md)：「→座標で変わる値と縦の原点の幻の点の置き場所は [ADR 0027](0027-variable-fonts.md)」（作業の3）。
- [ADR 0017](0017-base-baselines.md)、[ADR 0018](0018-phantom-point-shift.md)：「→可変フォントの差分は [ADR 0027](0027-variable-fonts.md)」（作業の3と4）。
- [ADR 0020](0020-shaping-foundation.md)：「→可変フォントのシェーピングは [ADR 0027](0027-variable-fonts.md)」（作業の5）。
- [ADR 0021](0021-cff-outlines.md)：「→座標つきのCFF2と箱の数の型は [ADR 0027](0027-variable-fonts.md)」（作業の4）。
- [ADR 0023](0023-color-glyphs.md)：「→可変の塗りは [ADR 0027](0027-variable-fonts.md)」（作業の6）。
- [ADR 0015](0015-c-abi.md)：「→`location` のNULLの例外は [ADR 0027](0027-variable-fonts.md)」（作業の2）。
- `docs/specifications.md` と README を、各プルリクエストの範囲で改める。

## データ構造

- `Location`：上の「座標の模型」の6つと、[ADR 0029](0029-variation-deltas.md) のストアごとの係数（`StoreScalars`）、gvarの共有の組の係数、作った `Font` の鍵 `font_key`。`Vec` は軸の数だけ持つ。既定のインスタンスは空の `Vec` とし、評価の関数は空の座標を「すべて0」として扱う。
- `Font`：[ADR 0029](0029-variation-deltas.md) の `font_key: u64` を足す。`Font::new` は引き続き確保しない。
- `Instance`：2つの参照だけを持つ。作るのに費用はかからない。
- `GlyphScratch`（`sofel-font`）：`f32` の点の列、`f32` の差分の列、IUPのための「差分あり」の印の列、合成の途中の状態。
- `sofel-outline` の `Scratch` に、16.16の点の差分の列（`point_deltas_x`、`point_deltas_y`）、IUPの作業の列（`points_org`、`points_out`、`has_delta`）、`unrounded` の26.6の点の列を足す。
- `ShapePlan`：`variations_index: [Option<u32>; 2]` と、それで置きかえたフィーチャの表を、lookupの集まりを作るときに使う。
- ビューは、どれもバイト列の上に重ね、確保しない（[ADR 0007](0007-sfnt-read-foundation.md)）。

## アルゴリズム

- **HarfBuzzの正規化**：軸ごとに `normalize_axis_value`（`f32`）、`roundf(v × 65536.f)` を `i32` に、avar版1を `map_coords_16_16`、版2を上の手順、最後に `(c + 2) >> 2`。式の型は [ADR 0026](0026-harfbuzz-rounding.md) の決まりで写す（`float` と `int` の混ざる式は `float` で計算する）。
- **FreeTypeの正規化**：軸ごとに、最大以上なら `0x10000`、最小以下なら `−0x10000`、ほかは `FT_DivFix(coord − def, max − def)` など。avar版1は区分の線形の補間を `FT_MulDiv` で、版2はItemVariationStoreの整数の差分の4倍を足して切り詰める。
- **領域の係数と差分の和**：[ADR 0029](0029-variation-deltas.md) のとおり。軸ごとの係数（HarfBuzzの `VarRegionAxis::evaluate`、FreeTypeの `tt_calculate_scalar`）とgvarの組の係数（`TupleVariationHeader::calculate_scalar`、`ft_var_apply_tuple`）は算術の方針が持ち、積と和の順は核が1つで行う。
- **IUP**：[ADR 0029](0029-variation-deltas.md) のとおり。区間の探し方と、差分のある点が輪郭に1つのときの平行移動（`tt_delta_shift`）は核が行い、区間の中の値はHarfBuzzの `infer_delta`（`float`）とFreeTypeの `tt_delta_interpolate`（16.16、`FT_DivFix` と `FT_MulFix`）を方針が持つ。
- **FeatureVariations**：記録を順に見て、ConditionSetのすべての条件が成り立つ最初の記録を選ぶ。形式1は `filterRangeMinValue ≤ 座標 ≤ filterRangeMaxValue`（2.14の整数の比較）。形式2〜5はHarfBuzzの再帰の評価（`_hb_recurse_condition_evaluate`、再帰の上限を含む）。
- **浮動小数点**：HarfBuzzの値の計算は、HarfBuzzと同じ型と演算の順で書き、FMAを使わない（[ADR 0026](0026-harfbuzz-rounding.md)）。IEEE 754の四則演算は、x86-64、aarch64、wasm32で同じ結果になる。

## 実装しないこと

- 「後回しにしたもの」に挙げたもの。
- Type1のMultiple Masters（`FT_Set_MM_Design_Coordinates`、`FT_Get_Multi_Master`、`FT_Get_MM_WeightVector`）。sfnt以外の形式とともに、8段階目のADRで扱う。
- HarfBuzzとFreeTypeの、座標を書きかえる可変のオブジェクトの形（`hb_font_set_variations` で既存の `hb_font_t` を変える形）。sofelは位置ごとに新しい `Location` を作る。
- 壊れた可変フォントの表についての、HarfBuzzのsanitizeとの完全一致。sofelはビューを作れない表を、HarfBuzzの方針の値では「表なし」、FreeTypeの方針の値ではFreeTypeの誤りの扱いとし、panicしない。突き合わせで違いが見つかれば、期待する失敗の一覧に理由を書いて残す。

## 後回しにしたもの

この ADR の実装の後に、それぞれのADRを書く。

- **可変フォントのインスタンス化**：[ADR 0022](0022-subsetting.md) の「後回しにしたもの」のとおり。この ADR の正規化と `item_delta::<HbF32>` の評価を使う。
- **cvarとヒンティング**（7段階目のヒンティングのADR）：cvarのビューと検証器、`tt_face_vary_cvt` によるcvtの変化、CFF2のPrivate DICTの座標つきの値のヒンティングへの適用、ppemを与えたときのヒンティングのDeviceのBASEとGDEFのキャレットへの適用。cvarは [ADR 0022](0022-subsetting.md) でそのまま写すが、そのときの検証は、このADRが検証器を足すまで、表の目録の範囲だけとする。
- **VARC**（13段階目）：VARCの表、MultiItemVariationStore、SparseVarRegionList。
- **GDEFのリガチャのキャレットとMATHの値の差分**：それぞれの問い合わせ（`hb_ot_layout_get_ligature_carets`、`hb_ot_math_*`）を公開するADRで、この ADR のVariationIndexの評価を使う。いまsofelはどちらも公開していない。
- **スタイルの値**（`hb_style_get_value`）：可変でないフォントの値（OS/2、post、head）を含む問い合わせなので、メトリクスのADRとして別に書く。
- **合成の太字と斜体**：[ADR 0016](0016-font-and-vertical-metrics.md) と [ADR 0020](0020-shaping-foundation.md) のとおり。
- **送り幅と縦の原点のキャッシュ**：HarfBuzzの `advance_cache`、`origin_cache` に当たる、呼び出し側が持つ値。領域の係数（`hb_scalar_cache_t`、`regionScalars`）は [ADR 0029](0029-variation-deltas.md) で `Location` が持つ。下の計測で遅い項目があればADRを書く。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。手で組むフォントは、[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方に、fvar、avar、gvar、HVAR、VVAR、MVAR、FeatureVariationsの組み立てを足して作る。

### 単体テスト

- **ビュー**：ItemVariationStoreの行（8、16、32bitの差分、`LONG_WORDS`、`wordDeltaCount` の境界）、DeltaSetIndexMapの形式0と1（項目の幅1〜4バイト、内の番号のbit数1〜16、範囲の外の番号、項目0）、gvarの短いオフセットと長いオフセット、`GVAR` の24bitのグリフの数、組の見出しの各印、圧縮した点（0個、「すべての点」、1バイトと2バイトの連）、差分（0の連、8bit、16bit、32bitの形式）、FeatureVariationsの条件の形式1〜5と未知の形式。範囲の外を読むバイト列で、イテレータが止まりpanicしないことを確かめる。
- **検証器**：上の各誤りについて、期待する種類、重大度、オフセットが報告されること。[ADR 0007](0007-sfnt-read-foundation.md) の不変条件（Errorが無ければ読み取りが成功する）を、変異させた入力で確かめる。
- **正規化**：HarfBuzzとFreeTypeのそれぞれで、軸の最小、既定、最大、範囲の外、既定の値と最小が等しい軸、avar版1の区分（端の−1と1の重複、同じ `from` の連続、区分の数0、1、2）、avar版2（差分が±2を超える、写像の無い軸、DeltaSetIndexMapの無いとき）。HarfBuzzとFreeTypeで結果の違う値（`roundf` の半分の境界、avarの最後の `from` と等しい値、軸の数の違うavar）を必ず含め、それぞれの期待値を手で計算して書く。
- **名前つきインスタンス**：範囲の外の番号、PostScript名IDの無いインスタンス、FreeTypeが既定のインスタンスを合成する場合としない場合、`sfnt_get_var_ps_name` の127字を超える名前（MurmurHash3の値を、参照の実装の値と比べる）。
- **評価**：[ADR 0029](0029-variation-deltas.md) のテスト（軸ごとの係数、組の係数、`item_delta`、キャッシュありとなしの一致）。`0xFFFF/0xFFFF` の番号で差分0。
- **gvarの適用**：手で組んだグリフで、組1つ、中間の範囲つきの組、共有の点と私用の点、IUP（差分のある点が0、1、2つ以上の輪郭、元の座標が同じ点）、幻の点とHVARの有無、合成グリフの成分の位置の差分。期待値は、FreeTypeの式を手で計算した16.16の値と、HarfBuzzの式の `f32` の値の2つを書く。
- **計画**：FeatureVariationsの記録の選び方（最初に成り立つもの、どれも成り立たない、条件の無い記録）、計画の鍵が座標の違いで変わることと、同じ記録を選ぶ座標では変わらないこと。
- **`Send` と `Sync`**：`Location` と `Instance` に `fn assert<T: Send + Sync>()` を呼ぶ。`Instance` は `Copy` も確かめる。

### HarfBuzzとFreeTypeとの突き合わせ

- **道具**：`tools/hb-dump/` と `tools/ft-dump/` に、座標の指定を足す。
  - 共通のオプション `--variations=<tag=value,...>` と `--named-instance=<i>`。`hb-dump` は `hb_font_set_var_named_instance` と `hb_font_set_variations`、`ft-dump` は `FT_Set_Named_Instance` と、hb-ftと同じ変換の `FT_Set_Var_Design_Coordinates` を呼ぶ。既存の `--outline`、`--raster`、`--metrics`、`--extents`、`--baselines`、`--shape`、`--shape-random`、`--paint`、`--colr`、`--names` に効かせる。
  - 新しい `--var`：`hb-dump` は `hb_ot_var_*` のすべての問い合わせと、与えた位置の `hb_font_get_var_coords_normalized` と `_design`、`ft-dump` は `FT_Get_MM_Var`、`FT_Get_Var_Axis_Flags`、`FT_Get_Default_Named_Instance`、与えた位置の `FT_Get_Var_Blend_Coordinates`、`FT_Get_Var_Design_Coordinates`、`FT_Get_Postscript_Name` を書く。
  - `--var-normalize <フォント>`：各軸について、最小から最大までを256等分した値と、範囲の外の2つ、`roundf` の半分の境界に当たる値を、1軸ずつ与えた正規化の結果を書く。
  - `hb-dump` と、計測の `bench/build-c.sh` のHarfBuzzとFreeTypeは、`-ffp-contract=off` でビルドする。aarch64のコンパイラが `float` の式をFMAに融合すると、sofel（FMAなし）と環境によって結果が違うためである。いまのビルドで融合が起きているかは未確認である。
- **位置の集まり**：突き合わせの対象のフォント（HarfBuzzとFreeTypeのテストのフォントのうちfvarを持つフェイス。`test/api/fonts/`、`test/shape/data/*/fonts/`、`test/subset/data/fonts/`、`perf/fonts/`）ごとに、次の位置を使う。位置の識別子は項目の末尾に `@<位置>` として付ける。
  - `default`、すべての名前つきインスタンス `named:<i>`（FreeTypeが合成したものを含む）。
  - 軸ごとに、最小と最大（ほかの軸は既定） `<tag>:min`、`<tag>:max`。
  - 決まった種の疑似乱数による8つの位置 `rand:<k>`。各軸の値を最小から最大の範囲から選ぶ。乱数は [ADR 0020](0020-shaping-foundation.md) の `--shape-random` と同じ線形合同法とし、`hb-dump`、`ft-dump`、`sofel-conformance` で同じ値を作り、位置そのものも書き出して照合する。
- **比べるもの**（すべて完全一致）：
  - 正規化：`--var` と `--var-normalize` の全値（HarfBuzzの2.14、FreeTypeの16.16、設計の座標、軸と名前つきインスタンスの情報、PostScript名）。
  - 輪郭：[ADR 0011](0011-glyf-outlines.md) と [ADR 0021](0021-cff-outlines.md) の `unscaled`、16、48画素を、すべての位置と全グリフで。
  - ラスタライズ：[ADR 0012](0012-antialiased-rasterizer.md) の12、24、48画素を、`default`、名前つきインスタンス、`rand:0`、`rand:1` で。
  - 送り幅（横と縦）、縦の原点、グリフの箱、メトリクスの位置、フォントの範囲、基準線：`hb-dump` の値と、すべての位置で。FreeTypeのサイドベアリングと `FtFaceMetrics` は `ft-dump --metrics` と。
  - シェーピング：[ADR 0020](0020-shaping-foundation.md) の `--shape` の組のうち可変フォントの組（SourceSerifVariable-Romanなど）をすべての位置で、`--shape-random` を可変フォントのフェイスについて `default` と `rand:0`〜`rand:3` で。
  - COLRv1：[ADR 0023](0023-color-glyphs.md) の塗りの命令の列（HarfBuzz）と生の値（FreeType）を、可変のCOLRのフォント（`test_glyphs-glyf_colr_1_variable.ttf` など）のすべての位置で。
- 期待する失敗の一覧は、既存のもの（`conformance/expected-failures-freetype.txt` など）に、理由を添えて足す。

### HarfBuzzのテスト

- `conformance/expected-failures.txt` の `variations` の263行のうち、`in-house/aat-trak.tests` の7行を `aat` に分類しなおし、残りの256行を作業の5のプルリクエストで消す。消した行が一致することを `sofel-conformance` で確かめる（[ADR 0005](0005-conformance-tests.md)）。`expected-failures-shape-random.txt` の `variations` の5行も消す。[ADR 0020](0020-shaping-foundation.md) の分類の説明から、`--font-ptem` による `opsz` を除く。
- HarfBuzzの `test/api/` のうち、`test-var-coords.c`、`test-ot-metrics-tt-var.c`、`test-ot-face.c` の可変フォントの部分、`test-ot-extents-cff.c` のCFF2の位置、`test-paint.c` の可変の塗り、`test-baseline.c` の可変の部分の検査を、同じフォントと同じ期待値でRustのテストに写す。
- `test/subset/` の `stage:var` の行は、[ADR 0022](0022-subsetting.md) の段階10で消す（上の節）。
- `conformance/expected-failures-freetype.txt` の、可変の塗りの73行（理由が `get_deltas_for_var_index_base`）は、作業の6で原因を確かめなおす。sofelの値がFreeTypeと一致すれば消し、残れば理由を書きなおす。

### そのほか

- **決定性**：上の突き合わせを、CIのLinux（x86-64）とmacOS（aarch64）で走らせる。`hb-dump` と `ft-dump` を要さない部分（単体テストと、手で書いた期待値の比較）は、`wasm32-wasip1` でもwasmtimeで走らせる。
- **ファジング**：`fuzz/` に、任意のバイト列のフォントと任意の位置（設計の座標、正規化した座標、名前つきインスタンスの番号）で、`Location` を作り、全グリフの `draw`、送り幅、箱、縦の原点、シェーピング（短い文字列）、COLRの塗りを呼ぶ対象を足す。panicしないことを確かめる。gvarの復号のイテレータと、FeatureVariationsの条件の評価には、それぞれ単独の対象を置く。
- **`sofel-capi`**：Cのテストで、各 `sofel_location_create*` の座標がRustの `Location` と一致すること、`location` にNULLを渡すと既定のインスタンスの値になること、不正な名前つきインスタンスと±1を超える正規化した座標で誤りが返ることを確かめる。`scripts/check-capi.sh` の対応の検査に新しい関数を加える。
- **wasm**：`sofel-read`、`sofel-font`、`sofel-outline`、`sofel-shape` の単体テストを `wasm32-wasip1` で通す。

## 計測

[ADR 0013](0013-benchmark-harness.md) の計測の道具に、既定でない位置の項目を足す。位置は、HarfBuzzの `perf/benchmark-font.cc` と同じく `wght=500` とする。フォントは、RobotoFlex-Variable、SourceSerifVariable-Roman（glyfとgvarとHVAR）、AdobeVFPrototype（CFF2）とする。

| 項目 | sofel | FreeType | HarfBuzz |
|---|---|---|---|
| 位置の作成 | `Location::new` | `FT_Set_Var_Design_Coordinates` | `hb_font_set_variations` |
| 送り幅（位置あり） | 全グリフで `Instance::advance` | `FT_Get_Advance`（`FT_LOAD_NO_SCALE`） | `hb_font_get_glyph_h_advance` |
| 箱（位置あり） | 全グリフで `glyph_extents` | なし | `hb_font_get_glyph_extents` |
| 輪郭（位置あり、拡大縮小なし、48画素） | 全グリフで `draw` | `FT_Load_Glyph` と `FT_Outline_Decompose` | `hb_font_draw_glyph` |
| ラスタライズ（位置あり、24画素） | 全グリフで `draw` と `render` | `FT_Load_Glyph` と `FT_Render_Glyph` | なし |
| シェーピング（位置あり） | SourceSerifVariable-Romanと `react-dom.txt` | なし | `hb_shape` |

- 計測の前の照合は、既存の項目と同じく結果の一致とする。
- 既定のインスタンスの既存の項目が、`Instance` への移動で遅くならないことを、作業の2の前後の記録で確かめる。
- 作業の7で、ネイティブとwasmの記録を `docs/benchmarks/` に置く。

## トレードオフ

- 座標を `Font` に持たせず、`Location` と `Instance` に分ける。`Font` は `Copy` で確保しない値のまま残り、位置ごとに `Font` を作りなおさずに済む。代わりに、座標で変わる問い合わせの引数が `Instance` になり、既存の `Font` の関数と、それを使う `sofel-shape`、`sofel-outline`、`sofel-color`、`sofel-capi` の呼び出しを書きかえる手間がかかる。HarfBuzzの `hb_face_t` と `hb_font_t` の分け方に近い。
- `Location` にHarfBuzzとFreeTypeの2つの座標を持たせる。2つは丸めとavarの解釈が違い、片方から他方を作れないためである。位置を作る費用は2倍になるが、位置を作る回数はグリフの数に比べて少ない。
- `sofel-font` が `alloc` を使うようになる。軸の数に上限が無いので、固定の長さの配列では表せない。呼び出し側の領域を借りる形も考えたが、`Location` を作る関数が引数を書きかえることになり、[ADR 0003](0003-crate-layers.md) の関数型の方針に逆らう。
- HarfBuzzのglyfの点（`f32`）とFreeTypeのglyfの点（16.16）を、別々の型で持つ。点を作る部分と合成グリフの再帰は `sofel-font` と `sofel-outline` に分かれるが、差分の適用とIUPの手順は [ADR 0029](0029-variation-deltas.md) の核を共有する。
- `GlyphScratch` を座標で変わる問い合わせの引数にするため、HVARのある送り幅のように点の要らない場合も引数が1つ増える。問い合わせごとに引数の形が変わるより、利用者が覚えることが少ない。
- HarfBuzzのCFF2の箱の解釈器を `f64` にする。[ADR 0021](0021-cff-outlines.md) が速さのために選んだ `i64` を捨てるので、既定のインスタンスで遅くなりうる。計測の条件で、その場合だけ `i64` を残す。
- `location` のNULLを既定のインスタンスとすることで、[ADR 0015](0015-c-abi.md) のNULLの決まりに例外が1つ増える。既定のインスタンスだけを使う利用者（Golem）は、`sofel_location_t` を作らずに済む。
- [ADR 0022](0022-subsetting.md) の段階10を、この ADR の読み取りの部分の後に始められるようにする。サブセット化の完成が早まる代わりに、作業の1のビューは、段階10の書きなおしと、この ADR の適用の両方の要求を満たす形で作る必要がある。

## 参考文献

- HarfBuzz 15.1.0（`3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`）
  - `src/hb-ot-var.cc`、`src/hb-ot-var.h`、`src/hb-ot-var-fvar-table.hh`、`src/hb-ot-var-avar-table.hh`、`src/hb-ot-var-gvar-table.hh`、`src/hb-ot-var-hvar-table.hh`、`src/hb-ot-var-mvar-table.hh`、`src/hb-ot-var-common.hh`、`src/hb-ot-var-cvar-table.hh`、`src/hb-ot-var-varc-table.hh`
  - `src/hb-font.cc`（`hb_font_create`、`hb_font_set_variations`、`hb_font_set_var_coords_design`、`hb_font_set_var_coords_normalized`、`hb_font_set_var_named_instance`、`_hb_font_adopt_var_coords`）、`src/hb-font.hh`、`src/hb-ft.cc`、`src/hb-ot-font.cc`、`src/hb-ot-hmtx-table.hh`、`src/hb-ot-metrics.cc`、`src/hb-style.cc`
  - `src/OT/glyf/glyf.hh`、`src/OT/glyf/Glyph.hh`
  - `src/hb-ot-layout-common.hh`（`VarRegionAxis`、`VarRegionList`、`ItemVariationStore`、`DeltaSetIndexMap`、`VariationDevice`、`Condition`、`FeatureVariations`）、`src/hb-ot-layout-gsubgpos.hh`（`find_variations_index`）、`src/hb-ot-layout.cc`（`hb_ot_layout_table_find_feature_variations`）、`src/hb-ot-shape.hh`（`hb_ot_shape_plan_key_t`）、`src/hb-shape-plan.cc`
  - `src/hb-cff2-interp-cs.hh`、`src/hb-cff-interp-common.hh`、`src/OT/Color/COLR/COLR.hh`、`src/hb-algs.hh`、`src/hb-config.hh`
  - `util/font-options.hh`、`perf/benchmark-font.cc`、`perf/benchmark-shape.cc`
  - `test/api/test-var-coords.c`、`test/api/test-ot-metrics-tt-var.c`、`test/api/test-ot-face.c`、`test/api/test-ot-extents-cff.c`、`test/api/test-paint.c`、`test/api/test-baseline.c`、`test/api/fonts/`、`test/shape/data/in-house/tests/variations.tests`、`variations-rvrn.tests`、`anchor-variations.tests`、`aat-trak.tests`、`test/shape/data/text-rendering-tests/tests/`（AVAR、GVAR、HVAR、CFF2、CVAR、GPOS-5）
- FreeType 2.15.0（`VER-2-15-0`、`37515cf56ba3349656049edbed89ffc6f6466f2d`）
  - `src/truetype/ttgxvar.c`（`ft_var_readpackedpoints`、`ft_var_readpackeddeltas`、`ft_var_load_avar`、`tt_var_load_item_variation_store`、`tt_var_load_delta_set_index_mapping`、`ft_var_load_hvvar`、`tt_calculate_scalar`、`tt_var_get_item_delta`、`tt_hvadvance_adjust`、`ft_var_load_mvar`、`tt_apply_mvar`、`ft_var_load_gvar`、`ft_var_apply_tuple`、`ft_var_to_normalized`、`ft_var_to_design`、`TT_Get_MM_Var`、`tt_set_mm_blend`、`TT_Set_MM_Blend`、`TT_Set_Var_Design`、`TT_Set_Named_Instance`、`TT_Get_Default_Named_Instance`、`tt_interpolate_deltas`、`tt_delta_shift`、`tt_delta_interpolate`、`TT_Vary_Apply_Glyph_Deltas`）
  - `src/truetype/ttgload.c`（`IS_DEFAULT_INSTANCE`、`TT_Process_Simple_Glyph`、`load_truetype_glyph`、`TT_Load_Glyph`）、`src/base/ftmm.c`、`src/sfnt/sfdriver.c`（`sfnt_get_var_ps_name`、`murmur_hash_3_128`）、`src/sfnt/ttmtx.c`（`tt_face_get_metrics`）、`src/sfnt/ttcolr.c`（`get_deltas_for_var_index_base`）、`src/cff/cffload.c`（`cff_blend_build_vector`、`cff_blend_check_vector`）、`include/freetype/config/ftoption.h`（`TT_CONFIG_OPTION_NO_BORING_EXPANSION`）
- OpenType Specification：fvar、avar（版2を含む）、gvar、HVAR、VVAR、MVAR、OpenType Font Variations Common Table Formats（ItemVariationStore、DeltaSetIndexMap、TupleVariationStore）、Chapter 2（FeatureVariations、ConditionSet） https://learn.microsoft.com/en-us/typography/opentype/spec/
- ISO/IEC 14496-22 第5版（`GVAR`、`GLYF`）
- MurmurHash3 https://github.com/aappleby/smhasher/blob/master/src/MurmurHash3.cpp
