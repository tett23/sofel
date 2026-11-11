# ADR 0020: シェーピングの土台（`sofel-shape`、GDEF、GSUB、GPOS、バッファ、計画、既定のシェーパー）

ステータス: 採択→`--font-ptem` の分類は [ADR 0028](0028-ptem-is-not-opsz.md)、既知の期待失敗の分類は [ADR 0044](0044-shaping-conformance-exceptions.md)

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の4段階目は、シェーピングの土台（バッファ、クラスタ、正規化、GSUB、GPOS）と、文字体系ごとのシェーパーである。
sofelは、HarfBuzzとFreeTypeを置きかえる汎用のライブラリであり、シェーピングの範囲はHarfBuzz 15.1.0のOTシェーパーの全体とする（[ADR 0001](0001-project-goal-and-scope.md)）。すべての言語と文字体系を対象にする。
3段階目（可変フォント）より先にこの段階に入る。最初の利用者であるGolemの必要が先に来たためで、[ADR 0001](0001-project-goal-and-scope.md) の「順序は利用者の必要に応じて入れかえてよい」による。可変フォントに依存する部分は後に回す。

この段階は2つのADRに分ける。

- この ADR：クレート、GDEF、GSUB、GPOSの読み取りと適用、バッファ、区間の性質、言語と文字体系のタグ、機能の文字列、計画、手順の全体、正規化の枠組み、位置とフォールバック、既定のシェーパー、バッファの直列化、`sofel-shape` のコマンド、適合テストの仕組み、計測、C言語のAPI。
- [ADR 0024](0024-complex-shapers.md)：文字体系ごとのシェーパー（arabic、hebrew、hangul、thai、indic、khmer、myanmar、USE）と、その音節の状態機械と文字の表。

HarfBuzz 15.1.0のシェーピングは、次のように組まれている。

- **バッファ**（`src/hb-buffer.hh`、`src/hb-buffer.cc`）：入力の文字列の位置をクラスタの値とし、前後の文脈（`item_offset` の外の文字）も持つ。クラスタのまとめ方は4つの水準（`HB_BUFFER_CLUSTER_LEVEL_*`）で選ぶ。`guess_segment_properties` は、文字体系をCommon、Inherited、Unknownでない最初の文字から、方向を文字体系から決め、言語は `hb_language_get_default()`、すなわちプロセスのロケール（`setlocale(LC_CTYPE, NULL)`）から取る。
- **直列化**（`src/hb-buffer-serialize.cc`、`src/hb-buffer-deserialize-*.rl`）：グリフの列と文字の列の、テキストとJSONの2つの形の書き出しと読み込み。`hb-shape` の出力はこの書き出しである。比較の `hb_buffer_diff` も持つ。
- **計画**（`src/hb-shape-plan.cc`、`src/hb-ot-shape.cc`、`src/hb-ot-map.cc`）：区間の性質と機能の組から、文字体系と言語のタグ（`src/hb-ot-tag.cc`、`src/hb-ot-tag-table.hh`）、使うlookupとマスク、シェーパー（`hb_ot_shaper_categorize`）、GPOSとフォールバックのどちらで位置を決めるかなどを決め、`hb_face_t` に覚える。
- **既定の機能**：すべての方向で `rvrn`、`Harf`、`HARF`、`Buzz`、`BUZZ`、`abvm`、`blwm`、`ccmp`、`locl`、`mark`、`mkmk`、`rlig` を有効にし、分数のための `frac`、`numr`、`dnom` と、`rand` を置く。LTRは `ltra`、`ltrm`、RTLは `rtla`（と `rtlm` の準備）。横書きは加えて `calt`、`clig`、`curs`、`dist`、`kern`、`liga`、`rclt`。縦書きは `vert` だけを、文字体系と言語によらずフォント全体から探して有効にする（`F_GLOBAL_SEARCH`）。`vrt2` と `vkrn` は既定では有効にならない。シェーパーは、この間に自分の機能と段を差しこむ。
- **手順**（`hb_ot_shape_internal`）：Unicodeの性質と書記素の印 → 点線の円の挿入 → クラスタの形成 → 本来の方向への調整 → シェーパーの前処理 → 鏡像と縦書きの文字の置きかえ → 正規化（`src/hb-ot-shape-normalize.cc`。モードはシェーパーが選ぶ）→ マスクの設定 → GDEFのクラス（無ければUnicodeから合成）→ GSUB → 既定で無視する文字の扱い → シェーパーの後処理 → 送り幅と原点 → マークの幅の0化 → GPOS → フォールバックのマークの配置（`src/hb-ot-shape-fallback.cc`）→ 逆向きなら反転 → フラグの伝播。
- **lookupの適用**（`src/hb-ot-layout.cc`、`src/hb-ot-layout-gsubgpos.hh`、`src/OT/Layout/`）：lookupの番号の順に、バッファの全体へ1つずつ適用する。lookupのフラグに従って飛ばすグリフを決める反復子（skippy iterator）、合字の成分の番号、文脈の入れ子（最大64）、操作の数の上限（`src/hb-limits.hh`）を持つ。HarfBuzzの既定のビルドでは、24bitのグリフIDを使う形式（ISO OFF第5版）も読む。
- **拡大縮小**（`src/hb-font.hh`）：送り幅、GPOSの値、付着点を `em_mult` で拡大縮小し、ppemがあればDevice表の補正を足す。`hb-shape` は `--font-size` が無ければ `unitsPerEm` で、つまりフォントの単位で出す。
- **シェーパー**：`hb_ot_shaper_categorize`（`src/hb-ot-shaper.hh`）が、文字体系とGSUBで選んだタグから、default、arabic、hangul、hebrew、indic、khmer、myanmar、myanmar_zawgyi、thai、USEのどれかを選ぶ。`morx` を使うときはdumberを選ぶ。

HarfBuzzのテストは、317の `*.tests`（`in-house`、`aots`、`text-rendering-tests`）に約6300行ある（[ADR 0005](0005-conformance-tests.md)）。
期待値は `hb-shape` の出力の形で、多くの行が `--font-size=1000`、`--ned`、`--no-glyph-names`、`--no-positions` を使う。グリフの名前で書かれた期待値を比べるには、グリフの名前（[ADR 0016](0016-font-and-vertical-metrics.md)）が要る。

GolemはHarfBuzz 14.6.0を、UTF-8の文字列、LTRかTTB、文字列で渡す文字体系と言語、`hb_buffer_guess_segment_properties`、機能の文字列（`palt`、`vpal`、`kern`、`vkrn`、`liga` など）で呼び、グリフID、UTF-8のバイトの位置のクラスタ、x/yの送り幅とずれをフォントの単位で受け取っている（`modules/golem-font/cbits/golem/golem_font.c`）。x86-64、aarch64、wasm32-wasiで同じ結果になることを要する。

