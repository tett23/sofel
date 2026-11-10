# ADR 0019: Unicodeの文字の性質と、シェーパーの文字の表の生成

ステータス: 採択（未実装）

## 文脈

シェーピング（[ADR 0001](0001-project-goal-and-scope.md) の4段階目。[ADR 0020](0020-shaping-foundation.md)、[ADR 0024](0024-complex-shapers.md)）は、文字ごとのUnicodeの性質を引く。
文字体系の推定、正規化（分解と合成）、マークの並べかえ、右から左の文字の鏡像、クラスタの作り方、文字体系ごとのシェーパーの音節の区切りが、それぞれ別の性質を使う。
[ADR 0003](0003-crate-layers.md) のクレートの表には、これを持つクレートが無い。

HarfBuzz 15.1.0の文字の表は、2種類に分かれる。

1つ目は、Unicodeの性質をそのまま引く `hb_unicode_funcs_t` の関数である（`src/hb-unicode.hh`）。既定の実装 `hb-ucd`（`src/hb-ucd.cc`）が値を持つ性質は、次の6つだけである。

| 性質 | HarfBuzzの関数 | 表の作り方 |
|---|---|---|
| General_Category | `general_category` | `src/gen-ucd-table.py`。30の値を、HarfBuzzの列挙（`hb_unicode_general_category_t`）の順の整数で持つ |
| Canonical_Combining_Class | `combining_class` | 同上。0〜254の整数 |
| Script | `script` | 同上。`hb-script-list.h` の順の番号で持ち、`_hb_ucd_sc_map` で `hb_script_t`（ISO 15924のタグ）に変える |
| Bidi_Mirroring_Glyph | `mirroring` | 同上。元の文字との差で持つ |
| 正規分解と合成 | `decompose`、`compose` | 同上。正規分解（`dt=can`）だけを、1文字の分解の表と、2文字の分解の表（11/7/14bitに詰めた32bitの表と64bitの表）に分ける。ハングルの音節は表に入れず、計算で分解と合成をする。合成は、合成した文字が合成の除外（`Comp_Ex`）でなく、かつその結合クラスが0である組だけを許す |
| Extended_Pictographic | `_hb_unicode_is_emoji_Extended_Pictographic` | `src/gen-emoji-table.py`。`emoji-data.txt` から作る1bitの表 |

- 表ごとの上限（General_CategoryではU+10FFFE）以上の文字は、表を引かずに既定の値（Cn、0、Unknown、差0、分解なし）を返す。
- East_Asian_Widthと互換分解は、関数の枠だけが残る非推奨の機能で、`hb-ucd` は値を持たない。
- Script_Extensions、Vertical_Orientation、Grapheme_Cluster_Break、Bidi_Classは使わない。書記素の扱いは、General_Category、いくつかの範囲の判定、Extended_Pictographicで行う（`src/hb-ot-shape.cc` の `hb_set_unicode_props`）。
- 結合クラスの付けかえ（`modified_combining_class`）、既定で無視する文字（`is_default_ignorable`）、空白の種類（`space_fallback_type`）、縦書きの文字の置きかえ（`vertical_char_for`）は、UCDから生成せず `src/hb-unicode.hh` に手で書かれている。

2つ目は、文字体系ごとのシェーパーが使う、HarfBuzz独自の分類の表である。UCDの性質を入力にするが、シェーパーの都合による付けかえを多く含む。

