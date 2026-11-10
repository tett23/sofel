# ADR 0029: 変化量の評価（1つの核、2つの算術、位置が持つ領域の係数）

ステータス: 提案

## 文脈

[ADR 0027](0027-variable-fonts.md) は、可変フォントの座標の模型（`Location`、`Instance`）と、各表の変化量をどこで使うかを決めた。
変化量の評価そのものは、HarfBuzzの手順（`hb_var`：`f32` の係数と差分）とFreeTypeの手順（`ft_var`：16.16の係数、64bitの和）の2つの関数の組とし、領域の係数のキャッシュは持たず、計測で遅ければ足すとした。
この ADR は、その2点を決めなおす。評価の核を1つにできるか、係数をどこに持つか、既定のインスタンスに費用をかけない方法、x86-64とaarch64とwasm32での決定性、[ADR 0015](0015-c-abi.md) のC ABIの形である。
[ADR 0027](0027-variable-fonts.md) の座標の模型と、各表の値の使い方は変えない。

### HarfBuzzとFreeTypeの評価の手順

両方のコードを読み、ItemVariationStoreの評価が、型と丸めを除いて同じ構造であることを確かめた。

| 段階 | HarfBuzz 15.1.0 | FreeType 2.15.0 |
|---|---|---|
| 軸ごとの係数 | `VarRegionAxis::evaluate`（`src/hb-ot-layout-common.hh` 2549行）。2.14の整数の座標から `float (coord - start) / (peak - start)` を `float` で求める。分岐の順は、`peak == 0` か `coord == peak` なら1、`coord == 0` なら0、不正な領域（`start > peak`、`peak > end`、`start < 0 < end`）なら1、範囲の外なら0、ほかは比例 | `tt_calculate_scalar`（`src/truetype/ttgxvar.c` 1035行）。16.16の座標から `FT_MulDiv` で求める。不正な領域は、表を読むときに `peak` を0にしておく（同1035行ではなく、`tt_var_load_item_variation_store` の631行。CFF2は `src/cff/cffload.c` の1220行）。分岐の順は、`peak == coord` か `peak == 0` なら1、`coord <= start` か `coord >= end` なら0、ほかは比例 |
| 領域の係数 | `VarRegionList::evaluate_impl`（2731行）。軸の順に `float` で掛け、0になれば打ち切る | `tt_calculate_scalar` の中で、軸の順に `FT_MulDiv(scalar, …)` を掛け重ね、0になれば打ち切る |
| 項目の差分 | `VarData::_get_delta`（3074行）。行の列を、32bit、16bit、8bitの順（行に並ぶ順）に読み、係数が0でない列だけ `delta += (Float) scalar * (int) d` を足す。`Float` は既定で `float`、CFF2とCOLRの一部で `double` | `tt_var_get_item_delta`（1195行）。同じ順に読み、係数が0でない列だけ `returnValue += (FT_Int64) delta * scalar` を足す（`ft_mul_add_delta_scalar`、1080行） |
| 丸め | 使う側が行う（送り幅は `roundf` と `hb_clamp_to<unsigned>`、MVARは `roundf`、GPOSは `em_scalef`） | `ft_round_and_shift16`（1164行）：`(FT_ItemVarDelta)(returnValue + 0x8000L) >> 16`。`FT_ItemVarDelta` は `FT_Int32`（`include/freetype/internal/ftmmtypes.h` 27行）なので、64bitの和を32bitに切り落としてから算術右シフトする |
| 係数のキャッシュ | `hb_scalar_cache_t`（2637行）：領域の数だけの `float` を、NaNを「未計算」として遅延で埋める。128個までは静的な領域、それより多ければ確保する。送り幅（`src/hb-ot-font.cc` の `varStore_cache`、81行）、GPOS（`src/hb-ot-shape.cc` の `_hb_ot_shaper_font_data_create`、433行。GDEFのストアのキャッシュをフォントのデータとして持つ）、avar版2（`src/hb-ot-var-avar-table.hh` 568行）、サブセット化で使う。CFF2は `VarData::get_region_scalars`（3126行）で、`vsindex` ごとの係数の列を `hb_vector_t<float>` に作って `blend` のたびに使う（`src/hb-cff2-interp-cs.hh` 146行） | `GX_ItemVarStoreRec::regionScalars`（`ftmmtypes.h` 78行）：ストアごとに領域の数だけの `FT_Fixed` を、最初の評価で全部求め、`cachedCoords` が座標と違えば求めなおす（`ttgxvar.c` 1253〜1310行）。CFF2は `cff_blend_build_vector`（`cffload.c` 1411行）で `vsindex` ごとの `BV` を作る |
| gvarの組の係数 | `TupleVariationHeader::calculate_scalar`（`src/hb-ot-var-common.hh` 113行）。`float`。共有の組（埋め込みのピークでも中間の範囲でもない組）だけ `hb_scalar_cache_t`（共有の組の数）に入れる（`src/hb-ot-font.cc` 259行の `gvar` のキャッシュ） | `ft_var_apply_tuple`（2288行）。`FT_MulDiv`。同じ条件の組だけ `blend->tuplescalars`（組の数。`-0x20000` を「未計算」とする）に入れる（4727〜4812行） |
| gvarの差分の適用 | `apply_deltas_to_points`（`src/hb-ot-var-gvar-table.hh` 769行）。`int` の差分に `float` の係数を掛けて `float` の点に足す。「すべての点」の組で私用の点が無ければ点に直接足し、そうでなければ `deltas` に溜めて、次の「すべての点」でない組の前と最後に点へ足す（`flush`）。IUPは組ごとに、元の点（`orig_points`）で行う（`infer_delta`、665行） | `TT_Vary_Apply_Glyph_Deltas`（4476行）。`FT_intToFixed` した差分に `FT_MulFix` で係数を掛け、`point_deltas_x` に16.16で溜める（4862行）。IUPは組ごとに、元の点（`points_org`）で行い（`tt_interpolate_deltas`、4371行。`tt_delta_interpolate` は `FT_DivFix` で傾きを求めて `FT_MulFix`、4303行）、最後に `FT_fixedToInt`（`FT_Short` に切り落とす。73〜82行）と `FT_fixedToFdot6` で点にする |

