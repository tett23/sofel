# sofel の仕様

この文書は、採択したADRから、現在の仕様をまとめる。
決定の経緯は各ADRを参照。

## 目的

HarfBuzzのシェーピングと、FreeTypeのグリフの読み込み、ヒンティング、ラスタライズを、1つのRustの実装で提供する（[ADR 0001](adr/0001-project-goal-and-scope.md)）。
品質の優先順位は、高速性を最優先とし、正しさの基準はHarfBuzzとFreeTypeの出力とする。

## 範囲

対象とする機能と対象としない機能は、[ADR 0001](adr/0001-project-goal-and-scope.md) の「対象とする範囲」と「実装しないこと」のとおり。
要点は次のとおり。

- 形式：sfnt（TrueType、OpenType/CFF、CFF2、コレクション、dfont）、WOFF、WOFF2、Type1（Multiple Mastersを含む）、CID、Type42、PFR、BDF、PCF、FNT、FONの読み込み。gzip、LZW、bzip2で包んだフォントの展開。AppleのHVFは扱わない。sfnt以外の形式も、sfntと同じ形（文字からグリフへの対応、メトリクス、輪郭またはビットマップ）で扱う。
- 表の検証器：各表と各形式の構造の誤りを、場所と理由つきで報告する。読み取りを足すときは、同じものの検証器も足す。
- シェーピング：HarfBuzzのOTシェーパーと同じ処理と、AATのシェーピング。Graphiteとプラットフォームのシェーパーは扱わない。
- 輪郭：glyf、CFF、CFF2、VARC、Type1とCIDのcharstring、PFR。可変フォントとMultiple Mastersの補間を含む。
- ヒンティング：TrueTypeバイトコード（FreeTypeのv35とv40）、CFF、Type1、CID、自動ヒンタ。
- ラスタライズ：アンチエイリアス、モノクロ、LCD向けのサブピクセル、SDF。
- カラーグリフと絵文字：COLRv0、COLRv1、埋め込みビットマップ（EBDT、CBDT、sbix）、SVGのグリフ。
- フォントの書き出し：sfntの書き出し、サブセット化、可変フォントの固定。sfnt以外の形式と、WOFF、WOFF2への書き出しは扱わない。
- 出力：SVGとPDFへの書き出し、GPUでの描画のためのデータとシェーダー（Slugの方式）。GPUの装置とAPIは扱わない。
- 行分割、双方向テキスト、フォントの選択は扱わない。
- C言語のAPIは、sofel独自のABI（`sofel-capi`）として持つ（[ADR 0015](adr/0015-c-abi.md)）。HarfBuzzやFreeTypeと互換のABI（`hb_*`、`FT_*`）は扱わない。

## 構成

Cargoのワークスペースで、依存は次のとおりとする（[ADR 0003](adr/0003-crate-layers.md)、`sofel-unicode` は [ADR 0019](adr/0019-unicode-character-data.md)）。矢印の先が依存先である。

```
フォントに依存しないクレート
  sofel-geom
  sofel-unicode
  sofel-inflate
  sofel-lzw
  sofel-bzip2
  sofel-brotli
  sofel-png    → sofel-inflate
  sofel-svg    → sofel-inflate, sofel-geom
  sofel-raster → sofel-geom
  sofel-gpu    → sofel-geom
  sofel-vector → sofel-geom

フォントの形式を読むクレート
  sofel-read        （sfnt、TTC、dfont）
  sofel-type1       （Type1、MM、CID、Type42、AFM、PFM）
  sofel-pfr
  sofel-bitmapfont  （BDF、PCF、FNT、FON）

フォントを扱うクレート
  sofel-woff    → sofel-inflate, sofel-brotli
  sofel-font    → sofel-read, sofel-type1, sofel-pfr, sofel-bitmapfont, sofel-geom
  sofel-shape   → sofel-font, sofel-unicode
  sofel-outline → sofel-font, sofel-geom
  sofel-bitmap  → sofel-font, sofel-png
  sofel-color   → sofel-outline, sofel-bitmap, sofel-svg, sofel-geom
  sofel-write   → sofel-read
  sofel-subset  → sofel-font, sofel-write, sofel-geom
  sofel-cli     → 上のすべて（道具のバイナリ群）
  sofel-capi    → 上のすべて（C ABI、[ADR 0015](adr/0015-c-abi.md)）
```

- `sofel-color` と `sofel-subset` の `sofel-geom` への依存は、HarfBuzzの丸め（`sofel_geom::float`）を共有するためである（[ADR 0026](adr/0026-harfbuzz-rounding.md)）。

- 輪郭と塗りは、`sofel-geom` の `Pen` と `Painter` で受け渡す。`sofel-raster`、`sofel-gpu`、`sofel-vector` はフォントの型を知らない。
- 形式のビューと検証器は、形式を読むクレートだけに置く。検証器は確保をせず、誤りを呼び出し側の関数に渡す。charstringの解釈とヒンティングは `sofel-outline` が行い、Type1とCFFで共有する。ただし、FreeTypeのType 2 charstringの解読器（演算子から装置の空間の線と曲線まで）は、全グリフを走らせる `CFF ` と `CFF2` の検証器が使うため `sofel-read` に置き、点への変換と仕上げを `sofel-outline` が行う（[ADR 0021](adr/0021-cff-outlines.md)）。
- `sofel-font` が形式を判別し、形式によらない形でフォントを表す。圧縮で包んだフォントの展開は、`sofel-font` に渡すまえに呼び出し側が行う（`sofel-cli` は自動で展開する）。
- ライブラリの値は不変で、内部可変性を使わない。キャッシュ（シェーピングの計画など）は、呼び出し側が持つ値とする。
- 公開の値はすべて `Send` と `Sync` を満たす。ライブラリの中でスレッドを作らない。
- ライブラリのクレートは `#![forbid(unsafe_code)]` とし、外部のクレートに依存しない。`unsafe` を使ってよいのは、Cから渡されたポインタを参照に変える `sofel-capi` だけである（[ADR 0015](adr/0015-c-abi.md)）。`scripts/check-deps.sh` が、ほかのクレートが `unsafe` を禁じていることを確かめる。
- ライブラリは `&[u8]` を受け取るだけで、ファイルを読まない。
- 検査には、必要に応じて形式手法（Kani、Verus、Creusotなど）を使ってよい。道具はライブラリの依存に入れない。

## 表の読み取り（`sofel-read`）

