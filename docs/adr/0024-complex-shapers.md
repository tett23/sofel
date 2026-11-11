# ADR 0024: 文字体系ごとのシェーパー（arabic、hebrew、hangul、thai、indic、khmer、myanmar、USE）

ステータス: 採択

## 文脈

[ADR 0020](0020-shaping-foundation.md) は、シェーピングの土台と既定のシェーパーを定め、計画が `hb_ot_shaper_categorize` と同じ判定でシェーパーの種類を決めるとした。
sofelは汎用のライブラリとして、HarfBuzz 15.1.0のOTシェーパーの全体を範囲とする（[ADR 0001](0001-project-goal-and-scope.md)）。この ADR は、残りのシェーパーを定める。

HarfBuzz 15.1.0のシェーパーは、`hb_ot_shaper_t`（`src/hb-ot-shaper.hh`）の差しこみ口の組である：機能の集め方（`collect_features`、`override_features`）、計画のデータ（`data_create`）、前処理（`preprocess_text`）、後処理（`postprocess_glyphs`）、分解と合成（`decompose`、`compose`）、マスクの設定（`setup_masks`）、マークの並べかえ（`reorder_marks`）、GPOSを使う条件（`gpos_tag`）、正規化のモード、マークの幅の0化の時期、フォールバックの配置の有無。

| シェーパー | ファイル | 正規化 | マークの幅の0化 | フォールバックの配置 |
|---|---|---|---|---|
| arabic | `hb-ot-shaper-arabic.cc`、`-arabic-fallback.hh`、`-arabic-win1256.hh`、`-arabic-table.hh`、`-arabic-joining-list.hh` | `AUTO` | GDEFで遅く | あり |
| hebrew | `hb-ot-shaper-hebrew.cc` | `AUTO` | GDEFで遅く | あり（`gpos_tag` は `hebr`） |
| hangul | `hb-ot-shaper-hangul.cc` | `NONE` | しない | あり |
| thai | `hb-ot-shaper-thai.cc` | `AUTO` | GDEFで遅く | なし |
| indic | `hb-ot-shaper-indic.cc`、`-indic-table.cc`、`-indic-machine.rl` | `COMPOSED_DIACRITICS_NO_SHORT_CIRCUIT` | しない | なし |
| khmer | `hb-ot-shaper-khmer.cc`、`-khmer-machine.rl` | 同上 | しない | なし |
| myanmar | `hb-ot-shaper-myanmar.cc`、`-myanmar-machine.rl` | 同上 | GDEFで早く | なし |
| myanmar_zawgyi | 同上 | `NONE` | しない | なし |
| USE | `hb-ot-shaper-use.cc`、`-use-table.hh`、`-use-machine.rl` | `COMPOSED_DIACRITICS_NO_SHORT_CIRCUIT` | GDEFで早く | なし |

- 振り分け（`hb_ot_shaper_categorize`）：Arabic、Syriacは、横書きでGSUBの文字体系が見つかるか文字体系がArabicのときarabic、それ以外はdefault。Thai、Laoはthai。Hangulはhangul。Hebrewはhebrew。インドの9つの文字体系（Bengali、Devanagari、Gujarati、Gurmukhi、Kannada、Malayalam、Oriya、Tamil、Telugu）は、GSUBの文字体系が `DFLT` か `latn` ならdefault、新しい仕様の3版のタグ（`dev3` など）ならUSE、そうでなければindic（古い仕様の `deva` と2版の `dev2` の両方）。Khmerはkhmer。Myanmarは、`DFLT`、`latn`、`mymr` ならdefault、そうでなければmyanmar。`Qaag`（Zawgyi）はmyanmar_zawgyi。Tibetan、Mongolian、Sinhala、N'Ko、Phags-paなど、Unicode 18.0までに加わった多くの文字体系は、`DFLT` か `latn` ならdefault、そうでなければUSE。
- 文字の分類の表は、UCDとMicrosoftのUSEの仕様の追加のデータから生成される（[ADR 0019](0019-unicode-character-data.md)）。
- indic、khmer、myanmar、USEは、分類の列を音節に区切る状態機械を持つ。状態機械はRagelの文法（`*-machine.rl`）で書かれ、Ragelが生成したCのコード（`*-machine.hh`）をコミットしている。4つの文法は、和、連接、`?`、`*`、`+`、回数の指定、括弧、名前の定義、`any`、最長一致の走査子（`|* ... *|`）だけを使う。
- 音節の扱いの共通部分（壊れた音節への点線の円の挿入）は `hb-ot-shaper-syllabic.cc`、母音の並びの禁止（点線の円を挟む）は `hb-ot-shaper-vowel-constraints.cc` にある。

