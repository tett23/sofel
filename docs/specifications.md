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
  sofel-color   → sofel-outline, sofel-bitmap, sofel-svg
  sofel-write   → sofel-read
  sofel-subset  → sofel-font, sofel-write
  sofel-cli     → 上のすべて（道具のバイナリ群）
  sofel-capi    → 上のすべて（C ABI、[ADR 0015](adr/0015-c-abi.md)）
```

- 輪郭と塗りは、`sofel-geom` の `Pen` と `Painter` で受け渡す。`sofel-raster`、`sofel-gpu`、`sofel-vector` はフォントの型を知らない。
- 形式のビューと検証器は、形式を読むクレートだけに置く。検証器は確保をせず、誤りを呼び出し側の関数に渡す。charstringの解釈とヒンティングは `sofel-outline` が行い、Type1とCFFで共有する。
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
- 読める形式と表：sfnt（`0x00010000`、`OTTO`、`true`、`typ1`）、TTC/OTC、head、maxp、MAXP、hhea、HHEA、hmtx、HMTX、cmap（フォーマット0、2、4、6、8、10、12、13、14、15）、loca、LOCA、glyf、GLYF（[ADR 0011](adr/0011-glyf-outlines.md)）、OS/2（版0〜5。版5を超える版は版5の欄まで）、name（形式0と1）、post（版1、2、2.5、3、4とMacの標準の258の名前）、vhea、VHEA、vmtx、VMTX（hheaとhmtxのビューを表の名前だけ変えて使う）、VORG（版1と2）（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）、BASE（版1.0と1.1。Axis、BaseTagList、BaseScriptList、BaseScript、BaseValues、BaseCoordの形式1〜4、MinMaxとFeatMinMax、Deviceの表の見出し（形式1〜3とVariationIndex）。[ADR 0017](adr/0017-base-baselines.md)）、GDEF（版1.0、1.2、1.3と、HarfBuzzの32bitのオフセットを足した1.4）、GSUBとGPOS（版1.0、1.1、1.2）（[ADR 0020](adr/0020-shaping-foundation.md)）。glyfの検証器は、各グリフをlocaの項目から次の項目までとして読み、locaの壊れた箇所はlocaの検証器が報告する。
- [ADR 0016](adr/0016-font-and-vertical-metrics.md) の表の検証器は、OS/2の版と長さの不一致、`usWeightClass` が1〜1000の外、`fsSelection` でREGULARとITALICかBOLDが同時、`usFirstCharIndex` > `usLastCharIndex`、nameの領域の外の文字列と言語タグ、記録の並び（プラットフォーム、エンコーディング、言語、名前ID）の乱れ、UTF-16BE（プラットフォーム0と3）の奇数長、postの版2の領域を越える文字列と範囲外の名前の番号、廃止の版2.5、VORGのグリフIDの昇順の乱れと重複とグリフの数以上のIDを報告する。nameの文字列が表の外にある記録は、ビューでは `None` を返す（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。
- BASEの検証器は、表に収まらないオフセットの先、タグで引く記録（BaseTagList、BaseScriptList、BaseLangSys、FeatMinMax）がタグの昇順でないこと、BaseValuesの座標の数とBaseTagListのタグの数の不一致、未知のBaseCoordの形式を `Error` に、`defaultBaselineIndex` がタグの数以上であることを `Warning` にする。ビューの `find` は、並びが乱れていてもHarfBuzzと同じ記録を返すように、HarfBuzzの二分探索（`hb_bsearch_impl`）と同じ順に調べる（[ADR 0017](adr/0017-base-baselines.md)）。
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
  - `glyf_extents`：`x_bearing` はhmtxの左のサイドベアリング（HarfBuzzの読み方）、`y_bearing` は `max(yMin, yMax)`、幅は `|xMax − xMin|`、高さは `−|yMax − yMin|`。空のグリフは0。glyfの数（locaの項目の数 − 1とグリフの数の小さいほう）以上のIDは `None`。
  - `metrics_position` と `metrics_position_with_fallback` は `hb_ot_metrics_tag_t` のすべての値を返す。postはHarfBuzzと同じく版1、2、3だけを使う。
  - `fs_type` はFreeTypeの `FT_Get_FSType_Flags`：OS/2が読めて版が0xFFFFでなければ `fsType`、そうでなければ0。
- 名前（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）：
  - `names` と `name_utf8`/`16`/`32` はHarfBuzzの `hb_ot_name_list_names` と `hb_ot_name_get_utf*` と同じ。言語はプラットフォーム3と1の言語IDをHarfBuzzの表で、プラットフォーム0は `ltag` で引く。名前IDと言語の組ごとに、エンコーディングの点の良い記録を選ぶ（(3,10)、(0,6)、(0,4)、(3,1)、(0,3)、(0,2)、(0,1)、(0,0)、(3,0)、(1,0) の順。ほかは使わない）。どれか1つの記録の文字列が表の外にあれば、HarfBuzzと同じく名前を1つも返さない。言語の照合は、完全に一致するものを先に、無ければフォントの言語が要求の接頭（`en` と `en-us`）になるものを、HarfBuzzの二分探索と同じ手順で探す。言語の指定が無ければ英語とする。UTF-16BEとASCII（Macの記録。0x80以上はU+FFFD）を変換し、呼び出し側の配列に終端の0の分を残して書く。確保しない。
  - `sfnt_name` と `sfnt_name_count` はFreeTypeの `FT_Get_Sfnt_Name`：長さが0の記録、文字列が記録（と言語タグの記録）より前か表の外にある記録、形式1の無効な言語タグを指す記録を除いた生の記録。
  - `postscript_name` はFreeTypeの `sfnt_get_ps_name`：上の記録のうち名前ID 6の、Windows（3、0か1。言語0x409の最後のもの、無ければ最初のもの）の文字のうちPostScript名に使えるもの、1文字も残らなければApple（1、0。言語0の最後のもの、無ければ最初のもの）。
  - `glyph_name` はHarfBuzzの `hb_font_get_glyph_name`（postの版1と2。版1は258個の標準の名前。版2の文字列は65535個まで、表の終わりを越える文字列からは使わない）。`glyph_from_name` は呼び出し側が持つ索引 `GlyphNames`（`glyph_names_len` 語の配列）で引き、HarfBuzzと同じく `hb_qsort_inline` で並べて二分探索する。同じ名前のグリフが複数あれば、HarfBuzzと同じものを返す。
  - FreeTypeの `FT_Get_Glyph_Name` と `FT_Get_Name_Index` は結果が違う（名前の無いグリフは `.notdef`、版1は258グリフのときだけ、版2.5を読む、文字列は0のバイトか表の終わりで切れる）ため、`ft_glyph_name` と `GlyphNames::ft_glyph_from_name` を別に持つ。
- FreeTypeの値：`ft_face_metrics` は `sfnt_load_face` の `FT_Face` の値（アセンダ、ディセンダ、高さ、最大の送り幅、下線の位置と太さ、`bbox`。値は16bitで巻き戻る。輪郭の無いフェイスとCBLC/CBDTのフェイスは0）、`ft_v_advance` は `FT_Get_Advance`（縦、拡大縮小なし）。
- `HbScale` はHarfBuzzの `hb_font_t` の拡大縮小を写す。`x_mult` は `x_scale × 0x10000` を `f32` にして `upem` で割り、0へ切り捨てる（HarfBuzzの `changed` と同じ。整数の割り算ではない）。整数の値は `(v × mult + 32768) >> 16`、小数の値はHarfBuzzの `roundf`（`floor((double) x + 0.5)`）、箱は `scale_glyph_extents`（角を `f32` で拡大し、左上を切り下げ、右下を切り上げ）で拡大縮小する。FMAは使わない。代わりの値は拡大縮小した `x_scale`、`y_scale` から求める。
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
- FreeTypeと同じく、読みこんだ輪郭を幻の点の `pp1.x` だけ左に動かす（[ADR 0018](adr/0018-phantom-point-shift.md)）。`pp1.x` は、グリフの見出しの `xMin` からFreeTypeの読み方の左のサイドベアリング（`Font::ft_left_side_bearing`）を引き、点と同じく `mul_fix` で拡大縮小した値で、丸めない。`head` の `flags` のビット1によらない。空のグリフは点が無いので動かない。`OutlineInfo` の `cbox` は動かしたあとの点から求める。
  - 合成グリフは、自身の見出しとサイドベアリングの `pp1.x` を使う。USE_MY_METRICSの部品があれば、最後のその部品の `pp1.x`（部品の見出しとサイドベアリングから求めたもの。部品の変換と位置合わせでは動かない）を使う。入れ子の合成グリフでも同じ規則を部品ごとに当てる。
  - 残りの幻の点（`pp2`、`pp3`、`pp4`）と、ヒンティングと可変フォントでの幻の点は求めない。
- HarfBuzz 15.1.0のテストのフォントのglyfを持つフェイスで、22万3925項目（拡大縮小なし、16画素、48画素）を比べ、期待する失敗は9項目である。すべて、CFFとglyfの両方を持つ `OTTO` のフォント（FreeTypeはCFFを描く）による。
- 同じフォントの単純なグリフ（見出しの `xMin` が点の最小のxと等しい4万9225個）で、拡大縮小なしの `cbox.x_min` が、HarfBuzzのグリフの箱の `x_bearing`（可変フォントの変化なし）と等しいことを確かめている。

## ラスタライズ（`sofel-raster`）

- 8bitのアンチエイリアスの画像を、FreeTypeのsmoothのラスタライザと同じ数値の計算で作る（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。セルは帯ごとの密な配列（上限64KiB）に積む。重なりの印のある輪郭は4×4の副標本で描く。FreeTypeとの差は0（完全一致）とする。
- 掃き出しは、帯の行ごとに触れた列の範囲だけを走査し、被覆率は触れたセルでだけ求めて次のセルまで使う。走査したセルは0に戻し、帯の配列を描画のたびに全体を0にはしない（[ADR 0014](adr/0014-raster-sweep-touched-cells.md)）。
- 重なりのときは、画素ごとに16個の副標本の `(coverage + 8) / 16` を16bitで合計し、最後に `min(合計, 255)` とする。合計は256以下なので、FreeTypeの標本ごとの `sum - (sum >> 8)` と同じ値になる（[ADR 0014](adr/0014-raster-sweep-touched-cells.md)）。
- FreeTypeは、輪郭をビットマップの原点へ動かしてから（重なりの印があればさらに4倍してから）命令に変換する。暗黙の曲線上の点（中点の0への切り捨て）はこの順序で決まるので、完全に一致させるには、`sofel_outline::load` で点を読み、`sofel_raster::pixel_box` で箱を求め、`Scratch::decompose` に `PixelBox::shift()` と倍率を渡して命令に変換し、`Rasterizer::render_placed` で描く。この手順は `sofel_cli::glyph::render` にまとめてある。`Rasterizer::render` は、記録した命令をあとから動かすので、中点のある輪郭では1画素の値が1〜2ずれることがある。
- HarfBuzz 15.1.0のテストのフォントのglyfを持つフェイスで、22万3917項目（12、24、48画素）を比べ、期待する失敗は9項目である。すべて、CFFとglyfの両方を持つフォントのグリフ（輪郭が一致しない）のものである。

## 道具

- `sofel-render [--face N] --ppem P (--glyph G | --char U+XXXX) FILE`：1グリフをヒンティングなしで描き、PGM（P5）を標準出力に書く。位置は注釈の行 `# left <x> top <y>` に書く（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。
- `sofel-validate <ファイル>...`：sfntとTTC/OTCの各表を検証する（[ADR 0008](adr/0008-validate-command-and-freetype-comparison.md)）。1件を1行で `<パス>[#<フォント番号>]: <タグ>+0x<オフセット>: <error|warning>: <説明>` の形で出す。`--json` では、1件を1行のJSONのオブジェクト（JSON Lines）で出す。キーは `path`、`font`、`table`、`offset`、`severity`、`kind`、`message`、`details`。`kind` と `details` のキーは出力の約束の一部とする。終了コードは、`Error` が無ければ0、あれば1、使い方の誤りか読めないファイルがあれば2で、`--json` の有無で変わらない。