- 基本の型は `Tag`、`GlyphId`（`u32`。24bitのグリフIDを扱う）、`Fixed`、`F2Dot14`、`LongDateTime` とし、生の値だけを持つ（[ADR 0007](adr/0007-sfnt-read-foundation.md)）。
- 表のビューは、作るときにヘッダと配列の範囲を一度だけ検査する。作れないほど壊れていれば `Err(ReadError)` を返す。作れたビューの読み取りは、範囲外の入力に `None` を返し、既定の値で埋めない。既定の値の方針は `sofel-font` で決める。
- どの入力でもpanicしない。clippyの `indexing_slicing`、`arithmetic_side_effects` などを拒否にして強制し、ファジングで確かめる。
- 読める形式と表：sfnt（`0x00010000`、`OTTO`、`true`、`typ1`）、TTC/OTC、head、maxp、MAXP、hhea、HHEA、hmtx、HMTX、cmap（フォーマット0、2、4、6、8、10、12、13、14、15）、loca、LOCA、glyf、GLYF（[ADR 0011](adr/0011-glyf-outlines.md)）、OS/2（版0〜5。版5を超える版は版5の欄まで）、name（形式0と1）、post（版1、2、2.5、3、4とMacの標準の258の名前）、vhea、VHEA、vmtx、VMTX（hheaとhmtxのビューを表の名前だけ変えて使う）、VORG（版1と2）（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）、BASE（版1.0と1.1。Axis、BaseTagList、BaseScriptList、BaseScript、BaseValues、BaseCoordの形式1〜4、MinMaxとFeatMinMax、Deviceの表の見出し（形式1〜3とVariationIndex）。[ADR 0017](adr/0017-base-baselines.md)）、CFF、CFF2（[ADR 0021](adr/0021-cff-outlines.md)）、GDEF（版1.0、1.2、1.3と、HarfBuzzの32bitのオフセットを足した1.4）、GSUBとGPOS（版1.0、1.1、1.2）（[ADR 0020](adr/0020-shaping-foundation.md)）。glyfの検証器は、各グリフをlocaの項目から次の項目までとして読み、locaの壊れた箇所はlocaの検証器が報告する。
- [ADR 0016](adr/0016-font-and-vertical-metrics.md) の表の検証器は、OS/2の版と長さの不一致、`usWeightClass` が1〜1000の外、`fsSelection` でREGULARとITALICかBOLDが同時、`usFirstCharIndex` > `usLastCharIndex`、nameの領域の外の文字列と言語タグ、記録の並び（プラットフォーム、エンコーディング、言語、名前ID）の乱れ、UTF-16BE（プラットフォーム0と3）の奇数長、postの版2の領域を越える文字列と範囲外の名前の番号、廃止の版2.5、VORGのグリフIDの昇順の乱れと重複とグリフの数以上のIDを報告する。nameの文字列が表の外にある記録は、ビューでは `None` を返す（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。
- `CFF ` と `CFF2`（[ADR 0021](adr/0021-cff-outlines.md)）：ヘッダ、INDEX（16bitと32bitの数）、DICTの字句、Top DICTとFont DICTとPrivate DICT（FreeType 2.15.0の `cfftoken.h` の全項目を、FreeTypeの既定の値、64bitの `FT_Long` の数の変換、項目の型の切り詰め、同じ演算子は後が勝つ、スタックの上限（Top DICTで96、Private DICTで97、CFF2では513と514）のまま読む）、charset（予め定められた3つとフォーマット0〜2。`nLeft` のあふれはFreeTypeと同じく切りつめる）、encoding（予め定められた2つ、フォーマット0と1と補遺）、FDSelect（フォーマット0、3、4）、CFF2の `blend`（係数はFreeTypeの `cff_blend_build_vector`）を読む。Item Variation Storeは共通のモジュール `item_variation` に置く。charstringの解読器は、FreeTypeのAdobe engine（`psintrp.c`、`pshints.c`）からヒンティングと太らせを外して移植したもので、スタックの型、サブルーチン（入れ子16）、送り幅、ステムの数とマスク、flex、seac、算術の演算子、CFF2の `blend` と `vsindex`、長さ0の線を捨てる経路の規則を保つ。検証器は、ヘッダ、INDEX、Name INDEXの数、DICT（オペランドの過多と不足、壊れた実数、FreeTypeが拒む値はError、未知の演算子、版で許されない演算子、重複はWarning）、CharStringsの数（0はError、`maxp` との違いはWarning）、charset、encoding、FDArray（257以上はError）、FDSelect（フォーマット4はWarning）、Variation Store（`fvar` と軸の数が違えばError）、ローカルのサブルーチン、全グリフのcharstring（誤りはグリフIDと位置つきのError、未知の演算子はWarning）を確かめる。FDSelectのフォーマット4を除き、Errorの無い表はFreeTypeが開ける。
- CFFのcharsetの索引（[ADR 0025](adr/0025-cff-charset-index.md)）：`Charset::num_ranges` はフォーマット1と2の範囲のうち、FreeTypeと同じくグリフの数を覆うまでの数（フォーマット0と予め定められたcharsetは0）。`Charset::index_into` は、呼び出し側の配列に各範囲の最初のグリフIDを書き、それを借りる `CharsetIndex`（`Copy`、`Send`、`Sync`）を返す。配列が短ければ何も書かずに要る長さを返す。確保はしない。`CharsetIndex::sid` は範囲の最初のグリフIDを二分探索し、`CharsetIndex::glyph` は範囲を順に見る。値は0xFFFFでの切りつめを含め、常に `Charset::sid`、`Charset::glyph` と等しい。
- BASEの検証器は、表に収まらないオフセットの先、タグで引く記録（BaseTagList、BaseScriptList、BaseLangSys、FeatMinMax）がタグの昇順でないこと、BaseValuesの座標の数とBaseTagListのタグの数の不一致、未知のBaseCoordの形式を `Error` に、`defaultBaselineIndex` がタグの数以上であることを `Warning` にする。ビューの `find` は、並びが乱れていてもHarfBuzzと同じ記録を返すように、HarfBuzzの二分探索（`hb_bsearch_impl`）と同じ順に調べる（[ADR 0017](adr/0017-base-baselines.md)）。
- `cff::hb`（[ADR 0021](adr/0021-cff-outlines.md)）：HarfBuzz 15.1.0が `CFF ` と `CFF2` を読む手順を、FreeTypeの読み方とは別に持つ。サニタイザの範囲の確かめ（`check_point`、`check_range`。オフセットは符号つきで、表の前を指せば失敗）と、範囲ごとに減る操作の予算（表の長さの64倍、16384以上、2^30 − 1以下。尽きれば以後の確かめはすべて失敗）、`CFFIndex` の `sanitize` と `operator []`（逆順や最後を越えるオフセットは空の文字列、オフセット0は1バイト前から）と `get_size`（64bitの和を `int` のオフセットにするときの下位32bit）、charset（フォーマット0〜2をフェイスのグリフの数ちょうどで確かめ、切りつめない）、encoding（補遺は要素が0個なら誤り）、FDSelect（CFFは0と3、CFF2は4も。範囲の始まりはフェイスのグリフの数未満、FDはFDArrayの数未満、番兵はフェイスのグリフの数以下でCFF2ではCharStringsの数以上）、CFF2のItem Variation Storeの確かめと `get_region_index_count`、DICTの解釈器（513の `double` のスタック、`pop_int` の `int` の範囲と0への切り捨て、`pop_uint` の負の誤り、BCDの実数は `hb_number_parser_t` と同じ `double` の計算）である。
- レイアウトの表（[ADR 0020](adr/0020-shaping-foundation.md)）：
  - 共通の構造（`sofel_read::layout`）は、ScriptList、Script、LangSys、FeatureList、Feature、LookupList（GSUBとGPOSの版1.2の `lookupList2` は32bitのオフセット）、Lookup、Coverage（形式1〜4）、ClassDef（形式1〜4）、文脈と連鎖文脈のサブテーブル。形式3と4のCoverage、形式3と4のClassDefは24bitのグリフID（ClassDefの形式3はクラスも24bit）を持ち、`ClassDef::get` は `u32` を返す。版1.2は、32bitのオフセットが0でなければそれを、0なら16bitのオフセットを使う。版1.1のFeatureVariationsのオフセットは持つだけで読まない。Featureの引数のオフセットは、HarfBuzz 15.1.0と同じく、`size` も含めてFeatureの表から数える（HarfBuzz 15.1.0は、FeatureListから数える古いフォントの例外を持たない）。
  - GSUBは型1〜8、GPOSは型1〜9のすべての形式を読む。24bitの形式は、HarfBuzz 15.1.0の `MediumTypes` と同じ幅とする（文脈の形式4は規則の集合への24bitのオフセットと16bitの規則へのオフセット、形式5は24bitのオフセットと16bitのクラス、連鎖文脈の形式4は24bit、形式5は16bitの数と24bitのオフセット）。文脈の形式6（24bitのCoverageのオフセット）もHarfBuzzと同じく読む。Deviceは形式1〜3の補正（`Device::delta_pixels`、HarfBuzzの `get_delta_pixels`）を読み、VariationIndexの補正は0とする。
  - サブテーブルが直接持つCoverageとClassDef、MarkArray、AnchorMatrix、LigatureArrayは、オフセットが0か読めないとき空のものとして読む。HarfBuzzは壊れたオフセットを0に書きかえ（neuter）、空のNullの表として読むためである。ほかのオフセットをたどる関数は、空のオフセットに `Ok(None)` を、読めない先に `Err` を返す。拡張のサブテーブル（GSUBの型7、GPOSの型9）は `gsub::subtable`、`gpos::subtable` が中身の型へたどり、拡張の先が拡張なら `Err` とする。拡張のlookupの全サブテーブルが同じ型でなければHarfBuzzはlookupごと使わないので、`Lookup::extension_types_agree` で調べる。
  - `gdef::is_blocklisted` は、HarfBuzzの `GDEF::is_blocklisted` の40の組（GDEF、GSUB、GPOSの表の長さ）をそのまま持つ。
  - 検証器は、表に収まらないオフセットの先、未知の形式とlookupの型、ScriptListとLangSysの記録のタグの乱れ、Featureの番号とlookupの番号が数以上、Coverageの並びの乱れと範囲の重なり、範囲の `startCoverageIndex` の不一致、ClassDefの範囲の重なり、Coverageに対応する配列の長さの不一致、拡張の型の不一致と入れ子、文脈のlookupの記録の位置が入力の長さ以上、入力の無い文脈と成分の無い合字、マークの絞り込みの集合の番号がGDEFの集合の数以上、PairSetの2つ目のグリフの乱れを `Error` に、lookupのフラグとValueFormatの予約のビット、Coverageの同じグリフの重複を `Warning` にする。たどったオフセットと調べた記録の数を、表の1バイトあたり64（最低16384）の予算から引き、使いきるか1000件を報告したら `WorkLimitExceeded`（`Error`）で止める。文脈のlookupの参照の循環は報告しない。`validate::sfnt` は、GSUBとGPOSのマークの絞り込みの集合の番号を、GDEFのマークのグリフの集合の数（GDEFが無ければ0）と比べる。
- 検証器は各表の `validate(data, report)` で、`Diagnostic`（タグ、オフセット、重大度、種類）を1つずつ渡す。重大度は `Error`（ビューを作れない、または読み取りの結果が仕様の意図と異なる）と `Warning`（仕様に反するが読み取りに影響しない）とする。`Error` の無い表は、ビューを作れ、範囲内の読み取りが `None` を返さない。

## 固定小数点と `Pen`（`sofel-geom`）

- `F26Dot6`、`Fixed`（16.16）、`F2Dot14` と、FreeTypeと同じ結果を返す `mul_fix`、`div_fix`、`mul_div`、`mul_div_no_round`、`vector_length`（CORDIC）、`transform` を持つ（[ADR 0009](adr/0009-geom-fixed-point-and-pen.md)）。
- 輪郭は `Pen`（`move_to`、`line_to`、`quad_to`、`cubic_to`、`close`）で受け渡す。命令の並びは `FT_Outline_Decompose` と同じ約束に従う。`Point` の単位（26.6かフォントの単位）は作る側が決める。
- `sofel_geom::float` は、HarfBuzzの値を写すための浮動小数点の補助を持つ（[ADR 0026](adr/0026-harfbuzz-rounding.md)）。Cの `floor`、`floorf`、`ceilf`、`truncf`、HarfBuzzの `roundf`（`hb-algs.hh` の `_hb_roundf` のマクロ。`hb_roundf(f64) -> f64` と `hb_roundf32(f32) -> f64`）、`hb_clamp_to<int> (double)`（`hb_clamp_to_i32`）と、整数と `float` の変換。HarfBuzzの `roundf` はCの `roundf` ではなく、値を倍精度に広げてから0.5を足し、床関数をとる。半分の値は負でも正の無限大の側へ丸まる（−2.5は−2）。sofelがHarfBuzzの値を写す箇所の `roundf` は、すべてこの意味とし、`sofel-font`、`sofel-outline`、`sofel-color`、`sofel-subset` はここを使う。HarfBuzzの式の型の混ざり方はC++の通常の算術変換に従って写し（整数と `float` の割り算は `float` で行う）、四則演算と床関数のほかは使わない。FMAは使わない。

## Unicodeの文字の性質（`sofel-unicode`）