## 実装すること

### 範囲

- HarfBuzz 15.1.0のOTシェーパーと同じ結果を出す。範囲は、この ADR と [ADR 0024](0024-complex-shapers.md) で、HarfBuzzの `hb_ot_shape` の全体から、下の「後回しにしたもの」を除いたものとする。
- 位置は、拡大縮小の倍率とppemを含めて求める。倍率の計算は [ADR 0016](0016-font-and-vertical-metrics.md) の拡大縮小の補助（HarfBuzzの `em_mult`、`em_multf` と同じ規則）を使う。倍率を `unitsPerEm` にしたとき、結果はフォントの単位になる。

### クレートと実装の分け方

- `crates/sofel-shape` を作る。`#![no_std]` と `alloc`、`#![forbid(unsafe_code)]`。依存は `sofel-font` と `sofel-unicode`（[ADR 0019](0019-unicode-character-data.md)）だけとする（[ADR 0003](0003-crate-layers.md)）。
- 表は `sofel_font::read` を通して読む（[ADR 0010](0010-font-foundation.md)）。
- HarfBuzzの方針で手で書かれたUnicodeの関数（`modified_combining_class`、`is_default_ignorable`、`space_fallback_type`、`vertical_char_for`、`hb_script_get_horizontal_direction`、地域指示子の判定）は、`sofel-shape` に置く（[ADR 0019](0019-unicode-character-data.md)）。
- 実装のプルリクエストは、次の順に分ける。各プルリクエストで、そこまでに通るようになったテストを期待する失敗の一覧から消す。
  1. `sofel-read`：GDEF、GSUB、GPOSのビューと検証器、otvalidとの照合。
  2. `sofel-shape` の土台と既定のシェーパー：バッファ、区間の性質、タグ、機能、計画、手順、正規化、GSUBとGPOSの適用、位置とフォールバック、直列化、`sofel-shape` のコマンド、適合テストと `hb-dump` の突き合わせの仕組み。Golemのラテン文字と日本語は、ここで正しくなる。
  3. C言語のAPIと計測の項目。
  4. 以後、[ADR 0024](0024-complex-shapers.md) のシェーパーを1つずつ。

### `sofel-read`：GDEF、GSUB、GPOS

- **共通の構造**（`src/hb-ot-layout-common.hh`）：ScriptList、Script、LangSys、FeatureList、Feature（`size` の機能の引数のオフセットの例外を含む）、LookupList、Lookup（フラグとマークの絞り込みの集合の番号）、Coverage（形式1〜4）、ClassDef（形式1〜4）、Device（形式1〜3）。VariationIndexの形式（0x8000）は見分けて、補正を0とする。
- **GDEF**：版1.0、1.2、1.3。グリフのクラス、付着点の一覧、合字のキャレット、マークの付着のクラス、マークのグリフの集合。版1.3のItemVariationStoreは、オフセットを持つだけで読まない。HarfBuzzの拒否の一覧（`GDEF::is_blocklisted`。特定のフォントの壊れたGDEFを、GDEF、GSUB、GPOSの表の長さの組で見分ける）も同じく持つ。
- **GSUB**：lookupの型1〜8のすべての形式（単一の形式1、2と24bitの形式3、4。複数、代替、合字の形式1と24bitの形式2。文脈と連鎖文脈の形式1〜3と24bitの形式4、5。拡張。逆向きの連鎖の単一）。
- **GPOS**：lookupの型1〜9のすべての形式（単一と対の形式1、2と24bitの形式3、4。筆記体。マークの付着3種の形式1と24bitの形式2。文脈と連鎖文脈。拡張）。ValueRecordの各形式、Anchorの形式1〜3（形式2の輪郭の点は、HarfBuzzと同じくppemがあるときだけ使う）。
- **GSUBとGPOSの版1.1**：FeatureVariationsのオフセットを持つだけで、読まない（「後回しにしたもの」）。
- 拡張のlookupは、ビューを作るときに中身の型へたどる。拡張の先が拡張であることは誤りとする。
- 検証器（[ADR 0007](0007-sfnt-read-foundation.md)）：範囲外のオフセット、数と長さの不一致、Coverageの並びの乱れ（二分探索の前提を崩す）、ClassDefの範囲の重なり、lookupの番号やFeatureの番号がリストの数以上、拡張の型の不一致、拡張の入れ子、文脈のlookupの記録の位置が入力の長さ以上、ValueFormatとlookupのフラグの予約のビット（Warning）、マークの絞り込みの集合の番号がGDEFの集合の数以上を報告する。文脈のlookupの参照の循環は、適用の側が入れ子の上限で止めるので、報告しない。検証はたどったオフセットの数に上限を置き、入力の大きさに比例する時間で終わる。
- FreeTypeのotvalidとの照合：`ft-dump` に `--otvalid` を足し（FreeTypeのotvalidのモジュールを有効にしてビルドする）、HarfBuzzのテストのフォントのGDEF、GSUB、GPOSを `FT_OpenType_Validate` で検証する。FreeTypeが拒むフォントで、sofelの検証器が `Error` を報告することを確かめる（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。

### `sofel-font` に要るもの

- 横の送り幅は `Font::advance`（[ADR 0010](0010-font-foundation.md)）、縦の送り幅と原点、グリフの箱、cmapの文字の付けかえ、グリフの名前と名前からのグリフ、拡大縮小の補助は [ADR 0016](0016-font-and-vertical-metrics.md) のものを使う。CFFのフォントのグリフの名前と箱は [ADR 0021](0021-cff-outlines.md)、カラーグリフの箱は [ADR 0023](0023-color-glyphs.md) が足す。

### 区間の性質

- `Direction`：`Ltr`、`Rtl`、`Ttb`、`Btt`。
- `Script` は `sofel-unicode` のもの。`Language` は `hb_language_from_string` と同じ正規化（ASCIIの小文字に、`_` を `-` に。区切りの文字以外が現れたらそこまで）をした文字列とする。
- `SegmentProperties { direction, script, language }`（どれも `Option`）と、`guess(self, text, default_language: Option<Language>) -> ResolvedProperties`。
  - 文字体系と方向の推定は `guess_segment_properties` と同じとする。
  - 言語が無いときは、引数の `default_language` を使う。それも無ければ「言語なし」とする。
  - ライブラリはプロセスのロケールを読まない。HarfBuzzは `hb_language_get_default()` でロケールを暗黙に読み、最初の呼び出しで覚える。暗黙の大域の状態は、結果が実行の環境で変わり、wasmやスレッドの間で決定性を崩し、内部可変性を要する（[ADR 0003](0003-crate-layers.md)）。HarfBuzzと同じ結果が要る呼び出し側は、自分でロケールを読んで `default_language` に渡す。`sofel-shape` のコマンドは `--default-language` を持ち、指定が無ければ「言語なし」とする。これはHarfBuzzが `LANG=C` で動くときと同じ結果である。