このうち、1つの核にできないのは、軸ごとの係数の関数と、算術だけである。

- 軸ごとの係数は、分岐の順が違い、結果も違う場合がある。不正な領域（`start < 0 < end`）で座標が0のとき、HarfBuzzは `coord == 0` を先に見て0、FreeTypeは読み込みで `peak` を0にしたので1になる。gvarの中間の範囲つきの組で `start == peak` かつ座標が `start` のとき、HarfBuzzは掛け算を飛ばして1、FreeTypeは `ncv <= im_start` で0になる。
- 算術は、HarfBuzzの `float` の割り算と掛け算（IEEE 754の単精度で各演算を丸める）と、FreeTypeの `FT_MulDiv`（64bitの積を丸めて割る）で、同じ入力から違う値を出す（例えば1/3は `f32` で0x3EAAAAAB、16.16で0x5555）。整数の算術で `float` の結果を出すには、単精度の丸めを整数で模倣するしかなく、それは `f32` の演算そのものである。

それ以外（列の読み方、0の係数の飛ばし方、和の順、領域の係数の積の順、gvarの組の順と `flush`、IUPの区間の探し方）は同じなので、1つの核を算術の方針で型づけできる。
FreeTypeの整数の和は順序によらないので、HarfBuzzの順（`flush` を含む）で足しても、FreeTypeの結果は変わらない。

### 既定のインスタンスの扱い

両方とも、座標がすべて0のときは評価を避ける分岐を持つが、その場所は値ごとに違う。

- HarfBuzz：送り幅（`src/hb-ot-font.cc` 409行の `has_nonzero_coords`）、glyfの点（`src/OT/glyf/glyf.hh` 481行）、VariationIndexの補正（`VariationDevice::get_x_delta`、6295行）、COLR（`ItemVarStoreInstancer::operator bool`、4225行。`num_coords` が0なら偽）は評価を飛ばす。MVAR（`src/hb-ot-metrics.cc` 68行の `GET_VAR`）は飛ばさず、座標が無ければ各軸の座標を0として評価する。
- FreeType：`tt_var_get_item_delta` は `blend->normalizedcoords` が無ければ0を返す（1212行）。`FT_Set_Var_Design_Coordinates` で既定の値を与えたフェイスは座標を持つので、評価する。

座標がすべて0のときの領域の係数は、ピークがすべて0の領域で1、それ以外で0である。ピークがすべて0の領域は、仕様上は無意味だが禁じられてはおらず、HarfBuzzのMVARとFreeTypeは、その差分を既定の座標でも足す。
そのため、「既定のインスタンスでは評価しない」とは決められない。既定のインスタンスで費用をかけないには、基準の側の分岐をそのまま写したうえで、評価そのものを、係数が0でない列が無ければ行を読まずに0を返す形にする。

### sofelの制約

- ライブラリの値は不変で、内部可変性を使わない（[ADR 0003](0003-crate-layers.md)）。HarfBuzzの遅延のキャッシュも、FreeTypeの `cachedCoords` による求めなおしも、そのままでは写せない。キャッシュは呼び出し側が持つ値とする。
- 決定性：`f32` の四則演算は、x86-64（SSE）、aarch64、wasm32のどれでもIEEE 754の単精度で同じ結果になる。RustはFMAへの融合を行わない（`mul_add` を書かない限り使わない）。HarfBuzzの `float` の式は、[ADR 0026](0026-harfbuzz-rounding.md) の決まり（同じ精度の四則演算、`hb_roundf`、FMAなし）で写す。FreeTypeの固定小数点は `sofel-geom` の `mul_div`、`mul_fix`、`div_fix`（[ADR 0009](0009-geom-fixed-point-and-pen.md)）で写す。
- いまmainにある `sofel-read` の `cff/blend.rs` の `FtBlend` は、`cff_blend_build_vector` を写した `BV` の計算である。これは下の係数の計算と同じものなので、置きかえる。

### fontationsとHarfRust

fontations（`read-fonts/src/tables/variations.rs`、2026-11-06の `e5e7f433`）は、`compute_delta_with_scalars(index, coords, scalars)` で、呼び出し側が持つ領域の係数の列（`Fixed`）を受け取り、`F48Dot16` で足す。算術はFreeTypeの形だけである。
HarfRust（`harfrust/src/ot/mod.rs` 518行、2026-11-08の `043820d5`）は、その `Fixed` の差分を `f32` にして使うので、HarfBuzzの `float` の結果とは一致しない。
sofelは [ADR 0005](0005-conformance-tests.md) により両方と完全一致させるので、係数の算術を2つ持つ必要がある。係数の列を呼び出し側が持つ形は、fontationsと同じである。

## 実装すること

### 1つの核と算術の方針

`sofel-font` の `var` モジュールに、評価の核を1つ置く。核は、算術の方針を型引数にとる。

- `trait Arith`：`Coord`（正規化した座標の型。`HbF32` と `HbF64` は2.14の `i32`、`Ft` は16.16の `i32`）、`Scalar`（領域と組の係数の型）、`Acc`（和の型）、`Delta`（結果の型）と、次の関数を持つ。
  - `axis_factor(coord, start, peak, end) -> Scalar`：ItemVariationStoreの軸ごとの係数。分岐の順を含めて、基準の側の関数を写す。
  - `tuple_axis_factor(coord, peak, intermediate: Option<(start, end)>) -> Option<Scalar>`：gvarの組の軸ごとの係数。`None` は「この軸では掛けない」（HarfBuzzの `continue`）。
  - `mul(Scalar, Scalar) -> Scalar`：係数の積。
  - `zero() -> Acc`、`mul_add(Acc, Scalar, i32) -> Acc`、`finish(Acc) -> Delta`：差分の和。
  - gvarのための `Point`（点の型）、`scale_delta(i32, Scalar) -> Coord`（差分に係数を掛ける）、`iup_segment` と `iup_interpolate`（下の「アルゴリズム」）。