| 表 | 生成のスクリプト | 入力 |
|---|---|---|
| `src/hb-ot-shaper-indic-table.cc`（indic、khmer、myanmarの文字の分類と位置） | `gen-indic-table.py` | `IndicSyllabicCategory.txt`、`IndicPositionalCategory.txt`、`Blocks.txt` |
| `src/hb-ot-shaper-use-table.hh`（USEの分類） | `gen-use-table.py` | `IndicSyllabicCategory.txt`、`IndicPositionalCategory.txt`、`ArabicShaping.txt`、`DerivedCoreProperties.txt`、`UnicodeData.txt`、`Blocks.txt`、`Scripts.txt`、HarfBuzzの `src/ms-use/IndicSyllabicCategory-Additional.txt`、`IndicPositionalCategory-Additional.txt` |
| `src/hb-ot-shaper-arabic-table.hh`（結合の型と、表示形の対応、合字） | `gen-arabic-table.py` | `ArabicShaping.txt`、`UnicodeData.txt`、`Blocks.txt` |
| `src/hb-ot-shaper-arabic-joining-list.hh`（結合する文字体系の一覧） | `gen-arabic-joining-list.py` | `ArabicShaping.txt`、`Scripts.txt` |
| `src/hb-ot-shaper-vowel-constraints.cc`（点線の円を挟む母音の組） | `gen-vowel-constraints.py` | `src/ms-use/IndicShapingInvalidCluster.txt`、`Scripts.txt` |
| `src/hb-ot-shaper-arabic-pua.hh`（記号フォントのアラビア文字の私用領域の対応） | `gen-arabic-pua.py` | `src/ArabicPUASimplified.txt`、`ArabicPUATraditional.txt` |

- Unicodeのファイルはすべて18.0.0である（各表の先頭の注釈）。`ms-use/` のファイルはMicrosoftのMITのライセンスで、HarfBuzzのリポジトリにある。
- 音節の区切りの状態機械（`*-machine.rl`）はRagelの文法で、文字の分類の上で動く。これは [ADR 0024](0024-complex-shapers.md) で扱う。

利用者の指示により、Pythonのスクリプトはコミットしない。HarfBuzzの生成のスクリプトはPythonなので、そのままは使えない。

## 実装すること

### クレートと表の置き場所

- `crates/sofel-unicode` を作る。`#![no_std]`、確保なし、`#![forbid(unsafe_code)]`。依存は無い。Unicodeの性質を、UCDの定義のまま持つ。
- [ADR 0003](0003-crate-layers.md) のクレートの表に、フォントに依存しないクレートとして `sofel-unicode`（役割：Unicodeの文字の性質、依存：なし）を足し、`sofel-shape` の依存に `sofel-unicode` を足す。この ADR は [ADR 0003](0003-crate-layers.md) のクレートの表を改める。実装のプルリクエストで、[ADR 0003](0003-crate-layers.md) のステータスを「採択→`sofel-unicode` の追加はADR 0019」と改め、`scripts/deps.txt` と `docs/specifications.md` の構成の図を改める。
- シェーパーの分類の表（上の2つ目の表のうち、PUAの対応を除くもの）は `sofel-shape` に置く。理由は次のとおり。
  - 値がUCDの性質でなく、HarfBuzz（とMicrosoftのUSEの仕様）の分類であり、シェーパーの実装と一緒に変わる。
  - 同じUCDの性質（Indic_Syllabic_Categoryなど）でも、indicとUSEで付けかえが違う。
- アラビア文字のPUAの対応は、cmapの文字の付けかえで使うので、それを足す [ADR 0016](0016-font-and-vertical-metrics.md) の `sofel-font` に置く。この ADR の生成の道具で作る。
- HarfBuzzの方針で手で書かれた関数（`modified_combining_class`、`is_default_ignorable`、`space_fallback_type`、`vertical_char_for`）は、シェーピングの一部として `sofel-shape` に置く（[ADR 0020](0020-shaping-foundation.md)）。

### Unicodeの版

- Unicode 18.0.0とする。HarfBuzz 15.1.0のすべての表と同じ版である。
- HarfBuzzの固定のコミット（`testdata.lock`）を上げるときは、同じプルリクエストで、HarfBuzzの表の版にこのクレートと生成の入力の版を揃える。

### `sofel-unicode` の性質と公開の関数