- `Script::from_iso15924` と `Language` の作り方は、Golemの `hb_script_from_string`、`hb_language_from_string` の置きかえとなる。

### 文字体系と言語のタグ

- 両方向の変換を、HarfBuzzの `src/hb-ot-tag.cc` と同じにする。
  - `ot_tags_from_script_and_language(script, language) -> (script_tags, language_tags)`：`hb_ot_tags_from_script_and_language`。文字体系の新しいタグ（`dev2`、`dev3`、`mym2` など）を先に試す規則、`kana` などの古いタグ、言語の私用の下位タグ（`-x-hbsc`、`-x-hbot`）、地域や文字体系の下位タグに依存する例外（`hb_ot_tags_from_complex_language`）、2文字と3文字の言語の表、複数のタグを持つ言語を含む。
  - `ot_tags_to_script_and_language(script_tag, language_tag) -> (Script, Option<Language>)`：`hb_ot_tags_to_script_and_language`、`hb_ot_tag_to_script`、`hb_ot_tag_to_language`。曖昧なタグの例外（`hb_ot_ambiguous_tag_to_language`）を含む。
- 言語のタグの表と例外は、HarfBuzz 15.1.0の `src/hb-ot-tag-table.hh` から作る。このファイルは、IANAの言語の下位タグの登録とOpenTypeの言語のタグの一覧から、手の調整を加えて生成されたもので、元の入力の版はHarfBuzzに残っていない。
  - 配列（`ot_languages2`、`ot_languages3`、`ot_languages3_blocked`、`ot_languages3_multi*`）は、`tools/gen-ot-tag/`（Rust、ワークスペースの外）がこのファイルを読んでRustの表に書き、コミットする。
  - 生成された2つの関数（`hb_ot_tags_from_complex_language`、`hb_ot_ambiguous_tag_to_language`）は、決まった形の分岐（`subtag_matches`、`lang_matches`、`strncmp`、`memcmp`、1文字目の `switch`）の並びである。`tools/gen-ot-tag/` は、この形を読み取ってRustの分岐の表（言語の接頭辞、要る下位タグ、結果のタグの列）に変える。知らない形が現れたら、生成を失敗させる。
  - HarfBuzzのMITの流用として表示する（[ADR 0002](0002-license-and-reuse.md)）。

### 機能の文字列

- `Feature { tag, value, start, end }` と `Feature::parse(&[u8]) -> Option<Feature>`、`Feature::format`。`hb_feature_from_string`、`hb_feature_to_string` と同じ文法とする（`kern`、`+kern`、`-kern`、`kern=0`、`aalt=2`、`kern[3:5]`、`kern[3]`、`kern[:5]`、`kern[]`、CSSの `"kern" on`、`'kern' 2`、空白と引用符の扱い）。`start` と `end` はクラスタの値で表し、`end` の既定は `u32::MAX` とする。

### 入力のバッファ

- `UnicodeBuffer` は、文字（コードポイント）、クラスタの値、前後の文脈、フラグ、クラスタの水準を持つ。
  - `push_utf8(text, item_offset, item_len)`、`push_utf16`、`push_utf32`、`push_codepoints`、`push_latin1`：HarfBuzzの `hb_buffer_add_utf*`、`hb_buffer_add_codepoints`、`hb_buffer_add_latin1` と同じ。クラスタの値は `text` の先頭からの位置（UTF-8はバイト、UTF-16は符号単位、UTF-32は要素の番号）とし、`item_offset` の前と後の最大5文字を文脈とする。不正な符号の列は置きかえの文字（既定U+FFFD、`set_replacement_codepoint` で変えられる）にし、区切り方は `src/hb-utf.hh` と同じとする。
  - フラグ：`BOT`、`EOT`、`PRESERVE_DEFAULT_IGNORABLES`、`REMOVE_DEFAULT_IGNORABLES`、`DO_NOT_INSERT_DOTTED_CIRCLE`、`PRODUCE_UNSAFE_TO_CONCAT`、`PRODUCE_SAFE_TO_INSERT_TATWEEL`。
  - クラスタの水準：`MonotoneGraphemes`（既定）、`MonotoneCharacters`、`Characters`、`Graphemes`。
  - 見えないグリフ、見つからないグリフ、見つからない異体字セレクタのグリフ（`hb_buffer_set_invisible_glyph`、`hb_buffer_set_not_found_glyph`、`hb_buffer_set_not_found_variation_selector_glyph`）。
  - `rand` の乱数の状態（HarfBuzzと同じ線形合同法。初期値1。`set_random_state` で変えられる）。
- 入力の長さと操作の数の上限は、HarfBuzzと同じ係数（`HB_BUFFER_MAX_LEN_FACTOR`、`HB_BUFFER_MAX_OPS_FACTOR`）と最大で決め、上限に達したときの振る舞い（それ以上のlookupの適用をやめる）も同じとする。上限は環境によらない定数とする。

### 出力

- `GlyphBuffer` は、グリフごとに `GlyphInfo { glyph: u32, cluster: u32, flags: GlyphFlags }` と `GlyphPosition { x_advance: i32, y_advance: i32, x_offset: i32, y_offset: i32 }` を持つ。値は拡大縮小した単位で、HarfBuzzの符号（縦書きの送り幅は下向きが負）に従う。
- `GlyphFlags` は `UNSAFE_TO_BREAK`、`UNSAFE_TO_CONCAT`、`SAFE_TO_INSERT_TATWEEL` とする。HarfBuzzと同じ箇所で立て、`hb_propagate_flags` と同じくクラスタの全グリフに広げる。
- `GlyphBuffer::into_unicode(self) -> UnicodeBuffer` で、確保した領域を次の呼び出しに回せる。
- `GlyphBuffer::normalize_glyphs()`：`hb_buffer_normalize_glyphs` と同じ。
- `diff(a, b, dottedcircle_glyph, position_fuzz) -> DiffFlags`：`hb_buffer_diff` と同じ。`--verify` と、テストでの比較に使う。

### 直列化

- `sofel-shape` に、`hb-buffer-serialize.cc` と同じ書き出し（グリフの列と文字の列。テキストとJSON。名前、クラスタ、位置、送り幅、フラグ、箱の各指定）と、読み込み（`hb_buffer_deserialize_glyphs`、`hb_buffer_deserialize_unicode` のテキストとJSON）を置く。
- 読み込みは、HarfBuzzのRagelの文法（`hb-buffer-deserialize-*.rl`）と同じ言語を受け付ける手書きの解析器とする。文法が小さく、状態機械の生成の道具を要しないためである。名前からのグリフは [ADR 0016](0016-font-and-vertical-metrics.md) の `glyph_from_name` を使う。