- 方針は3つ。`HbF32`（`Scalar = f32`、`Acc = f32`、`Delta = f32`。HarfBuzzの既定）、`HbF64`（`Scalar = f32`、`Acc = f64`、`Delta = f64`。HarfBuzzのCFF2の `blend` と `get_delta<double>`）、`Ft`（`Scalar = i32` の16.16、`Acc = i64`、`Delta = i32`。`finish` は `((acc.wrapping_add(0x8000)) as i32) >> 16` で、FreeTypeの32bitへの切り落としを含む）。`HbF32` と `HbF64` の `Scalar` は同じなので、係数の列は共有する。
- 評価の関数は、係数の出どころを型引数にとる。`trait ScalarSource<S>` は、ItemVariationDataの番号（outer）に対して「係数が0でない列の番号と係数」の列を返す。実装は2つ：`StoreScalars<S>`（下の「データ構造」。`Location` が持つ）と、座標から毎回求める `OnTheFly<A>`（`Location` が別のフォントで作られたときの代わりと、avar版2の評価、テスト用）。公開の関数は `StoreScalars` を使うものだけとし、`OnTheFly` はクレートの中に閉じる。
- `item_delta::<A>(data: &ItemVariationData, source: &impl ScalarSource<A::Scalar>, outer, inner) -> A::Delta`：係数が0でない列が無ければ、行を読まずに `finish(zero())` を返す。列があれば、行の列の幅（`wordDeltaCount`、`LONG_WORDS`）から各列の位置を求めて読み、列の番号の昇順に `mul_add` する。昇順なので、HarfBuzzの和の順と同じになる。
- `0xFFFF/0xFFFF` の番号と、範囲の外の番号は、両方の基準と同じく0とする。
- `sofel-read` の `item_variation` に、行の列を位置で読む関数 `ItemVariationData::delta(inner, column) -> Option<i32>` を足す（[ADR 0027](0027-variable-fonts.md) の `delta(outer, inner, region_k)` と同じもの。行の先頭からの位置は、列の番号と幅から求める）。

`hb_var` と `ft_var` の2つの評価の関数は作らない。[ADR 0027](0027-variable-fonts.md) の該当の箇所は、この ADR のとおりに改める（下の「[ADR 0027](0027-variable-fonts.md) の変更」）。

### 領域の係数は `Location` が持つ

- `Location` は、作ったときの `Font` が持つ各ストアについて、`StoreScalars<f32>`（HarfBuzzの座標 `hb_coords` から `HbF32::axis_factor` で求めた係数）と `StoreScalars<i32>`（FreeTypeの座標 `ft_coords` から `Ft::axis_factor` で求めた係数）を持つ。ストアは `StoreKind` で区別する：`Hvar`、`Vvar`、`Mvar`、`Gdef`、`Base`、`Colr`、`Cff2`。基準の側だけが使うストアは片方だけ持つ（GDEFとBASEはHarfBuzzだけ。HVAR、VVAR、MVAR、COLR、CFF2は両方）。
- gvarの共有の組の係数も同じく持つ：`gvar_hb: Vec<f32>`、`gvar_ft: Vec<i32>`（共有の組の数。両方の基準が「埋め込みのピークでも中間の範囲でもない組」だけをキャッシュするので、同じ範囲とする。中間の範囲つきの組と埋め込みのピークの組は、グリフごとに求める）。
- 係数は `Location` を作るときに全部求める（FreeTypeの `regionScalars` と同じ、先に求める形。HarfBuzzの遅延は、不変の値では写せない）。費用は、ストアごとに「領域の数 × 軸の数」の係数の計算で、位置を作るたびに1回である。
- avar版2の評価は、avarの適用前の座標で行い、最終の座標とは違うので、`OnTheFly` で求めて係数は持たない。
- `Location` は、作った `Font` の鍵 `font_key: u64` を持つ。`Font::new` は、表の目録（各表のタグ、オフセット、長さ）、データの長さ、フェイスの番号、`head` の `checkSumAdjustment` からFNV-1a（64bit）で鍵を求めて持つ。`Font::instance(&self, &Location)` は、鍵が違えば `Instance::scalars_match` を偽にし、評価は `OnTheFly` で座標から求める（[ADR 0027](0027-variable-fonts.md) の「`Location` を作った `Font` と違うフォントに使ってもよい」はそのまま。結果はHarfBuzzの `num_coords` の扱いと同じ）。鍵が同じなら `StoreScalars` を使う。両方の結果が等しいことは、下のテストで確かめる。
- 既定のインスタンス（座標がすべて0、名前つきインスタンスなし）の `StoreScalars` は、ピークがすべて0の領域だけが1で、ほかは0になる。ピークがすべて0の領域を持つストアは通常無いので、どのItemVariationDataも「0でない列」が空になり、評価は行を読まずに0を返す。基準の側の「座標が0なら評価しない」の分岐（上の「既定のインスタンスの扱い」の各箇所）は、[ADR 0027](0027-variable-fonts.md) のとおり、値ごとに写す。

### 問い合わせの形

[ADR 0027](0027-variable-fonts.md) の `Instance` の問い合わせの中で使う。公開の関数は次の2つで、既定と既定でない位置で分けない。