| 関数 | 返す値 |
|---|---|
| `general_category(cp: u32) -> GeneralCategory` | 30の値の列挙。`#[repr(u8)]` で、値をHarfBuzzの列挙の順と同じにする（HarfBuzzの、カテゴリをビットの位置にした判定をそのまま写せるため） |
| `combining_class(cp) -> u8` | Canonical_Combining_Class |
| `script(cp) -> Script` | `Script` は、ISO 15924のタグ（`Latn` など、大文字小文字はHarfBuzzと同じ）の4バイトを持つ値。Common、Inherited、Unknownは `Zyyy`、`Zinh`、`Zzzz` |
| `mirroring(cp) -> u32` | Bidi_Mirroring_Glyph。無ければ `cp` |
| `decompose(cp) -> Option<(u32, u32)>` | 正規分解の1段。1文字の分解は2つ目を0とする（HarfBuzzの `decompose` と同じ形） |
| `compose(a, b) -> Option<u32>` | HarfBuzzの `compose` と同じ。どちらかが0なら `None` |
| `is_extended_pictographic(cp) -> bool` | Extended_Pictographic |
| `vertical_orientation(cp) -> VerticalOrientation` | UAX #50のVertical_Orientation（`U`、`R`、`Tu`、`Tr`） |

- 範囲の外（0x110000以上を含む）は、HarfBuzzと同じ既定の値を返す。Vertical_Orientationの既定の値は、`VerticalOrientation.txt` の `@missing` の行に従う。
- `Script::from_iso15924(&[u8]) -> Option<Script>` は、HarfBuzzの `hb_script_from_string` と同じく、4文字のタグの大文字小文字を直し、`Qaai` をInherited、`Qaac` をCopticに読みかえる。
- `UNICODE_VERSION` に版（18、0、0）を置く。
- すべての関数は `const` の表を引くだけで、確保をしない。

### Vertical_Orientationを持つ理由

- HarfBuzzは使わない性質だが、`sofel-unicode` に持つ。
- 縦書きのシェーピング（[ADR 0020](0020-shaping-foundation.md) のTTB）は、渡された文字列をすべて正立の向きとして扱う。欧文や数字を横倒しにするには、呼び出し側が文字列を、正立で組む区間と、横書き（LTR）で組んで90度回す区間に分けてから、別々にシェーピングする必要がある。区間の分け方の基準がVertical_Orientationである（UAX #50。CSS Writing Modesの `text-orientation: mixed` も同じ性質を使う）。
- 区間の分割は、行分割や双方向テキストと同じく、sofelの範囲の外（[ADR 0001](0001-project-goal-and-scope.md)）に置く。ただし、文字の性質の表は、アルゴリズムを含まない単純な引き方なので範囲に入れる。HarfBuzzとFreeTypeを置きかえる利用者が、それぞれ同じ表を持つ必要をなくすためである。Golemの縦書きも、この表で区間を分けられる。
- 表は、ほかの性質と同じ作り方で数KBに収まる見込みで、使わない利用者の速さには影響しない。

### 生成の入力と固定

- UCDのファイルは同梱しない。`ucd.lock` に、取得するファイルの相対パス、URL、SHA-256を1行に1つずつ書く。ファイルは、`https://www.unicode.org/Public/18.0.0/ucd/` の次のものとする。SHA-256は、実装のプルリクエストで取得して書く。
  - `sofel-unicode` のため：`UnicodeData.txt`、`Scripts.txt`、`PropertyValueAliases.txt`、`BidiMirroring.txt`、`DerivedNormalizationProps.txt`、`emoji/emoji-data.txt`、`VerticalOrientation.txt`
  - シェーパーの表のため：`IndicSyllabicCategory.txt`、`IndicPositionalCategory.txt`、`ArabicShaping.txt`、`DerivedCoreProperties.txt`、`Blocks.txt`