### シェーピングの計画

- `ShapePlan::new(font: &Font, props: ResolvedProperties, features: &[Feature]) -> ShapePlan`。HarfBuzzの `hb_shape_plan_t` に当たり、呼び出し側が持って再利用する（[ADR 0003](0003-crate-layers.md)）。不変の値で、`Send` と `Sync` を満たす。
- 計画は、作るときに次を決める。
  - GSUBとGPOSの文字体系と言語の選択：上のタグと、`hb_ot_layout_table_select_script`（見つからなければ `DFLT`、`dflt`、`latn` の順）、`hb_ot_layout_script_select_language`。
  - シェーパー：`hb_ot_shaper_categorize` と同じ判定。`ShapePlan::shaper()` で返す。
  - 機能の集め方と順序：上の「既定の機能」、シェーパーの `collect_features` と `override_features`（[ADR 0024](0024-complex-shapers.md)）、利用者の機能を、`hb_ot_shape_collect_features` と同じ順で加える。
  - 対応表（map）の作成：`hb_ot_map_builder_t::compile` と同じく、機能ごとのマスクのビットの割り当て（大域の機能は共通のビット、範囲を持つ機能や値が2以上の機能は個別のビット、ビットが足りなければ機能を捨てる）、必須の機能、`F_GLOBAL_SEARCH`、`F_HAS_FALLBACK`、GSUBとGPOSの段とシェーパーの段の関数、lookupごとのフラグ（自動のZWJとZWNJ、`rand`、音節ごと）を決め、lookupを段ごとに番号の順に並べて重複をまとめる。
  - 位置の決め方：GDEFのグリフのクラスの有無、GPOSの適用（シェーパーの `gpos_tag` による無効化を含む）、マークの幅の0化の時期（シェーパーが選ぶ）、フォールバックのマークの配置の要否、`has_vert`、`kern`/`vkrn` のマスク。`hb_ot_shape_planner_t::compile` と同じ判定とする。AATの表と古い `kern` 表に関わる分岐は、それらを扱うADR（「後回しにしたもの」）までは、表が無いものとして判定する。
  - シェーパーの計画のデータ（[ADR 0024](0024-complex-shapers.md)。アラビア文字のフォールバックのlookupなど）。
  - lookupの要約（下の「データ構造」）。
- 計画が同じ入力から作られたかの判定は、呼び出し側が行う。`ShapePlan::key()` が `(ResolvedProperties, Vec<Feature>)` を返す。

### シェーピングの手順

- `shape(font: &Font, scale: &Scale, plan: &ShapePlan, buffer: UnicodeBuffer) -> GlyphBuffer`。`Scale` は [ADR 0016](0016-font-and-vertical-metrics.md) の拡大縮小の値（倍率とppem）とする。
- 手順は、文脈の節の `hb_ot_shape_internal` の順とし、各段をHarfBuzzと同じ規則で行う。シェーパーの差しこみ口（`preprocess_text`、`postprocess_glyphs`、`decompose`、`compose`、`setup_masks`、`reorder_marks`、段の関数）は、HarfBuzzの `hb_ot_shaper_t` と同じ位置に置く。
  - Unicodeの性質と書記素の印（`hb_set_unicode_props`）、点線の円（U+25CC）の挿入、クラスタの形成、本来の方向への調整（`hb_ensure_native_direction`。RTLの文字体系の数字の列と地域指示子）。
  - 鏡像（逆向き）と、`vert` が無い縦書きでの `vertical_char_for` の置きかえ。
  - 正規化：`_hb_ot_shape_normalize` の5つのモード（`NONE`、`DECOMPOSED`、`COMPOSED_DIACRITICS`、`COMPOSED_DIACRITICS_NO_SHORT_CIRCUIT`、`AUTO`）と3つの段（分解、結合クラスでの並べかえ、合成）。分解と合成は、フォントに無いグリフを避ける規則（短絡の判定、異体字セレクタ、空白の代わりのグリフ）を含めて同じとする。
  - 分数（U+2044の前後の数字への `numr`、`dnom`、`frac`）のマスク、利用者の機能の範囲のマスク。
  - GSUB、既定で無視する文字の非表示または削除、異体字セレクタの扱い、送り幅と原点、GPOS、マークの幅の0化（`BY_GDEF_EARLY`、`BY_GDEF_LATE`、`NONE`）、フォールバックのマークの配置、フラグの伝播。

### GSUBとGPOSの適用

- lookupを、計画の段の順、段の中では番号の順に、バッファの全体へ1つずつ適用する。逆向きの連鎖の単一の置換（GSUBの型8）だけは、後ろから前へ適用する。
- 各lookupは、マスクが合い、lookupのフラグ（基底、合字、マークを無視、マークの付着の型、マークの絞り込みの集合）で飛ばされないグリフに対して、サブテーブルを順に試し、最初に適用できたものを使う。
- 飛ばすグリフの判定、ZWJとZWNJの自動の扱い、既定で無視する文字の扱い、合字の成分の番号と合字の番号（`lig_props`）、マークと合字の成分の対応、文脈のlookupの入れ子（最大64）、操作の数の上限、`UNSAFE_TO_BREAK` と `UNSAFE_TO_CONCAT` を立てる範囲は、`src/hb-ot-layout-gsubgpos.hh` と同じとする。
- GPOSの付着（マーク、筆記体）は、適用の間は鎖として持ち、`hb_ot_layout_position_finish_offsets` と同じく最後にずれへ変える。筆記体の右から左のフラグと、鎖の向きの付けかえも同じとする。
- GPOSの値と付着点は `em_mult` で拡大縮小し、ppemがあればDevice表の補正を足す（[ADR 0016](0016-font-and-vertical-metrics.md)）。
- GDEFが無いときは、Unicodeの一般カテゴリからグリフのクラスを合成する（`hb_synthesize_glyph_classes`）。

### 位置

- 横書きの送り幅と、縦書きの送り幅と原点は、[ADR 0016](0016-font-and-vertical-metrics.md) の関数で、拡大縮小した値を使う。縦書きでは、HarfBuzzと同じく原点をずれから引く。
- 空白の代わりの幅（`_hb_ot_shape_fallback_spaces`）、フォールバックのマークの配置（`_hb_ot_shape_fallback_mark_position`。付けかえた結合クラスとグリフの箱で配置し、マークの分類を付けかえる `recategorize_marks` を含む）、マークの幅の0化と位置の付けかえは、HarfBuzzと同じとする。
- フォールバックのカーニング（`_hb_ot_shape_fallback_kern`）は、HarfBuzzの `ot` のフォントの関数がカーニングを持たないため、何もしない。

### 既定のシェーパーとfallbackのシェーパー