- `Instance::item_delta::<A: Arith>(&self, store: StoreKind, outer: u16, inner: u16) -> A::Delta`：`Location` の `StoreScalars` か `OnTheFly` で、`item_delta` を呼ぶ。ストアが無いフォントでは0。DeltaSetIndexMapによる番号の写像は、呼ぶ側が `sofel-read` の `delta_set_index_map::map` で行ってから渡す（HVARの送り幅の写像、COLRの `VarIdxBase + i`、avar版2の軸の写像）。
- `Instance::blend_scalars::<A>(&self, vsindex: u16) -> impl Iterator<Item = (u16, A::Scalar)>`：CFF2の `vsindex` のItemVariationDataの、係数が0でない列。FreeTypeの `BV`（`BV[0] = 1`、`BV[k+1]` が列 `k` の係数）と、HarfBuzzの `get_region_scalars` の列に当たる。`sofel-read` の `BlendWeights` の実装は、これで `FtBlend` を置きかえる。CFF2の `blend` の算術（FreeTypeの `cff_blend_doBlend`、HarfBuzzの `blend_deltas`）は [ADR 0021](0021-cff-outlines.md) と [ADR 0027](0027-variable-fonts.md) のとおりで、この ADR は係数の値だけを与える。HarfBuzzの `blend_deltas` は0の係数を飛ばさないが、和は `+0.0` から始まり、打ち消しあった和も `+0.0` になるので、`±0.0` を足しても値は変わらず、飛ばしても同じ結果になる。

各表の値の使い方（丸め、`em_scalef`、`hb_clamp_to`、`ADD_INT`）は [ADR 0027](0027-variable-fonts.md) のままで、`hb_var`・`ft_var` を `item_delta::<HbF32>`・`item_delta::<Ft>` に読みかえる。HarfBuzzのCFF2の箱は `item_delta` を使わず、`blend_scalars::<HbF64>` の係数を `f64` の解釈器で使う。

### gvar

`sofel-font` の `var::gvar` に、差分の適用の核を1つ置き、HarfBuzzの `apply_deltas_to_points` の順で書く。

- `apply::<A>(gvar: &Gvar, glyph, coords: &[A::Coord], shared: &[A::Scalar], points: &mut [A::Point], scratch: &mut GvarScratch<A>, phantom_only: bool) -> Result<bool, GvarError>`。`shared` は `Location` の共有の組の係数。組の係数は、共有の組なら `shared[index]`、ほかは `A::tuple_axis_factor` で求める。
- `HbF32` の `Point` は `f32` の組、`Ft` の `Point` は16.16の `i32` の組（FreeTypeの `points_org`）。`GvarScratch<A>` は、差分の列（`A::Coord` の組と「差分あり」の印）、元の点の写し、私用と共有の点の番号、復号した `i32` の差分の列を持つ。[ADR 0027](0027-variable-fonts.md) の `GlyphScratch` は `GvarScratch<HbF32>` を、`sofel-outline` の `Scratch` は `GvarScratch<Ft>` を持つ。
- 「すべての点」の組で私用の点が無い場合は点に直接足し、そうでなければ差分の列に溜めて `flush` で足す。この順はHarfBuzzの `f32` の和の順を決めるもので、FreeTypeの整数の和は順によらないので、`Ft` でも同じ順で足す。
- IUPは組ごとに、元の点で行う。区間（差分のある点の間の、差分の無い点の並び）の探し方と、差分のある点が輪郭に1つのときの平行移動は核が行い、区間の中の点の値は `A::iup_segment`（区間の両端から傾きなどを1回求める）と `A::iup_interpolate`（点ごと）で求める。
- 幻の点の扱い（HVARとVVARがあるときに幻の点の差分を0にする、`pp1.x` のずらし）、合成グリフの成分の位置、最後の `FT_fixedToInt` と `FT_fixedToFdot6`、`unrounded` の拡大縮小は、[ADR 0027](0027-variable-fonts.md) のとおり `sofel-font` の `hb_glyf` と `sofel-outline` が核の外で行う。
- `phantom_only` はHarfBuzzと同じく、最後の4点だけに差分を足し、IUPを行わない。FreeTypeには無いが、幻の点は輪郭に属さずIUPの影響を受けないので、`Ft` でも結果は変わらない。

### `sofel-capi`

[ADR 0015](0015-c-abi.md) の決まりに従い、Rustの公開の関数と同じプルリクエストで足す。

- `sofel_var_store_t`（`SOFEL_VAR_STORE_HVAR` …`_CFF2` の列挙）。
- `int32_t sofel_instance_item_delta_hb(const sofel_font_t *font, const sofel_location_t *location, sofel_var_store_t store, uint16_t outer, uint16_t inner, float *out)` と `sofel_instance_item_delta_ft(…, int32_t *out)`。`location` のNULLは既定のインスタンス（[ADR 0027](0027-variable-fonts.md) の例外）。`HbF64` は、C ABIでは使い道が無いので `unmapped.txt` に理由を書く。
- `sofel_instance_blend_scalars_ft(font, location, vsindex, int32_t *out, size_t cap, size_t *len)`（`BV` の形。`out` が足りなければ要る長さを返す）。HarfBuzzの `f32` の版は、HarfBuzzが公開していないので `unmapped.txt` に書く。
- `StoreScalars` と `font_key` は `sofel_location_t` の中にあり、Cには見せない。
- `scripts/check-capi.sh` の対応の検査に加える。

### [ADR 0027](0027-variable-fonts.md) の変更

[ADR 0027](0027-variable-fonts.md) は未コミットなので、同じプルリクエストで次のように改める。