- Unicode 18.0.0（HarfBuzz 15.1.0の表と同じ版）の文字の性質を、確保をせずに `const` の表から引く（[ADR 0019](adr/0019-unicode-character-data.md)）。HarfBuzzの固定のコミットを上げるときは、同じプルリクエストで `ucd.lock` の版を揃える。
- 性質は、General_Category（値はHarfBuzzの `hb_unicode_general_category_t` の順）、Canonical_Combining_Class、Script（ISO 15924のタグ。Common、Inherited、Unknownは `Zyyy`、`Zinh`、`Zzzz`）、Bidi_Mirroring_Glyph、正規分解の1段と合成（HarfBuzzの `decompose` と `compose` と同じ形。ハングルの音節は計算で扱う。合成の除外は `Full_Composition_Exclusion`）、Extended_Pictographic、Vertical_Orientation（UAX #50）とする。範囲の外（0x110000以上を含む）は、HarfBuzzと同じ既定の値（Cn、0、Unknown、鏡像なし、分解なし）を返し、Vertical_Orientationは `@missing` の `R` を返す。U+10FFFFを超える文字は合成しない。
- `Script::from_iso15924` は、大文字小文字を直し、HarfBuzzの読みかえ（`Aran` などの変種）と、UCDの別名 `Qaai`（Inherited）と `Qaac`（Coptic）を当てる。4文字の英字でなければ `None` とする。
- Vertical_Orientationは、縦書きの文字列を、正立で組む区間と、横に組んで回す区間に分けるために呼び出し側が使う。区間の分割そのものは範囲の外とする。
- 表は `tools/gen-unicode/`（ワークスペースの外のRustのパッケージ）が、`scripts/fetch-ucd.sh` で取得したUCDのテキストのファイル（`ucd.lock` でURLとSHA-256を固定）から生成し、コミットする。性質ごとに2段か3段の表とし、分け方（`k2`、`k3` はそれぞれ0〜8）は合計のバイト数が最小のものを選ぶ。表の下限と上限は既定でない値を持つ最初と最後の文字で決める。生成の道具は、書き出すまえに0〜0x10FFFFのすべての文字で詰めた表と元の値の一致を確かめる。入力が同じなら出力は同じバイト列になる。
- 同じ道具が、記号フォントのcmapの付けかえ（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）に使う、HarfBuzzの `gen-arabic-pua.py` と同じアラビア文字の私用領域の対応の表を `sofel-font` に書く。シェーパーの分類の表（indic、USE、アラビア文字、母音の組）は、`sofel-shape` を作るときに同じ道具で生成する。

## フォント（`sofel-font`）

- `Font::new(data, index)` は、sfntとTTC/OTCのフォントを開き、head、グリフの数、横書きのメトリクス、文字の対応に使うcmapのサブテーブルを一度だけ読む（[ADR 0010](adr/0010-font-foundation.md)）。不変で、キャッシュを持たない。
- グリフの数は `MAXP` を `maxp` より優先する。メトリクスは `HHEA`/`HMTX` を `hhea`/`hmtx` より優先する。
- cmapのサブテーブルは、HarfBuzzと同じ順（(3,0)、(3,10)、(0,6)、(0,4)、(3,1)、(0,3)、(0,2)、(0,1)、(0,0)、(1,0)、(1,任意)）で、読めるものを選ぶ。グリフの数以上のグリフIDは対応なしとする。HarfBuzzと同じく、記号フォント（(3,0)）では、対応の無い文字を、OS/2（版0）のフォントのページが無ければU+0000〜U+00FFをU+F000〜U+F0FFとして、簡体と繁体のアラビア文字のページならHarfBuzzのPUAの表で引きなおす。(1,0) では、ASCIIの外の文字をMacRomanに変換して引く（HarfBuzzと同じく、符号位置の下位16bitで表を引く）。ほかのMacのエンコーディングはASCIIだけを引く（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。
- 送り幅はHarfBuzzと同じく、表が覆うグリフ（`numberOfHMetrics` と表の長さから決まる。`numberOfHMetrics` がグリフの数より大きい壊れたフォントでは、グリフの数以上のIDも含む）には表の値を、その外には0を返す。hmtxのデータが全く無ければ `unitsPerEm / 2`（`unitsPerEm` が16〜16384の外なら1000とみなす）を返す。hheaが無くhmtxがあるフォントでは、長いメトリクスが0個なので、すべて0になる。
- `ft_left_side_bearing` は、輪郭の `pp1.x` のために、FreeTypeの `tt_face_get_metrics` と同じ読み方で左のサイドベアリングを返す（[ADR 0018](adr/0018-phantom-point-shift.md)）。送り幅（HarfBuzzの方針）とは別の読み方である。`numberOfHMetrics` 未満のグリフIDは長い記録から（記録が表に収まらなければ0）、以上はサイドベアリングの配列から（グリフの数によらず、表の外なら0。最後の送り幅が読めなければ0）引く。hheaかhmtxが無ければ0。表は、`GLYF` があれば `HHEA` と `HMTX`、無ければ `hhea` と `hmtx`（`hhea` が無ければ `HHEA` と `HMTX`）を使う。
- フォント全体と縦書きのメトリクスは、HarfBuzz 15.1.0と同じ値をフォントの単位で返す（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。表の位置は `Font::new` で一度だけ見つける。
  - `h_extents`：OS/2（`usWeightClass`、`usWidthClass`、`usFirstCharIndex`、`usLastCharIndex` のどれかが0でないもの）に `USE_TYPO_METRICS` があれば `sTypo*`、なければhhea（`HHEA` があればそれ）。アセンダは絶対値、ディセンダは絶対値の負。どれかが無ければ、アセンダを `y_scale × 0.8`（0へ切り捨て）、ディセンダをアセンダ − `y_scale`、行間を0とする。`v_extents` はvhea（`VHEA`）で、代わりの値はアセンダ `x_scale / 2`。
  - `v_advance`：vmtx（`VMTX` が送り幅を持てばそれ）の送り幅の負、vmtxが無ければ `h_extents` のディセンダ − アセンダ。
  - `v_origin`：xは横の送り幅の半分。yは、VORGがあればその値（記録が無ければ既定の値）、vmtxとglyfがあればglyfの上の幻の点（`yMax` + 上のサイドベアリング。複合グリフはHarfBuzzと同じく、USE_MY_METRICSの最後の部品の幻の点を使う。循環の検出もHarfBuzzの `hb_decycler_t` と同じにする。グリフを読めなければ `unitsPerEm`）、どちらでもなければ呼び出し側が渡す箱で `y_bearing + (フォントの高さ + 箱の高さ) >> 1`（箱が無ければアセンダ）。
  - `glyf_extents`：`x_bearing` はhmtxの左のサイドベアリング（HarfBuzzの読み方）、`y_bearing` は `max(yMin, yMax)`、幅は `|xMax − xMin|`、高さは `−|yMax − yMin|`。空のグリフは0。glyfの数（locaの項目の数 − 1とグリフの数の小さいほう）以上のIDは `None`。CFFとCFF2の箱は `sofel-outline` の `HbExtents` が受け持つ（[ADR 0021](adr/0021-cff-outlines.md)）。
  - `metrics_position` と `metrics_position_with_fallback` は `hb_ot_metrics_tag_t` のすべての値を返す。postはHarfBuzzと同じく版1、2、3だけを使う。
  - `fs_type` はFreeTypeの `FT_Get_FSType_Flags`：OS/2が読めて版が0xFFFFでなければ `fsType`、そうでなければ0。
- 名前（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）：
  - `names` と `name_utf8`/`16`/`32` はHarfBuzzの `hb_ot_name_list_names` と `hb_ot_name_get_utf*` と同じ。言語はプラットフォーム3と1の言語IDをHarfBuzzの表で、プラットフォーム0は `ltag` で引く。名前IDと言語の組ごとに、エンコーディングの点の良い記録を選ぶ（(3,10)、(0,6)、(0,4)、(3,1)、(0,3)、(0,2)、(0,1)、(0,0)、(3,0)、(1,0) の順。ほかは使わない）。どれか1つの記録の文字列が表の外にあれば、HarfBuzzと同じく名前を1つも返さない。言語の照合は、完全に一致するものを先に、無ければフォントの言語が要求の接頭（`en` と `en-us`）になるものを、HarfBuzzの二分探索と同じ手順で探す。言語の指定が無ければ英語とする。UTF-16BEとASCII（Macの記録。0x80以上はU+FFFD）を変換し、呼び出し側の配列に終端の0の分を残して書く。確保しない。
  - `sfnt_name` と `sfnt_name_count` はFreeTypeの `FT_Get_Sfnt_Name`：長さが0の記録、文字列が記録（と言語タグの記録）より前か表の外にある記録、形式1の無効な言語タグを指す記録を除いた生の記録。
  - `postscript_name` はFreeTypeの `sfnt_get_ps_name`：上の記録のうち名前ID 6の、Windows（3、0か1。言語0x409の最後のもの、無ければ最初のもの）の文字のうちPostScript名に使えるもの、1文字も残らなければApple（1、0。言語0の最後のもの、無ければ最初のもの）。
  - `glyph_name` はHarfBuzzの `hb_font_get_glyph_name`（postの版1と2。版1は258個の標準の名前。版2の文字列は65535個まで、表の終わりを越える文字列からは使わない）。postに名前が無ければ、HarfBuzzのCFFのアクセラレータが有効でCIDキーでないフォントで、charsetのSIDの文字列（391未満は標準の文字列、ほかはString INDEX）を返す。空の文字列は名前なしとする（[ADR 0021](adr/0021-cff-outlines.md)）。`glyph_from_name` は呼び出し側が持つ索引 `GlyphNames`（`glyph_names_len` 語の配列。CFFの名前にはグリフごとに2語）で引く。postの名前はHarfBuzzと同じく `hb_qsort_inline` で並べて二分探索し、同じ名前のグリフが複数あればHarfBuzzと同じものを返す。CFFの名前は、HarfBuzzの `gname_t::cmp`（短いほうの長さまでの `strncmp`、次に短いほうが先）で並べ、同じ名前はグリフIDの小さい順とし、一致したグリフのSIDを持つ最初のグリフ（`sid_to_glyph`、SIDは16bit）を返す。HarfBuzzの `qsort` の順とは一致しないことがあるが、決定性を優先する（[ADR 0021](adr/0021-cff-outlines.md)）。