## 実装すること

### 共通

- すべて `sofel-shape` に置き、[ADR 0020](0020-shaping-foundation.md) の差しこみ口に、HarfBuzzと同じ位置と順序で差しこむ。
- 各シェーパーの機能の集め方（機能、フラグ、段、段の関数）、計画のデータ、前処理、後処理、分解と合成、マスク、マークの並べかえを、HarfBuzzと同じ規則で書く。HarfBuzzのMITの流用として表示する（[ADR 0002](0002-license-and-reuse.md)）。
- 文字の分類の表は、[ADR 0019](0019-unicode-character-data.md) の生成の道具が `crates/sofel-shape/src/tables/` に書く。
- シェーパーごとの値（分類、位置、音節の番号、結合の形の動作）は、[ADR 0020](0020-shaping-foundation.md) のバッファの要素の名前のついた欄に置く。
- 計画のデータ（アラビア文字のフォールバックのlookup、indicの文字体系ごとの設定と仮想の字形のグリフなど）は、`ShapePlan` の中の不変の値とする。HarfBuzzが計画のデータに遅延して作る値（indicの `virama` のグリフなど）も、計画を作るときに作る（[ADR 0003](0003-crate-layers.md)）。

### arabic

- 結合の型による形の選択（`isol`、`fina`、`fin2`、`fin3`、`medi`、`med2`、`init`）。前後の文脈（[ADR 0020](0020-shaping-foundation.md) のバッファの文脈）も結合の判定に使う。
- 機能の段（`stch`、`ccmp`、`locl`、形の機能、`rlig`、`calt`、`mset` など）と、`rlig` と `calt` の手での順序。
- `stch`（シリア文字の伸ばし）の後処理と、`SAFE_TO_INSERT_TATWEEL` のフラグ。
- 結合クラスの付けかえ後のマークの並べかえ（`reorder_marks_arabic`。UAX #53の変更子の結合のマークの移動）。
- モンゴル文字の自由な異体字セレクタ（U+180B〜U+180D、U+180F）の結合の扱い。
- フォールバックの形の合成（`hb-ot-shaper-arabic-fallback.hh`）：フォントのGSUBに形の機能が無いとき、Unicodeの表示形（U+FB50〜U+FEFF）とcmapから、単一の置換と合字の置換のlookupを計画を作るときに組み立てて適用する。Windows-1256の旧来のフォントのための表（`hb-ot-shaper-arabic-win1256.hh`）も、Rustの表に写して同じく使う。
- USEに送られる結合する文字体系（N'Ko、Mongolian、Phags-paなど。結合する文字体系の一覧は [ADR 0019](0019-unicode-character-data.md) の生成の表）の結合の型の設定は、HarfBuzzと同じくarabicの関数をUSEから呼ぶ。

### hebrew

- フォントにGPOSのマークの配置が無いとき、表示形（U+FB1D〜U+FB4F）への合成で代える（`compose_hebrew`）。
- `gpos_tag` が `hebr` で、GPOSの文字体系が違えばGPOSを使わない判定。

### hangul

- 字母の列の音節への合成と分解（前処理）、字母の形の機能（`ljmo`、`vjmo`、`tjmo`）のマスク、声調の印の移動。正規化は `NONE`。

### thai

- `SARA AM`（U+0E33、U+0EB3）の分解と並べかえ（前処理）。
- GSUBが無い旧来のフォントでの、WindowsとMacの私用領域の字形への置きかえ（`thai_pua_shape`）。

### indic、khmer、myanmar

- 音節の区切り（下の「音節の状態機械」）、壊れた音節への点線の円の挿入（`hb-ot-shaper-syllabic.cc`）、母音の並びの禁止（`hb-ot-shaper-vowel-constraints.cc`）。
- indic：9つの文字体系の設定（`indic_configs`。virama、基底の位置、reph の位置と形、pref の扱い、blwf の扱い）、古い仕様（`deva` など）と新しい仕様（`dev2` など）の違い、最初の並べかえ（基底の子音の決定、reph、pref、マトラの移動）と最後の並べかえ、機能のマスク（`nukt`、`akhn`、`rphf`、`rkrf`、`pref`、`blwf`、`abvf`、`half`、`pstf`、`vatu`、`cjct`、`init`、`pres`、`abvs`、`blws`、`psts`、`haln`）、マトラの分解（`decompose_indic`）と合成の禁止、シンハラ文字、タミル文字、マラヤーラム文字のドットのreph などの個別の規則。
- khmer：Coeng と Robatic、前置の母音の並べかえ、機能のマスク。
- myanmar：前置の母音、medial ra、asat の並べかえ。zawgyi は機械を使わず、正規化と並べかえをしない。