- HarfBuzzのリポジトリにある入力（`src/ms-use/` の3つ、`src/ArabicPUA*.txt` の2つ）は、`testdata.lock` で固定したHarfBuzzの取得物（`target/testdata/harfbuzz/`）から読む。コミットが固定されているので、別の固定は持たない。
- `scripts/fetch-ucd.sh` は、`ucd.lock` を読んで `target/ucd/` に取得し、SHA-256を確かめる。違えば失敗する。すでにそろっていれば何もしない。POSIXのシェルで書き、macOSのBSDのコマンドでも動かす（[ADR 0008](0008-validate-command-and-freetype-comparison.md) の `fetch-testdata.sh` と同じ作り）。

### 生成の道具

- 生成の道具は、Rustで `tools/gen-unicode/` に書く。ワークスペースの外のCargoのパッケージとし、外部のクレートに依存しない（`bench/` と同じ扱い）。シェルとawkでなくRustにするのは、多段の表の分け方の探索と、生成した表の全数の照合と、HarfBuzzの生成のスクリプトの付けかえの規則の移植を書くためである。
- 1つの道具が、次のファイルを書く。生成したファイルはコミットする。先頭に、生成のコマンド、Unicodeの版、入力のファイルのSHA-256（HarfBuzzの入力はHarfBuzzのコミット）、各表の大きさを書く。
  - `crates/sofel-unicode/src/tables.rs`
  - `crates/sofel-shape/src/tables/indic.rs`、`use_.rs`、`arabic.rs`（結合の型、表示形、合字、結合する文字体系）、`vowel_constraints.rs`
  - `crates/sofel-font/src/tables/arabic_pua.rs`
- シェーパーの表は、HarfBuzzの生成のスクリプト（`gen-indic-table.py`、`gen-use-table.py`、`gen-arabic-table.py`、`gen-arabic-joining-list.py`、`gen-vowel-constraints.py`、`gen-arabic-pua.py`）の規則（付けかえ、既定の値、範囲の選び方）をRustに移植して作る。HarfBuzzのMITの流用として表示する（[ADR 0002](0002-license-and-reuse.md)）。表の形は、HarfBuzzの形でなく下の「データ構造」の多段の表とし、値の意味だけを揃える。母音の組の表は、HarfBuzzが生成するCのコードの代わりに、文字体系ごとの組の表と、それを引く手書きの関数にする。
- `sofel-unicode` の入力の読み方：
  - General_Categoryと正規分解は `UnicodeData.txt` から読む。`<..., First>` と `<..., Last>` の組は範囲として展開する。載っていない文字はCnとする。正規分解は、分解の欄が `<` で始まらないものだけとする。
  - Scriptは `Scripts.txt` から読み、長い名前を `PropertyValueAliases.txt` の `sc` の行で4文字のタグに変える。載っていない文字はUnknownとする。
  - Bidi_Mirroring_Glyphは `BidiMirroring.txt` の注釈でない行から読む。
  - 合成の除外は、`DerivedNormalizationProps.txt` の `Full_Composition_Exclusion` とする（UCDXMLの `Comp_Ex` と同じ性質）。
  - Extended_Pictographicは `emoji-data.txt` から、Vertical_Orientationは `VerticalOrientation.txt` から読む。
- 生成の結果は、入力が同じなら常に同じバイト列とする（並びの決まらない集まりを使わない）。
- 生成の道具は、書き出すまえに、0〜0x10FFFFのすべての文字について、詰めた表を引いた値が元の値と一致することを確かめ、一致しなければ失敗する。

## データ構造

- 性質ごとに別の多段の表にする（HarfBuzzと同じ分け方）。シェーパーの表も同じ形とする。
- 1つの表は、2段か3段とする。3段の表は、文字 `cp` について次のように引く。

  ```
  i1 = S1[cp >> (k2 + k3)]
  i2 = S2[(i1 << k2) + ((cp >> k3) & ((1 << k2) - 1))]
  v  = S3[(i2 << k3) + (cp & ((1 << k3) - 1))]
  ```

  `S3` は値の塊（大きさ `2^k3`）を重複を除いて並べたもの、`S2` は `S3` の塊の番号の塊（大きさ `2^k2`）を重複を除いて並べたもの、`S1` は `S2` の塊の番号である。2段の表は `k2 = 0` として `S2` を省く。
