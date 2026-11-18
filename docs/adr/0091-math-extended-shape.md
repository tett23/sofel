# ADR 0091: MATH表の採否と伸長形の判定をHarfBuzzに合わせる（[ADR 0062](0062-layout-query-device-adjustments.md)の補足）

Status: proposed

## 文脈

[ADR 0062](0062-layout-query-device-adjustments.md) は、MATH表の有無と、グリフが伸長形（MATH表の ExtendedShapeCoverage に登録されたグリフ）かを公開すると決めた。
不変条件1は、値がHarfBuzzと一致することを求める。
しかし、表をいつ「無い」とみなすかを決めておらず、MATH表の有無と伸長形の判定を確かめるテストも無い。

この ADR は、[ADR 0062](0062-layout-query-device-adjustments.md) の「実装すること」のうち、MATH表の有無と伸長形の判定について、表の採否の決まりとテストを補う。
問い合わせの関数は [ADR 0062](0062-layout-query-device-adjustments.md) の実装のものを使い、新しい関数は足さない。

## 調査した事項

### sofelのいまの実装

- `LayoutQuery::has_math`（`crates/sofel-font/src/layout_query.rs:173`）と `LayoutQuery::math_is_extended_shape`（同 `:444`）がある。どちらも内部の `math()`（同 `:96-99`）を使う。`math()` は `sofel_read::math::Math::new` で、10バイトの見出しとメジャーバージョンだけを確かめる。
- C ABI には `sofel_math_has_data`（`crates/sofel-capi/src/layout_query.rs:163`）と `sofel_math_is_extended_shape`（同 `:199`）がある。
- MATH表の有無と伸長形を、HarfBuzzと突き合わせるテストは無い（`rg -ln 'MathTestFont|hb_ot_math' crates/sofel-conformance tools/hb-dump` は該当なし）。

### HarfBuzz 15.1.0

`target/bench-c/aarch64-apple-darwin/libharfbuzz.a`（`bench/build-c.sh` が作るもの）とリンクしたCのプログラムで、`hb_ot_math_has_data` と `hb_ot_math_is_glyph_extended_shape` を呼んだ。

HarfBuzzの試験フォント（`test/api/fonts/`）の結果は次のとおりである。

| フォント | 表の有無 | グリフの数 | 伸長形のグリフ |
| --- | --- | --- | --- |
| `MathTestFontEmpty.otf` | 真 | 2 | なし |
| `MathTestFontFull.otf` | 真 | 142 | 3、6、9、12、15、18、21、24、27 |
| `MathTestFontNone.otf` | 偽 | 2 | なし |
| `MathTestFontPartial1.otf`〜`Partial4.otf` | 真 | 各2 | なし |
| `MathTestFontPretendToBeCambria.ttf` | 真 | 2 | なし |
| `math-variations.ttf` | 真 | 3 | なし |

`hb_face_builder` でMATH表だけのフェイスを組んだ結果は次のとおりである。

| MATH表 | 表の有無 | グリフ1 | グリフ3 |
| --- | --- | --- | --- |
| 版1.0の10バイトの見出しで、オフセットがすべて0 | 真 | 偽 | 偽 |
| MathGlyphInfo の ExtendedShapeCoverage（形式1）にグリフ3だけを登録 | 真 | 偽 | 真 |
| 上の表で、MathVariants へのオフセットだけを範囲の外にする | 偽 | 偽 | 偽 |
| メジャーバージョンが2 | 偽 | 偽 | 偽 |
| 9バイトの見出し | 偽 | 偽 | 偽 |

MathVariants へのオフセットが範囲の外の表を、sofelのいまの `math()` は採る（見出ししか確かめないため）。HarfBuzzは、表の検査（sanitize）に失敗した表を全体として捨てるので、表の有無も伸長形も偽を返す。

## 実装すること