- この ADR で、defaultのシェーパー（`hb-ot-shaper-default.cc`。正規化は `AUTO`、マークの幅の0化は `BY_GDEF_LATE`、フォールバックの配置あり）を実装する。ラテン文字、ギリシア文字、キリル文字、かな、漢字、記号など、[ADR 0024](0024-complex-shapers.md) のシェーパーに送られない文字体系は、ここで正しくなる。
- [ADR 0024](0024-complex-shapers.md) のシェーパーが実装されるまでは、そのシェーパーに送られる文字体系もdefaultの手順で処理する。結果はHarfBuzzと違いうるので、適合テストの期待する失敗の一覧に載る。
- HarfBuzzの `fallback` のシェーパー（`src/hb-fallback-shape.cc`。cmapと送り幅だけで形を作る）も `shape_fallback` として実装する。`hb-shape --shaper=fallback` のテストに使う。

### `sofel-cli` の `sofel-shape`

- `sofel-shape <フォント> [<文字列>]` を足す。`hb-shape` と同じオプションと出力とする。
- 解釈するオプション：文字列（`--text`、`--text-file`、`--unicodes`、`--unicodes-before`、`--unicodes-after`、`--text-before`、`--text-after`）、区間（`--direction`、`--script`、`--language`、`--default-language`）、機能（`--features`）、フォント（`--face-index`、`--font-size`、`--font-ppem`）、バッファ（`--bot`、`--eot`、`--preserve-default-ignorables`、`--remove-default-ignorables`、`--invisible-glyph`、`--not-found-glyph`、`--not-found-variation-selector-glyph`、`--cluster-level`、`--utf8-clusters`、`--unsafe-to-concat`、`--safe-to-insert-tatweel`、`--verify`、`--normalize-glyphs`、`--glyphs`、`--single-par`）、出力（`--output-format=text|json`、`--no-glyph-names`、`--no-positions`、`--no-advances`、`--no-clusters`、`--show-text`、`--show-unicode`、`--show-line-num`、`--show-flags`、`--show-extents`、`--ned`）、シェーパー（`--shaper=ot` と `fallback`）。
- `hb-shape` と同じく、`--utf8-clusters` が無ければクラスタの値を文字の番号に付けなおす。
- `--verify` は、HarfBuzzの `HB_BUFFER_FLAG_VERIFY`（`src/hb-buffer-verify.cc`）と同じ検査（`UNSAFE_TO_BREAK` でない位置で切って作りなおしても結果が変わらないこと、`UNSAFE_TO_CONCAT` の検査）を、`diff` を使って `sofel-cli` の中で行う。
- 解釈しないオプション（`--variations`、`--named-instance`、`--font-ptem`、`--font-slant`、`--font-bold`、`--font-grade`、`ot` と `fallback` 以外の `--shaper` など）は、使い方の誤りとして終了コード2で終わる。`--font-funcs` と `--face-loader` は受け付けて無視する。
- 書式と解釈は `sofel-cli` のライブラリの関数とし、適合テストはプロセスを起動せずに同じ関数を呼ぶ。

## データ構造

- バッファの1要素は、HarfBuzzの `hb_glyph_info_t` の `var1`、`var2` の共用体を使わず、名前のついた欄を持つ構造体とする：文字かグリフID、マスク、クラスタ、Unicodeの性質（一般カテゴリ、無視する文字、書記素の続き、付けかえた結合クラス）、グリフの性質（GDEFのクラスと、置換・合字・複数で作られた印）、`lig_props`、音節の番号、シェーパーの分類と位置（[ADR 0024](0024-complex-shapers.md)）。HarfBuzzはシェーパーごとに共用体の欄を使いまわすが、sofelは欄を分け、どの段がどの欄を使うかを型で示す。要素の大きさは計測で決め、まずHarfBuzzと同じ20バイトに収める。
- 位置は別の配列とし、付着の鎖の情報もそこに置く。
- 長さの変わる置換のために、HarfBuzzと同じく入力の配列と出力の配列を持ち、出力が入力より長くならない間は同じ配列に書く（`have_separate_output`）。配列は `UnicodeBuffer` と `GlyphBuffer` の間で受け渡し、呼び出しをまたいで再利用する。
- 計画は、使うlookupごとに次の要約を持つ。
  - そのlookupの全サブテーブルのCoverageのグリフIDの要約（HarfBuzzの `hb_set_digest_t` と同じ、いくつかのずらし幅で重ねたビットのマスク）。バッファのグリフの要約と交わらないlookupは、バッファを走査せずに飛ばす。グリフごとにも、まず要約で弾いてからCoverageを引く。
  - サブテーブルのビューの配列（拡張をたどった先）と、それぞれのCoverageの要約。
  - 要約は計画を作るときに一度だけ作る。表の読み取りは遅延させず、重い前処理は計画を作るときに行う（[ADR 0003](0003-crate-layers.md)）。
- CoverageとClassDefの引き方は、[ADR 0007](0007-sfnt-read-foundation.md) と同じく、ビッグエンディアンのバイト列のまま二分探索する。HarfBuzzの持つClassDefとCoverageの結果のキャッシュ（文脈の形式2、対の形式2）は、計測で必要と示せたら、呼び出し側の作業領域として足す。

## 適合テスト

- `sofel-conformance` に、[ADR 0005](0005-conformance-tests.md) のシェーピングの適合テストを作る。`in-house`、`aots`、`text-rendering-tests` の `*.tests` のすべての行を読み、`sofel-shape` の解釈と書式の関数で形を作り、期待値と比べる。
- `run-tests.py` と同じく、期待値が `*` でなければ `--verify`、`--unsafe-to-concat`、`--safe-to-insert-tatweel` を加える。結果が期待値と違い、結果が `gid` を含むときは、`run-tests.py` と同じく両方を `--glyphs --no-glyph-names` で読みなおしてグリフIDで比べなおす。`@font-funcs=`、`@face-loader=` の指示は無視する。
- 期待する失敗の一覧は `conformance/expected-failures.txt` とし、行ごとに理由を次の分類の1つでコメントに書く。
  - `shaper:<名前>`：[ADR 0024](0024-complex-shapers.md) のシェーパーが未実装の間だけ。
  - `variations`：可変フォント（`--variations`、FeatureVariations、ItemVariationStoreの補正、`--font-ptem` による `opsz`）。
  - `aat`：AATと古い `kern` 表、`--font-ptem` による `trak`。
  - `synthetic`：`--font-bold`、`--font-slant`、`--font-grade`。
  - `cff`、`color`：[ADR 0021](0021-cff-outlines.md)、[ADR 0023](0023-color-glyphs.md) が実装されるまでのグリフの名前と箱。
  - `out-of-scope:<名前>`：範囲の外のシェーパー（`graphite2`、`coretext`、`directwrite`、`uniscribe`、`harfrust`、`kbts`）。[ADR 0001](0001-project-goal-and-scope.md) により常に残る。
  - 分類に当てはまらない失敗は一覧に載せず、直す。