- HarfBuzzのCFFとCFF2のアクセラレータ（`HbCff1`、`HbCff2`、[ADR 0021](adr/0021-cff-outlines.md)）：HarfBuzzの `accelerator_templ_t` と同じ確かめを同じ順に、同じ操作の予算で行い、HarfBuzzのアクセラレータが無効（`is_valid` が偽）になるときちょうど作れない。表はHarfBuzzの `hb_face_reference_table` と同じく、ファイルの外にはみ出す部分を切って読む。CFFは、Name INDEX、Top DICT INDEX（数0は誤り）、Top DICT、予め定められていないcharset、CIDキーならFDArrayとFDSelect（予め定められたcharsetは誤り）、そうでなければ予め定められていないencoding、String INDEX、Global Subrs（壊れていれば無しとする）、CharStrings（数が `Font::num_glyphs` と違えば誤り）、各Private DICT（ローカルのサブルーチンは壊れていれば無し）を確かめる。CFF2は、Top DICT、Global Subrs、Variation Store（壊れていれば無し）、CharStrings、FDArray（いずれも無ければ誤り）、グリフの数（少ないのは `VARC` の表があるときだけ許す）、FDSelect、各Private DICT（`vsindex` を含む）を確かめる。値は呼び出し側が持つ `Copy` の値で確保をしない。CIDキーのフォントとCFF2では、Font DICTとPrivate DICTをグリフごとに読みなおす（最後のFont DICTのローカルのサブルーチンだけは、予算の尽きた結果を写すため作るときの値を持つ）。
  - FreeTypeの `FT_Get_Glyph_Name` と `FT_Get_Name_Index` は結果が違う（名前の無いグリフは `.notdef`、版1は258グリフのときだけ、版2.5を読む、文字列は0のバイトか表の終わりで切れる）ため、`ft_glyph_name` と `GlyphNames::ft_glyph_from_name` を別に持つ。
- FreeTypeの値：`ft_face_metrics` は `sfnt_load_face` の `FT_Face` の値（アセンダ、ディセンダ、高さ、最大の送り幅、下線の位置と太さ、`bbox`。値は16bitで巻き戻る。輪郭の無いフェイスとCBLC/CBDTのフェイスは0）、`ft_v_advance` は `FT_Get_Advance`（縦、拡大縮小なし）。
- CFFとCFF2（[ADR 0021](adr/0021-cff-outlines.md)）：`Font::is_cff`、`is_cff2` は表があり、ファイルで切った長さが0でないこと。`Cff::new` はFreeTypeのCFFの駆動と同じく `CFF2`、`CFF ` の順に表を選び、FreeTypeがフェイスを開けないときにだけ失敗する（ヘッダ、INDEX、Name INDEXの数、DICT、`CharStrings` の有無、Variation Store、FDArrayの数（256以下）、Font DICTとPrivate DICTとローカルのサブルーチン、FDSelect（フォーマット0と3。CFF2のフォーマット4は誤り）、charset、encoding）。FreeTypeは表の後ろもファイルの終わりまで読むので、同じく読む。`Cff` は呼び出し側が持つ値で、Font DICTとPrivate DICTはグリフごとに読む（`Cff::subfont`）。
  - CIDの問い合わせはFreeTypeに合わせる。`is_cid_keyed` はTop DICTの `ROS`、`cid` はcharsetの値（CIDキーでない、またはグリフIDが範囲外なら誤り）、`ros` は登録者と順序のSIDの文字列（0xFFFFと範囲外は `None`）と、整数に切り捨てた補遺。
  - FreeTypeの文字列は、String INDEXをNULで終わる写しに並べたもので、空の文字列は写されないため次の空でない文字列として読まれる。これも同じにする。
  - グリフ名：`charset_name` はCIDキーでないCFFのcharsetの名前。FreeTypeの手順（`Font::ft_glyph_name`、`GlyphNames::ft_*`）では、フェイスがグリフ名の印（sfntの読み込みで、読めた `post` が版3.0でないとき、またはCIDキーでないCFF）を持つときだけ名前を返し、CFFではcharsetの値の文字列（CIDキーのフォントではCIDの番号のSID）、文字列が無ければ空の名前、CFF2では `post` の手順（グリフの数はCharStringsの数）を使う。名前からグリフへは、グリフIDの順に線形に探す。`build_name_index` と `glyph_from_name` は、名前の順（同じ名前はグリフIDの小さい順）の索引を呼び出し側の配列に作り、二分探索する。`build_name_index` はcharsetを1回だけ順に読む。
  - charsetの索引（[ADR 0025](adr/0025-cff-charset-index.md)）：`cid`、`charset_name`、`ft_glyph_name`、`build_name_index`、`glyph_from_name` は、それぞれ `CharsetIndex` を受け取る版（`cid_with` など）を持ち、結果は索引の無い版と等しい。索引の無い版は、フォーマット1と2でグリフの前の範囲を毎回読むので、繰り返し引くなら索引を作る（範囲の数の配列を1回読む）。`Cff` は索引を持たない。
  - `ps_font_info` は `FT_Get_PS_Font_Info` の9つの値（CFF2では誤り）、`ps_fs_type` はTop DICTの `PostScript` の文字列の `/FSType <数> def` の数（数字、空白、CR、LF以外が混じるか、あふれれば0）。
- `HbScale` はHarfBuzzの `hb_font_t` の拡大縮小を写す。`x_mult` は、HarfBuzzの `changed` の `(int64_t) x_scale * 0x10000 / upem` で `upem` が `float` のため、`x_scale × 0x10000`（`int64_t`）を `float` にして `upem`（`float`）で割り、0へ切り捨てる（整数の割り算ではない。[ADR 0026](adr/0026-harfbuzz-rounding.md)）。整数の値は `(v × mult + 32768) >> 16`、小数の値はHarfBuzzの `roundf`（`sofel_geom::float::hb_roundf32`、`floor((double) x + 0.5)`）、箱は `scale_glyph_extents`（角を `f32` で拡大し、左上を切り下げ、右下を切り上げ）で拡大縮小する。FMAは使わない。代わりの値は拡大縮小した `x_scale`、`y_scale` から求める。
- 基準線（[ADR 0017](adr/0017-base-baselines.md)）：
  - 方向は `Direction`（横と縦）とし、シェーピングの4つの方向はこれに写す。基準線のタグは `Tag` で受け取り、`baseline` にHarfBuzzの `hb_ot_layout_baseline_tag_t` の9つの値を持つ。
  - BASEは、HarfBuzz 15.1.0の検査（`sanitize`）に通るときだけ使う。HarfBuzz 15.1.0は壊れたオフセットを0に書きかえないため、表に収まらない表、未知のBaseCoordの形式、版1以外、形式1でないItemVariationStoreのどれか1つで表全体を使わない。HarfBuzzと同じく、配列を調べるたびにそのバイト数を、表の1バイトあたり64（最低16384）の予算から引き、使いきれば表全体を使わない。
  - `baseline` はHarfBuzzの `hb_ot_layout_get_baseline`：方向の軸のBaseScriptListから文字体系のタグを、無ければ `DFLT` を引き、BaseValuesが無ければ（記録のBaseScriptが空でも）引けない。BaseTagListで基準線のタグを引き、その位置のBaseCoordの `coordinate` を、横書きは `em_scale_y`、縦書きは `em_scale_x` で拡大縮小する。形式2と4の参照のグリフの点は使わず、形式3のDeviceの差分は、ppemと可変フォントの座標を持たないため0とする。言語のタグは使わない。
  - `baseline_with_fallback` はHarfBuzzの `hb_ot_layout_get_baseline_with_fallback`：`romn` は0。`math` は横書きでU+2212（無ければ `-`）のグリフの箱があれば `y_bearing + height / 2`、無ければx-height（`metrics_position_with_fallback`）の半分。`idtp` と `ideo` は、反対側の端がBASEにあればそれに±1 em（横は `y_scale`、縦は `x_scale`）、無ければその方向の `h_extents` か `v_extents` のアセンダとディセンダ。`icft` と `icfb` は仮想ボディの上下から高さの10分の1だけ内側。`Idce` と `Icfc` は上下の和の半分（64bitで足し、0へ切り捨て）。`hang` は横書きで、HarfBuzzの `hb_ot_tag_to_script` で写した文字体系の文字（デーヴァナーガリーのU+0915など21個）のグリフの箱の `y_bearing`、無ければ `x_scale` か `y_scale` の6/10。ほかのタグは0。グリフの箱は呼び出し側が渡す関数で求める。文字体系の表は、[ADR 0019](adr/0019-unicode-character-data.md) の文字体系の型ができるまでの一時のもの。
  - `layout_font_extents` はHarfBuzzの `hb_ot_layout_get_font_extents`：BaseScriptが既定のMinMaxかBaseLangSysの記録を持てば、言語のMinMax（無ければ既定のMinMax。空のMinMaxと空の座標は0）の最大をアセンダ、最小をディセンダ、行間を0として `true`。持たなければその方向の `h_extents` か `v_extents` と `false`。FeatMinMaxは使わない。
- hheaの代わりに `HHEA` を使う条件と `HHEA` の `numberOfLongMetrics` の幅（24bit）は、[ADR 0007](adr/0007-sfnt-read-foundation.md) の読み方のままとする（HarfBuzzとFreeTypeは32bitで読む）。

## 輪郭（`sofel-outline`）