- 「HarfBuzzとFreeTypeの違い」の領域の係数の行：FreeTypeが不正な領域を特別に扱わないという記述を、読み込みで `peak` を0にする（`ttgxvar.c` 631行、`cffload.c` 1220行）と正す。
- 「正規化と変化量の評価」：`hb_var` と `ft_var` の2つの関数の組を、この ADR の1つの核と3つの方針に置きかえる。領域の係数のキャッシュを持たないという決定を、`Location` が持つに改める。
- 「`sofel-font` の値」の `hb_glyf` と「`sofel-outline`」のglyfとgvar：それぞれが別に写す（`Glyph::get_points`、`TT_Vary_Apply_Glyph_Deltas`）としていた差分の適用を、この ADR の核を `HbF32` と `Ft` で呼ぶに改める。幻の点、合成グリフ、`unrounded` はそのまま。
- CFF2：`BV` を `cff_blend_build_vector` の規則で求めるという記述を、`blend_scalars::<Ft>` に改める。HarfBuzzのCFF2の箱の `f64` 化はそのままで、係数は `blend_scalars::<HbF64>`。
- データ構造の `Location` に、`StoreScalars` と共有の組の係数と `font_key` を足す。`Font` に `font_key` を足す。
- アルゴリズムの「領域の係数」と「IUP」を、この ADR への参照に改める。
- 「後回しにしたもの」の「領域の係数と送り幅のキャッシュ」から領域の係数を除き、送り幅と縦の原点のキャッシュ（`advance_cache`、`origin_cache`）だけを残す。
- 作業の分け方の2（`hb_var` と `ft_var`）と4（`TT_Vary_Apply_Glyph_Deltas` の移植）を、この ADR の核に改める。
- トレードオフの「領域の係数のキャッシュを持たない」を除き、「HarfBuzzのglyfの点とFreeTypeのglyfの点を別々に持つ」を、点の型は別だが適用の手順は1つ、と改める。

## データ構造

- `StoreScalars<S>`（`sofel-font`、`Clone`、`Send`、`Sync`）：
  - `regions: Vec<S>`：領域の番号順の係数（領域の数）。CFF2の `BV` と、FreeTypeの `regionScalars`、HarfBuzzの `hb_scalar_cache_t` の全部が埋まった状態に当たる。
  - `columns: Vec<(u16, S)>`：すべてのItemVariationDataについて、係数が0でない列の番号と係数を、ItemVariationDataの順、列の昇順に並べたもの。
  - `ranges: Vec<(u32, u32)>`：ItemVariationDataごとの `columns` の始めと長さ（`dataCount` 個）。
  - 既定のインスタンスでは、`columns` が通常は空で、`ranges` の長さがすべて0になる。
- `Location` に足すもの：`hb_scalars: [Option<StoreScalars<f32>>; 7]`、`ft_scalars: [Option<StoreScalars<i32>>; 7]`（`StoreKind` の番号で引く）、`gvar_hb: Vec<f32>`、`gvar_ft: Vec<i32>`、`font_key: u64`。ストアの無いフォント、ビューを作れないストアは `None`。
- `Font` に足すもの：`font_key: u64`。`Font::new` は引き続き確保しない。
- `GvarScratch<A>`：`deltas: Vec<(A::Coord, A::Coord)>`、`has_delta: Vec<bool>`、`orig: Vec<A::Point>`、`shared_indices: Vec<u16>`、`private_indices: Vec<u16>`、`x_deltas: Vec<i32>`、`y_deltas: Vec<i32>`。呼び出し側が持ち、グリフをまたいで再利用する。
- `OnTheFly<A>`（クレートの中）：`store: ItemVariationStore`、`coords: &[A::Coord]` を持ち、outerごとに列を順に見て係数を求める。

## アルゴリズム

- **`StoreScalars` の構築**：領域ごとに、軸の順に `axis_factor` を掛け（`mul`）、0になれば打ち切る。`coords` が足りない軸は0とする（HarfBuzzの `coord_len`、FreeTypeの `axisCount` のとおり）。次に、ItemVariationDataごとに、`regionIndices` を順に見て、係数が0でない列を `columns` に積む。領域の番号が領域の数以上なら、その列の係数は0（HarfBuzzの `VarRegionList::evaluate`、2755行）。
- **`item_delta`**：`ranges[outer]` が空なら `finish(zero())`。`inner` が `itemCount` 以上なら同じ。ほかは、`columns[start..start+len]` の各 `(k, s)` について、行の列 `k` を読み（`k < wordDeltaCount` なら広い幅、ほかは狭い幅。`LONG_WORDS` で幅が2倍）、`acc = mul_add(acc, s, d)`。最後に `finish(acc)`。
- **`HbF32` の算術**：`axis_factor` は `VarRegionAxis::evaluate` の分岐の順のまま、`(coord - start) as f32 / (peak - start) as f32`。`mul` は `f32` の積。`mul_add` は `acc + s * (d as f32)`。`finish` は恒等。`HbF64` は `mul_add` を `acc + (s as f64) * (d as f64)` とする。
- **`Ft` の算術**：`axis_factor` は、不正な領域で `peak` を0にしてから `tt_calculate_scalar` の分岐の順のまま、`mul_div(scalar, coord - start, peak - start)`（座標と領域の値は16.16。領域の2.14は `<< 2`）。`mul_add` は `acc + (d as i64) * (s as i64)`。`finish` は `((acc.wrapping_add(0x8000)) as i32) >> 16`。
- **gvarの組の係数**：`tuple_axis_factor` は、HarfBuzzの `calculate_scalar` の軸ごとの分岐（`peak == 0` なら掛けない、`v == 0` なら0、`v == peak` なら掛けない、中間の範囲の不正な組は掛けない、範囲の外は0、`peak != start` のときだけ比例）と、FreeTypeの `ft_var_apply_tuple` の分岐（`tuple == 0` なら掛けない、`ncv == 0` なら0、`tuple == ncv` なら掛けない、中間でなければ同じ符号で `|ncv| < |tuple|` のとき `FT_MulDiv(apply, ncv, tuple)`、中間なら `ncv <= start` か `ncv >= end` で0、ほかは比例）を、それぞれの方針が持つ。核は軸の順に掛け、0で打ち切る。
- **IUP**：HarfBuzzの `apply_deltas_to_points` の区間の探し方（輪郭ごとに、差分のある点の数が0か全部なら何もしない。差分のある点から次の差分のある点までの間の点を補う）を核が行う。`HbF32::iup_segment` は `(prev_val, next_val, prev_delta, next_delta)` を持ち、`iup_interpolate` は `infer_delta` のとおり（同じ座標なら差分が等しいときだけその差分、範囲の外なら近い側の差分、ほかは `prev_delta + r * (next_delta - prev_delta)`、`r = (t - prev) / (next - prev)`）。`Ft::iup_segment` は、参照点を座標の昇順に並べかえ、`scale = div_fix(out2 - out1, in2 - in1)`（`in1 == in2` なら0）を求め、`iup_interpolate` は `tt_delta_interpolate` のとおり（`in1 == in2` かつ `out1 != out2` なら動かさない、`t <= in1` なら `d1`、`t >= in2` なら `d2`、ほかは `out1 + mul_fix(t - in1, scale)` から元の値を引いた差分）。
- **`font_key`**：FNV-1a（64bit。オフセットの基底 `0xcbf29ce484222325`、素数 `0x100000001b3`）で、データの長さ、フェイスの番号、表の目録の各記録のタグ・オフセット・長さ、`head` の `checkSumAdjustment` を、この順に8バイトずつ（タグは4バイト）ビッグエンディアンで混ぜる。
- **浮動小数点**：`f32` と `f64` の四則演算だけを使い、`mul_add` を使わない（[ADR 0026](0026-harfbuzz-rounding.md)）。`i32` から `f32` への変換は、Rustの `as`（最近接偶数への丸め）で、Cの `(float)` と同じ。