- この ADR と [ADR 0024](0024-complex-shapers.md) の実装の完了の条件は、一覧に `variations`、`aat`、`synthetic`、`out-of-scope:*`（と、そのときに未実装の [ADR 0021](0021-cff-outlines.md)、[ADR 0023](0023-color-glyphs.md) の分の `cff`、`color`）の行だけが残ることとする。この ADR の実装の完了の時点では、加えて `shaper:*` の行が残ってよい。
- HarfBuzzとの突き合わせ（[ADR 0016](0016-font-and-vertical-metrics.md) の `tools/hb-dump/` に足す）。`hb-dump` は `LC_ALL=C` で走らせ、HarfBuzzの言語を「なし」に揃える。
  - `--shape <フォント> <文字列のファイル> <方向> <機能>`：行ごとに `hb_buffer_add_utf8`、`hb_buffer_guess_segment_properties`、`hb_shape` を行い、`hb-shape` の形（UTF-8のクラスタ、フラグ）で書く。`sofel-conformance` で、次の組の全行の完全一致を確かめる。
    - `perf/benchmark-shape.cc` のすべての組（Roboto-Regularと英文2つ、SourceSerifVariable-Romanと `react-dom.txt`、NotoSansDevanagari-Regularと `hi-words.txt`、Amiri-Regular、NotoNastaliqUrdu-Regular、Gulzar-Regularとペルシア語の文章、NotoSansDuployan-Regularと `duployan.txt`）。可変フォントは既定のインスタンスとし、FeatureVariationsの条件が既定の座標で成り立つフォントは `variations` に分類する。
    - `test/subset/data/fonts/Mplus1p-Regular.ttf` と `ja-runs.txt`（LTRとTTB、機能なし、`vrt2`、`-vert`、`palt`、`vkrn`）。
  - `--shape-random <フォント> <件数> <種>`：文字体系ごとの網羅のため、HarfBuzzのテストのすべてのフォントについて、cmapにある文字を文字体系ごとに分け、決まった種の疑似乱数で長さ1〜16の文字列を件数だけ作り、HarfBuzzとsofelの結果を比べる。乱数の生成は `hb-dump` と `sofel-conformance` で同じ式（線形合同法）を使い、文字列そのものも書き出して照合する。件数は文字体系あたり200とする。期待する失敗は上と同じ分類で管理する。
  - `--unicode` の出力（[ADR 0019](0019-unicode-character-data.md)）に、全文字の `modified_combining_class`、`is_default_ignorable`、`space_fallback_type`、`vertical_char_for` の列を足す。
  - `--features`：機能の文字列の集まり（下のテスト設計）を `hb_feature_from_string` で読んだ結果と、`hb_feature_to_string` の結果。
  - `--ot-tags`：`sofel-unicode` の全文字体系と、言語の集まりのすべての組の `hb_ot_tags_from_script_and_language` の結果。言語の集まりは、`hb-ot-tag-table.hh` に現れるすべての言語と、生成された関数の分岐に現れるすべての下位タグの組み合わせ（主たる言語 × 文字体系の下位タグ × 地域の下位タグ × 私用の下位タグの有無）とする。逆向きに、現れるすべてのOpenTypeのタグの組の `hb_ot_tags_to_script_and_language` の結果も書く。
  - `--layout-map`：全テストのフォントと、全文字体系のタグについて、GSUBとGPOSで選ばれる文字体系と言語の番号。
  - `--serialize`：テストのフォントで形を作った結果の、テキストとJSONの書き出しのすべての指定の組。読み込みは、書き出した文字列と、`test/api/test-buffer.c` の直列化の入力を読んだ結果を書く。

## 計測

- [ADR 0013](0013-benchmark-harness.md) の `bench/` に、項目「シェーピング」を足す。HarfBuzzの `perf/benchmark-shape.cc` と同じく、文章を行ごとに区切り、行ごとに区間の性質を推定してシェーピングする。sofelは組ごとに計画を1つ作って使いまわし、HarfBuzzは `hb_shape`（内部の計画のキャッシュを使う）を呼ぶ。HarfBuzzのフォントの関数は `ot` とする（[ADR 0006](0006-performance-measurement.md)）。組は、`perf/benchmark-shape.cc` のすべての組と、Mplus1p-Regularと `ja-runs.txt`（LTRとTTB）とする。文字体系ごとのシェーパーの組は、そのシェーパーの実装のプルリクエストで有効にする。
- 項目「最初のシェーピング」を足す。フォントのバイト列から、計画を作って1行目をシェーピングするまで（[ADR 0006](0006-performance-measurement.md) の冷えた状態）。
- 計測の前に、全行で両者の結果（グリフID、クラスタ、位置、フラグ）が一致することを確かめる。

## C言語のAPI

[ADR 0015](0015-c-abi.md) の決まりに従い、次を足す。

| 関数 | 中身 |
|---|---|
| `sofel_feature_parse`、`sofel_feature_format` | `Feature::parse`、`format`。`sofel_feature_t` は `tag`、`value`、`start`、`end` の構造体 |
| `sofel_script_from_string`、`sofel_language_create`、`sofel_language_destroy` | `Script::from_iso15924`、`Language` |
| `sofel_ot_tags_from_script_and_language`、`sofel_ot_tags_to_script_and_language` | タグの変換 |
| `sofel_segment_guess(utf8, len, in, default_language, out)` | `SegmentProperties::guess` |
| `sofel_shape_plan_create(font, segment, features, n, error)`、`sofel_shape_plan_destroy`、`sofel_shape_plan_shaper` | `ShapePlan`。計画は不変で、スレッドの間で共有してよい |
| `sofel_buffer_create`、`sofel_buffer_destroy`、`sofel_buffer_clear`、`sofel_buffer_add_utf8`、`_utf16`、`_utf32`、`_codepoints`、`sofel_buffer_set_flags`、`sofel_buffer_set_cluster_level`、`sofel_buffer_set_*_glyph` | `UnicodeBuffer`。スレッドごとに持つ |
| `sofel_shape(font, scale, plan, buf)` | `shape` と `into_unicode` を、同じバッファの値の中で行う |
| `sofel_buffer_glyphs(buf, gids, clusters, flags, pos4, cap)` | 結果を、位置を1グリフ4つの形で書く。足りなければ要る数を返す（Golemの `golem_font_shape` の形） |
| `sofel_buffer_serialize`、`sofel_buffer_deserialize_glyphs`、`sofel_buffer_deserialize_unicode` | 直列化 |

## 後回しにしたもの