- glyfと `GLYF` の単純なグリフと合成グリフの輪郭を、ヒンティングをせずに、FreeTypeと同じ座標で `Pen` に渡す（[ADR 0011](adr/0011-glyf-outlines.md)）。`GLYF` があれば `GLYF` と `LOCA` を使い、そのときだけフラグ0x80を三次曲線、0x2000を24bitの成分のグリフIDとして扱う。
- 倍率は `FT_Set_Pixel_Sizes` と同じく `div_fix(ppem << 6, unitsPerEm)` で、点は `mul_fix` で拡大縮小し、丸めない。locaの解釈と合成の位置の計算はFreeTypeに合わせる。
- 作業領域（`Scratch`）は呼び出し側が持つ。
- HarfBuzzのグリフの箱（`HbExtents`、[ADR 0021](adr/0021-cff-outlines.md)）：`hb_ot_get_glyph_extents` のうちカラーとビットマップを除いた順（VARCはまだ無く、glyf、CFF2、CFF）で、拡大縮小の前の値を返す。CFF2が無効なら、または箱が得られなければCFFへ進む。CFFとCFF2の箱は、HarfBuzzの `cff1_cs_interp_env_t`、`cff2_cs_interp_env_t` と `opset_extents` を移植した解釈器で求める。スタックは513、サブルーチンの入れ子は10、演算子は1グリフ200000まで（200000個目で失敗）。算術の演算子とCFFの `blend`、`vsindex` は未知の演算子としてスタックを捨て、`dotsection` は何もしない。CFFでは `endchar` の無いcharstringは失敗し、`endchar` の引数が4つ以上ならseac（標準のエンコーディングの番号をcharsetで引き、基とアクセントの箱を、アクセントをずらして合わせる。入れ子と、グリフの無い番号は失敗）とする。CFF2は文字列の終わりで `return` か `endchar` とし、`blend` は既定のインスタンスでは差分を捨て、既定の値だけを残す（引数の不足と、`blend` の後の `vsindex` は失敗）。移動で経路を閉じ、線と曲線の始まりで現在の点を、その後に各点（曲線の制御点を含む）を箱に足す。数はHarfBuzzの `double` の代わりに16.16を `i64` で持つ。既定のインスタンスでは和と差しか生じないので、値は同じになる。丸めはHarfBuzz 15.1.0の `roundf` マクロ（`_hb_roundf (double)`）と同じ `floor (x + 0.5)` で、`float` を経ない（[ADR 0026](adr/0026-harfbuzz-rounding.md)）。16.16の値に `0x8000` を足して16bit右へずらす整数の計算で求め、これが `sofel_geom::float::hb_roundf` と常に等しいことをテストで確かめる。最小が最大以上の軸は0とする。拡大縮小は `HbScale::scale_glyph_extents` による。
- FreeTypeと同じく、読みこんだ輪郭を幻の点の `pp1.x` だけ左に動かす（[ADR 0018](adr/0018-phantom-point-shift.md)）。`pp1.x` は、グリフの見出しの `xMin` からFreeTypeの読み方の左のサイドベアリング（`Font::ft_left_side_bearing`）を引き、点と同じく `mul_fix` で拡大縮小した値で、丸めない。`head` の `flags` のビット1によらない。空のグリフは点が無いので動かない。`OutlineInfo` の `cbox` は動かしたあとの点から求める。
  - 合成グリフは、自身の見出しとサイドベアリングの `pp1.x` を使う。USE_MY_METRICSの部品があれば、最後のその部品の `pp1.x`（部品の見出しとサイドベアリングから求めたもの。部品の変換と位置合わせでは動かない）を使う。入れ子の合成グリフでも同じ規則を部品ごとに当てる。
  - 残りの幻の点（`pp2`、`pp3`、`pp4`）と、ヒンティングと可変フォントでの幻の点は求めない。
- CFFとCFF2（[ADR 0021](adr/0021-cff-outlines.md)）：`Outlines::new` が、FreeTypeの駆動の選び方で形式を決める。sfntの版が `OTTO` なら `Cff::new`（`CFF2`、`CFF ` の順）で、それ以外ならglyf（`GLYF`）で読む。FreeTypeのTrueTypeの駆動は `OTTO` を受け付けないので、CFFとglyfの両方を持つ `OTTO` のフォントはCFFで描く（ADRは「glyfかlocaがあればglyf」と書くが、FreeTypeに合わせた）。`OTTO` でCFFもCFF2も無ければ `UnsupportedFormat`、`Cff::new` が失敗すれば `InvalidCff`、charstringの解読やFont DICTの読み取りが失敗すれば `InvalidCharString` とする。
  - 点は、`sofel-read` のFreeTypeのcharstringの解読器が倍率 `0x400` の装置の空間で渡す線と曲線から、`ps_builder` と同じく `>> 10` でフォントの単位の整数にし（小数は切り捨て）、輪郭を閉じるときに始点と同じ最後の曲線上の点と、1点だけの輪郭を取り除く。そのあと、`cff_slot_load` と同じく、FontMatrixが単位行列でなければ変換し、平行移動を足し、`Scaled` なら（またはFDArrayのフォントでTop DICTとFont DICTの `units_per_em` が違えば `Unscaled` でも）`mul_fix` で拡大縮小する。`unitsPerEm` が0x7FFFを超えるフォントの拡大縮小は誤りとする。`overlap` は常に偽。
  - `random` の状態は、描くたびにPrivate DICTの `initialRandomSeed`（FreeTypeでフェイスの種を0にしたときの値）から始める。
  - FontMatrixは `cff_face_init` と同じく、`yy`（0なら `yx`）で割って正規化し、平行移動を整数のフォントの単位にする。Font DICTの行列はTop DICTの行列と掛け合わせる。行列の無いFont DICTは、正規化した上の行列を写して、もう一度平行移動を16bit右にずらす（FreeTypeと同じ）。
- `ft-dump` の `--cid` と `--ps-info` の結果を全フェイスで比べる（`:cid`、`:ps-info`、1294項目、期待する失敗は無い）。テストのフォントのすべてのCFFのフェイスで、charsetの索引を使う版と使わない版の `cid`、`charset_name`、`ft_glyph_name` が全グリフで等しいことも確かめる（[ADR 0025](adr/0025-cff-charset-index.md)、`tests/cff_charset_index.rs`）。`--outline` と `--raster` は、フェイスの `random-seed`（`FT_PARAM_TAG_RANDOM_SEED`）を0にして開く。`FT_Get_PS_Font_Extra` は公開のAPIでないので、`--ps-info` は `FT_Get_FSType_Flags`（`/FSType` が0でなければその値、そうでなければOS/2の `fsType`）を書く。
- HarfBuzz 15.1.0のテストのフォントで、FreeTypeが開けるglyf、CFF、CFF2のフェイスの53万2841項目（拡大縮小なし、16画素、48画素）を比べ、期待する失敗は無い。
- 同じフォントの単純なグリフ（見出しの `xMin` が点の最小のxと等しい4万9225個）で、拡大縮小なしの `cbox.x_min` が、HarfBuzzのグリフの箱の `x_bearing`（可変フォントの変化なし）と等しいことを確かめている。

## カラーグリフ（`sofel-color`、`sofel-bitmap`、`Painter`）

[ADR 0023](adr/0023-color-glyphs.md) の5a段階（`Painter`、CPAL、COLRv0、CBDTとsbixのデータ、SVGの文書、グリフの箱、フォールバックの問い合わせ）、5b段階（COLRv1）、5c段階（EBDTとビットマップだけのフォント）を実装している。5d（COLRv0の画像）は未実装である。

- `sofel-geom` の `Painter` は、HarfBuzz 15.1.0の `hb_paint_funcs_t` と同じ順で同じ値の命令を受ける。単色と3種類のグラデーションは `fill(&Brush)` にまとめ、グループの合成の方法は `push_group` と `pop_group` の両方で渡す。単位はフォントの単位でyは上に増え、HarfBuzzの根の拡大の変換と、`PaintGlyph` と `PaintColrGlyph` の周りの逆の変換は出さない。数は `f32` で、HarfBuzzの `float` の計算と同じ順で同じ演算をし、FMAを使わない。
- `Image` の `extents` は `Option` とする。HarfBuzzはSVGの文書を範囲なし（NULL）で渡すためで、ADRの型（`GlyphExtents`）から変えた。
- 三角関数（`sin_cos`、`sin`、`cos`、`tan`）はmusl 1.2.6の `sincosf` と `tanf` の移植で、`f64` の四則演算だけで計算する。|x| ≥ 2^28·π/2 の引数の剰余は、muslの `__rem_pio2_large` の代わりに、muslの2/πのビットの表から128bitの整数で求める。テストで、100万個の引数で `f64` の値を丸めたものとの差が1ulp以下であること、三角関数と変換の100万個の結果のハッシュがx86-64、aarch64、wasm32で同じことを確かめる。
- 補助の関数 `normalize_color_line`（安定な挿入の並べかえ）、`reduce_linear_anchors`、`sweep_gradient_tiles` はHarfBuzzの `hb-paint.cc` の移植である。
- `sofel-read` に、CPAL（版0と1）、COLR（版0の記録、版1のBaseGlyphList、LayerList、ClipList、塗りの形式1〜33）、CBLC/EBLC/`bloc`（索引の形式1〜5）、CBDT/EBDT/`bdat`、sbix、SVGのビューと検証器を置く。色の線は、停止点がすべて表に収まらなければ停止点なしとして読む（FreeTypeの `FT_Get_Colorline_Stops` と同じ）。`Colr::v1_fits` は、版1の塗りの木が表に収まるかを、HarfBuzzの検査（入れ子64、1バイトあたり64回の作業）と同じ範囲で確かめる。
- `sofel-color`：
  - パレット（`palette_count`、`palette_name_id`、`palette_flags`、`palette_color_name_id`、`palette_colors`、`palette_entry_count`、`find_palette`）はHarfBuzzの読み方による。範囲外の名前は0xFFFF、フラグは0。
  - `paint` は、HarfBuzzの `hb_ot_paint_glyph_or_fail` と同じくCOLR、SVG、CBDT、sbixの順に試す。COLRv1は、`v1_fits` が偽の表では使わない（HarfBuzzは版1のリストを捨てる）。結果は `Scratch` に表ごとに覚える。COLRv1の打ち切り（深さ64、辺の数16384、計算の量2^24で停止点1個に16）、循環の検出（`hb_decycler_t` のtortoise-hareと同じく、深さの半分の位置の値と比べる）、クリップの箱の無い木の有界の判定（`hb_paint_bounded`）はHarfBuzzと同じ。可変の塗りは差分0で塗る。色は `Palette::Font(i)`（範囲外は0番）か `Palette::Custom`（範囲外の番号は透明の黒）から引き、アルファはHarfBuzzの `roundf`（[ADR 0026](adr/0026-harfbuzz-rounding.md)）で丸める。
  - `Recording` は命令を値の列にし、`replay` で流しなおす。画像のデータはフォントを借りるので、`Recording` へは `paint_recording` で塗る。
  - `glyph_extents` と `scaled_glyph_extents` は `hb_ot_get_glyph_extents` と同じくsbix、CBDT（最大のストライク）、COLR、輪郭（`Outlines`）の順に探す。COLRはクリップの箱か、`hb_paint_extents` と同じ計算の塗りの範囲（層のグリフは読み込んだ輪郭の制御点の箱）とする。`Outlines` と `Scratch` を受け取る（ADRの `(font, glyph)` から、輪郭のアクセラレータと作業領域を呼び出し側が持つ形に変えた）。拡大縮小した箱は、根に拡大の変換を置いて求める。`HbScale` に、この箱を拡大縮小せずに使う `v_origin_scaled`、`metrics_position_with_fallback_scaled`、`baseline_with_fallback_scaled` を足した。
  - `color_formats`、`glyph_color_format`（塗りの木をたどらない。`v1_fits` も見ない）、`layers`、`svg_document` を持つ。
  - 生の塗りの木（`base_paint`、`PaintRef`、`clip_box`、`layer_paint`）は、FreeTypeの `FT_Get_Color_Glyph_Paint` などと同じ値を返す。基本のグリフの探索はFreeTypeの二分探索による。