## 実装しないこと

- 整数の算術でHarfBuzzの `f32` の結果を出すこと。単精度の丸めの模倣になり、`f32` を使うより遅く、利点が無い。
- 既定のインスタンスのための別の関数（`item_delta_default` など）。既定のインスタンスは `StoreScalars` が空になることで速くなり、関数は分けない。
- 領域の係数を遅延で求めること。不変の値では写せず、`Location` を作るときに全部求める。
- 送り幅と縦の原点のキャッシュ（HarfBuzzの `advance_cache`（`hb_cache_t<24, 16>`、1024バイト）、`origin_cache`）。これはグリフごとの結果の記憶で、差分の評価ではない。下の計測で、HVARのある送り幅がHarfBuzzより遅ければ、[ADR 0027](0027-variable-fonts.md) の「後回しにしたもの」のADRで決める。
- VARCのMultiItemVariationStoreとSparseVarRegionList（[ADR 0027](0027-variable-fonts.md) の13段階目）。核の `Arith` と `ScalarSource` は、そのときに同じ形で使う。
- HarfBuzzの `hb_budget_spend` による予算の打ち切り（`apply_deltas_to_points` の `budget`）。sofelは壊れたフォントをビューの検証で退ける。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。手で組むストアとgvarは、[ADR 0027](0027-variable-fonts.md) の組み立てを使う。

### 単体テスト

- **軸ごとの係数**：`HbF32::axis_factor` と `Ft::axis_factor` について、`peak == 0`、`coord == peak`、`coord == 0`、`coord == start`、`coord == end`、範囲の外、比例（1/3になる組み合わせ。`f32` の0x3EAAAAABと16.16の0x5555を期待値に書く）、不正な領域の3種（`start > peak`、`peak > end`、`start < 0 < end`）で座標が0のときと0でないとき（`start < 0 < end` で座標0は、HarfBuzzが0、FreeTypeが1になることを必ず含める）。期待値は、`VarRegionAxis::evaluate` と `tt_calculate_scalar` の式を手で計算して書く。
- **組の係数**：`tuple_axis_factor` について、同じ組み合わせと、中間の範囲つきの組で `start == peak` かつ座標が `start` のとき（HarfBuzzは掛けない、FreeTypeは0）。
- **`finish`**：`Ft::finish` で、和が `2^31` を超える値（例えば差分 `0x7FFFFFFF` × 係数 `0x10000` の2列）が、FreeTypeと同じく32bitに切り落とされること。`0x8000` の境界の前後。
- **`item_delta`**：8bit、16bit、32bitの列の混ざる行、`LONG_WORDS`、`wordDeltaCount == 0`、係数が0の列が先頭・中・末尾にある行、すべて0の行、`inner` が `itemCount` 以上、`outer` が `dataCount` 以上、`0xFFFF/0xFFFF`。`HbF32` の和の順が列の昇順であることを、丸めで順が見える値（`1e8` と `1` と `−1e8` の差分に係数1）で確かめる。
- **既定のインスタンス**：座標がすべて0の `StoreScalars` が、ピークがすべて0の領域だけ1で、`columns` が空になること。ピークがすべて0の領域を持つストアでは、`columns` にその列があり、差分が足されること。
- **`StoreScalars` の構築**：座標の数が軸の数より少ない・多いとき、領域の番号が領域の数以上のとき、`dataCount` が0のとき。
- **gvar**：[ADR 0027](0027-variable-fonts.md) のgvarの適用のテスト（組1つ、中間の範囲つき、共有と私用の点、IUPの各場合、幻の点、合成グリフ）を、`HbF32` と `Ft` の両方で、同じ手で組んだグリフに対して走らせる。期待値は両方の式を手で計算した値。加えて、「すべての点」の組と私用の点を持つ組が混ざるときの `flush` の順を、`f32` の和で順が見える値で確かめる。
- **`font_key`**：同じバイト列から同じ鍵、表の目録の1バイトの違いで違う鍵、フェイスの番号の違いで違う鍵。
- **`Send` と `Sync`**：`StoreScalars` に `fn assert<T: Send + Sync>()`。