次は後の段階に依存するため、この ADR と [ADR 0024](0024-complex-shapers.md) の実装のあとに、それぞれADRを作成する。

- **可変フォントのシェーピング**（3段階目の可変フォントのADRのあと）：GSUBとGPOSのFeatureVariations（`rvrn` での置きかえを含む）の読み取りと検証器と選択（`hb_ot_layout_table_find_feature_variations`）、GDEFのItemVariationStoreとVariationIndexの補正、HVARとVVARによる送り幅、付着点の変化、計画の鍵への座標の追加、`--variations`、`--named-instance`、`--font-ptem` による `opsz`。既定のインスタンスでも、条件が0を含むFeatureVariationsのあるフォントでは、それまでHarfBuzzと結果が違いうる。テストは `variations` に分類したもの。
- **AATのシェーピング**（[ADR 0001](0001-project-goal-and-scope.md) の10段階目）：morx、mort、kerx、ankr、trak、feat、ltagの読み取りと検証器（gxvalidとの照合）、HarfBuzzの `hb-aat-layout*`、計画のAATの分岐（`apply_morx`、`apply_kerx`、`apply_trak`、dumberのシェーパー）、`--font-ptem` による `trak`。古い `kern` 表（OpenTypeとAppleの形式0〜3）も、HarfBuzzがkerxの部分の表（`AAT::KerxSubTableFormat*`）で読むため、ここで扱う。それまでは、`kern` 表だけを持つフォントはカーニングされない。テストは `aat` に分類したもの。
- **合成の太字と斜体**：`hb_font_set_synthetic_bold`、`hb_font_set_synthetic_slant` と、それによる送り幅と箱の変化。HarfBuzzは浮動小数点で計算するため、決定性の扱いを決めるADRが要る。テストは `synthetic` に分類したもの。
- CFFとカラーグリフのグリフの名前と箱は、それぞれ [ADR 0021](0021-cff-outlines.md)、[ADR 0023](0023-color-glyphs.md) の実装で一致するようになる。新しいADRは要らない。

## 実装しないこと

- Graphite、wasmのシェーパー、CoreText、DirectWrite、Uniscribe、HarfRust、kb_text_shapeのシェーパー（[ADR 0001](0001-project-goal-and-scope.md)）。
- 文字列の区間への分割（文字体系、方向、フォント、縦書きの向きによる区切り）と、双方向テキストの並べかえ（[ADR 0001](0001-project-goal-and-scope.md)）。縦書きの向きの区切りに使う文字の性質は [ADR 0019](0019-unicode-character-data.md) が持つ。
- ロケールからの言語の推定（上の「区間の性質」）。
- lookupの集め方やグリフの閉包などのAPI（`hb_ot_layout_lookups_substitute_closure` など）。サブセット化（[ADR 0022](0022-subsetting.md)）で扱う。
- JSTFとMATHの表。シェーピングでは使わない。
- `hb_shape_justify`（実験的なAPI）と、HarfBuzzのバッファのメッセージの関数（`hb_buffer_set_message_func`）。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- `sofel-read`：GDEF、GSUB、GPOSの各構造を、[ADR 0007](0007-sfnt-read-foundation.md) のテストの組み立て方で手で組み、読み取りを確かめる。
  - Coverageの形式1〜4、ClassDefの形式1〜4で、範囲の中、外、境界のグリフID。Deviceの形式1〜3の補正とVariationIndexの0。
  - GSUBの型1〜8、GPOSの型1〜9の各形式（24bitの形式を含む）について、ビューの各欄。拡張から中身へたどること、拡張の入れ子が誤りになること。
  - ValueRecordの全てのValueFormatのビットの組、Anchorの形式1〜3。
  - GDEFの拒否の一覧の各項目が拒まれること。
  - 検証器：上に挙げた各誤りについて、期待する種類、重大度、オフセットを確かめる。[ADR 0007](0007-sfnt-read-foundation.md) の不変条件を、変異させた入力で確かめる。
- 機能の文字列：HarfBuzzの `test/api/test-common.c` の機能の文字列の例をすべて写したテストと、`tools/hb-dump --features` との突き合わせ。突き合わせの文字列の集まりは、タグ（4文字、短い、長い、引用符つき）、前置き（なし、`+`、`-`）、範囲（なし、`[]`、`[:]`、`[3]`、`[3:]`、`[:5]`、`[3:5]`、不正）、値（なし、`=0`、`=2`、` on`、` off`、不正）、前後の空白の直積とする。
- タグ：`test/api/test-ot-tag.c` の例をすべて写したテストと、`tools/hb-dump --ot-tags` との突き合わせ。`tools/gen-ot-tag/` の単体テストで、生成された関数の各分岐の形を読み取れることと、知らない形で失敗することを確かめる。
- 区間の性質：文字体系の推定（先頭がCommonやInheritedの列、空の列）、方向の推定（RTLの文字体系、方向の無い文字体系）、言語の正規化（`ja_JP.UTF-8` → `ja-jp`）、`default_language` の使い方を確かめる。
- バッファ：UTF-8、UTF-16、UTF-32、Latin-1の不正な列の置きかえとクラスタの値、文脈（前後の5文字）を、HarfBuzzの `test/api/test-buffer.c` の例と `src/hb-utf.hh` の規則から作った例で確かめる。
- 直列化：`test/api/test-buffer.c` の直列化の例をすべて写したテストと、`tools/hb-dump --serialize` との突き合わせ。読み込みは、書き出したものを読みなおして元に戻ること（往復）も確かめる。
- `diff`：`hb_buffer_diff` の各フラグが立つ組を手で作って確かめる。
- 計画：手で組んだGSUBとGPOSを持つフォントで、文字体系と言語の選択、機能の順、マスクのビットの割り当て（ビットが足りない場合を含む）、段とlookupの並び、縦書きでの `vert` の全体からの検索を確かめる。
- 手順の各段：手で組んだフォントで、点線の円、書記素の印とクラスタの4つの水準、正規化の5つのモード（分解、並べかえ、合成、フォントに無いグリフ）、分数、既定で無視する文字の3つの扱い、マークの幅の0化の3つの時期、フォールバックのマークの配置と空白の幅、`UNSAFE_TO_BREAK` と `UNSAFE_TO_CONCAT` の範囲、拡大縮小とDevice表を、HarfBuzzの対応する関数の振る舞いから作った例で確かめる。
- 実際のフォントでの完全一致は、上の「適合テスト」で確かめる。
- `sofel-shape`：出力の書式（`--ned`、`--no-advances` の足し合わせ、`--show-flags`、`--show-extents`、JSON、`gid` の名前）を、決まった結果から期待する文字列が出ることで確かめる。解釈しないオプションで終了コード2になることを確かめる。
- `Send` と `Sync`：`ShapePlan`、`GlyphBuffer`、`UnicodeBuffer` について確かめる。
- 決定性：適合テストを、Linux（x86-64）とmacOS（aarch64）のランナーで走らせ、同じ期待する失敗の一覧で通ることを確かめる。`sofel-shape` の単体テストを `wasm32-wasip1` で通し、「HarfBuzzとの突き合わせ」の `--shape` の組を、`wasm32-wasip1` でビルドした小さな道具でもシェーピングして、ネイティブの結果と一致することをCIで確かめる。
- ファジング：`fuzz/` に、任意のバイト列のフォントと任意の文字列と任意の機能の文字列で、計画を作ってシェーピングする対象を足す。panicしないこと、`MonotoneGraphemes` と `MonotoneCharacters` で出力のクラスタが単調であること、操作の数の上限で止まることを確かめる。GDEF、GSUB、GPOSのビューと検証器の対象と、直列化の読み込みの対象も足す。
- C言語のAPI：[ADR 0015](0015-c-abi.md) のCのテストに、Golemの `golem_font_shape` と同じ呼び出しの列（作成、推定、計画、シェーピング、結果の取り出し、要る数を返す場合）を足す。
- 計測：上の「計測」の項目が、`--once` で落ちず、結果の照合が通ることをCIで確かめる。