- `sofel-bitmap`：`strikes` はFreeTypeの `available_sizes`（CBLC、EBLC、`bloc`、sbixの順に最初の表、ppemが0のストライクは除く）、`match_strike` は `FT_Match_Size`、`choose_strike` はHarfBuzzの選び方（`max(x, y)` が要求以上で最小、無ければ最大、0は最大）、`glyph_bitmap` はFreeTypeの読み込み（sbixの `dupe` と `flip` を4回まで）で画像のデータとメトリクスを返す。PNGを塗るための `cbdt_png`、`sbix_png` とその箱はHarfBuzzの読み方（CBDTは索引の形式1と3、画像の形式17と18、sbixは `dupe` を8回まで）による。
- `sofel-bitmap` の `bitmap_layout` と `draw_bitmap` は、EBDT、CBDT、`bdat` の画像の形式1、2、5、6、7と成分の形式8、9を、1、2、4、8、32bitの深さで、FreeTypeの `tt_sbit_decoder_load_image`（索引の形式1〜5、形式2と7のバイト単位とビット単位の判定、成分のメトリクスの戻し方）と同じ画素とメトリクスにして、呼び出し側の領域（`pitch × rows` バイト、上の行から）に書く。32bitは `FT_LOAD_COLOR` と同じくBGRAのまま書く。PNGの画像は展開しない（`BitmapError::Png`）。
- `sofel-font` の `Font::new` は、FreeTypeと同じく、輪郭（glyf、GLYF、CFF、CFF2）が無くsbixも無いフォントでは、Appleの `bhed` を `head` の代わりに読む。`Font::has_outlines`（CBLCかCBDTがあれば偽。FreeTypeは輪郭を無視する）と `Font::has_bitmaps`（CBLC、EBLC、`bloc`、sbix）を持つ。`bhed` は `head` の検証器で検証する。Appleのビットマップだけのフォントでは、FreeTypeがOS/2を読まないため、ストライクの `width` は0になる。
- `sofel-render` は、`--ppem` に等しいストライク（`FT_Match_Size`）があり、グリフのビットマップがあれば、`--no-bitmap` が無いかぎりビットマップを書く。白黒と灰色はPGM（階調を0〜255に広げて反転）、32bitはPAM（`P7`、`TUPLTYPE RGB_ALPHA`、乗算済みでない値に直す）とし、位置は同じ注釈の行に書く。
- 検査：テストのデータにEBLCや `bloc` のフォントが無いため、画像の形式1、2、5、6、7、8、9、深さ1、2、4、8、32、索引の形式1〜5の組み合わせを、EBLC/EBDTと `bloc`/`bdat`/`bhed` で組み立てた110個のフォントと、glyfの輪郭とCBLC/CBDTを両方持つフォントを、`ft-dump --sbit`（`FT_Select_Size` と `FT_LOAD_COLOR` の画素とメトリクス）と比べ、すべて一致する。
- `sofel-paint` は塗りの命令をHarfBuzzの `test/api/test-paint.c` の字下げのテキストで書く。
- C ABIは `sofel_paint_glyph`（命令の配列。`sofel_paint_commands_free` で解放）、パレット、形式、層、SVGの文書、ストライク、グリフのビットマップ、補助の関数、三角関数を持つ。`sofel_font_glyph_extents` などグリフの箱を使う関数は `sofel_color` の箱を使う。
- 検査：HarfBuzzの `test/api/results-paint/` の24個（残りの2個はテストの実行の記録で塗りのテキストではない）と一致し、`hb-dump --paint` もこれを再現する。HarfBuzzのテストのカラーのフォント92個で `hb-dump --paint`、`--color`、`--extents` の7万8225項目を比べ、期待する失敗は22項目（理由は一覧）である。`hb-dump --metrics` の箱、縦書きの原点、代わりの基準線もカラーグリフで一致する。FreeTypeとは `ft-dump --colr` の生の塗りの木とCPAL、`--sbit` のストライクの一覧を比べる（2178項目、期待する失敗480項目。FreeTypeが `paletteTypes` を16bitで読むこと、FreeTypeが読み込まないCOLRと可変の塗り）。FreeTypeの試験の版はlibpngを持たないため、PNGのグリフのメトリクスは比べない。

## ラスタライズ（`sofel-raster`）

- 8bitのアンチエイリアスの画像を、FreeTypeのsmoothのラスタライザと同じ数値の計算で作る（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。セルは帯ごとの密な配列（上限64KiB）に積む。重なりの印のある輪郭は4×4の副標本で描く。FreeTypeとの差は0（完全一致）とする。
- 掃き出しは、帯の行ごとに触れた列の範囲だけを走査し、被覆率は触れたセルでだけ求めて次のセルまで使う。走査したセルは0に戻し、帯の配列を描画のたびに全体を0にはしない（[ADR 0014](adr/0014-raster-sweep-touched-cells.md)）。
- 重なりのときは、画素ごとに16個の副標本の `(coverage + 8) / 16` を16bitで合計し、最後に `min(合計, 255)` とする。合計は256以下なので、FreeTypeの標本ごとの `sum - (sum >> 8)` と同じ値になる（[ADR 0014](adr/0014-raster-sweep-touched-cells.md)）。
- FreeTypeは、輪郭をビットマップの原点へ動かしてから（重なりの印があればさらに4倍してから）命令に変換する。暗黙の曲線上の点（中点の0への切り捨て）はこの順序で決まるので、完全に一致させるには、`sofel_outline::load` で点を読み、`sofel_raster::pixel_box` で箱を求め、`Scratch::decompose` に `PixelBox::shift()` と倍率を渡して命令に変換し、`Rasterizer::render_placed` で描く。この手順は `sofel_cli::glyph::render` にまとめてある。`Rasterizer::render` は、記録した命令をあとから動かすので、中点のある輪郭では1画素の値が1〜2ずれることがある。
- HarfBuzz 15.1.0のテストのフォントで、FreeTypeが開けるglyf、CFF、CFF2のフェイスを12、24、48画素で比べ、期待する失敗は無い。CFFのグリフは重なりの印を持たず、非ゼロ規則で描く（FreeTypeの `REVERSE_FILL` と `HIGH_PRECISION` はsmoothのラスタライザが見ない）。

## 道具

- `sofel-paint [--face N] [--palette P] [--foreground RRGGBBAA] [--ppem P] [--exact] (--glyph G | --char U+XXXX) FILE`：カラーグリフの塗りの命令を、HarfBuzzの `test/api/test-paint.c` と同じ字下げのテキストで書く。数はCの `%.3g`、`--exact` では `f32` のビットの16進。カラーでないグリフは終了コード1、使い方の誤りは2（[ADR 0023](adr/0023-color-glyphs.md)）。
- `sofel-render [--face N] --ppem P [--no-bitmap] (--glyph G | --char U+XXXX) FILE`：1グリフをヒンティングなしで描き、PGM（P5）を標準出力に書く。位置は注釈の行 `# left <x> top <y>` に書く（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。そのppemのビットマップがあれば、`--no-bitmap` が無いかぎりそれを書く（32bitはPAM。[ADR 0023](adr/0023-color-glyphs.md)）。
- `sofel-validate <ファイル>...`：sfntとTTC/OTCの各表を検証する（[ADR 0008](adr/0008-validate-command-and-freetype-comparison.md)）。1件を1行で `<パス>[#<フォント番号>]: <タグ>+0x<オフセット>: <error|warning>: <説明>` の形で出す。`--json` では、1件を1行のJSONのオブジェクト（JSON Lines）で出す。キーは `path`、`font`、`table`、`offset`、`severity`、`kind`、`message`、`details`。`kind` と `details` のキーは出力の約束の一部とする。終了コードは、`Error` が無ければ0、あれば1、使い方の誤りか読めないファイルがあれば2で、`--json` の有無で変わらない。

## C ABI（`sofel-capi`）