- 生成の道具は、`k2` と `k3` の組（それぞれ0〜8）を全部試し、表の合計のバイト数が最小のものを選ぶ。同じなら段の少ないほう、次に `k3` の小さいほうを選ぶ。各段の要素の幅は、収まる最小の幅（1、4、8、16bit）とする。
- 表の下限と上限は、既定でない値を持つ最初と最後の文字で決め、範囲の外は表を引かずに既定の値を返す。indicやUSEの表のように一部の範囲にしか値が無い表では、下限で引く量が減る。
- 添字の読み取りは `get` で行い、範囲の外なら既定の値を返す（[ADR 0007](0007-sfnt-read-foundation.md) のlintのため）。生成の道具が全数を照合するので、この分岐は実際には通らない。
- 分解は、HarfBuzzと同じく、分解の番号の表と、1文字の分解の配列、2文字の分解の配列を持つ。2文字の分解の配列は、`(a, b, ab)` を21bitずつ64bitに詰めて `(a, b)` の順に並べ、合成はこの配列の二分探索で引く。合成を許さない組（`ab` が合成の除外であるか、`ab` の結合クラスが0でない）は、`ab` を0として配列に残す。ハングルの音節は表に入れず、HarfBuzzと同じ計算で扱う。

## 実装しないこと

- East_Asian_Width、Script_Extensions、Grapheme_Cluster_Break、Line_Break、Bidi_Class、互換分解、大文字と小文字の変換、Unihan。HarfBuzzのシェーピングが使わず、使う処理（行分割、双方向テキスト、文字列の区間への分割）がsofelの範囲の外（[ADR 0001](0001-project-goal-and-scope.md)）にあるためである。Vertical_Orientationとの違いは、これらの性質の利用者が、どれもsofelの外のアルゴリズムと組で使う点にある。必要になったら、範囲を広げるADRを作成する。
- UCDの生の性質としてのIndic_Syllabic_Category、Indic_Positional_Category、Joining_Type、Joining_Group。シェーパーはHarfBuzzの分類の表を使い、生の性質の利用者がまだいない。
- 実行時の表の差しかえ（HarfBuzzの `hb_unicode_funcs_set_*_func`、ICUやGLibとの統合）。
- HarfBuzzの `HB_OPTIMIZE_SIZE` に当たる、遅い代わりに小さい表の選択。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- 個別の値：`sofel-unicode` の単体テストで、次を確かめる。
  - U+0041（Lu、0、Latn、鏡像なし）、U+0301（Mn、230、Zinh）、U+3042（Lo、Hira）、U+30A2（Lo、Kana）、U+4E00（Lo、Hani）、U+30FC（Lm、Zyyy）、U+0028（鏡像U+0029）、U+0378（Cn、Zzzz）、U+E000（Co）、U+D800（Cs）、U+10FFFF、0x110000、`u32::MAX`（既定の値）。
  - 分解：U+00C5 → (U+0041, U+030A)、U+212B → (U+00C5, 0)、U+AC00 → (U+1100, U+1161)、U+AC01 → (U+AC00, U+11A8)、U+0041 → `None`。
  - 合成：(U+0041, U+030A) → U+00C5、(U+1100, U+1161) → U+AC00、(U+AC00, U+11A8) → U+AC01、合成の除外のU+0958の分解の組 → `None`、どちらかが0 → `None`。
  - Extended_Pictographic：U+1F600、U+00A9は真、U+0041は偽。
  - Vertical_Orientation：U+0041（R）、U+3042（U）、U+3001（Tu）、U+30FC（Tr）、U+FF08（Tr）。
  - `Script::from_iso15924`：`latn` → `Latn`、`Qaai` → Inherited、3文字や5文字 → `None`。