### 性質のテスト

`sofel-font` のテストに、決まった種の疑似乱数（[ADR 0020](0020-shaping-foundation.md) の線形合同法）で生成した入力を使う性質のテストを置く。各性質につき、1000の入力を確かめる。

- **キャッシュありとなしの一致**：乱数で作ったストア（軸1〜8、領域0〜64、ItemVariationData 1〜8、列の幅の混在、不正な領域と範囲の外の領域の番号を含む）と乱数の座標（2.14の全範囲、16.16の±1の範囲。0と±1を多く含む）について、`StoreScalars` と `OnTheFly` の `item_delta` が、`HbF32`、`HbF64`、`Ft` のすべてで、ビットまで等しいこと（`f32` は `to_bits` で比べる）。
- **別のフォントへの適用**：フォントAで作った `Location` をフォントBの `Instance` に使った結果が、フォントBと同じ座標で作った `Location` の結果（`OnTheFly` の結果）と等しいこと。
- **gvarの `phantom_only`**：`phantom_only` の幻の点の差分が、全部の点に適用したときの幻の点の差分と、`HbF32` と `Ft` の両方で等しいこと。
- **FreeTypeの和の順**：`Ft` の `item_delta` と `gvar` の結果が、列や組の順を入れかえた同じ内容の入力でも等しいこと（整数の和が順によらないことの確認。HarfBuzzの順で足してよい根拠）。

### HarfBuzzとFreeTypeとの突き合わせ

[ADR 0027](0027-variable-fonts.md) の突き合わせ（位置の集まり、輪郭、送り幅、箱、メトリクス、基準線、シェーピング、COLRv1。すべて完全一致）が、この ADR の核の結果を確かめる。`hb-dump` と `ft-dump` に次を足す。

- `hb-dump --metrics-variation`：`hb_ot_metrics_get_variation` の `float` の値を、MVARのすべてのタグについて、`to_bits` の16進で書く。HarfBuzzが `f32` の差分をそのまま公開する唯一の問い合わせで、`HbF32` の和と丸めの前の値を直接比べられる。
- `ft-dump --advances`：`FT_Get_Advance`（`FT_LOAD_NO_SCALE`）と `FT_Get_Advances` の全グリフの値。HVARの整数の差分を `ft_round_and_shift16` の後で比べる。
- 位置の集まりは [ADR 0027](0027-variable-fonts.md) のままとし、`rand:<k>` の位置の生成で、各軸の値が最小・既定・最大になる確率を上げる（係数の0と1の分岐を通すため）。

### そのほか

- **決定性**：`sofel-font` の単体テストと性質のテストを、CIのLinux（x86-64）、macOS（aarch64）、`wasm32-wasip1`（wasmtime）で走らせる。加えて、突き合わせの対象のフォントについて、すべての位置の `item_delta::<HbF32>`（MVARの全タグ、HVARの全グリフ、GDEFの全VariationIndex）と `Ft` の結果を並べたバイト列のFNV-1aの値を `conformance/var-deltas.digest` に置き、3つの環境で同じ値になることを確かめる。
- **ファジング**：`fuzz/` に、任意のバイト列のストアと任意の座標で `StoreScalars` を作り、すべての `(outer, inner)` について `StoreScalars` と `OnTheFly` の結果が等しく、panicしないことを確かめる対象を足す。gvarは、任意のバイト列のgvarと任意の点の列で、`HbF32` と `Ft` の `apply` がpanicしない対象を足す。
- **`sofel-capi`**：Cのテストで、`sofel_instance_item_delta_hb` と `_ft` がRustの値と一致すること、`location` のNULLで既定のインスタンスの値（0）になること、`sofel_instance_blend_scalars_ft` の長さの返し方。
- **wasm**：`sofel-font` と `sofel-outline` の単体テストを `wasm32-wasip1` で通す。

## 計測

[ADR 0027](0027-variable-fonts.md) の計測の項目に足す。位置は `wght=500`、フォントはRobotoFlex-Variable（軸が多く、GDEFのストアの領域が多い）、SourceSerifVariable-Roman、AdobeVFPrototypeとする。

| 項目 | sofel | HarfBuzz | FreeType |
|---|---|---|---|
| 位置の作成（係数を含む） | `Location::new` | `hb_font_set_variations` と、続けて全ストアの `create_cache` を温める1回の問い合わせ | `FT_Set_Var_Design_Coordinates` と、続けて `regionScalars` を温める1回の問い合わせ |
| HVARの送り幅（位置あり、全グリフ） | `Instance::advance` | `hb_font_get_glyph_h_advance`（キャッシュが温まった2回目） | `FT_Get_Advance` |
| GDEFのVariationIndex（位置あり） | SourceSerifVariable-Romanと `react-dom.txt` のシェーピング | `hb_shape` | なし |
| gvar（位置あり、全グリフ） | `draw`（`Ft`）、`glyph_extents`（`HbF32`） | `hb_font_get_glyph_extents` | `FT_Load_Glyph` |
| 係数の出どころの比較 | `StoreScalars` と `OnTheFly` の `item_delta`（HVARの全グリフ） | なし | なし |

- 既定のインスタンスの既存の項目（送り幅、箱、輪郭、シェーピング）が、この ADR の前後で遅くならないことを確かめる。
- グリフごとの項目は、HarfBuzzとFreeTypeのキャッシュが温まった状態と比べて遅くないことを目標とする（[ADR 0006](0006-performance-measurement.md)）。位置の作成は、HarfBuzzの遅延のキャッシュのぶん、sofelが遅くなりうる。記録に残し、HVARの送り幅の全グリフの時間より長ければ、構築の手順を見なおす。
- `StoreScalars` と `OnTheFly` の比は、係数を持つ価値の記録として `docs/benchmarks/` に残す。