- `crates/sofel-capi` は、ライブラリのクレートの機能をCの呼び出し規約で公開する（[ADR 0015](adr/0015-c-abi.md)）。静的ライブラリ、動的ライブラリ、rlibを作る。ヘッダ `crates/sofel-capi/include/sofel.h` は手で書く。
- 名前は `sofel_` で始め、型は不透明な構造体へのポインタと固定の幅の整数だけを使う。誤りうる関数は `int32_t` を返し、0を成功、負の値を誤り（`SOFEL_ERROR_*`、`sofel_error_string` で説明）とする。結果は出力の引数に書く。ポインタの引数がNULLなら `SOFEL_ERROR_NULL` を返す（`user` のポインタと `*_destroy` を除く）。panicしない。
- フォント（`sofel_font_t`）はデータを複製しない。呼び出し側は、フォントを捨てるまでデータを生かしておく。フォントは不変で、複数のスレッドから同時に使える。作業領域（`sofel_outline_scratch_t`、`sofel_rasterizer_t`）は呼び出し側が持ち、スレッドの間で共有しない。
- 可変長の出力（画像、文字列）は呼び出し側の配列に書く。足りなければ要る長さを書き、配列には何も書かず `SOFEL_ERROR_BUFFER_TOO_SMALL` を返す（`snprintf` と同じ形）。
- 輪郭はCの関数ポインタ（`move_to`、`line_to`、`quad_to`、`cubic_to`、`close` と `void *user`）で受け取る。検証の結果は、1件ずつ関数ポインタに `sofel_diagnostic_t` で渡す。
- ヘッダの各関数の説明に、対応するRustの関数（`Rust:`）と、相当するHarfBuzzとFreeTypeの関数を書く。`scripts/check-capi.sh` が、ライブラリのクレートの公開の関数（rustdocのJSONの出力から `scripts/public-api.sh` で求める）がすべてヘッダの `Rust:` の行か除外の一覧（`crates/sofel-capi/unmapped.txt`、理由つき）にあることを確かめる。ライブラリに公開の機能を足すADRは、同じプルリクエストでC ABIの関数とヘッダとCのテストも足す。
- CFFとCFF2（[ADR 0021](adr/0021-cff-outlines.md)）：`sofel_font_t` は作るときに `Outlines`（`Cff::new` の結果とHarfBuzzの箱の値）を持ち、CFFが壊れていてもフォントの作成は失敗しない。輪郭とラスタライズの関数はそれでCFFとCFF2も描く。`sofel_font_is_cff`、`is_cff2`（1か0）、`sofel_font_is_cid_keyed`、`sofel_font_cid`（誤りのとき `*cid` は0）、`sofel_font_ros`（NUL終端で最初のNULで切った文字列、NULLの文字列は空にして `present` のビットで区別）、`sofel_font_ps_font_info`（数の配列と、呼び出し側の配列に並べた文字列とその位置。NULLは `SIZE_MAX`）、`sofel_font_ps_fs_type` を持つ。`sofel_font_t` は作るときにCFFのcharsetの索引も作って持ち、`sofel_font_cid` と `sofel_font_ft_glyph_name` はそれで引く（[ADR 0025](adr/0025-cff-charset-index.md)。関数の形は変わらない）。FreeTypeが開けないCFFには `SOFEL_ERROR_INVALID_CFF`、FreeTypeの `Invalid_Argument` には `SOFEL_ERROR_INVALID_ARGUMENT` を返す。
- `SOFEL_VERSION_MAJOR` などのマクロと `sofel_version()` を持つ。1.0.0までABIの互換を約束しない。
- Cのテスト（`crates/sofel-capi/tests/c/`）は `scripts/test-capi.sh` で、ネイティブ、ThreadSanitizer、AddressSanitizer、wasm32-wasip1（wasi-sdkとwasmtime）で走らせる。期待値は、`crates/sofel-capi/tests/fixture.rs` がRustのAPIで求めて `fixture.h` に書く。静的ライブラリの `sofel_` のシンボルとヘッダの関数の一致も確かめる。wasmでは、リンクに例外処理（`-lsetjmp`）もC++の実行時ライブラリも要らない。

## wasm

- ライブラリのクレートは `#![no_std]`（必要なら `alloc`）とする（[ADR 0004](adr/0004-wasm-and-no-std.md)）。
- CIで、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テスト（wasmtime）を行う。ツールチェーンは `rust-toolchain.toml` でstableの版を固定する（[ADR 0007](adr/0007-sfnt-read-foundation.md)）。
- JavaScriptからの呼び出しの層は持たない。
- `sofel-capi` は `wasm32-wasip1` の静的ライブラリとしてwasi-sdkのCのプログラムとリンクでき、`wasm32-unknown-unknown` にはビルドだけを確かめる（[ADR 0015](adr/0015-c-abi.md)）。

## 正しさの検査

- テストのフォントと期待値は同梱せず、`testdata.lock` に書いたコミットのHarfBuzz（15.1.0）とFreeType（2.15.0）を `scripts/fetch-testdata.sh` で取得して使う（[ADR 0002](adr/0002-license-and-reuse.md)、[0005](adr/0005-conformance-tests.md)）。
- シェーピング（AATを含む）は、HarfBuzzの `test/shape/data/*/tests/*.tests` の期待値と比べる。
- サブセット化は、HarfBuzzの `test/subset/` の期待するフォントと、バイト単位で比べる。
- head、hhea、maxp、cmap、hmtxの送り幅は、`tools/ft-dump/` の出力と比べる。FreeTypeが捨てたcmapのサブテーブルは、`sofel-read` の検証器が `Error` を報告しなければならない（[ADR 0008](adr/0008-validate-command-and-freetype-comparison.md)）。`ft-dump` は自分で書いたCのプログラムで、取得したFreeTypeのソースから `tools/ft-dump/build.sh` でビルドし、FreeTypeの公開のAPIだけを使う。`ft-dump` の正しさは、値の分かっている小さなフォントで自動のテストにより確かめる。
  - 項目は `<フォント>#<フェイス>:<項目>` で識別する。項目は、`open`、`head`、`hhea`、`maxp`、`cmap:<プラットフォーム>:<エンコーディング>[:<n>]`、`cmap-validate:<プラットフォーム>:<エンコーディング>[:<n>]`、`hmtx`。
  - [ADR 0016](adr/0016-font-and-vertical-metrics.md) の項目：`fs_type`、`ps_name`、`face_metrics`、`sfnt_names`、`v_advance`（全グリフ、縦、拡大縮小なし）、`glyph_names`（全グリフの `FT_Get_Glyph_Name`）、`name_index`（その名前の `FT_Get_Name_Index`）。CFFのフォントのグリフ名はCFFのcharsetから来るため、期待する失敗とする（[ADR 0021](adr/0021-cff-outlines.md)）。
  - `open` は「FreeTypeが開けるフェイスは、sofelも表の目録を読める」の一方向だけを確かめる。FreeTypeは描けるフェイスだけを開く（glyfかCFFが要る）ため、逆向きは比べない。FreeTypeが開けないフェイスは、ほかの項目も比べない。
  - `maxp` は、FreeTypeが読み込みの時点で行う調整（`maxFunctionDefs` を64以上に、`maxTwilightPoints` を0xFFFB以下に。`src/sfnt/ttload.c`）を、sofelの生の値に当ててから比べる。
  - cmapの対応は、FreeTypeの `FT_Get_Next_Char` と同じく、グリフIDがFreeTypeの `num_glyphs` 未満のものだけを比べる。cmap表に無いプラットフォーム7（Adobe）のエンコーディングは、FreeTypeがPostScriptのグリフ名から合成したものなので比べない。
  - HarfBuzz 15.1.0のテストのフォント653個で9098項目を比べ、期待する失敗は550項目（CFFのグリフ名）である。
  - GDEF、GSUB、GPOSは、`ft-dump --otvalid`（FreeTypeのotvalidのモジュールを入れてビルドし、表ごとに `FT_OpenType_Validate` を呼ぶ。GDEFは、通ったGSUBとGPOSとともに検証する）の結果と比べる（[ADR 0020](adr/0020-shaping-foundation.md)）。項目は `<フォント>#<フェイス>:otvalid:<表>` で、FreeTypeが拒んだ表について、sofelの検証器が `Error` を報告することを確かめる。期待する失敗は `conformance/expected-failures-otvalid.txt` に書く。653個のフォントの1154の表のうちFreeTypeが拒むのは31で、sofelが `Error` を報告しないのは22である。内訳は、FreeTypeが連鎖文脈の形式2の空のClassDefのオフセットをサブテーブルの先頭のClassDefとして読む誤り（11）、numGlyphs以上のグリフIDをFreeTypeが拒むがsofelは調べないもの（10。HarfBuzzはそのまま読む）、FreeTypeが空の形式1のClassDefを拒むもの（1）である。
- テストのデータは、CIでキャッシュが当たったとき `SOFEL_TESTDATA_OFFLINE=1` で取得のスクリプトを走らせ、キャッシュの中身が欠けていれば失敗させる。シェルのスクリプトのテストは `scripts/run-tests.sh` で走らせる。
- フォント全体と縦書きのメトリクスは、`tools/hb-dump/` の出力と比べる（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。`hb-dump` はHarfBuzzの `src/harfbuzz.cc` を取りこむC++の1つの翻訳単位で、`tools/hb-dump/build.sh` でビルドする。FreeTypeとは統合しない。書き出す内容は引数の形（`--metrics` など）で選び、形ごとに関数を1つ足して広げる。
  - 拡大縮小は、(`upem`, `upem`)、(2 `upem`, 2 `upem`)、(`upem` / 2, `upem` / 2)、(768, 768)、(−`upem`, −`upem`)、(768, −1536) の6つ。
  - 項目は `<フォント>#<フェイス>:hb:<項目>` で、`num_glyphs`、`h_extents`、`v_extents`、`metrics`、`metrics-fallback`、`h_advance`、`v_advance`、`v_origin`、`extents`、`nominal`（U+10FFFFまでのすべての符号位置の `hb_font_get_nominal_glyph`。HarfBuzzの値のうちグリフの数以上のものは除く）、`names`、`name_utf`（全名前IDと、記録に現れる全言語と言語なしの、UTF-8、16、32。配列の長さは全体＋1と4）、`glyph_names`、`glyph_from_name`。各項目はすべての拡大縮小とすべてのグリフを含む。期待する失敗は `conformance/expected-failures-harfbuzz.txt` に書く。グリフの箱は `HbExtents`（glyf、CFF2、CFF）で、グリフ名はpostとCFFのcharsetで比べる（[ADR 0021](adr/0021-cff-outlines.md)）。同じ名前を2つ以上持つフォントでは、HarfBuzzの結果が整列の実装によるため、`glyph_from_name` は返ったグリフの名前が引いた名前と等しいことだけを確かめる。COLR、CBDT、sbixのカラーのグリフの箱（[ADR 0023](adr/0023-color-glyphs.md)）と、その箱で決まる縦の原点と、HarfBuzzが引かないcmapのフォーマット2と8（sofelはFreeTypeと同じく引く。[ADR 0010](adr/0010-font-foundation.md)）が期待する失敗である。653個のフォントで9128項目を比べ、期待する失敗は35項目である。
  - 基準線（[ADR 0017](adr/0017-base-baselines.md)、`hb-dump --baselines`）の項目は `baseline`（`hb_ot_layout_get_baseline` の有無と値）、`baseline-fallback`（`hb_ot_layout_get_baseline_with_fallback`）、`layout_font_extents`（`hb_ot_layout_get_font_extents` の有無と値）。6つの拡大縮小、横と縦、9つの基準線、文字体系のタグ（`hani`、`latn`、`deva`、`DFLT` と、代わりの値の文字を持つ文字体系の新旧のタグなど34個と、BASEに現れるすべてのタグ）、言語のタグ（`dflt` とBASEに現れるすべてのタグ）の組を比べる。BASEを持つフォントを足すため、HarfBuzzの `test/api/fonts/base*.ttf` も比べる。`baseline` と `layout_font_extents` はすべて一致し、`baseline-fallback` の期待する失敗は、`math` と `hang` の代わりの値が使うグリフの箱をsofelがまだ持たないカラーのグリフのフォントの1項目である（[ADR 0023](adr/0023-color-glyphs.md)。CFFとCFF2の箱は [ADR 0021](adr/0021-cff-outlines.md) の `HbExtents` を使う）。あわせて、653個のフォントとBASEの3個のフォントで11093項目を比べ、期待する失敗は36項目である。
  - 手で組んだ小さなCFFとCFF2のフォント（丸め、制御点、移動だけの経路、空の軸、seac、入れ子の10と11、演算子の上限、各曲線とflex、ヒントマスク、CIDキーのFDSelectとローカルのサブルーチン、CFF2の `blend`、`vsindex`、FDSelectのフォーマット4、グリフの数、CFF2が無効なときのCFF、charsetとencodingとTop DICTの有効性、重複する名前）でも、`hb-dump` とすべての項目が一致することを確かめる（`tests/harfbuzz_cff.rs`）。
  - 丸め（[ADR 0026](adr/0026-harfbuzz-rounding.md)）：`hb-dump --roundf` は標準入力の `float` と `double` の値（ビットの16進）にHarfBuzzの `roundf` を通した値を書き、`hb-dump --mult` は `upem` と拡大縮小の組ごとに `hb_font_t` の `x_mult` を書く。`sofel_geom::float` の `hb_roundf`、`hb_roundf32` は、単体テストの値と決まった種の乱数の値100万個（半分の値とその前後を多く含む）で、`HbScale::x_mult` は `upem` 16、1000、1024、2048、16384と負の値を含む拡大縮小で、完全に一致することを確かめる（`tests/harfbuzz_rounding.rs`）。