- 生成の道具：`tools/gen-unicode/` の単体テストで、`UnicodeData.txt` の範囲の行の展開、分解の欄の `<` の除外、`Scripts.txt` の範囲、`@missing` の行、表の詰め方（既知の小さな配列で、すべての `k2`、`k3` の組の表が元の値を返すこと、選ぶ組が最小であること）を確かめる。
- HarfBuzzとの全数の比較：[ADR 0016](0016-font-and-vertical-metrics.md) の `tools/hb-dump/` に `--unicode` を足す。0〜0x10FFFFと、0x110000、0x7FFFFFFF、0xFFFFFFFFのすべてについて、1文字1行で次を書き出す。
  - 公開のAPI：`hb_unicode_general_category`、`hb_unicode_combining_class`、`hb_unicode_script`、`hb_unicode_mirroring`、`hb_unicode_decompose`。
  - 内部の関数：Extended_Pictographic、indicの分類と位置（`hb_indic_get_categories`）、USEの分類（`hb_use_get_category`）、アラビア文字の結合の型（`hb-ot-shaper-arabic.cc` の `get_joining_type`）。これらは公開のAPIに無く、`static` の関数も含むため、`tools/hb-dump/` に `src/harfbuzz.cc` を取りこむC++の翻訳単位を1つ置き、その中から `extern "C"` で包んで呼ぶ。`hb-dump` は、HarfBuzzを別のライブラリとしてリンクせず、この翻訳単位でHarfBuzzを含める。
  - 合成は、全組を試せないため、分解に現れる1つ目の文字すべてと2つ目の文字すべての直積に、ハングルの字母の範囲（L、V、T、LVの音節）を加えた組について、`hb_unicode_compose` を書き出す。合成が成り立つ組は必ず分解の組に現れるので、成り立つ組はすべて含まれる。
  - 母音の組の表は、`--vowel-constraints` で、各文字体系の組の表に現れる文字の、長さ1〜3の全列について、HarfBuzzの `_hb_preprocess_text_vowel_constraints` が点線の円を挟む位置を書き出す。
  - アラビア文字のPUAの対応は、0x0000〜0xFFFFのすべてについて、HarfBuzzの `_hb_arabic_pua_simp_map` と `_hb_arabic_pua_trad_map` の値を書き出す。
  - `sofel-conformance` で、すべての行の完全一致を確かめる。期待する失敗の一覧は持たない（1つでも違えば失敗）。
- Vertical_Orientation：HarfBuzzに比べる相手が無いため、`sofel-conformance` に、`VerticalOrientation.txt` を生成の道具とは別に書いた単純な読み取りで読み、全文字の値が `vertical_orientation` と一致することを確かめるテストを置く。
- 生成の再現：CIの適合テストのジョブで、`scripts/fetch-ucd.sh` と生成の道具を走らせ、生成したファイルがコミットしたものと一致することを `git diff --exit-code` で確かめる。`ucd.lock` の内容をキーに `target/ucd/` をキャッシュする。
- `fetch-ucd.sh`：わざと違うSHA-256を書いた一時の `ucd.lock` で失敗することと、2回目に取得しなおさないことを、`scripts/tests/` のテストで確かめる（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。
- wasm：同じ単体テストを `wasm32-wasip1` で通し、`wasm32-unknown-unknown` でビルドする（[ADR 0004](0004-wasm-and-no-std.md)）。
- 計測：[ADR 0013](0013-benchmark-harness.md) の `bench/` に、項目「Unicodeの性質」を足す。`perf/texts/` のすべての文章の全文字について、General_Category、Canonical_Combining_Class、Script、Bidi_Mirroring_Glyphを引く。相手は `hb_unicode_funcs_get_default()` に対する `hb_unicode_general_category` などとし、関数の呼び出しの費用も相手の時間に含める。計測の前に、両者の値の和が一致することを確かめる。

## トレードオフ