## トレードオフ

- 計画を呼び出し側が持つため、`hb_shape` のように「フォントと文字列と機能を渡すだけ」の呼び方ができない。呼び出し側は、区間の性質と機能の組ごとに計画を作って覚える必要がある。代わりに、計画は不変で、スレッドの間で自由に共有でき、作りなおしの費用が利用者に見える。
- lookupの要約を計画が持つため、同じフォントで計画が複数あれば、同じlookupの要約を重ねて持つ。HarfBuzzはフォントの値に1つだけ持つ。多くの言語と文字体系を混ぜる利用者で計画の数が増え、計測で問題になれば、フォントごとの要約を別の値に分けるADRを作成する。
- ロケールを読まないため、`default_language` を渡さない呼び出し側は、`hb_buffer_guess_segment_properties` と結果が違う場合がある（HarfBuzzはロケールの言語のLangSysと `locl` を使う）。暗黙の大域の状態をなくし、どの環境でも同じ結果にすることを優先する。HarfBuzzと同じ結果が要る呼び出し側は、ロケールを読んで渡せばよい。Golemのプロセスでロケールが設定されていれば、いまのGolemはロケールの言語のLangSysでシェーピングしている。置きかえのときに、Golemの側で確かめる。
- 言語のタグの表と例外を、IANAの登録とOpenTypeの言語のタグの一覧からでなく、HarfBuzzの生成したヘッダから作る。HarfBuzzの生成のスクリプト（Python、約1500行、手の調整を含む）の入力は版が固定されておらず、再現できない。HarfBuzzの固定のコミットと同じ表になることを優先する。ヘッダの生成の形が変わると道具が失敗するので、固定のコミットを上げるときに気づける。
- バッファの要素で、シェーパーごとの欄を共用体にせず分けるため、要素が大きくなりうる。欄の意味が型で分かり、段の取り違えを防げる。大きさが速さに効くと計測で示せたら、詰め方を見直す。
- 縦書きで `vrt2` と `vkrn` を既定で有効にしないのはHarfBuzzの判断で、そのまま従う。
- 入力と出力の配列を共用する方式は、関数型のスタイル（[ADR 0003](0003-crate-layers.md)）から外れる。バッファの書きかえは、[ADR 0003](0003-crate-layers.md) が例に挙げた、計測で必要な内側の繰り返しに当たり、`shape` の外からは値を受け取って値を返す形を保つ。
- `--shape-random` の突き合わせは、HarfBuzzのテストのフォントにある文字体系だけを網羅する。テストのフォントに無い文字体系は、ほかの文字体系と同じシェーパーの経路で確かめられたものとみなす。

## 参考文献

- HarfBuzz 15.1.0
  - `src/hb-buffer.hh`、`src/hb-buffer.cc`、`src/hb-buffer.h`、`src/hb-buffer-serialize.cc`、`src/hb-buffer-deserialize-json.rl`、`src/hb-buffer-deserialize-text-glyphs.rl`、`src/hb-buffer-deserialize-text-unicode.rl`、`src/hb-buffer-verify.cc`、`src/hb-utf.hh`、`src/hb-limits.hh`、`src/hb-font.hh`
  - `src/hb-common.cc`（`hb_feature_from_string`、`hb_feature_to_string`、`hb_language_from_string`、`hb_language_get_default`、`hb_script_get_horizontal_direction`）
  - `src/hb-shape-plan.cc`、`src/hb-ot-shape.cc`、`src/hb-ot-shape.hh`、`src/hb-ot-map.cc`、`src/hb-ot-map.hh`、`src/hb-ot-shape-normalize.cc`、`src/hb-ot-shape-normalize.hh`、`src/hb-ot-shape-fallback.cc`、`src/hb-fallback-shape.cc`、`src/hb-ot-shaper.hh`、`src/hb-ot-shaper-default.cc`
  - `src/hb-ot-layout.cc`、`src/hb-ot-layout.hh`、`src/hb-ot-layout-common.hh`、`src/hb-ot-layout-gsubgpos.hh`、`src/hb-ot-layout-gdef-table.hh`、`src/OT/Layout/Common/`、`src/OT/Layout/GDEF/`、`src/OT/Layout/GSUB/`、`src/OT/Layout/GPOS/`、`src/hb-config.hh`
  - `src/hb-ot-tag.cc`、`src/hb-ot-tag-table.hh`、`src/gen-tag-table.py`、`src/hb-unicode.hh`
  - `util/hb-shape.cc`、`util/shape-options.hh`、`util/font-options.hh`、`util/text-options.hh`、`util/output-options.hh`
  - `test/shape/run-tests.py`、`test/shape/data/*/tests/*.tests`、`test/api/test-common.c`、`test/api/test-ot-tag.c`、`test/api/test-buffer.c`、`perf/benchmark-shape.cc`、`perf/texts/`、`perf/fonts/`
- FreeType `src/otvalid/`（2.15.0）
- Golem `modules/golem-font/cbits/golem/golem_font.h`、`golem_font.c`
- OpenType Specification：GDEF、GSUB、GPOS、OpenType Layout Common Table Formats、Script Tags、Language System Tags https://learn.microsoft.com/en-us/typography/opentype/spec/
- ISO/IEC 14496-22 第5版（24bitのグリフIDのlookupの形式）
- IANA Language Subtag Registry https://www.iana.org/assignments/language-subtag-registry/
- BCP 47 Tags for Identifying Languages https://www.rfc-editor.org/info/bcp47
- HarfRust 0.15.0 https://github.com/harfbuzz/harfrust
