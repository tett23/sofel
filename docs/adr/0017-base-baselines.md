# ADR 0017: BASEの表と基準線（`baseline_with_fallback`）

ステータス: 採択（未実装）

## 文脈

Golemは、縦組みで欧文を回転させるときや、ダッシュの位置を決めるときに、仮想ボディ（ideographic em-box）の中心を使う（Golemの `docs/golem-driver/adr/0015-vertical-dashes.md`）。
いまは、HarfBuzzの `hb_ot_layout_get_baseline_with_fallback` を、基準線 `Idce`（`HB_OT_LAYOUT_BASELINE_TAG_IDEO_EMBOX_CENTRAL`）、横書き、文字体系のタグ `hani` で呼んでいる（`modules/golem-font/cbits/golem/golem_font.c`）。

HarfBuzz 15.1.0の決め方は、次のとおりである（`src/hb-ot-layout-base-table.hh`、`src/hb-ot-layout.cc`）。

- **BASEの表から引く**（`hb_ot_layout_get_baseline`）：方向が横ならHorizAxis、縦ならVertAxisを使う。軸のBaseScriptListから文字体系のタグを二分探索し、無ければ `DFLT` を探す。見つけたBaseScriptにBaseValuesが無ければ、引けない。BaseTagListから基準線のタグを二分探索し、その位置のBaseCoordを使う。言語のタグは使わない。
- **BaseCoordの値**：形式1は `coordinate`。形式2と4（4は24bitのグリフID）は `coordinate` だけを使い、参照のグリフの点は使わない（HarfBuzzは未実装）。形式3は `coordinate` にDeviceの差分を足す。Deviceの差分は、ヒンティングのDeviceではppemが要り、VariationIndexでは可変フォントの座標が要る。拡大縮小なしでppemを与えず、既定のインスタンスで使う場合は、どちらも0になる。
- **代わりの値**（`hb_ot_layout_get_baseline_with_fallback`、CSS Inline Layout 3の基準線の合成に従う）：
  - `romn`：0。
  - `math`：横書きで、U+2212か `-` のグリフの箱が取れれば `y_bearing + height / 2`。取れなければ、xHeight（OS/2の版2以上の `sxHeight`、無ければ `x` のグリフの `y_bearing`、無ければ `unitsPerEm / 2`）の半分。
  - `idtp`（仮想ボディの上か右）と `ideo`（仮想ボディの下か左）：反対側の端が表にあれば、それに±`unitsPerEm`。無ければ、その方向のフォント全体の値（[ADR 0016](0016-font-and-vertical-metrics.md) の横か縦のアセンダとディセンダ、代わりの値を含む）。
  - `icft`、`icfb`（字面の上か右と、下か左）：仮想ボディの上下から、高さの10分の1だけ内側。
  - `Idce`（仮想ボディの中心）：仮想ボディの上と下（それぞれ代わりの値を含む）の和の半分（64bitで足して2で割り、0へ切り捨て）。
  - `Icfc`（字面の中心）：字面の上と下の和の半分。
  - `hang`：横書きで、文字体系ごとに決めた文字（デーヴァナーガリーのU+0915など）のグリフの箱の `y_bearing`。無ければ `unitsPerEm × 6 / 10`。縦書きでも `unitsPerEm × 6 / 10`。

## 実装すること

### `sofel-read`

- BASEの表のビューと検証器を足す（[ADR 0007](0007-sfnt-read-foundation.md)）。版1.0と1.1（ItemVariationStoreへのオフセット）、Axis、BaseTagList、BaseScriptList、BaseScript、BaseValues、BaseCoordの形式1〜4、MinMaxとFeatMinMaxの記録、Deviceの表の見出し（形式1〜3とVariationIndex）。
- 検証器は、範囲の外のオフセット、BaseTagListとBaseScriptListの記録がタグの昇順でないこと、BaseValuesの数とBaseTagListの数の不一致、未知のBaseCoordの形式、`defaultBaselineIndex` がタグの数以上であることを報告する。

### `sofel-font`

- `Font::baseline(tag, direction, script) -> Option<i32>`：HarfBuzzの `hb_ot_layout_get_baseline` と同じ値。可変フォントの座標はまだ持たないため、Deviceの差分は常に0とする。
- `Font::baseline_with_fallback(tag, direction, script) -> i32`：HarfBuzzの `hb_ot_layout_get_baseline_with_fallback` と同じ値。上の代わりの値をすべて実装する。
  - `math` と `hang` の代わりの値が使うグリフの箱は、[ADR 0016](0016-font-and-vertical-metrics.md) の `glyph_extents` を使う。CFFなどの箱は、その輪郭のADRが箱を足した時点で同じ値になる。
  - `hang` の文字体系の表は、HarfBuzzの `hb_ot_tag_to_script` が返す文字体系ごとに、OpenTypeの文字体系のタグ（新旧の両方。`deva` と `dev2` など）から引く小さな表として持つ。[ADR 0019](0019-unicode-character-data.md) の文字体系の型ができたら、それに置きかえる。