## C ABI（`sofel-capi`）

- `crates/sofel-capi` は、ライブラリのクレートの機能をCの呼び出し規約で公開する（[ADR 0015](adr/0015-c-abi.md)）。静的ライブラリ、動的ライブラリ、rlibを作る。ヘッダ `crates/sofel-capi/include/sofel.h` は手で書く。
- 名前は `sofel_` で始め、型は不透明な構造体へのポインタと固定の幅の整数だけを使う。誤りうる関数は `int32_t` を返し、0を成功、負の値を誤り（`SOFEL_ERROR_*`、`sofel_error_string` で説明）とする。結果は出力の引数に書く。ポインタの引数がNULLなら `SOFEL_ERROR_NULL` を返す（`user` のポインタと `*_destroy` を除く）。panicしない。
- フォント（`sofel_font_t`）はデータを複製しない。呼び出し側は、フォントを捨てるまでデータを生かしておく。フォントは不変で、複数のスレッドから同時に使える。作業領域（`sofel_outline_scratch_t`、`sofel_rasterizer_t`）は呼び出し側が持ち、スレッドの間で共有しない。
- 可変長の出力（画像、文字列）は呼び出し側の配列に書く。足りなければ要る長さを書き、配列には何も書かず `SOFEL_ERROR_BUFFER_TOO_SMALL` を返す（`snprintf` と同じ形）。
- 輪郭はCの関数ポインタ（`move_to`、`line_to`、`quad_to`、`cubic_to`、`close` と `void *user`）で受け取る。検証の結果は、1件ずつ関数ポインタに `sofel_diagnostic_t` で渡す。
- ヘッダの各関数の説明に、対応するRustの関数（`Rust:`）と、相当するHarfBuzzとFreeTypeの関数を書く。`scripts/check-capi.sh` が、ライブラリのクレートの公開の関数（rustdocのJSONの出力から `scripts/public-api.sh` で求める）がすべてヘッダの `Rust:` の行か除外の一覧（`crates/sofel-capi/unmapped.txt`、理由つき）にあることを確かめる。ライブラリに公開の機能を足すADRは、同じプルリクエストでC ABIの関数とヘッダとCのテストも足す。
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
  - 項目は `<フォント>#<フェイス>:hb:<項目>` で、`num_glyphs`、`h_extents`、`v_extents`、`metrics`、`metrics-fallback`、`h_advance`、`v_advance`、`v_origin`、`extents`、`nominal`（U+10FFFFまでのすべての符号位置の `hb_font_get_nominal_glyph`。HarfBuzzの値のうちグリフの数以上のものは除く）、`names`、`name_utf`（全名前IDと、記録に現れる全言語と言語なしの、UTF-8、16、32。配列の長さは全体＋1と4）、`glyph_names`、`glyph_from_name`。各項目はすべての拡大縮小とすべてのグリフを含む。期待する失敗は `conformance/expected-failures-harfbuzz.txt` に書く。CFFとCFF2のフォントの箱とCFFのグリフ名（[ADR 0021](adr/0021-cff-outlines.md)）と、COLR、CBDT、sbixのカラーのグリフの箱（[ADR 0023](adr/0023-color-glyphs.md)）と、その箱で決まる縦の原点と、HarfBuzzが引かないcmapのフォーマット2と8（sofelはFreeTypeと同じく引く。[ADR 0010](adr/0010-font-foundation.md)）が期待する失敗である。653個のフォントで9128項目を比べ、期待する失敗は1165項目である。
  - 基準線（[ADR 0017](adr/0017-base-baselines.md)、`hb-dump --baselines`）の項目は `baseline`（`hb_ot_layout_get_baseline` の有無と値）、`baseline-fallback`（`hb_ot_layout_get_baseline_with_fallback`）、`layout_font_extents`（`hb_ot_layout_get_font_extents` の有無と値）。6つの拡大縮小、横と縦、9つの基準線、文字体系のタグ（`hani`、`latn`、`deva`、`DFLT` と、代わりの値の文字を持つ文字体系の新旧のタグなど34個と、BASEに現れるすべてのタグ）、言語のタグ（`dflt` とBASEに現れるすべてのタグ）の組を比べる。BASEを持つフォントを足すため、HarfBuzzの `test/api/fonts/base*.ttf` も比べる。`baseline` と `layout_font_extents` はすべて一致し、`baseline-fallback` の期待する失敗は、`math` と `hang` の代わりの値が使うグリフの箱をsofelがまだ持たないCFFとカラーのグリフのフォントの218項目である（[ADR 0021](adr/0021-cff-outlines.md)、[0023](adr/0023-color-glyphs.md)）。あわせて、653個のフォントとBASEの3個のフォントで11093項目を比べ、期待する失敗は1383項目である。
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
- Unicodeの性質：`perf/texts/` のすべての文章の全文字について、General_Category、Canonical_Combining_Class、Script、Bidi_Mirroring_Glyphを引く。相手はHarfBuzzの既定のUnicodeの関数（`hb_unicode_general_category` などの呼び出しを含む）とする（[ADR 0019](adr/0019-unicode-character-data.md)）。
- CIでは、ネイティブとwasmの両方を `--once` で走らせ、落ちないことと結果の照合だけを確かめる。
- 目標は、各項目でsofelの時間が比較の相手以下であること。
- 結果は `docs/benchmarks/` に記録し、コミット後は変更しない。

## ライセンス

- sofelはMITとする。FreeTypeのコードを流用する場合はFTLを選び、文書でFreeType Projectを明示する（[ADR 0002](adr/0002-license-and-reuse.md)）。Slugを流用する場合は、READMEにクレジットを載せる。
- 流用元の著作権表示とライセンスは `THIRD_PARTY_NOTICES.md` に載せ、流用したファイルの先頭に流用元を書く。
- UCDから生成した表を含む `sofel-unicode` は、Unicode License v3にも従う（`MIT AND Unicode-3.0`）。