## トレードオフ

- 核を1つにし、算術を方針で分ける。`hb_var` と `ft_var` の2つの関数の組と、`Glyph::get_points` と `TT_Vary_Apply_Glyph_Deltas` の2つの移植に比べ、列の読み方、`flush`、IUPの区間の探し方を1度だけ書く。代わりに、型引数とトレイトのぶん読みにくくなり、基準の側のコードと行ごとに対応しなくなる。対応は、方針の関数に基準の関数の名前と行を書いて保つ。
- 軸ごとの係数の関数を方針が丸ごと持つ。分岐の順の違い（不正な領域で座標0のとき）があるためで、分岐を共有して算術だけ分けると、どちらかと一致しない。
- 係数を `Location` を作るときに全部求める。位置を作る費用が、ストアの領域の数の分だけ増える。位置を作る回数はグリフの数に比べて少なく、HarfBuzzの遅延のキャッシュも最初の問い合わせで同じ費用を払う。
- `Location` が `Font` の鍵を持ち、別のフォントでは係数を使わない。鍵の計算が `Font::new` に増える（表の目録の記録の数だけのFNV-1a）。鍵を持たない代わりに別のフォントへの使用を禁じる案は、禁じたことを検出できず、黙って違う値を返すので採らない。
- `ScalarSource` の実装を2つ持つ（`StoreScalars`、`OnTheFly`）。`OnTheFly` は公開せず、鍵の違いとavar版2とテストでだけ使う。
- 既定のインスタンスは、係数が空になることで速くなる。基準の側の「座標が0なら評価しない」の分岐は値ごとに写すので、分岐の数は減らない。
- 送り幅のキャッシュを持たない。HVARのある送り幅は、係数が0でない列の数だけの積和で、キャッシュの参照と同じ程度の費用を見込む。計測で確かめる。

## 参考文献

- HarfBuzz 15.1.0（`3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`）
  - `src/hb-ot-layout-common.hh`（`VarRegionAxis::evaluate` 2549行、`hb_scalar_cache_t` 2637行、`VarRegionList::evaluate_impl` 2731行、`VarData::_get_delta` 3074行、`VarData::get_region_scalars` 3126行、`ItemVariationStore::get_delta` 3572行、`ItemVarStoreInstancer` 4215行、`VariationDevice` 6286行）
  - `src/hb-ot-var-common.hh`（`TupleVariationHeader::calculate_scalar` 113行）、`src/hb-ot-var-gvar-table.hh`（`hb_glyf_scratch_t` 43行、`infer_delta` 665行、`apply_deltas_to_points` 769行）
  - `src/hb-ot-font.cc`（`hb_ot_font_advance_cache_t` 65行、`varStore_cache` 81行、`gvar` のキャッシュ 259行、`hb_ot_get_glyph_h_advances` 382行）、`src/hb-ot-hmtx-table.hh`（`get_advance_with_var_unscaled`）、`src/hb-ot-shape.cc`（`_hb_ot_shaper_font_data_create` 433行）、`src/hb-ot-var-avar-table.hh`（`map_coords` 545行）、`src/hb-cff2-interp-cs.hh`（146行、`blend_deltas` 187行）、`src/hb-ot-var-mvar-table.hh`（`get_var` 132行）、`src/hb-ot-metrics.cc`（`GET_VAR` 68行、`hb_ot_metrics_get_variation` 412行）、`src/OT/glyf/glyf.hh`（481行）、`src/OT/glyf/Glyph.hh`（357行、421行）、`src/hb-algs.hh`（`_hb_roundf`）
- FreeType 2.15.0（`VER-2-15-0`、`37515cf56ba3349656049edbed89ffc6f6466f2d`）
  - `src/truetype/ttgxvar.c`（固定小数点のマクロ 73〜82行、`ft_var_readpackeddeltas` 277行、領域の読み込みの `peak = 0` 631行、`tt_calculate_scalar` 1035行、`ft_mul_add_delta_scalar` 1080行、`ft_round_and_shift16` 1164行、`tt_var_get_item_delta` 1195行、`tuplescalars` の初期化 2172行、`ft_var_apply_tuple` 2288行、avar版2 2485行、`tt_delta_shift` 4266行、`tt_delta_interpolate` 4303行、`tt_interpolate_deltas` 4371行、`TT_Vary_Apply_Glyph_Deltas` 4476行）
  - `include/freetype/internal/ftmmtypes.h`（`FT_ItemVarDelta` 27行、`GX_ItemVarStoreRec` 64行）、`src/cff/cffload.c`（領域の読み込みの `peak = 0` 1220行、`cff_blend_doBlend` 1303行、`cff_blend_build_vector` 1411行）
- fontations（`e5e7f433`、2026-11-06）：`read-fonts/src/tables/variations.rs`（`compute_delta_with_scalars` 1581行、`compute_scalars` 1645行、`compute_scalar` 1666行）
- HarfRust（`043820d5`、2026-11-08）：`harfrust/src/ot/mod.rs`（`compute_delta` 518行）
- sofel：`crates/sofel-read/src/item_variation.rs`、`crates/sofel-read/src/cff/blend.rs`（`FtBlend`）、`crates/sofel-geom/src/fixed.rs`（`mul_fix`、`div_fix`、`mul_div`）
- OpenType Specification：OpenType Font Variations Common Table Formats（ItemVariationStore、DeltaSetIndexMap、TupleVariationStore） https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats
- IEEE 754-2019（単精度の四則演算の丸め）、WebAssembly Core Specification（`f32` の演算の意味） https://webassembly.github.io/spec/core/exec/numerics.html
- FNV-1a http://www.isthe.com/chongo/tech/comp/fnv/