- `Font::layout_font_extents(direction, script, language) -> (FontExtents, bool)`：HarfBuzzの `hb_ot_layout_get_font_extents`。BASEのMinMax（言語のMinMaxがあればそれ、無ければ既定のMinMax）から、アセンダを最大、ディセンダを最小、行間を0とする。引けなければ、[ADR 0016](0016-font-and-vertical-metrics.md) のその方向のフォント全体の値を返し、`false` とする。HarfBuzzが言語のタグを使うのはこの関数だけで、FeatMinMax（機能ごとの値）は使わない。
- 値はフォントの単位で返し、拡大縮小は [ADR 0016](0016-font-and-vertical-metrics.md) の `HbScale` で行う。HarfBuzzと同じく、BaseCoordには `em_scale` を使う。
- 方向は `Direction`（横と縦の2つ）として `sofel-font` に置く。シェーピングの方向（[ADR 0020](0020-shaping-foundation.md) の4つの方向）は、これに写す。
- 基準線のタグは `Tag` で受け取り、HarfBuzzの `hb_ot_layout_baseline_tag_t` の値（`romn`、`hang`、`icfb`、`icft`、`Icfc`、`ideo`、`idtp`、`Idce`、`math`）を定数として持つ。

### `sofel-capi`

[ADR 0015](0015-c-abi.md) の決まりに従い、`sofel_font_baseline`、`sofel_font_baseline_with_fallback`、`sofel_font_layout_font_extents` を足す。Golemの `golem_font_embox_central` は、`sofel_font_baseline_with_fallback(font, 'Idce', 横, 'hani')` で置きかえられる。

## 実装しないこと

- FeatMinMax（機能ごとの最小と最大）の問い合わせ。HarfBuzzにも公開の問い合わせが無い。ビューと検証器は作る。
- 参照のグリフの点を使うBaseCoordの形式2と4の解釈。HarfBuzzと同じく `coordinate` だけを使う。
- Deviceの差分（ppemを与えたヒンティングの差分と、可変フォントの差分）。

## 後回しにしたもの

この ADR の実装が済んだら、次のADRを起こす。

- **Deviceの表の差分**：ppemを与えたときのヒンティングのDevice（`get_delta_pixels`）と、可変フォントのVariationIndex（3段階目のItemVariationStore）。GPOS（[ADR 0020](0020-shaping-foundation.md)）、GDEFのキャレット、BASE、MATHが共有するので、1つのADRで決める。
- **MATHとJSTFの問い合わせ**：MATHの表のビューと検証器は、サブセット化（[ADR 0022](0022-subsetting.md)）が足す。その上に、HarfBuzzの `hb_ot_math_*` のすべて（定数、イタリック補正、上付きの位置、伸びるグリフの組み立て）を `sofel-font` の問い合わせとして足す。JSTFはHarfBuzzも読まないため、範囲に入れるかをそのADRで決める。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- **表の読み取りと検証器**：[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方で、横と縦の軸、`DFLT` だけ、形式1〜4のBaseCoord、範囲の外のオフセット、並びの乱れ、数の不一致のBASEを作り、値と報告される誤りを確かめる。ファジングの対象にBASEを足す。
- **MinMax**：言語のMinMaxと既定のMinMax、MinMaxの無いBaseScript（フォント全体の値に落ちる）で、`layout_font_extents` を確かめ、`hb-dump` で `hb_ot_layout_get_font_extents` と全フォントで突き合わせる。
- **引き方の単体テスト**：文字体系のタグがあるとき、無くて `DFLT` に落ちるとき、BaseValuesが無いとき（引けない）、基準線のタグが無いとき（引けない）。
- **代わりの値の単体テスト**：基準線のタグごとに、表に反対側の値があるときと無いとき、横と縦、`math` のU+2212、`-`、xHeightのそれぞれの経路、`hang` の文字がある文字体系と無い文字体系。
- **HarfBuzzとの突き合わせ**：[ADR 0016](0016-font-and-vertical-metrics.md) の `tools/hb-dump/` に、基準線の書き出しを足す。HarfBuzzのテストのフォントすべてについて、9つの基準線、横と縦、文字体系のタグ（`hani`、`latn`、`deva`、`DFLT`、BASEに現れるすべてのタグ）の組で、`hb_ot_layout_get_baseline` の有無と値、`hb_ot_layout_get_baseline_with_fallback` の値を書き出し、完全一致を確かめる（[ADR 0005](0005-conformance-tests.md)）。BASEを持つフォントがテストのデータに足りなければ、HarfBuzzの `test/api/fonts/base*.ttf` などを `testdata.lock` の範囲から選ぶ。
- **C ABI**：[ADR 0015](0015-c-abi.md) のCのテストに、足した関数を加える。

## トレードオフ

- HarfBuzzの未実装（形式2と4の参照の点）も写すため、仕様どおりの値にならない場合がある。HarfBuzzとの一致を基準とする方針（[ADR 0005](0005-conformance-tests.md)）を優先する。
- `hang` の文字体系の表を一時的に `sofel-font` に持つため、[ADR 0019](0019-unicode-character-data.md) の後で置きかえる手間が残る。代わりに、Golemに要る基準線を、シェーピングより先に使えるようにする。
- 方向の型を `sofel-font` と `sofel-shape` で分けて持つ。基準線に要るのは横か縦かだけで、シェーピングの4つの方向を `sofel-font` に持ちこむと、シェーピングの概念が下の層に漏れる。

## 参考文献

- HarfBuzz `src/hb-ot-layout-base-table.hh`、`src/hb-ot-layout.cc`（`hb_ot_layout_get_baseline`、`hb_ot_layout_get_baseline_with_fallback`）、`src/hb-ot-layout.h`、`src/hb-ot-metrics.cc`（`HB_OT_METRICS_TAG_X_HEIGHT`）、`src/hb-font.hh`（`get_extents_for_direction`）（15.1.0）
- OpenType仕様 BASE https://learn.microsoft.com/typography/opentype/spec/base
- CSS Inline Layout Module Level 3, "Synthesizing Baselines" https://www.w3.org/TR/css-inline-3/#baseline-synthesis-fonts
- Golem `modules/golem-font/cbits/golem/golem_font.c`（`golem_font_embox_central`）