### USE

- USEの分類（[ADR 0019](0019-unicode-character-data.md) の生成の表）による音節の区切り、並べかえ（repha、前置の要素）、機能の段（基本の機能、位置の機能、表示の機能）、結合する文字体系の結合の型。
- 状態機械に入れるまえに、既定で無視する文字を飛ばす規則と、分類の付けかえ（HarfBuzzの `setup_syllables_use` の前処理）を同じとする。

### 音節の状態機械

- Ragelは使わない。Ragelの生成したコード（`*-machine.hh`）も写さない。
- `tools/gen-machine/`（Rust、ワークスペースの外、外部のクレートに依存しない）を書く。正規表現の文法（Ragelの文法のうち、上の4つの機械が使う部分）を読み、NFAからDFAを作り、最小化して、状態の遷移の表を `crates/sofel-shape/src/tables/machine_<名前>.rs` に書く。生成したファイルはコミットする。
- 文法は、HarfBuzzの `*-machine.rl` から、分類の番号の定義と規則の部分だけを写したファイル（`tools/gen-machine/grammars/<名前>.machine`）とする。動作（`found_syllable` など）は、規則の番号で表す。HarfBuzzのMITの流用として表示する。
- 走査子の意味はRagelと同じとする：入力の位置から、規則のどれかに合う最長の列を取り、同じ長さなら先に書いた規則を選ぶ。どれにも合わなければ1文字を `other` とする（4つの文法はどれも `other = any` を最後に置く）。
- 実行は、表を引くだけの関数を手で書く。音節の番号（HarfBuzzと同じく4bitで巡回）と型をバッファの要素に書く。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- 状態機械の生成：`tools/gen-machine/` の単体テストで、文法の各構文（和、連接、`?`、`*`、`+`、回数、括弧、名前）の小さな例について、受理する列と受理しない列を確かめる。最長一致と、同じ長さでの規則の優先を確かめる。DFAの最小化の前後で、長さ6までの全列の結果が同じことを確かめる。
- 状態機械のHarfBuzzとの突き合わせ：[ADR 0016](0016-font-and-vertical-metrics.md) の `tools/hb-dump/` に `--syllables <機械>` を足す。`src/harfbuzz.cc` を取りこむC++の翻訳単位（[ADR 0019](0019-unicode-character-data.md)）で、各機械の `find_syllables_*` を、分類を直接書いたバッファに対して走らせ、音節の境界と型を書き出す。入力の列は、各機械の分類の値の、長さ1〜4のすべての列と、決まった種の疑似乱数による長さ5〜32の列100万個とする。`sofel-conformance` で完全一致を確かめる。
- 分類の表：[ADR 0019](0019-unicode-character-data.md) の全数の比較で確かめる。
- 各シェーパーの規則：手で組んだフォント（[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方で作るGSUBとGPOSを持つフォント）で、次を確かめる。
  - arabic：結合の形の選択（文脈を含む）、`stch`、マークの並べかえ、モンゴル文字の異体字セレクタ、フォールバックの形の合成で組み立てたlookupの中身。
  - hebrew：表示形への合成。hangul：字母の合成と分解、マスク。thai：`SARA AM` と私用領域の置きかえ。
  - indic：9つの文字体系の設定ごとの基底の決定と並べかえ。khmer、myanmar、USE：並べかえと点線の円。
- 実際のフォントでの一致：[ADR 0020](0020-shaping-foundation.md) の適合テストで、各シェーパーの実装のプルリクエストごとに、そのシェーパーの `shaper:<名前>` の行が一覧から消えることを確かめる。`in-house` の `arabic-*`、`hebrew-*`、`hangul-*`、`indic-*`、`khmer-*`、`myanmar-*`、`use-*`、`sara-am`、`sinhala`、`tibetan-*`、`mongolian-variation-selector`、`text-rendering-tests` の `SHARAN`、`SHBALI`、`SHKNDA`、`SHLANA` などが対象となる。
- 文字体系ごとの網羅：[ADR 0020](0020-shaping-foundation.md) の `--shape-random` と、`perf/benchmark-shape.cc` の組（Devanagari、Arabic、Nastaliq、Duployan）の `--shape` の突き合わせが、各シェーパーの実装で通ることを確かめる。
- 計測：各シェーパーの実装のプルリクエストで、[ADR 0020](0020-shaping-foundation.md) の「シェーピング」の項目のうち、そのシェーパーを使う組を有効にする。

## 実装しないこと

- AATのフォントで使うdumberのシェーパー（[ADR 0020](0020-shaping-foundation.md) の「後回しにしたもの」のAATのシェーピング）。
- Ragelそのものの移植と、この4つの機械が使わないRagelの構文（動作の埋めこみ、優先度の演算子、否定など）。
- HarfBuzzのビルドの切りかえ（`HB_NO_OT_SHAPER_*`）に当たる、シェーパーを外すfeature。

## 実装の順序

各シェーパーを別のプルリクエストとし、次の順に実装する。依存の少ないものと、利用者の多い文字体系を先にする。

1. 音節の状態機械の生成の道具と、`hb-dump --syllables` の突き合わせ。
2. hebrew、hangul、thai（状態機械を使わない小さなもの）。
3. arabic（フォールバックの形の合成を含む）。
4. indic（と、共通の音節の扱い、母音の並びの禁止）。
5. khmer、myanmar。
6. USE。

## トレードオフ

- Ragelを使わずに状態機械の生成の道具を自分で書くため、作業が増える。Ragelは外部の道具で、Cのコードを出す。sofelは外部の道具に依存せず、生成した表をRustで持つ。4つの文法が使う構文は小さく、正規表現からDFAを作る標準の方法で足りる。HarfBuzzの機械との全数と乱数の突き合わせで、意味の違いを見つける。
- Ragelの生成したコードの表を写す案もある。写せば一致は確実だが、表の意味が読めず、HarfBuzzの文法が変わったときに、何が変わったかを文法の差分で見られない。文法を写す案を選ぶ。
- 計画のデータを計画を作るときにすべて作るため、indicの仮想の字形のグリフの探索やアラビア文字のフォールバックのlookupの組み立てを、使わない場合にも行う。HarfBuzzは一部を遅延させる。費用は計画ごとに1回で、内部可変性を避ける（[ADR 0003](0003-crate-layers.md)）。
- シェーパーの規則は、HarfBuzzとUniscribeの振る舞いに合わせた例外を多く含む。仕様（OpenTypeのスクリプトの開発の文書）より、HarfBuzzとの一致を優先する（[ADR 0005](0005-conformance-tests.md)）。

## 参考文献

- HarfBuzz 15.1.0
  - `src/hb-ot-shaper.hh`、`src/hb-ot-shaper-arabic.cc`、`src/hb-ot-shaper-arabic.hh`、`src/hb-ot-shaper-arabic-fallback.hh`、`src/hb-ot-shaper-arabic-win1256.hh`、`src/hb-ot-shaper-arabic-table.hh`、`src/hb-ot-shaper-arabic-joining-list.hh`
  - `src/hb-ot-shaper-hebrew.cc`、`src/hb-ot-shaper-hangul.cc`、`src/hb-ot-shaper-thai.cc`
  - `src/hb-ot-shaper-indic.cc`、`src/hb-ot-shaper-indic.hh`、`src/hb-ot-shaper-indic-table.cc`、`src/hb-ot-shaper-indic-machine.rl`、`src/hb-ot-shaper-indic-machine.hh`
  - `src/hb-ot-shaper-khmer.cc`、`src/hb-ot-shaper-khmer-machine.rl`、`src/hb-ot-shaper-myanmar.cc`、`src/hb-ot-shaper-myanmar-machine.rl`
  - `src/hb-ot-shaper-use.cc`、`src/hb-ot-shaper-use-table.hh`、`src/hb-ot-shaper-use-machine.rl`
  - `src/hb-ot-shaper-syllabic.cc`、`src/hb-ot-shaper-syllabic.hh`、`src/hb-ot-shaper-vowel-constraints.cc`、`src/hb-ot-shaper-default.cc`
  - `test/shape/data/in-house/tests/*.tests`、`test/shape/data/text-rendering-tests/tests/SH*.tests`
- Ragel State Machine Compiler User Guide https://www.colm.net/open-source/ragel/
- Microsoft, Developing OpenType Fonts（Arabic、Hebrew、Indic、Khmer、Myanmar、Thai、Universal Shaping Engine） https://learn.microsoft.com/en-us/typography/script-development/
- UAX #53 Unicode Arabic Mark Rendering https://www.unicode.org/reports/tr53/
- A. V. Aho, M. S. Lam, R. Sethi, J. D. Ullman, Compilers: Principles, Techniques, and Tools（第2版）、3.7節（正規表現からDFAへ）