- 入力を、HarfBuzzの使うUCDXMLでなく、UCDのテキストのファイルにする。テキストのファイルは行ごとに読めて、生成の道具が単純になる。HarfBuzzのシェーパーの表の生成も同じテキストのファイルを読むので、入力の種類が1つで済む。2つの形は同じ性質を表すが、読み方の誤りで値が違いうる。全数の比較でHarfBuzzとの一致を確かめるので、違いは必ず見つかる。
- シェーパーの表を `sofel-shape` に置くため、`sofel-unicode` だけを使う利用者は、Indicなどの分類を得られない。分類はHarfBuzzのシェーパーの内部の値で、外に出すと仕様として固定されてしまう。
- 性質ごとに別の表を引くため、General_CategoryとCanonical_Combining_Classの両方が要る文字では、表を2回引く。性質の組を1つの表にまとめれば1回で済むが、組の種類が増えて表が大きくなる。HarfBuzzと同じ分け方で始め、計測で必要と示せたら、よく一緒に引く性質だけをまとめるADRを作成する。
- 表の分け方を合計のバイト数だけで選ぶため、引く速さは段の数で決まり、最速の組とは限らない。表の大きさはwasmの配布の大きさに効くため、まず小ささで選び、速さは計測で確かめる。
- HarfBuzzの生成のスクリプトの付けかえの規則をRustに移植するため、HarfBuzzが規則を変えるたびに移植しなおす必要がある。全数の比較が、固定のコミットを上げるときに違いを示す。
- Vertical_OrientationはHarfBuzzに無く、基準になる実装が無い。UCDのファイルとの直接の照合で正しさを確かめる。
- 生成の道具をRustで書くため、表を作りなおすにはCargoが要る。
- UCDのファイルの取得にunicode.orgへの接続が要る。CIではキャッシュし、SHA-256で内容を固定する。
- 生成した表はUnicodeのデータから作られるため、Unicode License v3の表示が要る。`ms-use/` の入力から作る表には、MicrosoftのMITのライセンスの表示が要る。実装のプルリクエストで `THIRD_PARTY_NOTICES.md` に加える（[ADR 0002](0002-license-and-reuse.md)）。

## 参考文献

- HarfBuzz 15.1.0
  - `src/hb-unicode.hh`、`src/hb-unicode.cc`、`src/hb-ucd.cc`、`src/hb-ucd-table.hh`、`src/gen-ucd-table.py`、`src/hb-unicode-emoji-table.hh`、`src/gen-emoji-table.py`、`src/hb-common.cc`（`hb_script_from_iso15924_tag`）、`src/hb-ot-shape.cc`（`hb_set_unicode_props`）
  - `src/gen-indic-table.py`、`src/hb-ot-shaper-indic-table.cc`、`src/gen-use-table.py`、`src/hb-ot-shaper-use-table.hh`、`src/gen-arabic-table.py`、`src/hb-ot-shaper-arabic-table.hh`、`src/gen-arabic-joining-list.py`、`src/hb-ot-shaper-arabic-joining-list.hh`、`src/gen-vowel-constraints.py`、`src/hb-ot-shaper-vowel-constraints.cc`、`src/gen-arabic-pua.py`、`src/hb-ot-shaper-arabic-pua.hh`、`src/update-unicode-tables.make`、`src/ms-use/`
- packTab https://github.com/harfbuzz/packtab
- Unicode 18.0.0 Character Database https://www.unicode.org/Public/18.0.0/ucd/
- UAX #44 Unicode Character Database https://www.unicode.org/reports/tr44/
- UAX #15 Unicode Normalization Forms https://www.unicode.org/reports/tr15/
- UAX #50 Unicode Vertical Text Layout https://www.unicode.org/reports/tr50/
- UTS #51 Unicode Emoji https://www.unicode.org/reports/tr51/
- UAX #29 Unicode Text Segmentation https://www.unicode.org/reports/tr29/
- CSS Writing Modes Level 4（`text-orientation`） https://www.w3.org/TR/css-writing-modes-4/
- Microsoft, Developing OpenType Fonts for the Universal Shaping Engine https://learn.microsoft.com/en-us/typography/script-development/use