- glyfの輪郭は、`ft-dump --outline` の出力と、拡大縮小なし、16画素、48画素で、命令の列、重なりの印、制御の箱が完全に一致することを確かめる（項目 `outline:<SIZE>:<グリフ>`、[ADR 0011](adr/0011-glyf-outlines.md)）。ラスタライズは、`ft-dump --raster` の出力と、12、24、48画素で画像が完全に一致することを確かめる（項目 `raster:<PPEM>:<グリフ>`、[ADR 0012](adr/0012-antialiased-rasterizer.md)）。
- 輪郭、ラスタライズ（各形式）、埋め込みビットマップ、sfnt以外の形式は、FreeTypeの出力を書き出す `tools/ft-dump/` と比べる。輪郭は完全一致を求める。sfnt以外の形式のテストのフォントは、その形式を実装するまえに、ADRで `testdata.lock` に加える。
- カラーグリフ、SVGとPDF、GPU、WOFF、検証器、展開器の基準は、それぞれのADRで定める。
- Unicodeの文字の性質は、CIで `scripts/fetch-ucd.sh` と生成の道具を走らせ、生成したファイルがコミットしたものと一致することを `git diff --exit-code` で確かめる（[ADR 0019](adr/0019-unicode-character-data.md)）。Vertical_Orientationは、`VerticalOrientation.txt` を生成の道具とは別の読み取りで読み、全文字の値の一致を確かめる。
- Unicodeの文字の性質は、`hb-dump` のファイルを取らない形と比べる（[ADR 0019](adr/0019-unicode-character-data.md)）。期待する失敗の一覧は持たず、1つでも違えば失敗とする。項目は `unicode:<項目>` である。
  - `hb-dump --unicode` は、0〜0x10FFFFと、0x110000、0x7FFFFFFF、0xFFFFFFFFのすべてについて、`hb_unicode_general_category`、`hb_unicode_combining_class`、`hb_unicode_script`、`hb_unicode_mirroring`、`hb_unicode_decompose`、Extended_Pictographic（`_hb_unicode_is_emoji_Extended_Pictographic`）、indicの分類と位置、USEの分類、アラビア文字の結合の型を1文字1行で書く。続けて、2文字の分解の1つ目の文字（と0、ハングルのL、LVの音節）と2つ目の文字（と0、ハングルのV、T）の直積の組について `hb_unicode_compose` を書く。項目は `general_category`、`combining_class`、`script`、`mirroring`、`decompose`、`extended_pictographic`、`compose`。
  - `hb-dump --arabic-pua` は、0x0000〜0xFFFFについて `_hb_arabic_pua_simp_map` と `_hb_arabic_pua_trad_map` を書く。`sofel-font` の生成した表のファイルを適合テストのクレートに取りこんで比べる（項目 `arabic_pua_simp`、`arabic_pua_trad`）。
  - `hb-dump --vowel-constraints FILE` は、HarfBuzzの `src/ms-use/IndicShapingInvalidCluster.txt` の列の1つ目の文字の文字体系ごとに、その文字体系の列に現れる文字の長さ1〜3の全列について、`_hb_preprocess_text_vowel_constraints` の後の文字の列を書く。
  - indicの分類と位置、USEの分類、結合の型、母音の組は、まだ比べない。比べる相手の表は `sofel-shape` に置くもので（[ADR 0020](adr/0020-shaping-foundation.md)、[ADR 0024](adr/0024-complex-shapers.md)）、そのクレートとともに比べる。
- 適合テストは `cargo test -p sofel-conformance` で走らせる。期待する失敗の一覧に無い失敗と、一覧にあるテストの成功は、ともにテストの失敗とする。

## 性能の計測

- 比較の相手は、HarfBuzz 15.1.0とFreeType 2.15.0とする（[ADR 0006](adr/0006-performance-measurement.md)）。シェーピング、サブセット化、COLRv1の描画はHarfBuzzと、輪郭、ラスタライズ、埋め込みビットマップ、WOFF2の展開、sfnt以外の形式はFreeTypeと比べる。
- 計測はワークスペースの外の `bench/` で行い、ネイティブとwasm（wasmtime）の両方で計測する。HarfBuzzとFreeTypeは、取得したソースから `bench/build-c.sh` で静的ライブラリにする。wasmにはwasi-sdk（`bench/wasi-sdk.lock` で版とSHA-256を固定）を使う（[ADR 0013](adr/0013-benchmark-harness.md)）。
- 1回の標本を10ミリ秒以上にまとめ、ウォームアップを3つ捨てて30個の標本を取り、中央値と中央絶対偏差を出す。計測の前に、相手と同じ結果を出していることを確かめる。
  - ラスタライズの照合は、すべてのグリフで行う（[ADR 0018](adr/0018-phantom-point-shift.md) で、FreeTypeと同じく輪郭をpp1.xだけずらすようになったため）。
  - HarfBuzzの輪郭は、glyfを分解する命令の列がFreeTypeと違うため、命令の数を照合せず、時間だけを計る。
- FreeTypeはcmapの検証に `setjmp` を使うため、wasmでは例外処理の提案（`-mllvm -wasm-enable-sjlj`、`wasmtime -W exceptions=y`）でビルドし、走らせる。
- 2段階目の項目：文字からグリフ（FreeType、HarfBuzz）、送り幅（FreeType、HarfBuzz）、輪郭（拡大縮小なしと48画素。FreeType、HarfBuzz）、ラスタライズ（12、24、48、96画素。FreeType）。
- 縦書きの項目：全グリフの縦の送り幅と縦の原点（HarfBuzz。[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。
- CFFとCFF2の項目（HarfBuzz。[ADR 0021](adr/0021-cff-outlines.md)）：HarfBuzzの `perf/benchmark-font.cc` のフォント（`SourceSansPro-Regular.otf`、`NotoSerifMyanmar-Regular.otf`、`AdobeVFPrototype.otf` の既定のインスタンス）とCIDキーの `SourceHanSans-Regular_subset.otf` で、全グリフの箱（`HbExtents::glyph_extents` と `hb_font_get_glyph_extents`、拡大縮小なし）、CIDキーでないものの全グリフの名前と名前からグリフ（`GlyphNames` と `hb_font_get_glyph_name`、`hb_font_get_glyph_from_name`）、開くこと（`Font::new` と `HbExtents::new` と箱の1回目の問い合わせ、`hb_face_create` と `hb_font_create` と箱の1回目の問い合わせ）。
- CFFの項目（FreeType。[ADR 0021](adr/0021-cff-outlines.md)、[ADR 0025](adr/0025-cff-charset-index.md)）：輪郭とラスタライズ、CIDキーのフォントの全グリフのCID（`cid` はcharsetの索引を使う `Cff::cid_with`、`cid (no index)` は索引の無い `Cff::cid`、相手は `FT_Get_CID_From_Glyph_Index`）、開くこと（`Cff::new` とcharsetの索引の配列の確保と作成、`FT_New_Memory_Face`）。
- Unicodeの性質：`perf/texts/` のすべての文章の全文字について、General_Category、Canonical_Combining_Class、Script、Bidi_Mirroring_Glyphを引く。相手はHarfBuzzの既定のUnicodeの関数（`hb_unicode_general_category` などの呼び出しを含む）とする（[ADR 0019](adr/0019-unicode-character-data.md)）。
- CIでは、ネイティブとwasmの両方を `--once` で走らせ、落ちないことと結果の照合だけを確かめる。
- 目標は、各項目でsofelの時間が比較の相手以下であること。
- 結果は `docs/benchmarks/` に記録し、コミット後は変更しない。

## ライセンス

- sofelはMITとする。FreeTypeのコードを流用する場合はFTLを選び、文書でFreeType Projectを明示する（[ADR 0002](adr/0002-license-and-reuse.md)）。Slugを流用する場合は、READMEにクレジットを載せる。
- 流用元の著作権表示とライセンスは `THIRD_PARTY_NOTICES.md` に載せ、流用したファイルの先頭に流用元を書く。
- UCDから生成した表を含む `sofel-unicode` は、Unicode License v3にも従う（`MIT AND Unicode-3.0`）。