- `sofel-font` の内部の `math()` を、HarfBuzzの `OT::MATH::sanitize`（`src/hb-ot-math-table.hh`）と同じ検査に通った表だけを採るように改める。MathConstants、MathGlyphInfo（ItalicsCorrectionInfo、TopAccentAttachment、ExtendedShapeCoverage、MathKernInfo）、MathVariants の各構造と配列が表に収まるかを、オフセットをたどって確かめる。処理の量は、BASE表の検査（[ADR 0017](0017-base-baselines.md)）と同じ上限で止める。
- 検査に失敗した表は、無いものとして扱う。MATHを使う問い合わせ（[ADR 0062](0062-layout-query-device-adjustments.md) のMATHの定数、カーニング、異体グリフ、組み立て部品、イタリック補正、アクセントの位置、接続部の最小の重なり、伸長形、表の有無）は、すべてこの判定を共有する。
- HarfBuzzが受け付ける未知のCoverageの形式は、空の集合として扱う。
- 公開の関数の名前と形は変えない。C ABIの `sofel_math_has_data` と `sofel_math_is_extended_shape` の形も変えない。NULLの扱いは [ADR 0015](0015-c-abi.md) のとおりとする。
- `tools/hb-dump/` に、フォントのMATH表の有無と、全グリフの伸長形を書き出す形を足し、`sofel-conformance` で突き合わせる。
- この ADR の実装のプルリクエストで、[ADR 0062](0062-layout-query-device-adjustments.md) のステータスの行を `superseded [{"part":"実装すること","by":["docs/adr/0091-math-extended-shape.md"],"kind":"supplement"}]` に改める。

## 実装しないこと

- 新しい問い合わせの関数（MATHのグリフ用のビューなど）。[ADR 0062](0062-layout-query-device-adjustments.md) の関数で足りる。
- JSTF表。
- 表の検査の結果を `Font` の中に持つこと（[ADR 0003](0003-crate-layers.md) は内部可変性を禁じる）。問い合わせのたびに検査する。

## 不変条件

- I1: MATH表の有無（`LayoutQuery::has_math`）は、同じフェイスの `hb_ot_math_has_data` と等しい。
- I2: 任意のグリフIDについて、伸長形の判定（`LayoutQuery::math_is_extended_shape`）は `hb_ot_math_is_glyph_extended_shape` と等しい。
- I3: C ABIの関数が成功したときの出力はRustの値と等しく、引数の誤りで失敗したときは出力を書きかえない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| MATH表なし | 表の有無は偽、伸長形はすべて偽 | I1, I2 |
| 9バイトの見出し、メジャーバージョン2 | 表の有無は偽、伸長形はすべて偽 | I1, I2 |
| 版1.0の10バイトの見出しで、オフセットがすべて0 | 表の有無は真、伸長形はすべて偽 | I1, I2 |
| ExtendedShapeCoverage にグリフ3だけ | 表の有無は真、グリフ3だけ真 | I1, I2 |
| 上の表で、MathVariants へのオフセットだけが範囲の外 | 表の有無は偽、グリフ3も偽 | I1, I2 |
| グリフIDがグリフの数以上、0xFFFF | 伸長形は偽 | I2 |
| NULLのフォント、NULLの出力 | 誤りを返し、出力は変わらない | I3 |

## 受けいれ条件

- A1: HarfBuzzの試験フォント（`test/api/fonts/` の `MathTestFont*` と `math-variations.ttf` の9個と、MATH表を持つすべてのテストのフォント）の全グリフで、I1 と I2 が成り立つ。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。テストの名前は `adr0091_i1_...` の形とする。

- 不変条件の表の各行を、[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方で作ったフォントで確かめる。Coverageの形式1と2、空の集合、未知の形式も含める。範囲の外の MathVariants の行は、いまの実装で失敗する。
- A1：`hb-dump` のMATHの書き出しと、sofelの値を全グリフで比べる。表を壊したフォント（MathConstants、MathGlyphInfo、MathVariants のそれぞれのオフセットを範囲の外にしたもの）も比べる。
- I3：C ABIのテストで、有効な入力、NULLのフォント、NULLの出力（出力の番兵の値が残ること）を確かめる。

## トレードオフ

- 問い合わせのたびに表を検査するので、多くのグリフを調べる呼び出し側では、同じ検査を繰り返す。検査の費用はMATH表の大きさに比例し、上限で止まる。計測で遅いと分かったら、呼び出し側が持つ検査済みの値をADRで足す。
- 表の一部（MathVariants など）が壊れているだけで、伸長形の判定のように関係の無い問い合わせも偽になる。HarfBuzzとの一致を優先する（[ADR 0005](0005-conformance-tests.md)）。

## 参考文献

- HarfBuzz `src/hb-ot-math.cc`（`hb_ot_math_has_data`、`hb_ot_math_is_glyph_extended_shape`）、`src/hb-ot-math-table.hh`（`OT::MATH::sanitize`）、`test/api/test-ot-math.c`、`test/api/fonts/MathTestFont*.otf`（15.1.0）
- OpenType仕様 MATH https://learn.microsoft.com/typography/opentype/spec/math
