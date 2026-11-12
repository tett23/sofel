# sofel の仕様

対象範囲、クレートの層、適合テスト、性能計測の土台は [ADR 0045](adr/0045-foundation-completion-criteria.md) の完了条件を満たした。今後実装する形式と機能は[ロードマップ](roadmap.md)で追跡し、各段階で適合テストと計測項目を加える。

この文書は、採択したADRから、現在の仕様をまとめる。
決定の経緯は各ADRを参照。
可変フォントの [ADR 0027](adr/0027-variable-fonts.md) は採択（未実装）である。変化量評価の [ADR 0029](adr/0029-variation-deltas.md) と、その修正を定める [ADR 0030](adr/0030-variation-evaluation-corrections.md) は採択済みである。`--font-ptem` の分類訂正は [ADR 0028](adr/0028-ptem-is-not-opsz.md) に従って実装済みである。
[ADR 0027](adr/0027-variable-fonts.md) の表の読み取りと検証器は実装済みである。
`sofel-font::var` には、設計座標と正規化座標からの位置の構築、avar版1と版2の写像、[ADR 0029](adr/0029-variation-deltas.md) と [ADR 0030](adr/0030-variation-evaluation-corrections.md) に従うItemVariationStoreの差分の評価の核、fvarの軸と名前つきインスタンスの問い合わせ、FreeTypeのMMVarの読み取り、位置ごとのPostScript名を実装している。MVARはタグに対応する丸め前の差分を `Instance::metrics_variation` で取得でき、横・縦のスケールを適用した値もRustとC ABIから取得できる。FreeTypeが既定のインスタンスを合成した場合は、名前のIDが0でもその番号と設計座標を使える。同じタグを持つ軸が複数ある場合、タグによる指定はHarfBuzz側では該当する全軸に、FreeType側では先頭の該当軸に適用する。FreeTypeの正規化座標が全軸で0になると、設計座標の問い合わせはfvarの既定値を16.16固定小数点のまま返す。名前つきインスタンスに専用のPostScript名がない場合は、接頭辞とサブファミリー名から生成する。
ItemVariationStore の行の評価は、C ABI の `sofel_instance_item_delta_hb` と `sofel_instance_item_delta_ft` からも呼べる。ストアや行がなければ0を返し、不正なストアの列挙値は `SOFEL_ERROR_INVALID_ARGUMENT` を返す。NULL の位置は静的な既定位置とする（[ADR 0030](adr/0030-variation-evaluation-corrections.md)）。
C ABIは [ADR 0027](adr/0027-variable-fonts.md) の位置の作成、座標の取得、Variationの文字列変換、軸と名前つきインスタンスの問い合わせ、位置ごとのPostScript名を実装している。
メトリクス、基準線、輪郭、シェーピング、塗りの既存C ABI関数は位置の引数を受け取る。
メトリクスではHVARとVVARの送り幅、VORGとVVARの縦原点、MVARのフォント全体のメトリクス、FreeTypeの縦送り幅に位置を適用する。
基準線とレイアウトのフォント全体の値では、BASEのVariationIndexに位置を適用する。
レイアウトの問い合わせでは [ADR 0062](adr/0062-layout-query-device-adjustments.md) に従い、GPOS、GDEF、BASE、MATH の Device 補正に対象方向の ppem または位置の座標を使う。Rust と C ABI は合字キャレットと MATH の値・列を公開する。
gvarの幻の点による送り幅と箱、輪郭、シェーピング、塗りの変化量は未実装である。
FreeTypeが合成する既定の名前つきインスタンスは、必要なnameレコードが欠ける場合も件数と既定番号に含める。nameレコードが欠けた末尾のスタイルは名前IDと全座標がゼロである（[ADR 0036](adr/0036-freetype-missing-default-named-style.md)）。
`sofel-shape` は位置によるFeatureVariations、GPOSのVariationIndex、送り幅と縦原点を使う。`sofel-shape` のCLIは `--variations` と `--named-instance` で位置を指定する。これらと上記以外の位置による値の変更は未実装である。
追加フォントの検証は [ADR 0031](adr/0031-diverse-font-corpus.md) に従って実装した。
文書だけの変更でCIを省く方針は [ADR 0032](adr/0032-documentation-only-ci.md) と [ADR 0034](adr/0034-ci-document-input-classification.md) に従って実装している。
以下の実装済みの機能の記述は、各実装の PR で更新する。

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
  sofel-shape   → sofel-font, sofel-unicode, sofel-geom
  sofel-outline → sofel-font, sofel-geom
  sofel-bitmap  → sofel-font, sofel-png
  sofel-color   → sofel-outline, sofel-bitmap, sofel-svg, sofel-geom
  sofel-write   → sofel-read
  sofel-subset  → sofel-font, sofel-write, sofel-geom, sofel-unicode
  sofel-cli     → 上のすべて（道具のバイナリ群）
  sofel-capi    → 上のすべて（C ABI、[ADR 0015](adr/0015-c-abi.md)）
```

- `sofel-color` と `sofel-subset` の `sofel-geom` への依存は、HarfBuzzの丸め（`sofel_geom::float`）を共有するためである（[ADR 0026](adr/0026-harfbuzz-rounding.md)）。
- `sofel-subset` の `sofel-unicode` への依存は、鏡像の文字（bidiの閉包）のためである（[ADR 0022](adr/0022-subsetting.md)）。

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
- どの入力でもpanicしない。clippyの `indexing_slicing`、`arithmetic_side_effects` などを拒否にして強制し、ファジングで確かめる。シェーピングのファジングの対象（[ADR 0020](adr/0020-shaping-foundation.md)）は、任意のバイト列のフォントと文字列と機能の文字列で計画を作ってシェーピングする `shape`（グリフの数がバッファの長さの上限以下であること、`MonotoneGraphemes` と `MonotoneCharacters` でクラスタが単調であること）と、直列化を読んで書きなおし、読みなおして同じになることを確かめる `deserialize` である。GDEF、GSUB、GPOSは `gdef`、`gsub`、`gpos` で確かめる。
- 読める形式と表：sfnt（`0x00010000`、`OTTO`、`true`、`typ1`）、TTC/OTC、head、maxp、MAXP、hhea、HHEA、hmtx、HMTX、cmap（フォーマット0、2、4、6、8、10、12、13、14、15）、loca、LOCA、glyf、GLYF（[ADR 0011](adr/0011-glyf-outlines.md)）、OS/2（版0〜5。版5を超える版は版5の欄まで）、name（形式0と1）、post（版1、2、2.5、3、4とMacの標準の258の名前）、vhea、VHEA、vmtx、VMTX（hheaとhmtxのビューを表の名前だけ変えて使う）、VORG（版1と2）（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）、BASE（版1.0と1.1。Axis、BaseTagList、BaseScriptList、BaseScript、BaseValues、BaseCoordの形式1〜4、MinMaxとFeatMinMax、Deviceの表の見出し（形式1〜3とVariationIndex）。[ADR 0017](adr/0017-base-baselines.md)）、CFF、CFF2（[ADR 0021](adr/0021-cff-outlines.md)）、GDEF（版1.0、1.2、1.3と、HarfBuzzの32bitのオフセットを足した1.4）、GSUBとGPOS（版1.0、1.1、1.2）（[ADR 0020](adr/0020-shaping-foundation.md)）、MATH（版1.0。MathConstants、MathGlyphInfo、MathItalicsCorrectionInfoとMathTopAccentAttachment、MathKernInfoとMathKern、MathVariants、MathGlyphConstruction、MathGlyphAssembly。[ADR 0022](adr/0022-subsetting.md)）。glyfの検証器は、各グリフをlocaの項目から次の項目までとして読み、locaの壊れた箇所はlocaの検証器が報告する。
- 大文字の `HHEA` と `VHEA`（[ADR 0054](adr/0054-wide-metrics-header-count.md)）のメトリクス件数はオフセット34から32ビットで読む。38バイト未満はデータ不足とする。小文字の `hhea` と `vhea` の件数は16ビット、`MAXP` のグリフ数は24ビットを保つ。
- 可変フォントの表（[ADR 0027](adr/0027-variable-fonts.md)）：ItemVariationStoreの差分の行（8、16、32bitと`LONG_WORDS`）、DeltaSetIndexMapの形式0と1、HVARとVVARの送り幅やサイドベアリングの写像、MVARのタグ順の値、gvarとGVARの共有の組と圧縮した点や差分、avar版2のDeltaSetIndexMapとItemVariationStoreを読む。BASE、GDEF、COLRのItemVariationStoreとCOLRのDeltaSetIndexMapも同じビューで開く。検証器は、オフセットや数の矛盾、範囲外の番号、未知の条件の形式を診断し、`sofel-validate` から呼ばれる。
- VARC のビュー（[ADR 0067](adr/0067-varc-components.md)）は、版1のヘッダ、Coverage、CFF2 INDEX の部品記録、軸の番号、条件、MultiItemVariationStore を開く。ヘッダと各表のオフセットを検査し、範囲外なら `ReadError` を返す。
- [ADR 0016](adr/0016-font-and-vertical-metrics.md) の表の検証器は、OS/2の版と長さの不一致、`usWeightClass` が1〜1000の外、`fsSelection` でREGULARとITALICかBOLDが同時、`usFirstCharIndex` > `usLastCharIndex`、nameの領域の外の文字列と言語タグ、記録の並び（プラットフォーム、エンコーディング、言語、名前ID）の乱れ、UTF-16BE（プラットフォーム0と3）の奇数長、postの版2の領域を越える文字列と範囲外の名前の番号、廃止の版2.5、VORGのグリフIDの昇順の乱れと重複とグリフの数以上のIDを報告する。nameの文字列が表の外にある記録は、ビューでは `None` を返す（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。
- `CFF ` と `CFF2`（[ADR 0021](adr/0021-cff-outlines.md)）：ヘッダ、INDEX（16bitと32bitの数）、DICTの字句、Top DICTとFont DICTとPrivate DICT（FreeType 2.15.0の `cfftoken.h` の全項目を、FreeTypeの既定の値、64bitの `FT_Long` の数の変換、項目の型の切り詰め、同じ演算子は後が勝つ、スタックの上限（Top DICTで96、Private DICTで97、CFF2では513と514）のまま読む）、charset（予め定められた3つとフォーマット0〜2。`nLeft` のあふれはFreeTypeと同じく切りつめる）、encoding（予め定められた2つ、フォーマット0と1と補遺）、FDSelect（フォーマット0、3、4）、CFF2の `blend`（係数はFreeTypeの `cff_blend_build_vector`）を読む。Item Variation Storeは共通のモジュール `item_variation` に置く。charstringの解読器は、FreeTypeのAdobe engine（`psintrp.c`、`pshints.c`）からヒンティングと太らせを外して移植したもので、スタックの型、サブルーチン（入れ子16）、送り幅、ステムの数とマスク、flex、seac、算術の演算子、CFF2の `blend` と `vsindex`、長さ0の線を捨てる経路の規則を保つ。検証器は、ヘッダ、INDEX、Name INDEXの数、DICT（オペランドの過多と不足、壊れた実数、FreeTypeが拒む値はError、未知の演算子、版で許されない演算子、重複はWarning）、CharStringsの数（0はError、`maxp` との違いはWarning）、charset、encoding、FDArray（257以上はError）、FDSelect（フォーマット4はWarning）、Variation Store（`fvar` と軸の数が違えばError）、ローカルのサブルーチン、全グリフのcharstring（誤りはグリフIDと位置つきのError、未知の演算子はWarning）を確かめる。FDSelectのフォーマット4を除き、Errorの無い表はFreeTypeが開ける。
- CFF2 の FDSelect フォーマット4（[ADR 0056](adr/0056-cff2-fdselect-format4-warning.md)）は `fd_select_format4` の Warning として報告する。範囲や番兵が壊れていれば Error も報告する。ほかに Error がなければ `sofel-validate` は終了コード0を返す。FreeType 互換の輪郭取得はフォーマット4を拒む。
- CFF2の座標つきの解釈（[ADR 0027](adr/0027-variable-fonts.md)、[ADR 0030](adr/0030-variation-evaluation-corrections.md)）：FreeType側の係数は `Instance::blend_scalars::<Ft>` から元の列順で渡す。係数が0の列も残し、Private DICTとcharstringを位置ごとに評価する。HarfBuzz側の箱は、`f32` の領域係数と `f64` の差分の積和を使う。C ABIの `sofel_cff2_blend_vector` は、既定成分の65536を先頭に置き、必要な長さを返す。出力配列が短ければ変更しない。HarfBuzzのCFF2の箱は、fvar軸のあるフォントの既定位置でも全軸の0座標で領域を評価する。FreeTypeの静的な既定位置では差分の係数を0にする。
- CFFのcharsetの索引（[ADR 0025](adr/0025-cff-charset-index.md)）：`Charset::num_ranges` はフォーマット1と2の範囲のうち、FreeTypeと同じくグリフの数を覆うまでの数（フォーマット0と予め定められたcharsetは0）。`Charset::index_into` は、呼び出し側の配列に各範囲の最初のグリフIDを書き、それを借りる `CharsetIndex`（`Copy`、`Send`、`Sync`）を返す。配列が短ければ何も書かずに要る長さを返す。確保はしない。`CharsetIndex::sid` は範囲の最初のグリフIDを二分探索し、`CharsetIndex::glyph` は範囲を順に見る。値は0xFFFFでの切りつめを含め、常に `Charset::sid`、`Charset::glyph` と等しい。
- BASEの検証器は、表に収まらないオフセットの先、タグで引く記録（BaseTagList、BaseScriptList、BaseLangSys、FeatMinMax）がタグの昇順でないこと、BaseValuesの座標の数とBaseTagListのタグの数の不一致、未知のBaseCoordの形式を `Error` に、`defaultBaselineIndex` がタグの数以上であることを `Warning` にする。ビューの `find` は、並びが乱れていてもHarfBuzzと同じ記録を返すように、HarfBuzzの二分探索（`hb_bsearch_impl`）と同じ順に調べる（[ADR 0017](adr/0017-base-baselines.md)）。
- MATHの検証器（[ADR 0022](adr/0022-subsetting.md)）は、表に収まらないオフセットの先とDeviceの表、Coverageの壊れた形と並び、Coverageの番号で引く配列（MathItalicsCorrectionInfo、MathTopAccentAttachment、MathKernInfo、MathVariantsの縦と横）の長さとCoverageのグリフの数の不一致を `Error` に、変種と部品のグリフIDがグリフの数以上であることと `partFlags` の予約のビットを `Warning` にする。GDEF、GSUB、GPOSと同じく、たどる回数に表の大きさに比例する上限を設ける。
- `cff::hb`（[ADR 0021](adr/0021-cff-outlines.md)）：HarfBuzz 15.1.0が `CFF ` と `CFF2` を読む手順を、FreeTypeの読み方とは別に持つ。サニタイザの範囲の確かめ（`check_point`、`check_range`。オフセットは符号つきで、表の前を指せば失敗）と、範囲ごとに減る操作の予算（表の長さの64倍、16384以上、2^30 − 1以下。尽きれば以後の確かめはすべて失敗）、`CFFIndex` の `sanitize` と `operator []`（逆順や最後を越えるオフセットは空の文字列、オフセット0は1バイト前から）と `get_size`（64bitの和を `int` のオフセットにするときの下位32bit）、charset（フォーマット0〜2をフェイスのグリフの数ちょうどで確かめ、切りつめない）、encoding（補遺は要素が0個なら誤り）、FDSelect（CFFは0と3、CFF2は4も。範囲の始まりはフェイスのグリフの数未満、FDはFDArrayの数未満、番兵はフェイスのグリフの数以下でCFF2ではCharStringsの数以上）、CFF2のItem Variation Storeの確かめと `get_region_index_count`、DICTの解釈器（513の `double` のスタック、`pop_int` の `int` の範囲と0への切り捨て、`pop_uint` の負の誤り、BCDの実数は `hb_number_parser_t` と同じ `double` の計算）である。
- レイアウトの表（[ADR 0020](adr/0020-shaping-foundation.md)）：
  - 共通の構造（`sofel_read::layout`）は、ScriptList、Script、LangSys、FeatureList、Feature、LookupList（GSUBとGPOSの版1.2の `lookupList2` は32bitのオフセット）、Lookup、Coverage（形式1〜4）、ClassDef（形式1〜4）、文脈と連鎖文脈のサブテーブル。形式3と4のCoverage、形式3と4のClassDefは24bitのグリフID（ClassDefの形式3はクラスも24bit）を持ち、`ClassDef::get` は `u32` を返す。版1.2は、32bitのオフセットが0でなければそれを、0なら16bitのオフセットを使う。版1.1のFeatureVariationsは、Conditionの形式1〜5とFeatureTableSubstitutionを読む（[ADR 0027](adr/0027-variable-fonts.md)）。Featureの引数のオフセットは、HarfBuzz 15.1.0と同じく、`size` も含めてFeatureの表から数える（HarfBuzz 15.1.0は、FeatureListから数える古いフォントの例外を持たない）。
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
- 同じ道具が、記号フォントのcmapの付けかえ（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）に使う、HarfBuzzの `gen-arabic-pua.py` と同じアラビア文字の私用領域の対応の表を `sofel-font` に書く。同じ道具が、シェーパーの分類の表を `sofel-shape` の `src/tables/` に書く（[ADR 0024](adr/0024-complex-shapers.md) のシェーパーが使う）。HarfBuzz 15.1.0の生成のスクリプトの規則（付けかえ、既定の値、範囲の選び方）を移植し、値の意味はHarfBuzzと揃え、表の形は上の多段の表とする。
  - `indic.rs` の `indic_categories` は `hb_indic_get_categories`（`gen-indic-table.py`）と同じ値を返す。下位のバイトがindic、khmer、myanmarのシェーパーの分類（番号はHarfBuzzの状態機械の `export` の値）、上位のバイトが位置（`ot_position_t`）である。
  - `use_.rs` の `use_category` は `hb_use_get_category`（`gen-use-table.py`。Microsoftの `ms-use/` の付けかえを含む）と同じ値を返す。
  - `arabic.rs` は、`joining_type`（HarfBuzzの表の値そのもので、表に無い文字は `JOINING_TYPE_X` の7）、表示形の表 `SHAPING_TABLE`（`SHAPING_TABLE_FIRST` から `SHAPING_TABLE_LAST` までの各文字の語頭形、語中形、語末形、独立形）、合字の表 `LIGATURE_TABLE`、`LIGATURE_MARK_TABLE`、`LIGATURE_3_TABLE`（1つ目の構成要素と、`(残りの構成要素, 合字)` の並び。HarfBuzzと同じ内容と順で、HarfBuzzの0の詰め物は持たない）、`has_arabic_joining`（`gen-arabic-joining-list.py`。ISO 15924のタグで引く）を持つ。
  - `vowel_constraints.rs` は、`ms-use/IndicShapingInvalidCluster.txt` の列を1つ目の文字の文字体系ごとにまとめた表と、それを引く `vowel_constraint_match` を持つ。HarfBuzzの生成のスクリプトと同じく、一方が他方の先頭である2つの列は短いほうだけを残す。`vowel_constraint_match(script, cps, i)` は、HarfBuzzの `_hb_preprocess_text_vowel_constraints` の繰り返しが位置 `i` で一致させる列の長さ（2か3）を返す。長さ3の列は `i + 2 < cps.len()` のときだけ一致する。呼び出し側は、一致すれば列の最後の文字の前に U+25CC を挟んで `i + 長さ` へ進み、一致しなければ `cps[i]` を出して `i + 1` へ進む。生成の道具は、HarfBuzzの生成のスクリプトが扱えない形の列の組を、誤りとして止める。

## 言語と、文字体系と言語のタグ（`sofel-shape`）

- `Language::new(bytes)` は、HarfBuzzの `hb_language_from_string` と同じ正規化をする（[ADR 0020](adr/0020-shaping-foundation.md)）。入力の先頭の63バイトを取り、ASCIIの英字を小文字に、`_` を `-` にし、英数字と `-`、`_` 以外のバイトでそこまでとする（`ja_JP.UTF-8` は `ja-jp`）。入力が空か、先頭がNULなら `None`（`HB_LANGUAGE_INVALID`）とする。先頭のバイトが対象の外なら、HarfBuzzと同じく空の言語（`""`）とする。
- 文字体系と言語からOpenTypeのタグへの変換（`ot_tags_from_script_and_language`）と、その逆（`ot_tags_to_script_and_language`、`ot_tag_to_script`、`ot_tag_to_language`）は、HarfBuzz 15.1.0の `src/hb-ot-tag.cc` と同じとする。新しい世代のタグ（`dev3`、`dev2`、`mym2` など）を先に、古いタグ（`kana`、`lao ` など）を後に返す規則、私用の下位タグ `-x-hbsc-`、`-x-hbot-`（16進の8桁か4文字まで）、拡張言語の下位タグ、地域や文字体系の下位タグに依存する例外、2文字と3文字の言語の表、複数のタグを持つ言語、曖昧なタグの言語を含む。
  - タグの数の上限は、HarfBuzzのシェーパーと同じく文字体系に3、言語に3とする（`MAX_TAGS_PER_SCRIPT`、`MAX_TAGS_PER_LANGUAGE`）。`ot_tags_from_script_and_language_max` は上限を引数で取る。
  - 文字体系の `None` は `HB_SCRIPT_INVALID`、言語の `None` は `HB_LANGUAGE_INVALID`、言語のタグの `None` は `dflt` とする。
  - HarfBuzzの `hb_ot_tag_to_script` は、知らないタグを先頭の文字を大文字にしたそのままの値で返す。`ot_tag_to_script` と `ot_tags_to_script_and_language` は、ゼロを `None` とし、それ以外は `Aran`、`Qaai` や ISO 15924 の形でない値も4バイトのまま `Script` に格納する（[ADR 0050](adr/0050-preserve-opentype-script-values.md)）。`ot_tags_to_script_and_language` の言語に付ける `-x-hbsc-` は、HarfBuzzと同じく元のタグの値から作る。
  - HarfBuzzはC文字列をポインタでたどる。移植は、言語を末尾の先が0のバイト列として読み、位置をその中の添字として扱って、空の下位タグ、言語の無い私用の部分、`limit` が空のポインタの場合も同じ結果にする。HarfBuzzの「最後に見つけた位置」のキャッシュは、結果を変えないので持たない。
- 言語の表（`ot_languages2`、`ot_languages3`、`ot_languages3_blocked`、`ot_languages3_multi*`）と、生成された2つの関数（`hb_ot_tags_from_complex_language`、`hb_ot_ambiguous_tag_to_language`）は、`tools/gen-ot-tag/`（ワークスペースの外のRustのパッケージ）が、`scripts/fetch-testdata.sh` で取得したHarfBuzzの `src/hb-ot-tag-table.hh` から `crates/sofel-shape/src/tag_table.rs` に書き、コミットする。
  - 道具は、コメントを除いた行ごとに、決まった形（配列の要素、`subtag_matches`、`lang_matches`、`strncmp` と `subtag_matches` の組、`strcmp`、1文字目の `switch` の `case`、1つのタグか `possible_tags` の配列の結果、`case` と `hb_language_from_string`）だけを受け付け、知らない形、長さの引数と文字列の長さの不一致、並びの乱れで失敗する。
  - 関数は、分岐の表（1文字目、条件の種類と文字列、結果のタグの列）として書く。`sofel-shape` は表を順にたどり、HarfBuzzの分岐と同じ順で最初に成り立つものを使う。
  - CIは、道具の単体テストと、作りなおした表とコミットしたものの一致を `git diff --exit-code` で確かめる。

## フォント（`sofel-font`）

- `Font::new(data, index)` は、sfntとTTC/OTCのフォントを開き、head、グリフの数、横書きのメトリクス、文字の対応に使うcmapのサブテーブルを一度だけ読む（[ADR 0010](adr/0010-font-foundation.md)）。不変で、キャッシュを持たない。
- グリフの数は `MAXP` を `maxp` より優先する。メトリクスは `HHEA`/`HMTX` を `hhea`/`hmtx` より優先する。
- cmapのサブテーブルは、HarfBuzzと同じ順（(3,0)、(3,10)、(0,6)、(0,4)、(3,1)、(0,3)、(0,2)、(0,1)、(0,0)、(1,0)、(1,任意)）で、読めるものを選ぶ。グリフの数以上のグリフIDは対応なしとする。HarfBuzzと同じく、記号フォント（(3,0)）では、対応の無い文字を、OS/2（版0）のフォントのページが無ければU+0000〜U+00FFをU+F000〜U+F0FFとして、簡体と繁体のアラビア文字のページならHarfBuzzのPUAの表で引きなおす。(1,0) では、ASCIIの外の文字をMacRomanに変換して引く（HarfBuzzと同じく、符号位置の下位16bitで表を引く）。ほかのMacのエンコーディングはASCIIだけを引く（[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。
- シェーピングは、HarfBuzzの `hb_font_get_nominal_glyph`（`ot` のフォントの関数）と同じ `Font::nominal_glyph_raw` と `variation_glyph_raw` を使う（[ADR 0020](adr/0020-shaping-foundation.md)）。グリフの数以上のグリフIDもそのまま返し（送り幅は0、輪郭は無い）、HarfBuzzが引かないフォーマット2と8のサブテーブルからは何も返さない。`nominal_glyph` と `variation_glyph` は、これまでどおりフォーマット2と8も引き、グリフの数以上のIDを対応なしとする（[ADR 0010](adr/0010-font-foundation.md)）。
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
- FreeTypeの値：`ft_face_metrics` は `sfnt_load_face` の `FT_Face` の値（アセンダ、ディセンダ、高さ、最大の送り幅、下線の位置と太さ、`bbox`。値は16bitで巻き戻る。輪郭の無いフェイスとCBLC/CBDTのフェイスは0）。glyfの `ft_h_advance` と `ft_v_advance` は、位置設定後でグリフをまだ読んでいないfaceの初回 `FT_Get_Advances`（拡大縮小なし、縦は `FT_LOAD_VERTICAL_LAYOUT` も指定）の対応する値を返す。結果が `u16` に収まらなければ `None` とし、問い合わせ順で値は変わらない（[ADR 0048](adr/0048-freetype-advance-query-state.md)）。
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
- レイアウトの問い合わせ（[ADR 0062](adr/0062-layout-query-device-adjustments.md)）では、`Instance::layout_queries` に `HbScale` と横・縦の ppem を渡す。Device のピクセル補正は対象方向の ppem とスケールを使い、VariationIndex は GDEF または BASE の ItemVariationStore と位置の座標を使う。基準線と MinMax の値は横書きで縦方向、縦書きで横方向の補正を受ける。合字キャレットは GDEF の列を返し、MATH の定数、イタリック補正、アクセント、伸長形、カーニング、異体グリフ、組み立て部品、最小重なりを取得する。対象グリフのアクセントの記録がなければ、横送り幅の半分を返す。`style_value` は同じ位置の fvar、STAT、OS/2、head、post と GPOS の `size` feature を参照し、fvar に軸がない光学サイズでは `HbScale` のポイントサイズを STAT より優先する。
- hheaの代わりに `HHEA` を使う条件と `HHEA` の `numberOfLongMetrics` の幅（24bit）は、[ADR 0007](adr/0007-sfnt-read-foundation.md) の読み方のままとする（HarfBuzzとFreeTypeは32bitで読む）。

## 輪郭（`sofel-outline`）

- glyfと `GLYF` の単純なグリフと合成グリフの輪郭を、ヒンティングをせずに、FreeTypeと同じ座標で `Pen` に渡す（[ADR 0011](adr/0011-glyf-outlines.md)）。`GLYF` があれば `GLYF` と `LOCA` を使い、そのときだけフラグ0x80を三次曲線、0x2000を24bitの成分のグリフIDとして扱う。
- 倍率は `FT_Set_Pixel_Sizes` と同じく `div_fix(ppem << 6, unitsPerEm)` で、点は `mul_fix` で拡大縮小し、丸めない。locaの解釈と合成の位置の計算はFreeTypeに合わせる。
- 作業領域（`Scratch`）は呼び出し側が持つ。
- VARC の輪郭（[ADR 0067](adr/0067-varc-components.md)）は、対象のグリフで部品を記録順に処理し、条件、部品用の軸座標、累積変換を評価する。現在の再帰経路での循環を止め、深さ64とグリフごとの共有辺数16384を上限とする。HarfBuzz と比較する浮動小数点の描画命令は `Outlines::varc_glyph_path` で返し、既存の整数 `Pen` API を保つ。
- [ADR 0063](adr/0063-font-functions-and-synthesis.md) の `Scratch::hb_decompose` は、読み込んだglyfの点をHarfBuzzの経路の命令に変える。連続する曲線外の点の中点を `f32` で計算する。`HbExtents::cff_glyph_path` はCFFとCFF2のcharstringから、既定位置と可変フォントの指定位置でHarfBuzzの描画命令を返す。これらを親子のフォント関数へ渡す輪郭の供給に使う。`Scratch::oblique_slot` と `Scratch::embolden_slot` はFreeTypeの輪郭スロットの斜体と太字を適用し、後者は `SlotMetrics` も更新する。
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

## シェーピング（`sofel-shape`）

[ADR 0063](adr/0063-font-functions-and-synthesis.md) の `SubFont` は親への参照と問い合わせ別の関数を持つ。子に関数がなければ親へ委ね、送り、原点、箱と輪郭の座標を子のスケールへ換算する。`shape_with_subfont` はこの関数で文字からのグリフ検索と配置を行い、合成太字の送りへの加算を `in_place` の指定に従って切り替える。輪郭はルートの `set_outline_func` で供給する。合成斜体と太字は、HarfBuzzと同じ順にその輪郭へ適用する。`sofel-shape` コマンドは `--font-slant`、`--font-bold`、`--font-grade` を受け、glyfとCFF/CFF2の輪郭を供給する。縦メトリクスがないフォントでは、HarfBuzzの合成した横方向のフォントの箱から縦送りと縦原点を求める。

### 機能の文字列

- `Feature { tag, value, start, end }` を、HarfBuzzの `hb_feature_from_string` と同じ文法で `Feature::parse(&[u8])` が読み、`hb_feature_to_string` と同じ形で `Feature::format`（と、追記する `Feature::write`）が書く（[ADR 0020](adr/0020-shaping-foundation.md)）。`kern`、`+kern`、`-kern`、`kern=0`、`aalt=2`、`kern[3:5]`（`;` でもよい）、`kern[3]`（3..4）、`kern[:5]`、`kern[3:]`、`kern[]`、CSSの `"kern" on` と `'kern' 2` を受け付ける。`end` の既定は `u32::MAX` である。
- HarfBuzzの癖もそのまま写す。タグは空白、`=`、`[`、引用符（引用符が無ければ0のバイト）でだけ終わるので、`kern\t` は5バイトのタグとして失敗する。引用符のタグはちょうど4バイトとする。短いタグは空白で埋め、書くときに末尾の空白を落とす。数はCの `int` として読み、負の数は折り返す（`kern[-1]` の開始は `u32::MAX`）。`=` の後の値は読めなければ失敗するが、`=` の無い位置の数でも `on`、`off` でもない英字の列は読み飛ばす（`kern xyz` は `kern`）。
- 書き出しはバイト列で、タグのバイトをそのまま書く。

### 直列化

- `hb-buffer-serialize.cc` と同じ書き出しを、バッファの型によらない関数として置く（[ADR 0020](adr/0020-shaping-foundation.md)）。`serialize_glyphs(_range)` はグリフの列（`GlyphInfo` と、位置を持てば `GlyphPosition`）を、`serialize_unicode(_range)` は文字とクラスタの列を、`SerializeFormat::{Text, Json}` と `SerializeFlags`（HarfBuzzと同じビット）で書く。範囲の関数は、HarfBuzzの `buf_size` と同じく、残りの大きさより短いグリフだけを書いて、書いた数を返す。`[` と `]` は0番目と範囲の最後のグリフに付く。`serialize_invalid` は内容の無い空のバッファの `!!` と `[]` を書く。
- フォントへの問い合わせは `SerializeFont`（グリフ名、グリフの箱、名前からのグリフ、文字からのグリフ。既定は何も知らない `EmptyFont`、HarfBuzzの `hb_font_get_empty`）にまとめる。名前は `hb_font_glyph_to_string` と同じく127バイトと最初の0のバイトで切り、名前が無ければ `gid<n>` と書く。`glyph_from_string` は名前、10進のグリフID、`gid<n>`、`uni<16進>`（cmap）の順に読む。
- 読み込み（`deserialize_glyphs`、`deserialize_unicode`）は、HarfBuzzのRagelの文法と同じ言語を受け付ける手書きの解析器で、結果を呼び出し側のベクタに追記する。受け付けない入力でのRagelの機械の振る舞いも写す。アクション（範囲外の数、名前の解決できないグリフ、文字のバッファのグリフ）が失敗すれば、それまでの項目を残し、終わりの位置（`DeserializeError::consumed`、HarfBuzzの `end_ptr`）を最後に項目を足した位置とする。文法に無いバイトではそのバイトの位置で止まる。項目の途中で入力が終われば、その項目を除いて成功とする。テキストの項目は次の項目が始まるときか入力の終わりで足され、JSONの項目は `,` か `]` と、その後の空白のたびに足される。テキストのグリフ名の後の `<` は名前の続きとグリフの箱の始まりの両方であり、HarfBuzzと同じく両方をたどる。

### 区間の性質とバッファ

- `SegmentProperties { direction, script, language }`（どれも `Option`）の `guess(buffer, default_language)` が、`hb_buffer_guess_segment_properties` と同じく、文字体系をCommon、Inherited、Unknownでない最初の文字から、方向を `script_horizontal_direction`（`hb_script_get_horizontal_direction`。両方向に書く文字体系は `None` で、そのときは左から右）から決め、`ResolvedProperties` を返す。言語は引数の `default_language` で、ロケールは読まない（[ADR 0020](adr/0020-shaping-foundation.md)）。
- `UnicodeBuffer` は文字、クラスタの値、前後の文脈（各5文字まで）、フラグ（`BufferFlags`、HarfBuzzと同じビット）、クラスタの水準（`ClusterLevel`）、見えないグリフ、見つからないグリフ、見つからない異体字セレクタのグリフ、置きかえの文字、`rand` の乱数の状態（HarfBuzzと同じ `state × 48271 mod 2³¹ − 1`、積は32ビットで折り返す。初期値1）を持つ。`push_utf8`、`push_utf16`、`push_utf32`、`push_codepoints`、`push_latin1` は `hb_buffer_add_utf*` と同じで、クラスタの値は `text` の先頭からの位置、不正な符号の列は `src/hb-utf.hh` と同じ区切りで置きかえの文字になる。`clear` は `hb_buffer_clear_contents` と同じく設定を残して中身を消し、乱数の状態を1に戻す。入力の長さと操作の数の上限は `HB_BUFFER_MAX_*` と同じ定数（`MAX_LEN_FACTOR` など）で決める。
- `UnicodeBuffer::append(source, start, end)` は、Unicodeのバッファの `hb_buffer_append` と同じく、範囲の文字をクラスタの値ごと足し、空のバッファに足すときは範囲の前の文字と元の前の文脈を、いつも範囲の後の文字と元の後の文脈を、各5文字まで文脈にする。`similar` は `hb_buffer_create_similar` と同じく設定（フラグ、クラスタの水準、置きかえの文字、見えないグリフ、見つからないグリフ、見つからない異体字セレクタのグリフ）だけを写す。`number_clusters` は、`hb-shape` が `--utf8-clusters` の無いときにするように、クラスタの値を文字の番号に付けなおす。
- `GlyphBuffer` は `GlyphInfo`（グリフID、クラスタ、`GlyphFlags`）と `GlyphPosition` の列を持つ。`into_unicode` は、設定を残し確保した領域を再利用する空の `UnicodeBuffer` を返す。`normalize_glyphs` は `hb_buffer_normalize_glyphs`、`from_parts` は読み込んだ列から作る。`diff` と `diff_unicode` は `hb_buffer_diff` と同じ `DiffFlags` を返す。
- 作業用のバッファの要素は、HarfBuzzの `var1`、`var2` の共用体を名前のついた欄に分けた28バイトの構造体である（[ADR 0051](adr/0051-shaping-buffer-item-size.md)）。HarfBuzzの20バイトに、正規化が選んだグリフの欄（HarfBuzzは `glyph_props`、`lig_props`、`syllable` と重ねる）が加わる。位置は別の配列（20バイト）で、`GPOS` の付着の鎖を持つ。出力の配列は、入力より長くなるまで入力の配列そのものに書く。

### 計画

- AAT の置換を選んだとき、文字体系に専用の OT シェーパーがあれば `dumber` のフックで処理する。既定のシェーパーはそのまま使う。
- `feat` の設定数・範囲を検査し、フォントが持つ機能だけを AAT のセレクターに対応付ける。チェーンのフラグはクラスタの開始・終了位置ごとに決定し、無効区間では状態機械を開始状態に戻す。
- `morx` の言語セレクターは、`ltag` のタグが指定言語全体またはサブタグ境界までの接頭辞と一致したとき、チェーンの有効フラグを更新する。たとえば `sr` は `sr-Latn-RS` に一致し、`srb` には一致しない。
- `morx` を使えない場合は旧 `mort` を調べる。`mort` のチェーンは16ビットの件数とバイトオフセット、状態機械は8ビットの遷移エントリ番号を使う。
- `morx` の並べ替えは状態表の先頭からグリフを分類し、指定された両端のグリフを、クラスタを統合して並べ替える。文脈置換は状態遷移に従い、マークしたグリフと現在のグリフにそれぞれ置換表を適用する。挿入はマークと現在位置の前後に指定グリフを加える。合字は部品のスタックから置換先を引き、消費したグリフのクラスタを統合する（[ADR 0060](adr/0060-aat-shaping.md)）。
- 旧 `kern` の形式 0 はグリフ対、形式 2 は左右のクラスから調整量を引く。形式 3 は左右のクラスを組み合わせて調整値のインデックスを引く。形式 1 は状態遷移で位置を最大8件まで積み、値の下位ビットが立つまで位置を取り出して送りとオフセットを調整する。シェーパーが旧カーニングを使う文字体系では、`kerx` を選ばず、選んだ GPOS にカーニング機能がない場合に適用する。
- `kerx` の形式 0 はグリフ対、形式 2 は左右のクラスからカーニング量を引く。形式 6 は左右のクラスから 16 ビットまたは 32 ビットの行列を引き、タプルがある場合はベクター内の調整値を読む。形式 1 は状態遷移に従い、直近 8 個の位置を参照して調整する。形式 4 はアンカーテーブルまたは直接指定された座標を使い、マークしたグリフに現在のグリフを配置する（[ADR 0060](adr/0060-aat-shaping.md)）。
- `--font-ptem` でポイントサイズを指定すると、`STAT` と `trak` を持つフォントではそのサイズに応じて字間を調整する。
- `ShapePlan::new(instance, props, features)` が、`hb_ot_shape_plan_t` と同じ判断を一度だけ行う（[ADR 0020](adr/0020-shaping-foundation.md)）。`GSUB` と `GPOS` の文字体系と言語の選択（`hb_ot_layout_table_select_script` の `DFLT`、`dflt`、`latn` への後退と `hb_ot_layout_script_select_language`）、シェーパー（`Shaper`、`hb_ot_shaper_categorize`）、`hb_ot_shape_collect_features` の順の機能、`hb_ot_map_builder_t::compile` と同じマスクのビットの割り当て（大域の値1の機能は31ビット目、ほかはグリフのフラグの後の4ビット目から。足りなければ機能を捨てる）、必須の機能、`F_GLOBAL_SEARCH`、`F_HAS_FALLBACK`、段ごとにlookupの番号の順に並べて重複をまとめたlookupの列、位置の決め方（`GPOS` の有無、マークの幅の0化、フォールバックのマークの配置）である。AATの表と古い `kern` 表は無いものとして判定する。
- 計画は `GDEF` のグリフのクラス（HarfBuzzの拒否の一覧に載る表は無いものとする）とマークのグリフの集合、`GSUB` と `GPOS` のすべてのlookupについて、拡張をたどったサブテーブルのビューと、それぞれの最初のグリフのCoverageの要約（`hb_set_digest_t` と同じ3つの64ビットのマスク）を持つ。拡張の型がそろわないlookupと、読めないサブテーブルは、HarfBuzzと同じく無いものとする。
  GSUBとGPOSのCoverageを三つの64 bitマスクに要約するときは、全ビットが立った時点でそのCoverageの走査を終える。どのグリフが照合の候補になるかは全範囲を読んだ場合と一致する（[ADR 0042](adr/0042-stop-at-saturated-coverage-digest.md)）。
- `ShapePlan::key` は計画を作った区間の性質、機能、GSUBとGPOSで選んだFeatureVariationsの記録番号を返す。計画は不変で、`Send` と `Sync` を満たす。
- `layout_selection(font, script_tags, language_tags)` は、計画と同じ文字体系と言語の選択の結果を返す。
- [ADR 0024](adr/0024-complex-shapers.md) のシェーパー（hebrew、hangul、thai、arabic、indic、khmer、myanmar、myanmar_zawgyi、USE）をすべて実装した（下の「文字体系ごとのシェーパー」）。シェーパーの差しこみ口（`collect_features`、`override_features`、`data_create`、`preprocess_text`、`postprocess_glyphs`、`decompose`、`compose`、`setup_masks`、`reorder_marks`、`gpos_tag`、正規化の方式、マークの幅の0化の時期、フォールバックの配置、段の間の関数）は、HarfBuzzの `hb_ot_shaper_t` と同じ位置に置く。

### 手順

- `shape(instance, scale, plan, buffer)` と、グリフの箱をフォントの単位で返す関数を受け取る `shape_with_extents` が、`hb_ot_shape_internal` の順に処理する（[ADR 0020](adr/0020-shaping-foundation.md)）。`shape` は位置に応じたグリフの箱を使う。`Scale` は `HbScale` とppemを持つ。ppemが0ならヒンティングのDevice表を使わないが、VariationIndexは位置に応じて評価する。
  - Unicodeの性質と書記素の続き、点線の円（U+25CC）の挿入、書記素のクラスタへのまとめ、本来の方向への調整（RTLの文字体系の数字の列と地域指示子）、鏡像と、`vert` の無い縦書きでの `unicode::vertical_char_for`。
  - 正規化（`_hb_ot_shape_normalize`）：分解（フォントにある短い形を選ぶ短絡、異体字セレクタ、空白と U+2011 の代わり）、結合クラスでの並べかえ（32個まで）、CGJの扱い、合成。default のシェーパーの `AUTO` は `COMPOSED_DIACRITICS` になる。
  - 分数（U+2044 の前後の数字への `numr`、`frac`、`dnom`）と利用者の機能の範囲のマスク、`GDEF` のクラスか一般カテゴリからの合成。
  - `GSUB` と `GPOS` の適用は `hb-ot-layout-gsubgpos.hh` と同じで、飛ばすグリフの判定、自動のZWJとZWNJ、合字の成分の番号、入れ子（64まで）、操作の数の上限、`UNSAFE_TO_BREAK` と `UNSAFE_TO_CONCAT` の範囲、4つより多い規則の速い経路（フラグの立て方が変わる）、逆向きの連鎖の単一の置換を写す。付着はHarfBuzz 15.1.0と同じく筆記体の鎖を交差方向に足して求める。
  - 既定で無視する文字の非表示（空白のグリフか見えないグリフ）、削除、保持、見つからない異体字セレクタのグリフ、送り幅と縦書きの原点、空白の代わりの幅、マークの幅の0化、フォールバックのマークの配置、逆向きなら反転、クラスタごとのフラグの伝播。
- `shape_fallback` は HarfBuzzの `fallback` のシェーパー（`src/hb-fallback-shape.cc`）である。
- HarfBuzzの `roundf`（`floor ((double) x + 0.5)`）は `sofel_geom::float` を使う（[ADR 0026](adr/0026-harfbuzz-rounding.md)）。

### 文字体系ごとのシェーパー

[ADR 0024](adr/0024-complex-shapers.md) のシェーパーは `crates/sofel-shape/src/shapers/` に置き、HarfBuzz 15.1.0の `hb-ot-shaper-*.cc` と同じ規則とする。シェーパーごとの値（分類、位置、音節の番号、ハングルの字母の機能やアラビア文字の結合の動作）は、バッファの要素の名前のついた欄（`shaper_category`、`shaper_position`、`syllable`、`shaper_aux`）に置く。要素は28バイトである。

- hebrew：フォントのGPOSに `mark` が無いとき、正規の合成に無い表示形（U+FB1D〜U+FB4F）へ合成する（`compose_hebrew`）。パタハかカメツ、シェヴァかヒリク、メテグか下のマークの並びで、後ろの2つを入れかえる。`gpos_tag` は `hebr` で、GPOSの文字体系が違えばGPOSを使わない。
- hangul：前処理で、字母の列を音節に合成するか（フォントに音節のグリフがあるとき）、音節を字母に分解して `ljmo`、`vjmo`、`tjmo` のマスクを付ける。声調の印は、幅が0でなければ音節の前へ移し、音節が無ければ点線の円を足す。字母では `calt` のマスクを外す。正規化は `NONE`。
- thai（タイ文字とラオ文字）：前処理で `SARA AM` をNIKHAHITと `SARA AA` に分解し、NIKHAHITを上のマークの前へ移す。文字体系がThaiで、GSUBにThaiの文字体系が無いとき、上下のマークの状態機械でWindowsとMacの私用領域の字形に置きかえる（`thai_pua_shape`）。

- arabic（アラビア文字と、横書きでGSUBに文字体系があるか `Arab` のときのシリア文字）：結合の型（`tables/arabic.rs`。表に無い文字はMn、Me、CfならT、ほかはU）と前後の文脈から、HarfBuzzの状態表で `isol`、`fina`、`fin2`、`fin3`、`medi`、`med2`、`init` のマスクを選び、`SAFE_TO_INSERT_TATWEEL` と `UNSAFE_TO_CONCAT` を立てる。モンゴル文字では、自由な異体字セレクタ（U+180B〜U+180D、U+180F）が前の文字の形を受け継ぐ。機能の段は `stch`（段の後で増えたグリフを伸ばしの部品として記録）、`ccmp` と `locl`、形の機能（1つずつ段を分ける）、`rlig`（アラビア文字ではその後にフォールバック）、`calt`（`rclt` が無ければ段を分ける）、`liga`、`clig`、`mset` の順とする。後処理で、`stch` の部品を、続く語（既定で無視する文字と語の一般カテゴリの文字）の幅を埋めるまで繰り返す（部品は256まで）。マークの並べかえは、UAX #53の変更子の結合のマーク（結合クラス220と230）を先頭へ移し、結合クラスを22と26に付けかえる。
- アラビア文字のフォールバック：文字体系がArabicで、形の機能がフォントに無いとき、計画を作るときに、Unicodeの表示形とcmapから単一の置換4つ（`init`、`medi`、`fina`、`isol`）と合字の置換3つ（`rlig`。3文字の合字と2文字の合字はマークを無視、マークの合字は無視しない）を、GSUBのバイト列として組み立てる。1つも作れなければ、Windows-1256の旧来のフォント（アレフ、ラーム、アレフ・マクスーラ、ヤー、スクーンのグリフが199、225、236、237、250）にHarfBuzzの手書きのlookupを使う。`rlig` の後の段の関数で、空のGDEFで適用する。

- indic（インドの9つの文字体系で、GSUBの文字体系が `DFLT`、`latn`、3版のタグでないとき）：`setup_masks` で `indic_categories` の分類と位置を `shaper_category` と `shaper_position` に置く。機能の段は、音節の区切り（`INDIC_MACHINE`。壊れた音節があれば `HAS_BROKEN_SYLLABLE` を立てる）、`locl` と `ccmp`、最初の並べかえ、基本の機能（`nukt`、`akhn`、`rphf`、`rkrf`、`pref`、`blwf`、`abvf`、`half`、`pstf`、`vatu`、`cjct`。1つずつ段を分ける）、最後の並べかえ、ほかの機能（`init`、`pres`、`abvs`、`blws`、`psts`、`haln`）の順で、すべて音節ごとに適用する。`liga` は切る。文字体系ごとの設定（virama、rephの位置と形、`blwf` を基底の前にも使うか）はHarfBuzzの `indic_configs` と同じとする。GSUBの文字体系のタグの最後が `2` でなければ古い仕様（`deva` など）とし、基底の後の最初のハラントを最後の子音の後へ移し、基底の前の子音に `blwf` を付けない。計画のデータは、計画を作るときに引くviramaのグリフ、機能のマスク、`rphf`、`pref`、`blwf`、`pstf`、`vatu` の段のlookupの番号である。
  - 最初の並べかえ：位置が基底の子音は、viramaとの2通りの並び（virama、子音と、子音、virama）を `blwf`、`vatu`、`pstf`、`pref` のlookupが置きかえるか（`hb_ot_layout_lookup_would_substitute`。新しい仕様ではマラヤーラム文字を除き前後の文脈の無い規則だけを見る）で、下、後ろ、基底の位置に分ける。次に、壊れた音節に点線の円を入れ、音節ごとに基底の子音を後ろから探し（Ra、ハラントが `rphf` に合えばreph。マラヤーラム文字は符号化されたrepha。カンナダ文字はRa、ハラント、ZWJをRa、ZWJ、ハラントとして扱う）、位置で安定に並べかえ、左のマトラの並びを反転し、基底の後で動いたものだけのクラスタをまとめ、reph、半字形、下と後ろの形、`pref` のマスクを付ける。ZWNJは前の `half` を外す。
  - 最後の並べかえ：合字や複数の置換で失われたviramaのハラントの分類を戻し、基底を探しなおし（形を作らなかった `pref` の候補、マラヤーラム文字の形を作らなかった下の形）、前置のマトラを最後の独立したハラントの後へ（タミル文字とマラヤーラム文字では基底の直前へ）、rephを文字体系の位置へ、`pref` の形を基底の前へ移し、語頭の左のマトラに `init` を付ける。
  - 前処理で母音の並びの禁止を行う。正規化は `COMPOSED_DIACRITICS_NO_SHORT_CIRCUIT` で、U+0931、U+09DC、U+09DD、U+0B94は分解せず、マークの後には合成せず、U+09AF U+09BCはU+09DFに合成する。マークの幅は0にせず、フォールバックの配置は無い。
- khmer：`setup_masks` で `indic_categories` の分類を `shaper_category` に置く。機能の段は、音節の区切り（`KHMER_MACHINE`）、並べかえ、`locl` と `ccmp`、基本の機能（`pref`、`blwf`、`abvf`、`pstf`、`cfar`。段を分けずに音節ごとに適用する）、音節の消去、ほかの機能（`pres`、`abvs`、`blws`、`psts`）の順とする。`clig` を有効にし、`liga` を切る。並べかえでは、壊れた音節に点線の円を入れ、子音の音節と壊れた音節で、基底の後の要素に `blwf`、`abvf`、`pstf` のマスクを付け、Coeng、Roを音節の先頭へ移して `pref` を、その後ろの要素に `cfar` を付け（Coengは3つ目まで見る）、前置の母音を先頭へ移す。移すときは、移した範囲のクラスタをまとめる。正規化は `COMPOSED_DIACRITICS_NO_SHORT_CIRCUIT` で、Unicodeの分解の無い分かれた母音（U+17BE、U+17BF、U+17C0、U+17C4、U+17C5）をU+17C1とその母音に分解し、マークの後には合成しない。母音の並びの禁止は無く、マークの幅は0にせず、フォールバックの配置は無い。
- myanmar（GSUBの文字体系が `DFLT`、`latn`、`mymr` でないとき）：`setup_masks` で `indic_categories` の分類を `shaper_category` に置く。機能の段は、音節の区切り（`MYANMAR_MACHINE`）、`locl` と `ccmp`、並べかえ、基本の機能（`rphf`、`pref`、`blwf`、`pstf`。1つずつ段を分け、音節ごとに適用する）、音節の消去、ほかの機能（`pres`、`abvs`、`blws`、`psts`）の順とする。並べかえでは、壊れた音節に点線の円を入れ、子音の音節と壊れた音節で、位置を決めて（kinziのRa、asat、ハラントは基底の後、基底の前の要素とmedial Raは基底の前、前置の母音はその前、下の母音とその後の要素は下と下の後）、クラスタをまとめながら位置で安定に並べかえ、前置の母音の並びを元の順に戻す。正規化は `COMPOSED_DIACRITICS_NO_SHORT_CIRCUIT`、マークの幅はGDEFで早く0にし、フォールバックの配置は無い。
- myanmar_zawgyi（`Qaag`）：機能も並べかえも持たず、正規化は `NONE`、マークの幅は0にせず、フォールバックの配置は無い。
- USE（Universal Shaping Engine。チベット文字、モンゴル文字、シンハラ文字、バリ文字など多くの文字体系で、GSUBの文字体系が `DFLT`、`latn` でないときと、インドの9つの文字体系で3版のタグのとき）：`setup_masks` で、結合する文字体系（`has_arabic_joining`。N'Ko、モンゴル文字、パスパ文字など）ではarabicの結合の型の設定（モンゴル文字の自由な異体字セレクタを含む）を行い、`use_category` の分類を `shaper_category` に置く。機能の段は、音節の区切り、`locl`、`ccmp`、`nukt`、`akhn`、置換の印の消去、`rphf`、置換の印の消去、`pref`、基本の機能（`rkrf`、`abvf`、`blwf`、`half`、`pstf`、`vatu`、`cjct`）、並べかえ、位置の機能（`isol`、`init`、`medi`、`fina`）、ほかの機能（`abvs`、`blws`、`haln`、`pres`、`psts`）の順で、ほかの機能と位置の機能のほかは音節ごととする。
  - 音節の区切り：`USE_MACHINE` は、CGJと、次のCGJでない要素がUnicodeのマークであるZWNJを飛ばして走る（`find_syllables_use`）。壊れた音節があれば `HAS_BROKEN_SYLLABLE` を立てる。音節の始めの3つの要素（符号化されたrepha `R` で始まる音節では1つ）に `rphf` のマスクを付ける。結合しない文字体系では、音節を1つの単位として、前の音節と結合するかで `isol`、`init`、`medi`、`fina` のマスクを付ける（ヒエログリフと音節でないものは結合しない）。
  - `rphf` の段の後で、`rphf` のマスクの続く範囲で置換された最初の要素を `R` に、`pref` の段の後で、置換された最初の要素を `VPre` にする。
  - 並べかえ：壊れた音節に点線の円（分類 `B`、rephaの後）を入れ、ヴィラーマ、サコットで終わる音節、標準、記号、壊れた音節で、rephaを最初の後置の要素（後置の分類か、合字にならなかったハラント）の前か音節の末尾へ、前置の要素（`VPre`、`VMPre`。複数の置換の2つ目以降の成分を除く）を音節の始めか最後のハラントの後へ移し、移した範囲のクラスタをまとめる。
  - 前処理で母音の並びの禁止を行う。正規化は `COMPOSED_DIACRITICS_NO_SHORT_CIRCUIT` で、マークの後には合成しない。マークの幅はGDEFで早く0にし、フォールバックの配置は無い。
- 音節を扱うシェーパーの共通の部分（`shapers/syllabic.rs`、`shapers/vowel_constraints.rs`、`layout/would.rs`）：`insert_dotted_circles` は、指定した型の壊れた音節の先頭（指定した分類のrephaがあればその後）に、フォントの点線の円のグリフを入れる（`hb_syllabic_insert_dotted_circles`）。円は後ろの要素のクラスタ、マスク、音節を受け継ぎ、分類と位置は引数で指定する。`DO_NOT_INSERT_DOTTED_CIRCLE` のとき、壊れた音節が無いとき、フォントに円が無いときは何もしない。母音の並びの禁止（`_hb_preprocess_text_vowel_constraints`）は、`vowel_constraint_match` の並びの最後の文字の前に、その文字を写したU+25CC（書記素の続きではない）を入れる。`lookup_would_substitute` は、HarfBuzzの `would_apply` と同じく、単一、複数、代替、逆向きの連鎖の置換では1グリフがCoverageにあるか、合字では成分がちょうど合うか、文脈と連鎖の文脈では入力の長さと値が合うか（文脈の無い規則だけを見る指定では、前後の文脈を持つ規則は合わない）を、lookupのダイジェストで最初のグリフを確かめてから答え、たどった数をバッファの操作の数の上限から引く。

### 音節の状態機械

- indic、khmer、myanmar、USEのシェーパーが文字の分類の列を音節に区切る状態機械は、`tools/gen-machine/`（ワークスペースの外のRustのパッケージ、外部のクレートに依存しない）が `tools/gen-machine/grammars/<名前>.machine` から `crates/sofel-shape/src/tables/machine_<名前>.rs` に書き、コミットする（[ADR 0024](adr/0024-complex-shapers.md)）。Ragelとその生成したコードは使わない。
- 文法のファイルは、HarfBuzz 15.1.0の `src/hb-ot-shaper-<名前>-machine.rl` の分類の番号（`export`）と規則を写し、動作を音節の型の番号に置きかえたもの。文法は、和（`|`）、連接（`.` か並べる）、`?`、`*`、`+`、回数（`{n}`、`{n,}`、`{,m}`、`{n,m}`）、括弧、名前の定義、`any`、最長一致の走査子（`main := |* ... => 番号; ... *|;`）を読む。
- 道具は、規則の和のNFA（Thompsonの構成）から部分集合の構成でDFAを作り、Mooreの分割の細分で最小化する。入力は、規則が名指す分類の値ごとの組と、それ以外の値の組の、等しい値の組に分ける。状態0は行き止まり、状態1は始まりとし、ほかは始まりからの幅優先の順に番号を振る。書き出すまえに、長さ3までのすべての入力で、最小化したDFAとNFAの受理する規則が一致することを確かめる。
- 実行（`crates/sofel-shape/src/machine.rs`）は、表を引くだけの手書きの関数で、Ragelの走査子と同じく、いまの位置から規則のどれかに合う最長の空でない列を取り、同じ長さなら先に書いた規則を選ぶ。音節の値は、HarfBuzzと同じく、番号（1〜15で巡回）を上位4bit、型を下位4bitとする。USEは、HarfBuzzの `find_syllables_use` と同じく、CGJと、次のCGJでない文字がUnicodeのマークであるZWNJを飛ばして走らせ、音節は最初の残った文字から次の音節の最初の残った文字の手前まで（最後の音節はバッファの終わりまで）を覆う。

### 手書きのUnicodeの関数

- `sofel_shape::unicode` に、HarfBuzzが手で書いた `modified_combining_class`、`is_default_ignorable`、`space_fallback_type`、`vertical_char_for`、`is_variation_selector`、`is_regional_indicator` を置く（[ADR 0019](adr/0019-unicode-character-data.md)、[ADR 0020](adr/0020-shaping-foundation.md)）。全文字の値を `hb-dump --unicode` と比べる。
`sofel-render` は設計座標と名前つきインスタンスを受け付ける。FreeTypeで既定以外のインスタンスになるときは埋め込みビットマップを省き、指定位置の輪郭を描く（[ADR 0027](adr/0027-variable-fonts.md)）。

## ベクトル出力

`sofel-vector` の `SvgPage` と `PdfPage` は、呼び出し側が配置した輪郭を指定色で入力順に塗る。SVG は y 軸を反転したグループにパスを書き、PDF は二次曲線を三次曲線に変換してページに書く。`SvgPage::draw_image` はグリフの配置先に PNG を埋め込み、指定された変換を適用する。矩形クリップは親の座標系で指定する。[ADR 0074](adr/0074-svg-feature-conformance.md) の出力検査では、空ページ、単独のグリフ、半透明のグリフが重なるページ、変換とクリップを指定した画像を固定版 HarfBuzz の SVG と照合する。C の `sofel_svg_page_layers` は各要素の変換を使い、輪郭と PNG を配列順に配置する。`sofel_vector_page` にはフォント、軸座標と、グリフごとの変換・色を渡す（[ADR 0068](adr/0068-vector-and-gpu-output.md)）。

## カラーグリフ（`sofel-color`、`sofel-bitmap`、`Painter`）

[ADR 0023](adr/0023-color-glyphs.md) の5a段階（`Painter`、CPAL、COLRv0、CBDTとsbixのデータ、SVGの文書、グリフの箱、フォールバックの問い合わせ）、5b段階（COLRv1）、5c段階（EBDTとビットマップだけのフォント）、5d段階（COLRv0の画像）を実装している。

- `sofel-geom` の `Painter` は、HarfBuzz 15.1.0の `hb_paint_funcs_t` と同じ順で同じ値の命令を受ける。単色と3種類のグラデーションは `fill(&Brush)` にまとめ、グループの合成の方法は `push_group` と `pop_group` の両方で渡す。単位はフォントの単位でyは上に増え、HarfBuzzの根の拡大の変換と、`PaintGlyph` と `PaintColrGlyph` の周りの逆の変換は出さない。数は `f32` で、HarfBuzzの `float` の計算と同じ順で同じ演算をし、FMAを使わない。
- `Image` の `extents` は `Option` とする。HarfBuzzはSVGの文書を範囲なし（NULL）で渡すためで、ADRの型（`GlyphExtents`）から変えた。
- [ADR 0063](adr/0063-font-functions-and-synthesis.md) の `Image::slant` は、HarfBuzzの画像callbackと同じく0とする。`paint_recording_at_with_slant` は画像を含む描画命令の外側に合成斜体の変換を置く。
- 三角関数（`sin_cos`、`sin`、`cos`、`tan`）はmusl 1.2.6の `sincosf` と `tanf` の移植で、`f64` の四則演算だけで計算する。|x| ≥ 2^28·π/2 の引数の剰余は、muslの `__rem_pio2_large` の代わりに、muslの2/πのビットの表から128bitの整数で求める。テストで、100万個の引数で `f64` の値を丸めたものとの差が1ulp以下であること、三角関数と変換の100万個の結果のハッシュがx86-64、aarch64、wasm32で同じことを確かめる。
- 補助の関数 `normalize_color_line`（安定な挿入の並べかえ）、`reduce_linear_anchors`、`sweep_gradient_tiles` はHarfBuzzの `hb-paint.cc` の移植である。
- `sofel-read` に、CPAL（版0と1）、COLR（版0の記録、版1のBaseGlyphList、LayerList、ClipList、塗りの形式1〜33）、CBLC/EBLC/`bloc`（索引の形式1〜5）、CBDT/EBDT/`bdat`、sbix、SVGのビューと検証器を置く。色の線は、停止点がすべて表に収まらなければ停止点なしとして読む（FreeTypeの `FT_Get_Colorline_Stops` と同じ）。`Colr::v1_fits` は、版1の塗りの木が表に収まるかを、HarfBuzzの検査（入れ子64、1バイトあたり64回の作業）と同じ範囲で確かめる。
- `sofel-color`：
  - パレット（`palette_count`、`palette_name_id`、`palette_flags`、`palette_color_name_id`、`palette_colors`、`palette_entry_count`、`find_palette`）はHarfBuzzの読み方による。範囲外の名前は0xFFFF、フラグは0。
  - `paint` は、HarfBuzzの `hb_ot_paint_glyph_or_fail` と同じくCOLR、SVG、CBDT、sbixの順に試す。COLRv1は、`v1_fits` が偽の表では使わない（HarfBuzzは版1のリストを捨てる）。結果は `Scratch` に表ごとに覚える。COLRv1の打ち切り（深さ64、辺の数16384、計算の量2^24で停止点1個に16）、循環の検出（`hb_decycler_t` のtortoise-hareと同じく、深さの半分の位置の値と比べる）、クリップの箱の無い木の有界の判定（`hb_paint_bounded`）はHarfBuzzと同じ。`paint`・`paint_recording` は `Instance` または既定位置を表すフォント参照を受け取る。箱の問い合わせも輪郭と同じ位置でクリップ矩形または塗りを評価する。`paint_recording_at` とC ABIの `sofel_paint_glyph` は、指定位置でCOLRのItemVariationStoreを評価して塗り、色の停止点、変形、クリップ矩形に差分を適用する。クリップ矩形の最小辺は差分を下に、最大辺は上に丸める（[ADR 0027](adr/0027-variable-fonts.md)）。色は `Palette::Font(i)`（範囲外は0番）か `Palette::Custom`（範囲外の番号は透明の黒）から引き、アルファはHarfBuzzの `roundf`（[ADR 0026](adr/0026-harfbuzz-rounding.md)）で丸める。
  - `Recording` は命令を値の列にし、`replay` で流しなおす。画像は `paint_recording` で記録する。gzip 圧縮された SVG は展開して `Svg { gzip: false }` として渡し、記録には展開後のバイト列を保持する（[ADR 0066](adr/0066-color-glyph-pixels.md)）。それ以外の画像はフォントから借りる。
  - `glyph_extents` と `scaled_glyph_extents` は `hb_ot_get_glyph_extents` と同じくsbix、CBDT（最大のストライク）、COLR、輪郭（`Outlines`）の順に探す。COLRはクリップの箱か、`hb_paint_extents` と同じ計算の塗りの範囲（層のグリフは読み込んだ輪郭の制御点の箱）とする。`Outlines` と `Scratch` を受け取る（ADRの `(font, glyph)` から、輪郭のアクセラレータと作業領域を呼び出し側が持つ形に変えた）。拡大縮小した箱は、根に拡大の変換を置いて求める。`HbScale` に、この箱を拡大縮小せずに使う `v_origin_scaled`、`metrics_position_with_fallback_scaled`、`baseline_with_fallback_scaled` を足した。
  - `color_formats`、`glyph_color_format`（塗りの木をたどらない。`v1_fits` も見ない）、`layers`、`svg_document` を持つ。
  - 生の塗りの木（`base_paint`、`PaintRef`、`clip_box`、`layer_paint`）は、FreeTypeの `FT_Get_Color_Glyph_Paint` などと同じ値を返す。基本のグリフの探索はFreeTypeの二分探索による。
- `sofel-bitmap`：`strikes` はFreeTypeの `available_sizes`（CBLC、EBLC、`bloc`、sbixの順に最初の表、ppemが0のストライクは除く）、`match_strike` は `FT_Match_Size`、`choose_strike` はHarfBuzzの選び方（`max(x, y)` が要求以上で最小、無ければ最大、0は最大）、`glyph_bitmap` はFreeTypeの読み込み（sbixの `dupe` と `flip` を4回まで）で画像のデータとメトリクスを返す。PNGを塗るための `cbdt_png`、`sbix_png` とその箱はHarfBuzzの読み方（CBDTは索引の形式1と3、画像の形式17と18、sbixは `dupe` を8回まで）による。
- `Instance::ft_control_values` と C の `sofel_font_ft_control_values` は、指定した軸位置の `cvar` 差分を `cvt ` 表に適用し、ピクセルサイズで拡大縮小する前の制御値を 26.6 形式で返す（[ADR 0064](adr/0064-freetype-hinting-and-render-modes.md)）。
- `Rasterizer::render_harmony` と C の `sofel_rasterizer_render_harmony` は、記録した輪郭を FreeType Harmony の既定 RGB 配置で描く。水平 LCD の幅と垂直 LCD の行数はサブピクセル単位で、カバレッジは上の行からパディングなしで返す（[ADR 0064](adr/0064-freetype-hinting-and-render-modes.md)）。
- `sofel-bitmap` の `bitmap_layout` と `draw_bitmap` は、EBDT、CBDT、`bdat` の画像の形式1、2、5、6、7と成分の形式8、9を、1、2、4、8、32bitの深さで、FreeTypeの `tt_sbit_decoder_load_image`（索引の形式1〜5、形式2と7のバイト単位とビット単位の判定、成分のメトリクスの戻し方）と同じ画素とメトリクスにして、呼び出し側の領域（`pitch × rows` バイト、上の行から）に書く。32bitは `FT_LOAD_COLOR` と同じくBGRAのまま書く。CBDT と sbix の PNG はアルファ乗算済み BGRA に展開する。sbix の flip は各行を左右に反転し、単純輪郭を持つグリフでは FreeType の水平方向ロードに合わせて左サイドベアリングと輪郭の下端で原点を補正する（[ADR 0066](adr/0066-color-glyph-pixels.md)）。
- [ADR 0063](adr/0063-font-functions-and-synthesis.md) の `convert_bitmap` はFreeTypeの `FT_Bitmap_Convert` と同じくMONO、GRAY2、GRAY4、GRAY、LCD、LCD_V、BGRAを8bitの行へ変え、行の整列と負のpitchを保つ。`embolden_bitmap` はFreeTypeの `FT_Bitmap_Embolden` と同じく画素を太らせ、GRAY2とGRAY4を先に8bitへ変え、BGRAは変更しない。`embolden_bitmap_slot` は太字にした画像とともに、FreeTypeの `FT_GlyphSlot_Embolden` と同じスロットの送り、幅、高さ、上端を返す。
- `sofel-font` の `Font::new` は、FreeTypeと同じく、輪郭（glyf、GLYF、CFF、CFF2）が無くsbixも無いフォントでは、Appleの `bhed` を `head` の代わりに読む。`Font::has_outlines`（CBLCかCBDTがあれば偽。FreeTypeは輪郭を無視する）と `Font::has_bitmaps`（CBLC、EBLC、`bloc`、sbix）を持つ。`bhed` は `head` の検証器で検証する。Appleのビットマップだけのフォントでは、FreeTypeがOS/2を読まないため、ストライクの `width` は0になる。
- `sofel-render` は、`--ppem` に等しいストライク（`FT_Match_Size`）があり、グリフのビットマップがあれば、`--no-bitmap` が無いかぎりビットマップを書く。白黒と灰色はPGM（階調を0〜255に広げて反転）、32bitはPAM（`P7`、`TUPLTYPE RGB_ALPHA`、乗算済みでない値に直す）とし、位置は同じ注釈の行に書く。
- 検査：テストのデータにEBLCや `bloc` のフォントが無いため、画像の形式1、2、5、6、7、8、9、深さ1、2、4、8、32、索引の形式1〜5の組み合わせを、EBLC/EBDTと `bloc`/`bdat`/`bhed` で組み立てた110個のフォントと、glyfの輪郭とCBLC/CBDTを両方持つフォントを、`ft-dump --sbit`（`FT_Select_Size` と `FT_LOAD_COLOR` の画素とメトリクス）と比べ、すべて一致する。
- `sofel-raster` の `blend_layer` は、8bitの被覆率の画像を色で重ねて乗算済みのBGRAの画像（`BgraImage`）に足す。FreeTypeの `tt_face_colr_blend_layer` と同じく、最初の画素のある層で画像の箱を決め（画素の無い層では箱を決めず、次の層が決めなおす）、以後の層で箱を広げて写しなおし、画素ごとに整数で重ねる（`fa = alpha × coverage / 255`、`dst × (255 − fa) / 255 + color × fa / 255`）。
- `sofel_cli::glyph::render_color` は、FreeTypeの `FT_LOAD_COLOR | FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP` と `FT_RENDER_MODE_NORMAL` の描画と同じく、グリフ自身の輪郭を読んでから、COLRv0の層をそれぞれ `glyph::render` で描いて `blend_layer` で重ねる。層は `color_layers`（FreeTypeの `FT_Get_Color_Glyph_Layer` と同じく、基本のグリフの記録が層の記録を越えれば無し、グリフの番号がグリフの数以上か、パレットの番号が前景色（0xFFFF）以外でパレットの項目の数以上の層で打ち切り）による。色は `PaintOptions` のパレットと前景色から取る。層が無いか、層のどれかが描けなければ、グリフ自身の輪郭を灰色で描く（`ColorGlyph::Gray`）。
- `sofel-render --color [--palette N]` は、COLRv0、輪郭と矩形でクリップした COLRv1 の線形・放射グラデーションと、`rect`、`circle`、`path` と線形・放射グラデーションからなる OT-SVG のグリフをPAM（乗算済みでない値に直す）で書く。OT-SVG では対象グリフの要素を選び、`defs` の停止点、`stop-opacity`、図形の境界ボックス、`gradientTransform`、グループの `transform` を適用し、図形の順に重ねる（[ADR 0072](adr/0072-color-gallery-acceptance.md)）。前景色は、パレットに暗い背景向けの印があれば白、無ければ黒とする。
- OT-SVG のパスは絶対座標と相対座標の直線、2次・3次ベジェ曲線、楕円弧を描く。楕円弧は最大90度ずつ3次ベジェ曲線で近似する。グループと図形には平行移動、拡大縮小、中心を指定した回転、せん断と行列の変換を適用する（[ADR 0066](adr/0066-color-glyph-pixels.md)）。
- 検査（5d）：`ft-dump --raster PPEM --color` で、HarfBuzzのテストのカラーのフォントのCOLRv0のグリフを、12、24、48画素、パレット0と最後のパレットでFreeTypeと比べる（1818項目）。前景色はFreeTypeと同じく、FreeTypeが16bitずつ読んだ `paletteTypes` の暗い背景の印で決める。期待する失敗は、FreeTypeが読み込まないCOLRを持つ2つのフォントの12項目だけで、ほかは画素まで完全に一致する。
- `sofel-paint` は塗りの命令をHarfBuzzの `test/api/test-paint.c` の字下げのテキストで書く。
- C ABIは `sofel_paint_glyph`（命令の配列。`sofel_paint_commands_free` で解放）、パレット、形式、層、SVGの文書、ストライク、グリフのビットマップ、補助の関数、三角関数、COLRv0の画像（`sofel_render_color_glyph`、`sofel_blend_layer`）を持つ。`sofel_font_glyph_extents` などグリフの箱を使う関数は `sofel_color` の箱を使う。
- 検査：HarfBuzzの `test/api/results-paint/` の24個（残りの2個はテストの実行の記録で塗りのテキストではない）と一致し、`hb-dump --paint` もこれを再現する。HarfBuzzのテストのカラーのフォント92個で `hb-dump --paint`、`--color`、`--extents` の7万8225項目を比べ、期待する失敗は22項目（理由は一覧）である。`hb-dump --metrics` の箱、縦書きの原点、代わりの基準線もカラーグリフで一致する。FreeTypeとは `ft-dump --colr` の生の塗りの木とCPAL、`--sbit` のストライクの一覧を比べる（2184項目、期待する失敗17項目。残る差分の理由は `conformance/expected-failures-freetype.txt` に記録する）。`FtPaintGraph` は指定位置でFreeTypeの整数差分を計算する。塗りの座標と角度には16.16、色のアルファには2.14、クリップ矩形にはフォント単位を使う。`RawPaintGraph` はFreeTypeの読み込み時に対応する変化量ストアと索引表の検査を行う。データ表数0のストアや、リージョン総数を超える列数を持つストアでは、COLRの生の塗りとクリップ矩形を返さない。FreeTypeの試験の版はlibpngを持たないため、PNGのグリフのメトリクスは比べない。

## ラスタライズ（`sofel-raster`）

- 8bitのアンチエイリアスの画像を、FreeTypeのsmoothのラスタライザと同じ数値の計算で作る（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。セルは帯ごとの密な配列（上限64KiB）に積む。重なりの印のある輪郭は4×4の副標本で描く。FreeTypeとの差は0（完全一致）とする。
- 掃き出しは、帯の行ごとに触れた列の範囲だけを走査し、被覆率は触れたセルでだけ求めて次のセルまで使う。走査したセルは0に戻し、帯の配列を描画のたびに全体を0にはしない（[ADR 0014](adr/0014-raster-sweep-touched-cells.md)）。
- 重なりのときは、画素ごとに16個の副標本の `(coverage + 8) / 16` を16bitで合計し、最後に `min(合計, 255)` とする。合計は256以下なので、FreeTypeの標本ごとの `sum - (sum >> 8)` と同じ値になる（[ADR 0014](adr/0014-raster-sweep-touched-cells.md)）。
- FreeTypeは、輪郭をビットマップの原点へ動かしてから（重なりの印があればさらに4倍してから）命令に変換する。暗黙の曲線上の点（中点の0への切り捨て）はこの順序で決まるので、完全に一致させるには、`sofel_outline::load` で点を読み、`sofel_raster::pixel_box` で箱を求め、`Scratch::decompose` に `PixelBox::shift()` と倍率を渡して命令に変換し、`Rasterizer::render_placed` で描く。この手順は `sofel_cli::glyph::render` にまとめてある。`Rasterizer::render` は、記録した命令をあとから動かすので、中点のある輪郭では1画素の値が1〜2ずれることがある。
- HarfBuzz 15.1.0のテストのフォントで、FreeTypeが開けるglyf、CFF、CFF2のフェイスを12、24、48画素で比べ、期待する失敗は無い。CFFのグリフは重なりの印を持たず、非ゼロ規則で描く（FreeTypeの `REVERSE_FILL` と `HIGH_PRECISION` はsmoothのラスタライザが見ない）。

## サブセット化（`sofel-write`、`sofel-subset`）

[ADR 0022](adr/0022-subsetting.md) のとおり、出力はHarfBuzz 15.1.0の `hb-subset` とバイト単位で一致させる。実装は段階ごとに進め、まだ書けない表や設定は `SubsetError::Unsupported` とする（黙って捨てたり、違う形で書いたりしない）。

- `sofel-write` の `build_sfnt` は、HarfBuzzの面の組み立て（`hb-face-builder.cc`）と同じく、表のデータを長さの昇順（同じ長さはタグの昇順）に4バイトに揃えて並べ、表の記録をタグの順に書き、チェックサムと `checkSumAdjustment` を求める。sfntの版は `CFF ` か `CFF2` があれば `OTTO` とする。
- `sofel-write` の `Serializer` は、HarfBuzzの `hb_serialize_context_t` と同じ規則でオブジェクトのグラフを組む。子を先に詰め、後に詰めたものほど前に置き、同じバイト列とリンクのオブジェクトをまとめる（統合の表は `BTreeMap`）。オフセットのあふれは `Packed::resolve` が報告する。
- `sofel-write` の再配置（`Packed::repack`、`serialize_objects`）は、HarfBuzzの `hb-repacker.hh` と `src/graph/` を写したものである。最短距離による位相の整列（同点の扱いもHarfBuzzの二分ヒープと同じ）、32bitと24bitのオフセットの下の部分グラフの空間への分離、共有された子の複製、優先度の引き上げを、HarfBuzzと同じ繰り返しの上限（1回の呼び出しで最大の回数、全体で500回）で行う。GSUBとGPOSでは、PairPos（形式1と2）、MarkBasePos、LigatureSubstの部分表の分割と拡張のlookupへの昇格も行う。頂点の親や孤立させる部分グラフは、HarfBuzzの `hb_hashmap_t` と同じハッシュ（種を持たない乗算のハッシュ）と開番地法の表に入れ、その並びの順で辿る。`Plan::execute` は、表の直列化でオフセットがあふれたとき、`_hb_subset_repack` と同じく既定の設定（32回、拡張への昇格は解けないときだけ）で再配置する。解けなければ `SubsetError::OffsetOverflow` とする。`serialize_objects` は `hb_subset_serialize_or_fail` と同じく、呼び出し側が作ったオブジェクトとリンクの列（根を最後に置く）を20回、拡張への昇格ありで再配置して書く。HarfBuzzの `src/test-repacker.cc` と `src/graph/test-classdef-graph.cc` のテストを同じグラフと期待の値で写し、`hb-dump --repack` との突き合わせで出力がバイト単位で一致することを確かめる。
- `sofel-subset` の `Input` は `hb_subset_input_t` に相当する不変の値で、集合（文字、グリフ、名前ID、名前の言語、レイアウトのフィーチャ、文字体系、捨てる表、写す表）は反転した集合を表せる `RangeSet`、フラグはHarfBuzzと同じビットの `Flags` とする。`Input::new()` はHarfBuzzの既定の値、`keep_everything()` は `hb_subset_input_keep_everything` と同じ値を持つ。`to_args()` は `hb_subset_input_to_string_or_fail` と同じオプションの列を返す。
- `Plan::new` は `hb-subset-plan.cc` の順に、鏡像の文字の追加、cmap（DMAPを先に引く）からの文字とグリフの収集（2つの集め方）、異体字シーケンスの閉包、MATHの閉包（MATHを捨てないとき）、レイアウトの閉包、COLRの閉包、名前IDの閉包、glyfの合成グリフの閉包（HarfBuzzの操作とバイトの上限）、グリフの対応（既定、`RETAIN_GIDS`、`RETAIN_NUM_GLYPHS`、対応の指定）を求める。
- 前処理（`Prepared::new`）は、HarfBuzzの `hb_subset_preprocess` が面に付ける `hb_subset_accelerator_t` の中身を、元のフォントから直接作る。`keep_everything` の計画が集める文字とグリフの対応とその逆引き、文字の集合（異体字セレクタを含む）、`seac` を持つグリフの有無、CFFの全グリフの解析済みのcharstringとサブルーチン（グリフの順に解析し、ヒントの印を付け、字句をまとめたもの）、cmapの残す部分表ごとの文字の集合である。HarfBuzzは `keep_everything` でサブセット化した面に付けて後のサブセット化に使うが、sofelは元のフォントを使い、作り直した面は持たない。値は不変で、`Arc` で共有し、スレッドの間で使える。`Plan::with_prepared` は、HarfBuzzの `plan->accelerator` の経路（文字の集め方3つのうち逆引きを使う経路、`seac` の無いフォントでの閉包の省略、CFFの解析済みの字句の利用とそこからのサブルーチンの閉包、cmapの部分表の文字の集合の再利用）で計画を作り、`Plan::new` と同じバイト列を返す。グリフの数とcmap、DMAP、`CFF ` の表の長さが前処理のときと違うフォントには `SubsetError::InvalidInput` を返す。適合テストは全組を前処理の有無で2回走らせ（識別子の末尾の `+prepared`）、`hb-dump --subset --preprocess` とも突き合わせる。HarfBuzz自身が前処理の有無で違う出力を返すのは、壊れたGSUBを前処理が書き直す `bad-gsub.otf` だけで、sofelはどちらも前処理の無いときの出力を返す。
- `cff_charstrings_index` と `cff_charstring_data` は、HarfBuzzの `hb_subset_cff_get_charstrings_index` と `hb_subset_cff_get_charstring_data` と同じく、`CFF ` の表のCharStrings INDEXと各グリフのcharstringのバイト列を写さずに返す。HarfBuzzが表を読めないとき、グリフIDが数以上のとき、空のcharstringには `None` を返す。
- レイアウトの閉包は `hb-subset-plan-layout.cc` のとおり、GSUBとGPOSのそれぞれで、残すフィーチャと文字体系から言語系をたどってフィーチャとlookupを集め（FeatureVariationsの置きかえのフィーチャのlookupも足す）、GSUBではそのlookupで置換の閉包（最大 `HB_CLOSURE_MAX_STAGES` 段）を求め、文脈のlookupからたどれるlookupを足して、どのグリフにも当たらないlookupを除く。そのあと、lookupの無くなったフィーチャを除き、同じタグで同じlookupと引数を持つフィーチャをまとめ、既定の言語系と同じ言語系を除く。上限（`HB_MAX_NESTING_LEVEL`、`HB_MAX_LOOKUP_VISIT_COUNT`、`HB_MAX_SCRIPTS` など）は `src/hb-limits.hh` と同じ値を持つ。名前IDの閉包は、STAT（捨てないとき）とfvar（軸をすべて固定するときを除く）の名前ID、CPAL（捨てないとき）の版1のパレットのラベルと残すパレットの項目のラベル、残すフィーチャのFeatureParams（`size`、`ssXX`、`cvXX`）の名前IDを足す。GDEFのマークのグリフの集合は、閉包の後のグリフに当たるものだけを残して番号を振りなおす。
- `Plan::execute` は、元のフォントの表をタグの順に処理する。書く表は、glyf、loca、head（glyfが書く）、GLYF、LOCA、hmtx、hhea、vmtx、vhea、HMTX、HHEA、VMTX、VHEA、maxp、MAXP、OS/2、post、name、cmap、DMAP、hdmx、GDEF、GSUB、GPOS、BASE、COLR、CPAL、CBLC、CBDT（CBLCが書く）、sbix、MATH。cvt 、fpgm、prep、gasp、VDMXと、軸を指定しないときのfvar、avar、cvar、MVAR、STATはそのまま写す。HarfBuzzが既定で捨てる表と扱いを持たない表は捨てる（`PASSTHROUGH_UNRECOGNIZED` では写す）。表の処理は `_dependencies_satisfied` のとおりに遅らせる（インスタンス化ではhmtx、vmtx、maxp、OS/2などがglyfを待つ）。
- GDEF、GSUB、GPOS、BASEは、HarfBuzzの各表の `subset` と同じ値を同じ順で書く。CoverageとClassDefの形式は大きさの比較で選び（ClassDefは同じ大きさなら形式1）、ClassDefのクラスはHarfBuzzと同じく詰めなおす（すべてのグリフがクラスを持つときは最小のクラスを0とする）。値の記録とアンカーのDeviceは、ヒンティングのDeviceを写す。変化量の番号のDeviceは、残すSinglePos形式1・2とPairPos形式1・2のものだけ番号を再配置して写す。`NO_HINTING` ではヒンティングのDeviceと0の値を除き、SinglePosのVariationIndexも除く。PairPosのVariationIndexは保ち、アンカーの形式2を形式1にする。
- CFF（名前のキーとCIDのキー）は、HarfBuzzの `hb-subset-cff1.cc` と `hb-subset-cff-common.{hh,cc}` のとおりに書く。charstringは `sofel-subset` が持つ解釈器（HarfBuzzの解釈器から図形の計算を除いたもの）で走査する。既定では、残すグリフから到達するサブルーチンだけを残して番号を詰め、偏りを数から決めなおして符号化しなおす。`DESUBROUTINIZE` ではサブルーチンを展開し、数を最短の形で符号化しなおす。`NO_HINTING` ではヒントの命令と数、Private DICTのヒントの値を除く。`RETAIN_GIDS` の空きのグリフは `endchar` だけとする。名前のキーのフォントは使うSIDを振りなおしてcharsetとEncodingを作りなおし、CIDのキーのフォントは使うFont DICTだけを残してFDSelect（形式3）を作りなおす。charsetとEncodingの形式は大きさの比較で選ぶ。`CFF_IDENTITY_CHARSET` ではCIDを新しいグリフIDとし、`IFTB_REQUIREMENTS` ではCharStrings INDEXのオフセットの幅を4とする。表はHarfBuzzの直列化と同じオブジェクトの順で組む。出力から取り出した `CFF ` の表は、それだけで検証器が `Error` を出さない（適合テストの `:validate` で確かめる）。
- CFF2は、すべての軸を固定したとき（既定の値への固定）に書き、HarfBuzzの `hb-subset-cff2.cc` の完全なインスタンスの経路のとおりに扱う。charstringは `sofel-subset` が持つCFF2の解釈器で2回走査する。1回目は座標を与えずに走らせ、残す演算子が `blend` の結果を受け取り、`flex1` を使わないグリフを選ぶ。2回目は正規化した座標で `blend` を評価してサブルーチンを展開する。1回目で選んだグリフは、絶対の位置を `floor (x + 0.5)` で丸めてから相対の値に戻し（`hb-subset-cff2-round.hh`）、ほかのグリフは `blend` の結果をCの `round` で丸める。Private DICTは `blend` を評価して整数に丸め、`vsindex` と `Subrs` を除き、実数はfontToolsと同じBCD（`%.8G` から作る）で書きなおす。Top DICTから `vstore` を除き、VariationStoreとサブルーチンは書かない。グリフの箱、送り幅、サイドベアリング、headの箱は、HarfBuzzと同じくCFF2の輪郭から求めなおす（`update_instance_metrics_map_from_cff2`。縦の原点はVORG、なければ箱から求める）。HVARとVVARの差分は、既定の位置では0として扱う。
- 軸を固定しない CFF2 に `DESUBROUTINIZE` を指定したとき、または残すグリフがサブルーチンを呼ばないときは、残すグリフの charstring にサブルーチンを展開し、`blend` と `vsindex` を保持する。Private DICT から Subrs の記録を除き、VariationStore を保持する。指定がなく、残すグリフが即値の番号で global/local のサブルーチンを呼ぶ場合は、解釈器が通った call の位置から到達するサブルーチンだけを残して番号と偏りを計算しなおし、Private DICT の Subrs と INDEX を書く。ヒントを保持するときは、`hintmask` と `cntrmask` のデータバイトを call として扱わず、サブルーチン内で増えた stem もマスク長に数える。`NO_HINTING` では、使用する glyph と subroutine のヒント命令、マスク、同じ charstring 内で連続した数値の引数、明示的な `return` と `endchar` を除く。引数が別の charstring から渡る場合、ヒントより前に描画命令または同じ charstring 内の call がある場合、`blend`・`vsindex`・`dotsection` を含む場合、使うサブルーチンが除去後に空になる場合、同じサブルーチンのマスク長が呼び出しごとに異なる場合は未対応である。グリフ集合と Font DICT は閉包と再配置の結果を使う（[ADR 0022](adr/0022-subsetting.md) の10段階目）。
- `DOWNGRADE_CFF2` では、固定したCFF2をCIDキーのCFF（`hb-subset-cff2-to-cff1.hh`）に変換して `CFF ` の表とし、CFF2の表は書かない。名前は `CFF1Font`、ROSは `Adobe-Identity-0`、charsetは形式2の恒等の対応とし、FDSelectは元のものから作りなおすか、無ければ形式3の1つの範囲とする。FontBBoxはheadの箱を使う。charstringは、命令の列を一般の形に戻してから `hmoveto`、`vmoveto`、`hlineto`、`vlineto` に特殊化し、スタックの上限48の中でまとめなおし（`hb-cff-specializer.hh`）、`defaultWidthX` と `nominalWidthX` を送り幅から選んで（`hb-cff-width-optimizer.hh`）幅を前に置き、`endchar` で終える。
- VORGは、残すグリフの記録を元の順で残し、新しいグリフIDを書く。
- HarfBuzzは、すべての表を1つの直列化の作業領域（`hb_subset_plan_execute_or_fail` の `buf`）に書く。表ごとに作業領域を大きさの見積もり（`_hb_subset_estimate_table_size`）まで広げ、足りなければ倍にして書きなおし（`allocated * 2 + 16`）、元の表の長さの256倍を超えるところで失敗する。sofelの `Serializer` は必要なだけ伸びるので、表を書くあいだに保持した最大のバイト数（`Serializer::peak_len`）と、表から表へ引き継ぐ作業領域の大きさ（`hb_vector_t::alloc` と同じ伸ばし方）から、HarfBuzzが書けたかを求め、書けなければ `SubsetError::Limit` とする。例えば、形式13のcmapを持つ `AdobeBlank-Regular.ttf` で多くのグリフを残すと、形式12のcmapが元の256倍を超え、HarfBuzzと同じく失敗する。
- MATHは、HarfBuzzの `hb-ot-math-table.hh` の `closure_glyphs` と `subset` のとおりに扱う。閉包は、cmapの閉包の後のグリフの集合が持つ縦と横の構成（MathGlyphConstruction）の変種と部品のグリフを1回だけ足す（足したグリフの構成はたどらない）。書くときは、MathConstantsを写し、MathGlyphInfoの各表はMATHの閉包の後の集合（`_glyphset_mathed`）のグリフの記録を残し、MathVariantsはHarfBuzzと同じくcmapの閉包の後の集合（`_glyphset_cmaped`）のグリフの構成だけを残す。ヒンティング用のDeviceを写す。軸を固定せずGDEFの変化量ストアをサブセット化するときは、残したMATHの値が参照するVariationIndexも詰め直す。
- COLRの閉包は `_colr_closure` のとおり、版0の基底グリフの層のグリフを足し、版1の塗りを `HB_MAX_NESTING_LEVEL` の深さまでたどって、PaintGlyphのグリフ、PaintColrGlyphの基底グリフ、PaintColrLayersの層、色の番号を集める。その後、残すグリフの版0の層の色の番号を足す。層は集めた順に0から、色の番号は昇順に0から振りなおす（前景色の0xFFFFはそのまま）。
- COLRは `COLR::subset` のとおりに書く。版0の基底グリフの記録を新しいグリフIDの順に並べ、層をグリフごとに複製して振りなおす。版1の塗りを持つグリフが残らなければ版0として書く。版1では、BaseGlyphList、LayerList（残す層）、ClipList（新しいグリフIDの連続する範囲で同じクリップの箱を持つものをまとめる）を書き、塗りは各形式の静的な大きさを写して、グリフID、層、色の番号を書きかえる。変化量の表がない場合、変化量の番号は `NO_VARIATION` だけを残せる（それ以外の塗りは書けず、含む部分を除く）。版0に落とすときは変化量の表を読み取り、ItemVariationStoreとDeltaSetIndexMapを取り除く。色のグリフを1つも残さない場合も、空の版0の表を書く。版1のままでも、閉包が残す基底グリフの塗りをすべてたどり、塗りとクリップがどれも静的であれば、この2つの表を取り除く。軸を固定しない場合、塗り、勾配の停止点、変換、クリップの変化量インデックスがすべて `NO_VARIATION` なら、値は静的であり、この番号を出力に保つ。軸を固定せずに塗りとクリップがItemVariationStoreを参照するときは、使う行と領域だけを残し、参照番号を振り直す。DeltaSetIndexMapを通して参照する場合は、使うマップ項目を元の番号順に詰め、末尾で同じ行に対応する項目を1つにまとめる。全軸を既定位置に固定すると、可変の塗り、勾配の停止点、変換、クリップの箱は既定値を持つ静的な形式で書く。この場合は変化量の番号と両方の表を取り除く。それ以外でItemVariationStoreかDeltaSetIndexMapを持つCOLRは `Unsupported` とする。
- CPALは、COLRが使う色の番号だけを残し、残す色が同じパレットは同じ色の記録を指す。版1では、パレットのフラグとラベルを写し、色のラベルは残す番号のものを残す。
- CBLCとCBDTは、残すグリフのうち画像を持つものを、新しいグリフIDの順に索引の部分表（形式1と3。ほかの形式の部分表を含む大きさの表は除く）へまとめなおし、画像のデータを新しいCBDTへ写す。同じ形式、データ、グリフごとの長さの塊は前のものを共有する。写す量は元のCBDTの長さの `HB_SUBSET_CBDT_MAX_COPY_FACTOR`（64）倍までとする。残すグリフを持たない大きさの表は除く。
- sbixは、各大きさの表で新しいグリフIDの順にグリフのデータを写し、残すグリフのデータを持たない大きさの表を除く。
- 軸の固定（`Input::with_all_axes_pinned_to_default`、`with_axis_pinned_to_default`、`with_axis_pinned`、`with_axis_range`、`axis_range`。`hb_subset_input_pin_all_axes_to_default` などと同じく、fvarの無いフォントや無い軸では失敗して値を変えない）：`Plan::new` は `normalize_axes_location` のとおりに軸の位置を正規化する（fvarと同じ範囲の指定を除き、2.14に丸め、avarの版1の写像を通す。浮動小数点はHarfBuzzと同じ `f32` と `roundf`）。すべての軸を既定の値に固定する場合に加え、fvarとavar版1以外の変化量表、CFF2、GDEF、GSUB、GPOS、BASE、MATH、COLR、VARCを持たないフォントでは [ADR 0061](adr/0061-subset-axis-instancing.md) に従い、固定した軸を除き、残る軸の範囲と名前つきインスタンスを書き直す。STATがある場合は、軸範囲の外の軸値を除いて残す。avar版1がある場合は、既定位置への固定に限り、残る軸の写像を保つ。それ以外の位置と範囲、avarの版2、avarが既定の値を0以外に写すフォントは `Unsupported` とする（[ADR 0022](adr/0022-subsetting.md) の後回しにしたもの）。固定したときは、fvar、avar、gvar、GVAR、HVAR、VVAR、MVAR、cvarを捨て、CFF2を上のとおりに書き、glyfのグリフを点から符号化しなおし（`compile_bytes_with_deltas`）、そこから求めた値でheadの箱と `flags` のビット1、hmtxとvmtxのサイドベアリング、hheaとvheaの最大の送り幅、最小のサイドベアリング、最大の広がり、maxpの点と輪郭と成分の数、OS/2の `xAvgCharWidth` を書きなおす。OS/2の `usWeightClass` と `usWidthClass`、postの `italicAngle` は軸の値から、STATは軸の範囲の外の軸の値を除いて書く。名前IDの閉包はfvarの名前を足さない。GSUBとGPOSは、FeatureVariationsを既定の位置で評価して最初に当たる記録の置きかえのフィーチャを使い、FeatureVariationsを除いて版1.0とする。GDEFの変化量の表を除き、値の記録とアンカーとキャレットの変化量の番号のDeviceを除く（値の記録の形式からはDeviceのビットを除く）。fvar、avar、STATのビューと検証器は `sofel-read` にある。
- 軸を指定しないgvarとGVARは、残すグリフの変化量のデータを新しいグリフIDの順に書き、短いオフセットでは奇数長のデータを0で埋める。`IFTB_REQUIREMENTS` では長いオフセットを使い、埋めない。
- [ADR 0061](adr/0061-subset-axis-instancing.md) の軸指定で、gvar または GVAR の共有 tuple が0件かつ全グリフの変化記録のオフセットが0の場合は、非既定値への固定と範囲指定を受け付ける。軸を指定した出力からこの空の表を除く。
- [ADR 0061](adr/0061-subset-axis-instancing.md) の gvar に変化記録があるフォントでも、全軸を非既定値に固定し、輪郭を残すグリフが単純グリフだけなら、`sofel-font::var::gvar::apply_gvar_hb` による点と幻の点の差分を glyf とメトリクスへ反映する。この経路は avar と、ほかの変化量表を持たない入力に限定する。合成グリフを残す場合は `Unsupported` とする。
- 軸を指定しない HVAR と VVAR は、複数のItemVariationDataから残したグリフに対応する行と使う領域を詰め、全ての値が0の列を除いて書き出す。使わないItemVariationDataを除き、暗黙・明示の送り幅とサイドベアリングのマップを新しいグリフIDに合わせて書く（[ADR 0022](adr/0022-subsetting.md)）。 明示マップの末尾で複数のグリフが同じ行を参照する場合は、その最初の項目までを書き、残りは最後の値を繰り返して読む。空のサイドベアリングと縦の原点のマップは出力しない。
- VARC のサブセット化（[ADR 0067](adr/0067-varc-components.md)）は、残す部品を再帰的に集め、グリフ ID と参照、条件、部品の軸座標、変化量ストアの番号を再配置する。VARC が参照しない1軸を既定値に固定し、gvar、GVAR、HVAR、VVAR、MVAR、cvar、CFF2、GDEF、GSUB、GPOS、BASE、MATH、COLR、STAT が存在しないか捨てられ、avar が版2でない場合は、残る軸の番号を書き直す。これらの表と avar が元のフォントにない場合は、非既定値への固定と範囲の指定でも VARC の軸番号を写す。参照される軸の固定と対象外の変化量表を持つフォントの部分軸固定は `Unsupported` とする。VARC を `NoSubsetTables` でそのまま写す場合は、`RETAIN_GIDS` を指定して軸を変えないことを要し、満たさなければ `Unsupported` とする。名前の表の上書きも `Unsupported` とする。
- 軸を指定しない BASE 1.1 の ItemVariationStore は、残すスクリプトとフィーチャに属する横と縦の BaseCoord の VariationIndex を集め、使う行と領域だけを詰めて番号を振りなおす。参照が0件なら、HarfBuzzと同じくストアを除いて座標を残す。BaseValues、既定の MinMax、言語別とフィーチャ別の MinMax を調べ、壊れた参照はこの処理の対象にしない（[ADR 0022](adr/0022-subsetting.md) の10段階目）。
- 軸を指定しないGDEF 1.3のItemVariationStoreは、GSUBのFeatureVariationsが軸範囲の条件だけを持つ場合も含めて、残すLigCaretList、値の記録、アンカー、MATHの定数・斜体補正・上アクセント・カーニング・組み立てのVariationIndexに合わせて行と領域を詰めて書く。参照が0件ならストアを除く。GPOSの値の記録とアンカーをたどってVariationIndexを集める。送り幅などの値に現れる `0x8000` は参照に数えない。MATHとGDEFの一方だけをコピーして参照番号を合わせられない場合と、同じバイト列があるGPOSをそのままコピーする場合は拒否する。GDEFだけをコピーしてGPOSをサブセット化するとき、HarfBuzzと同じくGPOSの参照番号は詰めるが、GDEFのストアは元のバイト列のまま写す。壊れたGDEFはこの処理の対象にしない（[ADR 0022](adr/0022-subsetting.md) の10段階目）。
- レイアウトの表のうち、FeatureVariationsを持つGSUBをそのままコピーする場合のGDEF、GSUBとGPOSの版1.2とGDEFの版1.4、FeatureVariationsの版1.1と形式1でない条件、65536を超えるグリフのフォントの24bitの形式も `Unsupported` とする。

## WOFF と圧縮フォントの読み込み

`sofel_woff::decode_woff` は出力サイズの上限を受け取り、WOFF 1.0 の表を物理順に展開して sfnt を返す。フォントを読み込む CLI は gzip または UNIX compress（LZW）を先に展開してから WOFF の署名を検査する。検証器は WOFF の不正なフィールドか圧縮表の入力オフセットを報告する。gzip の破損はメンバーを含むコンテナ先頭で報告する。[ADR 0065](adr/0065-legacy-faces-and-webfonts.md)

## Web フォントの書き出し（`sofel-woff`）

`sofel_woff::encode` は単一フェイスの sfnt と出力形式を受け取り、WOFF または WOFF2 のバイト列を返す。表のタグ、範囲、パディング、チェックサムとフォント全体のチェックサムを検査する。コマンドは変換を終えてから出力先を開く。[ADR 0070](adr/0070-webfont-encoding.md)

WOFF は表を元の物理順に並べ、zlib で短くなる表だけを圧縮する。WOFF2 は DSIG を除き、head.flags のビット11を立て、全表を変換なしで一つの Brotli ストリームに格納する。

## 道具

統一 CLI `sofel` は render、shape、paint、validate、subset を受け付ける（[ADR 0071](adr/0071-unified-sofel-cli.md)）。
引数なし、`help`、`-h`、`--help` は操作一覧を、`help NAME`、`NAME -h`、`NAME --help` は使用例を含む詳細を標準出力に書く。
未知の操作やヘルプ対象は終了コード 2 で診断する。

`sofel render --ppem P (--text TEXT | --text-file PATH) FONT` は一行を一つのランとしてシェーピングし、アウトラインを白地に黒の PGM に描いて標準出力に書く。
フェイス、可変軸と OpenType フィーチャーを指定できる。
UTF-8 ファイルの末尾から LF または CRLF を一組だけ除き、前後の空白も送り幅に含める。
空文字は白い 1 × 1 画素になる。
入力指定の重複、不正な UTF-8、残った CR や LF は終了コード 2、ファイルの読み込み失敗は終了コード 1 で診断する。

- `sofel-paint [--face N] [--palette P] [--foreground RRGGBBAA] [--ppem P] [--exact] (--glyph G | --char U+XXXX) FILE`：カラーグリフの塗りの命令を、HarfBuzzの `test/api/test-paint.c` と同じ字下げのテキストで書く。数はCの `%.3g`、`--exact` では `f32` のビットの16進。カラーでないグリフは終了コード1、使い方の誤りは2（[ADR 0023](adr/0023-color-glyphs.md)）。
- `sofel-render [--face N] --ppem P [--no-bitmap] [--color [--palette N]] (--glyph G | --char U+XXXX) FILE`：1グリフをヒンティングなしで描き、PGM（P5）を標準出力に書く。位置は注釈の行 `# left <x> top <y>` に書く（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。そのppemのビットマップがあれば、`--no-bitmap` が無いかぎりそれを書く（32bitはPAM。[ADR 0023](adr/0023-color-glyphs.md)）。`--color` では、COLRv0のグリフを層の色で描いてPAMで書く。
- `sofel-shape [オプション] <フォント> [<文字列>]`：HarfBuzz 15.1.0の `hb-shape` と同じオプションと出力で文字列を形にする（[ADR 0020](adr/0020-shaping-foundation.md)）。オプションはGLibと同じく `--名前=値` と `--名前 値` の形で、数はCの `strtol`（基数0）と `sscanf` と同じに読む。文字列（`--text`、`--text-file`、`--unicodes`/`-u`、`--unicodes-before`、`--unicodes-after`、`--text-before`、`--text-after`、`--single-par`。どれも無ければ標準入力）、区間（`--direction`、`--script`、`--language`、`--default-language`）、機能（`--features`）、フォント（`--face-index`/`-y`、`--font-size`、`--font-ppem`）、バッファ（`--bot`、`--eot`、`--preserve-default-ignorables`、`--remove-default-ignorables`、`--invisible-glyph`、`--not-found-glyph`、`--not-found-variation-selector-glyph`、`--cluster-level`、`--utf8-clusters`、`--unsafe-to-concat`、`--safe-to-insert-tatweel`、`--verify`、`--normalize-glyphs`、`--glyphs`）、出力（`--output-format`/`-O` の `text` と `json`、`--no-glyph-names`、`--no-positions`、`--no-advances`、`--no-clusters`、`--show-text`、`--show-unicode`、`--show-line-num`、`--show-flags`、`--show-extents`、`--ned`）、シェーパー（`--shaper`、`--shapers` の `ot` と `fallback`）を解釈する。`--font-funcs` と `--face-loader` は受け付けて無視する。
  - 文字列は `hb-shape` と同じく行ごとに形にし、1行に1行を書く。`--unicodes` の値は `hb-shape` と同じ区切りの文字で分けた16進の数で、GLibと同じく6バイトまでのUTF-8にしてからバッファに入れる。`--utf8-clusters` が無ければ、クラスタの値を文字の番号に付けなおす。言語は `--language`、無ければ `--default-language`、それも無ければ「言語なし」（HarfBuzzの `LANG=C` と同じ結果）とする。計画は、区間の性質が前の行と同じあいだ使いまわす。グリフの名前は `post` とCFF、グリフの箱は `sofel_outline::HbExtents`（glyf、CFF2、CFF）で、`shape_with_extents` と `--show-extents` に使う。
  - `--glyphs` は行をグリフの列として読み（閉じの `]` は省ける）、位置が無ければフォントの送り幅を入れ、フォントの大きさに合わせて拡大縮小して書きなおす。`--verify` は `HB_BUFFER_FLAG_VERIFY`（`src/hb-buffer-verify.cc`）と同じ検査（クラスタの値の単調さ、`UNSAFE_TO_BREAK` でない位置で切って形にしなおした結果、`--unsafe-to-concat` があれば `UNSAFE_TO_CONCAT` でない位置で切った断片を2つの列に交互に入れて形にした結果）を `diff` で行い、失敗した行には `error: Shaping failed.` を書く。
  - 終了コードは、成功が0、フォントか文字列のファイルを読めなければ1、使い方の誤りと、解釈しないオプション（`--font-slant`、`--font-bold`、`--font-grade`、`ot` と `fallback` 以外のシェーパーなど）が2、`--verify` の検査が失敗した行があれば3。
  - 書式と解釈は `sofel_cli::shape::run_with_input` にあり、適合テストはプロセスを起動せずにこれを呼ぶ。
- `sofel-subset [オプション]...`：`hb-subset`（`util/hb-subset.cc`）と同じオプションを受け付けてサブセット化する（[ADR 0022](adr/0022-subsetting.md)）。引数の解析は `sofel_cli::subset` に純粋な関数として置き、適合テストと共有する。`--batch` は、1行に `;` で区切った引数を読み、成功なら `success` を書く。`--preprocess`（`--preprocess-face`）は、フォントを `Prepared::new` で前処理し、`Plan::with_prepared` で計画を作る（出力は前処理の無いときと同じ。`sofel_cli::subset::prepare` と `subset_once`）。軸の指定（`--variations`、`--instance`）は `parse_instancing_spec` と同じくフォントを開いた後に入力に移す（`sofel_cli::subset::resolve`）。`drop`、`*=drop`、既定の値と同じ値は既定の値への固定になる。既定の値以外への固定や範囲の指定は、まだ誤りとして終了の状態1で終わる。`--glyphs`、`--glyphs+`、`--glyphs-`、`--glyphs-file` の名前も、フォントを開いた後に `resolve` が引く。引き方はHarfBuzzの `hb_font_glyph_from_string` と同じで、postの名前（版1の標準の名前と版2の名前。HarfBuzzと同じ並べ方の二分探索）、CFFのcharsetの名前（`GlyphNames::glyph_from_name`）、10進のグリフID、`gidDDD`、`uniXXXX`（cmapで引いたグリフ）の順に試す。CFFで同じ名前のグリフが複数あるとき、sofelはグリフIDの最も小さいものを返し、HarfBuzzの結果はその並べ方による（[ADR 0021](adr/0021-cff-outlines.md)）。グリフの集合を変えるオプション（`--gids`、`--glyphs`、`--gid-map`、`--keep-everything` と、その `-file` の形）は、解析のときに変更の列（`GlyphOp`）として記録し、名前があれば `resolve` がコマンドラインの順に再生してグリフの集合を作る。見つからない名前は `Failed parsing glyph name: '...'`（名前から引数の終わりまで）で終了の状態1になり、`--glyphs-file` の中ではHarfBuzzと同じくそのファイルの読み込みだけが止まる。名前の引き方は、`test/subset/data/fonts/` の名前を持つ全フォントで `hb-dump --subset --glyphs` と突き合わせる。
- `sofel-validate <ファイル>...`：sfntとTTC/OTCの各表を検証する（[ADR 0008](adr/0008-validate-command-and-freetype-comparison.md)）。1件を1行で `<パス>[#<フォント番号>]: <タグ>+0x<オフセット>: <error|warning>: <説明>` の形で出す。`--json` では、1件を1行のJSONのオブジェクト（JSON Lines）で出す。キーは `path`、`font`、`table`、`offset`、`severity`、`kind`、`message`、`details`。`kind` と `details` のキーは出力の約束の一部とする。終了コードは、`Error` が無ければ0、あれば1、使い方の誤りか読めないファイルがあれば2で、`--json` の有無で変わらない。

## C ABI（`sofel-capi`）

- `crates/sofel-capi` は、ライブラリのクレートの機能をCの呼び出し規約で公開する（[ADR 0015](adr/0015-c-abi.md)）。静的ライブラリ、動的ライブラリ、rlibを作る。ヘッダ `crates/sofel-capi/include/sofel.h` は手で書く。
- 名前は `sofel_` で始め、オブジェクトの型は不透明な構造体へのポインタ、値の型は固定幅の整数とヘッダに宣言した構造体を使う。
  誤りうる関数は `int32_t` を返し、0を成功、負の値を誤り（`SOFEL_ERROR_*`、`sofel_error_string` で説明）とする。
  結果は出力の引数に書く。
  ポインタの引数がNULLなら `SOFEL_ERROR_NULL` を返す（`user` のポインタ、`*_destroy`、要素数0の位置の構築子の入力配列を除く）。
  panicしない。
- フォント（`sofel_font_t`）はデータを複製しない。呼び出し側は、フォントを捨てるまでデータを生かしておく。フォントは不変で、複数のスレッドから同時に使える。作業領域（`sofel_outline_scratch_t`、`sofel_rasterizer_t`）は呼び出し側が持ち、スレッドの間で共有しない。
- 可変長の出力（画像、文字列、座標の配列）は呼び出し側の配列に書く。足りなければ要る長さを書き、配列には何も書かず `SOFEL_ERROR_BUFFER_TOO_SMALL` を返す（`snprintf` と同じ形）。
- `sofel_location_t` は、作成に使った `sofel_font_t` のハンドルを破棄した後も使えるが、元のフォントのバイト列は位置を破棄するまで生かして変更しない。
- 輪郭はCの関数ポインタ（`move_to`、`line_to`、`quad_to`、`cubic_to`、`close` と `void *user`）で受け取る。検証の結果は、1件ずつ関数ポインタに `sofel_diagnostic_t` で渡す。
- ヘッダの各関数の説明に、対応するRustの関数（`Rust:`）と、相当するHarfBuzzとFreeTypeの関数を書く。`scripts/check-capi.sh` が、ライブラリのクレートの公開の関数（rustdocのJSONの出力から `scripts/public-api.sh` で求める）がすべてヘッダの `Rust:` の行か除外の一覧（`crates/sofel-capi/unmapped.txt`、理由つき）にあることを確かめる。ライブラリに公開の機能を足すADRは、同じプルリクエストでC ABIの関数とヘッダとCのテストも足す。
- CFFとCFF2（[ADR 0021](adr/0021-cff-outlines.md)）：`sofel_font_t` は作るときに `Outlines`（`Cff::new` の結果とHarfBuzzの箱の値）を持ち、CFFが壊れていてもフォントの作成は失敗しない。輪郭とラスタライズの関数はそれでCFFとCFF2も描く。`sofel_font_is_cff`、`is_cff2`（1か0）、`sofel_font_is_cid_keyed`、`sofel_font_cid`（誤りのとき `*cid` は0）、`sofel_font_ros`（NUL終端で最初のNULで切った文字列、NULLの文字列は空にして `present` のビットで区別）、`sofel_font_ps_font_info`（数の配列と、呼び出し側の配列に並べた文字列とその位置。NULLは `SIZE_MAX`）、`sofel_font_ps_fs_type` を持つ。`sofel_font_t` は作るときにCFFのcharsetの索引も作って持ち、`sofel_font_cid` と `sofel_font_ft_glyph_name` はそれで引く（[ADR 0025](adr/0025-cff-charset-index.md)。関数の形は変わらない）。FreeTypeが開けないCFFには `SOFEL_ERROR_INVALID_CFF`、FreeTypeの `Invalid_Argument` には `SOFEL_ERROR_INVALID_ARGUMENT` を返す。
- シェーピング（[ADR 0020](adr/0020-shaping-foundation.md)）：機能（`sofel_feature_t` は `tag`、`value`、`start`、`end` の構造体。`sofel_feature_parse`、`sofel_feature_format`）、方向（`SOFEL_DIRECTION_*` はHarfBuzzの `hb_direction_t` と同じ値で、0は不明）、文字体系（`sofel_unicode_script` と同じ形の32ビットのタグ、0は不明。`sofel_script_from_string`）、言語（`sofel_language_t`、`sofel_language_create`、`destroy`）、OpenTypeのタグの変換、区間の性質（`sofel_segment_t` は方向、文字体系、言語のポインタの構造体。`sofel_segment_guess(utf8, len, in, default_language, out)`）、計画（`sofel_shape_plan_create(font, segment, features, n, error)`、`destroy`、`shaper`。不変で、スレッドの間で共有してよい。フォントより先に捨てる）、バッファ（`sofel_buffer_t`。作成、`clear`、`add_utf8` などの文字の追加、設定の `get` と `set`。スレッドごとに持つ）を持つ。
  - バッファは文字かグリフのどちらかを持つ。`sofel_shape(font, scale, plan, buf)` は文字をシェーピングして同じバッファの値にグリフを置き（`shape_with_extents`）、`sofel_buffer_clear` は設定とメモリを保ったまま文字の空のバッファに戻す（`GlyphBuffer::into_unicode`）。違う種類を渡された関数は `SOFEL_ERROR_BUFFER_CONTENT` を返す。`scale` がNULLならフォントの単位で、ppemは0（Device表は効かない）とする。グリフの箱は、HarfBuzzの `ot` の関数と同じく、sbix、CBDT、COLR、glyf、CFF、CFF2の順に最初に得たものを使う。
  - `sofel_scale_create_with_ppem(font, x_scale, y_scale, x_ppem, y_ppem, error)` は横と縦の ppem を保持し、`sofel_shape` と `sofel_shape_fallback` に渡す。Device 表の補正はそれぞれの軸の ppem に従う（[ADR 0052](adr/0052-c-shaping-ppem.md)）。従来の `sofel_scale_create` と NULL のスケールは両軸とも 0 を使う。
  - レイアウト問い合わせ（[ADR 0062](adr/0062-layout-query-device-adjustments.md)）の `sofel_scale_baseline*` と `sofel_scale_layout_font_extents` は、渡したスケールの横・縦の ppem を Device の評価に使う。`sofel_layout_ligature_carets`、`sofel_math_kernings`、`sofel_math_variants`、`sofel_math_assembly` は、開始位置から容量まで格納し、格納件数と列の総数を別々に返す。容量0では出力配列を NULL にでき、容量外の要素は書き換えない。`sofel_math_*` の単一値と `sofel_style_value` も位置を受け取る。`sofel_scale_create_with_ptem` で作ったスケールを `sofel_style_value` に渡すと、光学サイズの問い合わせにポイントサイズを使う。
  - `sofel_buffer_glyphs(buf, gids, clusters, flags, pos4, cap)` は、グリフID、クラスタ、フラグと、1グリフ4つの位置（x送り幅、y送り幅、xずれ、yずれ）を書き、グリフの数を返す。数が `cap` より大きければ何も書かない（Golemの `golem_font_shape` の形）。誤りは負の値で返す。
  - 直列化は `sofel_buffer_serialize`（`hb_buffer_serialize` と同じく、入る項目だけをNUL終端で書き、書いた項目の数とバイト数を返す）、`sofel_buffer_deserialize_glyphs`、`sofel_buffer_deserialize_unicode`、`sofel_font_glyph_to_string`、`sofel_font_glyph_from_string` で、比べるのは `sofel_buffer_diff` で行う。HarfBuzzの手書きの文字の性質も `sofel_unicode_modified_combining_class` などで公開する。
  - [ADR 0053](adr/0053-preserve-buffer-settings-after-glyph-deserialization.md) に従い、グリフ列を読み込んだ後の `sofel_buffer_clear` は通常のクリアと同じ設定を引き継ぐ。初回の読み込み、追記、パースエラーのいずれでも乱数状態は1に戻る。
  - Cのテストは、Golemの `golem_font_shape` と同じ呼び出しの列（作成、推定、計画、シェーピング、結果の取り出し、要る数を返す場合）を、テストのデータのRobotoとM+ 1pで行い、RustのAPIで求めた値（HarfBuzzと一致することを手元で確かめた）と比べる。値は `tests/c/fixture_shape.h` に書き、フォントは実行時に `SOFEL_TESTDATA` から読む。テストのデータが無ければ飛ばし、`SOFEL_REQUIRE_TESTDATA` があれば失敗とする。CIでは適合テストとcapi-wasmのジョブで必ず走らせる。
- サブセット化（[ADR 0022](adr/0022-subsetting.md)）：入力（`sofel_subset_input_t`。`sofel_subset_input_create` はHarfBuzzの既定の値で作る）は、`keep_everything`、集合（種類 `SOFEL_SUBSET_SET_*` はHarfBuzzの `hb_subset_sets_t` と同じ番号）の `add`、`add_range`、`remove`、`clear`、`invert`、`contains`、`count`、`next`、フラグ（`SOFEL_SUBSET_FLAGS_*` はHarfBuzzと同じビット。定義の無いビットは `SOFEL_ERROR_INVALID_ARGUMENT`）、グリフの対応の指定、軸の固定（`pin_all_axes_to_default`、`pin_axis_to_default`、`pin_axis_location`、`set_axis_range`。フォントを引数にとり、fvarの無いフォントや無い軸では `SOFEL_ERROR_NOT_FOUND`、`set_axis_range` で最小が最大を超えれば `SOFEL_ERROR_INVALID_ARGUMENT` を返して入力を変えない）、軸の範囲の取り出し、名前の表の上書き（プラットフォーム1でASCIIでない文字列はHarfBuzzと同じく拒む）、`to_args`（オプションを空白で区切った文字列）を持つ。RustのAPIの `Input` は不変の値なので、Cの関数は入力の値を変えた値に置きかえる。前処理（`sofel_subset_prepared_t`）は `sofel_subset_prepared_create(font, out)` で作り、`sofel_subset_prepared_destroy` で解放する（計画は前処理の値への参照を自分で持つので、計画より先に解放してよい）。計画（`sofel_subset_plan_t`）は `sofel_subset_plan_create(font, prepared, input, out)`（`prepared` はNULLか、同じフォントの前処理の値）、`execute`、出力のグリフの数、新旧のグリフと文字から元のグリフへの対応（1つを引く関数と、対を呼び出し側の2つの配列に書く関数）を持つ。`sofel_subset(font, input, out)` は計画と実行を続けて行う。`sofel_subset_cff_charstrings_index` と `sofel_subset_cff_charstring_data` は、フォントのデータを指すポインタと長さを返す（無ければ `SOFEL_ERROR_NOT_FOUND`）。サブセット化の誤りは `SOFEL_ERROR_NO_GLYPHS`、`_INVALID_INPUT`、`_MALFORMED_TABLE`、`_UNSUPPORTED`、`_OFFSET_OVERFLOW`、`_LIMIT`、`_SUBSET_FAILED` とする。
  - 出力のフォントと `sofel_serialize_objects`（呼び出し側のオブジェクトとリンクの配列から、再配置を含めて表を作る。リンクは幅、位置、相手に加えて、符号、起点、偏りを持つ）の出力は、sofelが確保して `sofel_bytes_t`（ポインタと長さ）で返し、`sofel_bytes_destroy` で解放する。
  - 前処理（`sofel_subset_prepared_*`）、CFFのcharstringの取り出しは、それぞれのRustの関数を足す段階で足す。
  - Cのテストは、入力の関数で作った入力の `to_args` がRustで作った同じ入力の値と一致すること、`sofel_subset` と計画の `execute` がRustと同じバイト列（長さとFNV-1aのハッシュ）を返すこと、対応の関数が `Plan` と同じ値を返すこと、NULL、定義の無いフラグ、定義の無い集合の種類で誤りが返ることを確かめる。Golemの `golem_font_subset` と同じ呼び出しの列（作成、グリフの追加、`RETAIN_GIDS`、`sofel_subset`、破棄）を、テストのデータのRobotoとSource Han Sansで行い、`tests/c/fixture_subset.h` の値と比べる。軸の固定の関数は、テストのデータの `Roboto-Variable.ABC.ttf` で、Rustの関数と同じ範囲を設けること、無い軸と逆の範囲とfvarの無いフォントで誤りを返して入力を変えないこと、すべての軸を既定の値に固定した `sofel_subset` がRustと同じバイト列を返すことを確かめる。この値は、`hb-dump` があれば `hb-dump --subset` の出力とも一致することを `fixture.rs` が確かめる。
- `SOFEL_VERSION_MAJOR` などのマクロと `sofel_version()` を持つ。1.0.0までABIの互換を約束しない。
- Cのテスト（`crates/sofel-capi/tests/c/`）は `scripts/test-capi.sh` で、ネイティブ、ThreadSanitizer、AddressSanitizer、wasm32-wasip1（wasi-sdkとwasmtime）で走らせる。期待値は、`crates/sofel-capi/tests/fixture.rs` がRustのAPIで求めて `fixture.h` に書く。静的ライブラリの `sofel_` のシンボルとヘッダの関数の一致も確かめる。wasmでは、リンクに例外処理（`-lsetjmp`）もC++の実行時ライブラリも要らない。

## wasm

- ライブラリのクレートは `#![no_std]`（必要なら `alloc`）とする（[ADR 0004](adr/0004-wasm-and-no-std.md)）。
- CIで、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テスト（wasmtime）を行う。ツールチェーンは `rust-toolchain.toml` でstableの版を固定する（[ADR 0007](adr/0007-sfnt-read-foundation.md)）。
- JavaScriptからの呼び出しの層は持たない。
- `sofel-capi` は `wasm32-wasip1` の静的ライブラリとしてwasi-sdkのCのプログラムとリンクでき、`wasm32-unknown-unknown` にはビルドだけを確かめる（[ADR 0015](adr/0015-c-abi.md)）。

## 正しさの検査

SVG の検査器は [ADR 0074](adr/0074-svg-feature-conformance.md) の必須ケースと観測を [台帳](../conformance/svg-cases.tsv) と照合する。欠落、重複、未知の項目を報告し、ケースごとに期待値の出典と結果を出す。共有文書の平文のバイト列と範囲外のグリフを検査し、検査器を実装していない観測は未実行として扱う。

検査器は必須ケースの集計に続いて、[OpenType 1.9.1 の SVG 要求項目](../conformance/svg-opentype.tsv)ごとに、対応するケース ID または対象外・未決の理由を表示する。

- 追加フォントの集合は [ADR 0031](adr/0031-diverse-font-corpus.md) の13ファイル、57フェイスと既存の可変COLRフォントで構成する。`fonts.lock` は取得ファイルとライセンスをHTTPS URL、SHA-256、バイト数、保存先で固定し、`conformance/fonts.tsv` はフォントとライセンスと対象機能を結ぶ。`scripts/fetch-fonts.sh` は保存済みのファイルも再検証し、欠落とハッシュ不一致を失敗にする。フォントのバイナリをリポジトリに含めず、著作権表示とライセンス全文を `THIRD_PARTY_NOTICES.md` に記録する。
- 追加フォントの各フェイスでは表の読み取り、検証器、全Unicodeスカラー値の既定座標のnominal glyph ID、最大128グリフの横・縦メトリクスとFreeTypeの非ヒンティング輪郭・advanceを照合する。サンプル集合にはグリフ0と末尾と登録した全座標のシェーピング出力を必ず入れ、[ADR 0031](adr/0031-diverse-font-corpus.md) の等間隔の式で補う。HarfBuzzのメトリクスとFreeTypeの輪郭・advanceは別々の参照値と比較する。
- 追加フォントのシェーピング入力は `conformance/font-cases.tsv` にコードポイント、script、language、direction、feature、座標を固定する。可変フォントはこのシェーピングのケースで各軸単独の最小・最大と全軸の最小・最大を覆う。HarfBuzzの結果を `conformance/font-case-hb.tsv` に固定し、未実装の機能による失敗を `conformance/expected-failures-extra-fonts.txt` にケースIDと理由で記す。
- 非既定座標のメトリクスと輪郭はArabic、Source Serif、Amstelvarの代表10ケースに実際の位置を渡し、固定版のHarfBuzzとFreeTypeでそれぞれ比べる。期待失敗は `conformance/expected-failures-extra-variation.txt` に各ケースの理由を記す。既定座標の輪郭・メトリクスの一致とは区別する。可変COLRの44軸は、既定座標、各軸の最小・最大、全軸の最小・最大の91ケースを `conformance/extra-color-var-cases.tsv` に固定する。`sofel-paint` の出力を既定および指定した位置で91ケースすべて固定版HarfBuzzの描画命令と照合する。CFFのCOLRv1はpaint列を、sbixはPNGの長さとFNV-1a digestを固定版のHarfBuzzと比較する。予期しない成功も失敗にする。nativeでは固定版のFreeTypeとHarfBuzzを実行して参照テキストを照合し、wasmでは同じケースIDと固定テキストで結果を検査する。
- 既存のテスト集合のフォント本体は同梱せず、`testdata.lock` に書いたコミットのHarfBuzz（15.1.0）とFreeType（2.15.0）を `scripts/fetch-testdata.sh` で取得して使う（[ADR 0002](adr/0002-license-and-reuse.md)、[0005](adr/0005-conformance-tests.md)）。
- シェーピング（AATを含む）は、HarfBuzzの `test/shape/data/*/tests/*.tests` の期待値と比べる（[ADR 0020](adr/0020-shaping-foundation.md)、`tests/harfbuzz_shape.rs`）。
  - `in-house`、`aots`、`text-rendering-tests` のすべての行を、`run-tests.py` と同じく `<フォント> --unicodes <文字> <オプション> --shaper=<シェーパー>` で `sofel_cli::shape` に渡し、期待値が `*` でなければ `--verify --unsafe-to-concat --safe-to-insert-tatweel` を加える。結果が期待値と違い `gid` を含むときは、両方を `--glyphs --no-glyph-names` で読みなおしてグリフIDで比べる。`@shapers=` と行の `--shaper=` の指示でシェーパーを選び（既定は `ot`）、sofelに無いシェーパーの行は走らせずに失敗とする。`@font-funcs=` と `@face-loader=` は無視する。システムのフォントを絶対パスで指す行は飛ばし、HarfBuzz自身が走らせない（失敗する）ファイル（`text-rendering-tests/DISABLED`。`CMAP-3.tests` はMacの非Unicodeのcmapで、HarfBuzzもASCIIしか引かない）は除く。
  - 項目は `<スイート>/<ファイル>:<行>` で、`ot` 以外のシェーパーには `@<シェーパー>` を付ける。期待する失敗は `conformance/expected-failures.txt` に、[ADR 0020](adr/0020-shaping-foundation.md)の分類をちょうど1つコメントに書く：`shaper:<名前>`（[ADR 0024](adr/0024-complex-shapers.md) の未実装のシェーパー）、`variations`、`aat`（AATと古い `kern` 表、`--font-ptem` による `trak`）、`synthetic`、`cff`、`color`、`out-of-scope:<名前>`。テストは分類が行に合うこと（`shaper:` は計画のシェーパー、`out-of-scope:` は行のシェーパー、`synthetic` はオプション、`variations` は `--variations` または `--named-instance` か `fvar`（`--font-ptem` 単独は含めない）、`aat`、`cff`、`color` はフォントの表）を確かめる。[ADR 0044](adr/0044-shaping-conformance-exceptions.md) で認めた2行の分類は `dfont`（Macのリソースの形式のフォント。[ADR 0007](adr/0007-sfnt-read-foundation.md) で後回し）と `sanitize`（HarfBuzzのsanitizerがGSUB全体を拒むフォント。sofelの検証器が `Error` を報告することを確かめる）とする。
  - 適合テストの現在の期待失敗は [`conformance/expected-failures.txt`](../conformance/expected-failures.txt) に分類つきで列挙し、件数は `harfbuzz_shaping_tests` の実行結果で確かめる。`aat` の行はAATと `kern` を除いてビルドしたHarfBuzzと、すべて一致することを手元で確かめた。
- `hb-dump` によるシェーピングの突き合わせ（[ADR 0020](adr/0020-shaping-foundation.md)、`tests/harfbuzz_shape_dump.rs`）。`hb-dump` は `LC_ALL=C` で走らせる。期待する失敗の分類は上と同じで、同じく確かめる。
  - `hb-dump --shape FONT TEXTFILE DIRECTION FEATURES` は、行ごとに `hb_buffer_add_utf8`、`hb_buffer_guess_segment_properties`、`hb_shape` を行い、`hb-shape` の形（グリフID、UTF-8のクラスタ、フラグ）で書く。`perf/benchmark-shape.cc` のすべての組（可変フォントは既定のインスタンス）と、`Mplus1p-Regular.ttf` と `ja-runs.txt`（LTRとTTB、機能なし、`vrt2`、`-vert`、`palt`、`vkrn`）を、`sofel-shape --text-file --utf8-clusters --no-glyph-names --show-flags` と比べる。項目は `<フォント>+<文字列>:<方向>:<機能>` で、1行でも違えば失敗とし、分類は違ったすべての行に合わなければならない。期待する失敗は `conformance/expected-failures-shape-pairs.txt` に書く。52万9399行のうち、デーヴァナーガリーとデュプロワイエの組の5778行が `shaper:*` で違い、ほかはすべて一致する。
  - `hb-dump --shape-random FONT COUNT SEED` は、テストのフォントのフェイスごとに、cmapで引ける文字を文字体系ごとに分け、同じ線形合同法（C標準の例の式、上位15ビット）で長さ1〜16の文字列をCOUNT個作り、`BOT`、`EOT`、`PRODUCE_UNSAFE_TO_CONCAT` で形にする。文字列も書き、テストはsofelのcmapから同じ文字列を作って一致を確かめる。件数は200、種は20。項目は `<フォント>#<フェイス>:<文字体系>` で、期待する失敗は `conformance/expected-failures-shape-random.txt` に書く。sofelが開けないフェイス（headの無いもの。[ADR 0010](adr/0010-font-foundation.md)）は比べない。2017項目のうち1875が一致し、期待する失敗は `aat` 134、`variations` 5（既定の座標で条件の成り立つFeatureVariations）、`sanitize` 3である。
- 音節の状態機械は、HarfBuzzの機械と突き合わせる（[ADR 0024](adr/0024-complex-shapers.md)、`tests/harfbuzz_syllables.rs`）。`hb-dump --syllables MACHINE ALPHABET COUNT SEED` は、分類を直接書いたバッファ（ほかの欄は0）にHarfBuzzの `find_syllables_<機械>` を走らせ、音節の値を書く。分類の値の集まりは、HarfBuzzの `.rl` の `export` の値と、`export` に無い最小の値とする。長さ1〜4のすべての列と、`--shape-random` と同じ線形合同法（種1）による長さ5〜32の列100万個について、sofelの結果とすべて一致することを確かめる。期待する失敗の一覧は持たない。4つの機械の合わせて約894万列が一致する。
  - `hb-dump --layout-map FONT` は、全文字体系と、フォントの言語のシステムのタグの言語（と言語なし）について、`hb_ot_tags_from_script_and_language` のタグで `hb_ot_layout_table_select_script` と `hb_ot_layout_script_select_language` が選ぶGSUBとGPOSの文字体系と言語の番号を書く。`layout_selection` と比べ、項目は `<フォント>#<フェイス>`、期待する失敗は `conformance/expected-failures-layout-map.txt` に書く。652項目のうち、HarfBuzzのsanitizerがGSUBを拒む3つのフォント（`sanitize`）を除いてすべて一致する。
  - 決定性：`--shape` の組を `sofel-shape` のライブラリで形にした全行の結果（グリフ、クラスタ、フラグ、位置）のFNV-1aのハッシュを、コミットした値と比べる（`crates/sofel-shape/tests/shape_hash.rs`）。CIは、Linux（x86-64）とmacOS（aarch64）の適合テストと同じ一覧に加えて、このテストをネイティブと `wasm32-wasip1` で走らせる。
- サブセット化は、HarfBuzzの `test/subset/` の期待するフォントと、バイト単位で比べる（[ADR 0022](adr/0022-subsetting.md)）。`test/subset/data/tests/*.tests` の104ファイルの1123件を `subset_test_suite.py` と同じ規則で解析し、`run-tests.py` と同じ引数（`--drop-tables+=DSIG,BASE --drop-tables-=sbix` とプロファイルとインスタンス）を `sofel_cli::subset` で解析して `sofel-subset` を呼ぶ。バイト列が異なるときは各表の最初の差の位置を示し、片方だけにある表は位置0と欠けている側を示す（[ADR 0005](adr/0005-conformance-tests.md)）。項目は `<testsのファイル名>/<期待する出力のファイル名>:bytes`（バイト単位の一致）と `:validate`（検証器が `Error` を出さない）で、期待する失敗は `conformance/expected-failures-subset.txt` に理由（`stage:<段階>`、`deferred:instance`、`deferred:iup`）を添えて書く。
  - `test/subset/data/repack_tests/` の7件は、`run-repack-tests.py` と同じ引数（`--unicodes` と `--drop-tables-=GPOS,GSUB,GDEF`）で呼び、成功すること（`repack_tests/<testsのファイル名>:success`）と `:validate` を同じテストで確かめる。このテスト（`tests/harfbuzz_subset.rs`）は `hb-dump` も `ft-dump` も要らないので、CIはLinux（x86-64）とmacOS（aarch64）に加えて `wasm32-wasip1`（wasmtime）でも走らせる。3つの環境で同じ期待する出力と一致することで、出力が環境によらないことを確かめる。
  - 期待する出力の無い組は、`hb-dump --subset` の出力と比べる（`tests/harfbuzz_subset_hb.rs`）。`hb-dump --subset` は `hb-subset` と同じオプションを受け取り、HarfBuzzを実験的なAPI（`HB_EXPERIMENTAL_API`）を有効にしてビルドしたもので `hb_subset_or_fail` の出力を書く。sofelの出力が同じバイト列であるか、両方が失敗すれば一致とする。項目は、`test/subset/data/fonts/` の全フォントでのGolemの使い方（`perf/texts/ja-runs.txt` と `en-paragraph.txt` の文字をcmapで引いたグリフ、グリフID 1、最後のグリフを `--retain-gids` で残す）の `hb-dump/golem/<フォント>:keep` と、fvarを持つフォントですべての軸を既定の値に固定する（`--instance=*=drop`）`hb-dump/golem/<フォント>:pin`（HarfBuzzは軸の無いフォントの固定を拒む）、`repack_tests` の `hb-dump/repack/<testsのファイル名>` である。期待する失敗は `conformance/expected-failures-subset-hb.txt` に理由（`stage:<段階>`）を添えて書く。
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
  - 項目は `<フォント>#<フェイス>:hb:<項目>` で、`num_glyphs`、`h_extents`、`v_extents`、`metrics`、`metrics-fallback`、`h_advance`、`v_advance`、`v_origin`、`extents`、`nominal`（U+10FFFFまでのすべての符号位置の `hb_font_get_nominal_glyph` と `Font::nominal_glyph_raw`）、`names`、`name_utf`（全名前IDと、記録に現れる全言語と言語なしの、UTF-8、16、32。配列の長さは全体＋1と4）、`glyph_names`、`glyph_from_name`。各項目はすべての拡大縮小とすべてのグリフを含む。期待する失敗は `conformance/expected-failures-harfbuzz.txt` に書く。グリフの箱は `HbExtents`（glyf、CFF2、CFF）で、グリフ名はpostとCFFのcharsetで比べる（[ADR 0021](adr/0021-cff-outlines.md)）。同じ名前を2つ以上持つフォントでは、HarfBuzzの結果が整列の実装によるため、`glyph_from_name` は返ったグリフの名前が引いた名前と等しいことだけを確かめる。COLR、CBDT、sbixのカラーのグリフの箱（[ADR 0023](adr/0023-color-glyphs.md)）と、その箱で決まる縦の原点が期待する失敗である。653個のフォントで9128項目を比べ、期待する失敗は33項目である。
  - 基準線（[ADR 0017](adr/0017-base-baselines.md)、`hb-dump --baselines`）の項目は `baseline`（`hb_ot_layout_get_baseline` の有無と値）、`baseline-fallback`（`hb_ot_layout_get_baseline_with_fallback`）、`layout_font_extents`（`hb_ot_layout_get_font_extents` の有無と値）。6つの拡大縮小、横と縦、9つの基準線、文字体系のタグ（`hani`、`latn`、`deva`、`DFLT` と、代わりの値の文字を持つ文字体系の新旧のタグなど34個と、BASEに現れるすべてのタグ）、言語のタグ（`dflt` とBASEに現れるすべてのタグ）の組を比べる。BASEを持つフォントを足すため、HarfBuzzの `test/api/fonts/base*.ttf` も比べる。`baseline` と `layout_font_extents` はすべて一致し、`baseline-fallback` の期待する失敗は、`math` と `hang` の代わりの値が使うグリフの箱をsofelがまだ持たないカラーのグリフのフォントの1項目である（[ADR 0023](adr/0023-color-glyphs.md)。CFFとCFF2の箱は [ADR 0021](adr/0021-cff-outlines.md) の `HbExtents` を使う）。あわせて、653個のフォントとBASEの3個のフォントで11093項目を比べ、期待する失敗は34項目である。
  - 手で組んだ小さなCFFとCFF2のフォント（丸め、制御点、移動だけの経路、空の軸、seac、入れ子の10と11、演算子の上限、各曲線とflex、ヒントマスク、CIDキーのFDSelectとローカルのサブルーチン、CFF2の `blend`、`vsindex`、FDSelectのフォーマット4、グリフの数、CFF2が無効なときのCFF、charsetとencodingとTop DICTの有効性、重複する名前）でも、`hb-dump` とすべての項目が一致することを確かめる（`tests/harfbuzz_cff.rs`）。
  - 丸め（[ADR 0026](adr/0026-harfbuzz-rounding.md)）：`hb-dump --roundf` は標準入力の `float` と `double` の値（ビットの16進）にHarfBuzzの `roundf` を通した値を書き、`hb-dump --mult` は `upem` と拡大縮小の組ごとに `hb_font_t` の `x_mult` を書く。`sofel_geom::float` の `hb_roundf`、`hb_roundf32` は、単体テストの値と決まった種の乱数の値100万個（半分の値とその前後を多く含む）で、`HbScale::x_mult` は `upem` 16、1000、1024、2048、16384と負の値を含む拡大縮小で、完全に一致することを確かめる（`tests/harfbuzz_rounding.rs`）。
- glyfの輪郭は、`ft-dump --outline` の出力と、拡大縮小なし、16画素、48画素で、命令の列、重なりの印、制御の箱が完全に一致することを確かめる（項目 `outline:<SIZE>:<グリフ>`、[ADR 0011](adr/0011-glyf-outlines.md)）。ラスタライズは、`ft-dump --raster` の出力と、12、24、48画素で画像が完全に一致することを確かめる（項目 `raster:<PPEM>:<グリフ>`、[ADR 0012](adr/0012-antialiased-rasterizer.md)）。
- ラスタの参照出力はグリフごとに比較する（[ADR 0037](adr/0037-stream-raster-reference.md)）。参照プロセスが非ゼロで終了した入力は従来どおり除外し、その呼び出しの件数と判定を加えない。終了状態0の空出力、グリフ数や順序の不一致、フェイス末尾の欠落は失敗にする。比較後は件数、問題の詳細、照合した期待する失敗の識別子を保持し、全成功ケースの識別子を蓄積しない。
- 輪郭の参照出力もグリフごとの比較を共用する（[ADR 0040](adr/0040-stream-outline-reference.md)）。拡大縮小なし、16画素、48画素で全命令、overlap、制御箱を照合する。終了状態が非ゼロの呼び出しを除外し、正常終了した空出力、グリフ番号や個数の不一致、フェイス末尾の欠落を失敗にする。
- 輪郭、ラスタライズ（各形式）、埋め込みビットマップ、sfnt以外の形式は、FreeTypeの出力を書き出す `tools/ft-dump/` と比べる。輪郭は完全一致を求める。sfnt以外の形式のテストのフォントは、その形式を実装するまえに、ADRで `testdata.lock` に加える。
- カラーグリフ、SVGとPDF、GPU、WOFF、検証器、展開器の基準は、それぞれのADRで定める。
- Unicodeの文字の性質は、CIで `scripts/fetch-ucd.sh` と生成の道具を走らせ、生成したファイルがコミットしたものと一致することを `git diff --exit-code` で確かめる（[ADR 0019](adr/0019-unicode-character-data.md)）。Vertical_Orientationは、`VerticalOrientation.txt` を生成の道具とは別の読み取りで読み、全文字の値の一致を確かめる。
- Unicodeの文字の性質は、`hb-dump` のファイルを取らない形と比べる（[ADR 0019](adr/0019-unicode-character-data.md)）。期待する失敗の一覧は持たず、1つでも違えば失敗とする。項目は `unicode:<項目>` である。
  - `hb-dump --unicode` は、0〜0x10FFFFと、0x110000、0x7FFFFFFF、0xFFFFFFFFのすべてについて、`hb_unicode_general_category`、`hb_unicode_combining_class`、`hb_unicode_script`、`hb_unicode_mirroring`、`hb_unicode_decompose`、Extended_Pictographic（`_hb_unicode_is_emoji_Extended_Pictographic`）、indicの分類と位置、USEの分類、アラビア文字の結合の型を1文字1行で書く。続けて、2文字の分解の1つ目の文字（と0、ハングルのL、LVの音節）と2つ目の文字（と0、ハングルのV、T）の直積の組について `hb_unicode_compose` を書く。項目は `general_category`、`combining_class`、`script`、`mirroring`、`decompose`、`extended_pictographic`、`compose`、`indic`、`use`、`joining`。
  - `hb-dump --arabic-pua` は、0x0000〜0xFFFFについて `_hb_arabic_pua_simp_map` と `_hb_arabic_pua_trad_map` を書く。`sofel-font` の生成した表のファイルを適合テストのクレートに取りこんで比べる（項目 `arabic_pua_simp`、`arabic_pua_trad`）。
  - `hb-dump --vowel-constraints FILE` は、HarfBuzzの `src/ms-use/IndicShapingInvalidCluster.txt` の列の1つ目の文字の文字体系ごとに、その文字体系の列に現れる文字の長さ1〜3の全列について、`_hb_preprocess_text_vowel_constraints` の後の文字の列を書く。
  - `sofel-shape` の生成した表のファイル（`src/tables/`）を適合テストのクレートに取りこみ、全文字の `indic_categories`、`use_category`、HarfBuzzの `get_joining_type` と同じく補った結合の型（表がXなら、Mn、Me、CfはT、ほかはU）を比べる（項目 `indic`、`use`、`joining`）。`--vowel-constraints` のすべての行は、`vowel_constraint_match` でHarfBuzzの繰り返しをなぞった結果と比べる（項目 `vowel_constraints`）。
  - `hb-dump` が書けないアラビア文字の表示形と合字の表、結合する文字体系の一覧は、HarfBuzzの生成したソース（`src/hb-ot-shaper-arabic-table.hh`、`src/hb-ot-shaper-arabic-joining-list.hh`、文字体系のタグは `src/hb-script-list.h`）を読んで比べる（項目 `arabic_shaping_table`、`arabic_ligature_table`、`arabic_ligature_mark_table`、`arabic_ligature_3_table`、`arabic_joining_scripts`）。
- 文字体系と言語のタグは、`hb-dump --ot-tags`（ファイルを取らない形）と比べる（[ADR 0020](adr/0020-shaping-foundation.md)、`tests/harfbuzz_ot_tags.rs`）。期待する失敗の一覧は持たず、1つでも違えば失敗とする。
  - 文字体系は、HarfBuzzの全符号位置の `hb_unicode_script` の値（`sofel-unicode` の全文字体系と一致することを確かめる）と、`HB_SCRIPT_INVALID`、`Zmth`、`Hrkt` とする。
  - 言語の集まりは、表のすべての言語、曖昧なタグの言語、生成された関数の分岐の言語と、分岐の主たる言語（と `und`、`en`）× なしか分岐のほかの下位タグ × なしか分岐の地域の下位タグ × 私用の部分（なし、`-x-hant-hk-fonipa`、`-u-hk-x-hbsc-latn`）、`test/api/test-ot-tag.c` の言語と境界の例とする。分岐の文字列は、`hb-ot-tag-table.hh` をもう一度取りこみ、4つの比較を記録して失敗する関数に置きかえて走らせ、HarfBuzzのコンパイルされたコードから読む。`tools/gen-ot-tag/` の読んだ分岐と一致することも確かめる。
  - すべての言語と文字体系の組の `hb_ot_tags_from_script_and_language`（上限は3と3）、境界の例の言語と6つの文字体系の上限0〜4の組、現れたすべての文字体系のタグと言語のタグ（と表のタグ、`DFLT`、`dflt`）の `hb_ot_tag_to_script`、`hb_ot_tag_to_language` と、その組の `hb_ot_tags_to_script_and_language` を比べる。約240万の比較である。
- 機能の文字列と直列化は、`hb-dump` と比べる（[ADR 0020](adr/0020-shaping-foundation.md)）。
  - `hb-dump --features` は、タグ（4文字、短い、長い、引用符つき）、前置き（なし、`+`、`-`）、範囲（なし、`[]`、`[:]`、`[3]`、`[3:]`、`[:5]`、`[3:5]`、不正）、値（なし、`=0`、`=2`、` on`、` off`、` 2`、不正）、前後の空白の直積と、境目の例を `hb_feature_from_string` で読み、`hb_feature_to_string` で書く。1万900の文字列のすべてで、`Feature::parse` と `Feature::format` が一致することを確かめる（`tests/harfbuzz_features.rs`。期待する失敗の一覧は持たない）。
  - `hb-dump --serialize FONT TEXTFILE` は、`conformance/serialize-text.txt` の各行をHarfBuzzで形にした生の結果（グリフ、クラスタ、マスク、送り幅、ずれ）と、その直列化をテキストとJSONの全64の指定の組で書き、それぞれをHarfBuzzで読みなおした結果（成否、終わりの位置、項目）を書く。続けて `test/api/test-buffer.c` の読み込みの入力とそのすべての接頭辞を読んだ結果を書く。`hb-dump --deserialize FONT CORPUS` は、与えた文字列の集まりを読んだ結果を書く。テストは生の結果を `GlyphInfo` と `GlyphPosition` に読み（sofelのシェーパーに依らない）、sofelで書いてバイトごとに比べ、すべての文字列をsofelで読んで比べる。集まりは、書き出した文字列の接頭辞、1バイトを除いたものと区切りの文字を各位置に挿したもの、手で書いた例である。フォントは、postの名前を持つglyf（adwaita）、名前の無いglyf（Roboto）、CFF（SourceSansPro）、マークの位置を持つglyf（OpenSans、NotoNastaliqUrdu）とする（`tests/harfbuzz_serialize.rs`）。項目は `<フォント>:<種類>` で、期待する失敗は `conformance/expected-failures-serialize.txt` に書く。
- 適合テストは `cargo test -p sofel-conformance` で走らせる。期待する失敗の一覧に無い失敗と、一覧にあるテストの成功は、ともにテストの失敗とする。
- 直列化の適合テストでは、全変異の byte arena と範囲の索引に必要な容量を一度確保し、索引だけを並べ替えて重複を除く（[ADR 0035](adr/0035-stream-serialization-reference-output.md)）。エスケープした入力を一行ずつファイルへ書き、HarfBuzz の参照出力も再利用する一行のバッファで比較する。入力と出力の全文を重複して保持しない。入力の集合と順序、全5フォントの比較を保つ。読み取りエラー、参照プロセスの失敗、復元結果の順序の違いと比較件数の不足や余分な行はテストの失敗にする。
- `hb-dump --paint` の適合テストは、フェイス内の各塗りの宣言行数だけ本文を読み、グリフごとに比較する。参照プロセスが成功した場合にその呼び出しの比較結果を確定する。既知の24件では参照の成功と選んだグリフのテキスト再現も検査する（[ADR 0043](adr/0043-stream-harfbuzz-paint-records.md)）。
- `--shape` の行比較は、フォント、文章、方向、機能の組ごとに行数を数え、不一致行の順序、入力、両者の出力をカテゴリ判定と診断に渡す。一回の判定で不一致の各フォントをパスごとに一度読み込む。入力・参照・sofelの行数が食い違うと失敗とする（[ADR 0041](adr/0041-compact-shape-comparison-results.md)）。
- ネイティブの適合テストの runner は `scripts/test-conformance.sh [JOBS]` とする（[ADR 0033](adr/0033-ci-latency-and-ligature-iteration.md)）。`JOBS` の既定値と CI の値は2で、正の整数以外は終了コード2となる。Cargo がビルドしたすべてのテストの実行ファイルを最大 `JOBS` 本で実行し、各実行ファイルの libtest のスレッド数は1とする。テスト自身が作るスレッドと子プロセスは変えない。doctest も実行し、ビルドやテストの失敗は非ゼロの終了コードで伝える。
- CI の `conformance` はRustのrelease成果物を、`generators` は生成ツールのビルド成果物をそれぞれキャッシュし、更新の判定は Cargo に任せる（[ADR 0033](adr/0033-ci-latency-and-ligature-iteration.md)）。C の参照ツールと計測用ライブラリは、OS、アーキテクチャ、runner image、コンパイラ、固定した外部ソースとビルド入力が一致するキーだけを復元する。キャッシュの有無によってテストや照合を省かない。
- CI の参照ツールのビルド、適合テストと計測用 C ビルドは、`scripts/measure-process-tree.sh` で起動する（[ADR 0033](adr/0033-ci-latency-and-ligature-iteration.md)）。起動したコマンドと子孫の RSS の合計を採取し、取得の後に0.1秒待って繰り返す。`peak_tree_rss_kib` に最大観測値、`samples` に取得回数を出力する。共有ページも各プロセスで数えるため、物理メモリの使用量や瞬間的な最大値とは限らない。計測に失敗しなければ、起動したコマンドの終了コードをそのまま返す。

生成表の再現検査はLinuxとmacOSで適合テストと並列に実行する（[ADR 0039](adr/0039-generator-ci-parallelism.md)）。

CI の文書だけの変更は、比較起点の `ci/documentation-only.rules` で判定する（[ADR 0032](adr/0032-documentation-only-ci.md)、[ADR 0034](adr/0034-ci-document-input-classification.md)）。
`document` にだけ一致するパスの変更では、Rust、生成器、適合テスト、wasm、fuzz、ベンチマーク、C ABI の七つのビルドとテストのジョブを省く。
`input` に一致するパス、未知のパス、規則の変更では、七つとも実行する。
文書だけでも `adr` は変更禁止の ADR、監査、計測記録と文書を検査する。
`ci-result` は判定と保護の成功、および七つのジョブの結果を確認する。
文書をプログラムの入力にする変更には `input` 規則を追加する。

## 性能の計測

- 比較の相手は、HarfBuzz 15.1.0とFreeType 2.15.0とする（[ADR 0006](adr/0006-performance-measurement.md)）。シェーピング、サブセット化、COLRv1の描画はHarfBuzzと、輪郭、ラスタライズ、埋め込みビットマップ、WOFF2の展開、sfnt以外の形式はFreeTypeと比べる。
- 計測はワークスペースの外の `bench/` で行い、ネイティブとwasm（wasmtime）の両方で計測する。HarfBuzzとFreeTypeは、取得したソースから `bench/build-c.sh` で静的ライブラリにする。wasmにはwasi-sdk（`bench/wasi-sdk.lock` で版とSHA-256を固定）を使う（[ADR 0013](adr/0013-benchmark-harness.md)）。
- 1回の標本を10ミリ秒以上にまとめ、ウォームアップを3つ捨てて30個の標本を取り、中央値と中央絶対偏差を出す。計測の前に、相手と同じ結果を出していることを確かめる。
  - ラスタライズの照合は、すべてのグリフで行う（[ADR 0018](adr/0018-phantom-point-shift.md) で、FreeTypeと同じく輪郭をpp1.xだけずらすようになったため）。Roboto と Comfortaa の[除外なしの照合結果](benchmarks/2026-11-11-unexcluded-phantom-point-check.md)を記録した。
  - HarfBuzzの輪郭は、glyfを分解する命令の列がFreeTypeと違うため、命令の数を照合せず、時間だけを計る。
- FreeTypeはcmapの検証に `setjmp` を使うため、wasmでは例外処理の提案（`-mllvm -wasm-enable-sjlj`、`wasmtime -W exceptions=y`）でビルドし、走らせる。
- 2段階目の項目：文字からグリフ（FreeType、HarfBuzz）、送り幅（FreeType、HarfBuzz）、輪郭（拡大縮小なしと48画素。FreeType、HarfBuzz）、ラスタライズ（12、24、48、96画素。FreeType）。
- 縦書きの項目：全グリフの縦の送り幅と縦の原点（HarfBuzz。[ADR 0016](adr/0016-font-and-vertical-metrics.md)）。
- CFFとCFF2の項目（HarfBuzz。[ADR 0021](adr/0021-cff-outlines.md)）：HarfBuzzの `perf/benchmark-font.cc` のフォント（`SourceSansPro-Regular.otf`、`NotoSerifMyanmar-Regular.otf`、`AdobeVFPrototype.otf` の既定のインスタンス）とCIDキーの `SourceHanSans-Regular_subset.otf` で、全グリフの箱（`HbExtents::glyph_extents` と `hb_font_get_glyph_extents`、拡大縮小なし）、CIDキーでないものの全グリフの名前と名前からグリフ（`GlyphNames` と `hb_font_get_glyph_name`、`hb_font_get_glyph_from_name`）、開くこと（`Font::new` と `HbExtents::new` と箱の1回目の問い合わせ、`hb_face_create` と `hb_font_create` と箱の1回目の問い合わせ）。
- CFFの項目（FreeType。[ADR 0021](adr/0021-cff-outlines.md)、[ADR 0025](adr/0025-cff-charset-index.md)）：輪郭とラスタライズ、CIDキーのフォントの全グリフのCID（`cid` はcharsetの索引を使う `Cff::cid_with`、`cid (no index)` は索引の無い `Cff::cid`、相手は `FT_Get_CID_From_Glyph_Index`）、開くこと（`Cff::new` とcharsetの索引の配列の確保と作成、`FT_New_Memory_Face`）。
- Unicodeの性質：`perf/texts/` のすべての文章の全文字について、General_Category、Canonical_Combining_Class、Script、Bidi_Mirroring_Glyphを引く。相手はHarfBuzzの既定のUnicodeの関数（`hb_unicode_general_category` などの呼び出しを含む）とする（[ADR 0019](adr/0019-unicode-character-data.md)）。
- シェーピングの項目（HarfBuzz。[ADR 0020](adr/0020-shaping-foundation.md)）：「シェーピング」（`shaping <文章>`）は、HarfBuzzの `perf/benchmark-shape.cc` と同じく、文章を改行で終わる行に区切り、行ごとに区間の性質を推定してシェーピングする。sofelは性質の組ごとに計画を前もって作って使いまわし、HarfBuzzは `hb_shape`（計画のキャッシュと `ot` のフォントの関数）を呼ぶ。区間の性質の言語は、sofelがロケールを読まないため、両者とも無し（`HB_LANGUAGE_INVALID`）とする。「最初のシェーピング」（`first shaping <文章>`）は、フォントのバイト列から、フォント（とグリフの箱）と計画を作って1行目をシェーピングするまでで、HarfBuzzはblob、face、font、bufferを作って捨てる。組は `perf/benchmark-shape.cc` のすべての組と、Mplus1p-Regularと `ja-runs.txt`（LTRとTTB）である。計測の前に、全行でグリフID、クラスタ、フラグ、位置が一致することを確かめる。
- 可変位置のシェーピング項目（[ADR 0027](adr/0027-variable-fonts.md)）は、SourceSerifVariable-Romanと `react-dom.txt` に `wght=500` を設定する。既定位置の項目と同じく、全行をHarfBuzzと照合してから時間を測る。結果は [nativeとwasmの計測](benchmarks/2026-11-11-variable-shaping.md) に記録する。
- [ADR 0006](adr/0006-performance-measurement.md) のサブセット化の文字数別の計測は、HarfBuzz 15.1.0 の `perf/benchmark-subset.cc` の静的フォント7個を使う。各フォントを前処理し、文字コード順の先頭10文字からフォント別の上限までを、既定の設定と `RETAIN_GIDS` でそれぞれ残す。件数は両端と、その間の8の累乗とする。フォントの文字数が指定より少ないときは全ての文字を残す。計測には入力集合の作成と出力のハッシュ計算を含む。
- サブセット化の項目（HarfBuzz。[ADR 0022](adr/0022-subsetting.md)）：`Roboto-Regular.ttf`（glyf）、`SourceHanSans-Regular_subset.otf`（CIDキーのCFF）、`NotoNastaliqUrdu-Bold.ttf`（大きなGSUBとGPOS）を、グリフIDと `RETAIN_GIDS`（Golemの使い方）、文字と既定のフラグでサブセット化する。グリフと文字は `hb-dump` の突き合わせと同じく `ja-runs.txt` と `en-paragraph.txt` から作る。sofelは `Input` を作って `subset` を呼び、HarfBuzzは `hb_subset_input_t` を作って `hb_subset_or_fail` と `hb_face_reference_blob` を呼ぶ。どちらも出力のFNV-1aのハッシュを求め、計測の前に一致を確かめる。HarfBuzz は、`bench/build-c.sh` が `harfbuzz-subset.cc` の include の内容と順序を保って最大4個の翻訳単位に分割し、同じフラグでコンパイルしてビルドする（[ADR 0038](adr/0038-partition-benchmark-harfbuzz-units.md)）。native と wasm は並列のまま、ターゲット内のコンパイルは一つずつ実行する。前処理つきの項目は、前処理の段階で足す。
- CIでは、ネイティブとwasmの両方を `--once` で走らせ、落ちないことと結果の照合だけを確かめる。
- 目標は、各項目でsofelの時間が比較の相手以下であること。
- [ADR 0030](adr/0030-variation-evaluation-corrections.md) の HVAR の項目は、RobotoFlex-Variable、SourceSerifVariable-Roman、AdobeVFPrototype の全グリフの送り幅に対応する変化量を `HbF32` で評価する。位置の作成時に `wght=500` を指定し、同じフォント、同じ内容の別コピー、別フォントで作った位置の3通りを計る。別フォントの場合は、この3フォントを順に並べて次のフォントで位置を作り、末尾から先頭に戻る。計測前に、同じ正規化座標のキャッシュを使った値と各グリフの差分を照合する。MVAR を持つ後ろの2フォントでは、各レコードの変化量を静的な既定位置で HarfBuzz と照合し、その位置での問い合わせを計る。表のインデックスを集める時間は含めない。 [計測結果](benchmarks/2026-11-11-item-variation-deltas.md)。
- 追加フォントの代表項目（[ADR 0031](adr/0031-diverse-font-corpus.md)）は13ファイルのフェイス0・既定座標を使う。10文字のcmapと先頭最大128グリフの横advance・輪郭コマンド数、Source Hanの縦advance、STIXのMATH全56定数、3カラー形式、9フォントの短いシェーピング入力を含む。MATH定数は個別にも照合し、COLRv1は固定HarfBuzzの正規化済みpaint列も照合する。シェーピングはscriptを推定し、言語なし・既定featureとする。具体的な入力と計測値は [役割別計測記録](benchmarks/2026-11-11-extra-font-roles.md) に記す。
- 結果は `docs/benchmarks/` に記録し、コミット後は変更しない。

## ライセンス

- sofelはMITとする。FreeTypeのコードを流用する場合はFTLを選び、文書でFreeType Projectを明示する（[ADR 0002](adr/0002-license-and-reuse.md)）。Slugを流用する場合は、READMEにクレジットを載せる。
- 流用元の著作権表示とライセンスは `THIRD_PARTY_NOTICES.md` に載せ、流用したファイルの先頭に流用元を書く。
- UCDから生成した表を含む `sofel-unicode` と `sofel-shape` は、Unicode License v3にも従う（`MIT AND Unicode-3.0`）。

## フォント描画見本

[sofel ADR 0069](adr/0069-font-render-gallery.md) に従い、追加フォント台帳の全フェイスと固定版 HarfBuzz の可変 COLR フォントを描画する。
`sh scripts/render-font-gallery.sh` は各フェイスの単字とシェーピング済みの固定文字列を等倍で一枚の PNG に配置し、入力、ppem、未対応の理由をローカルの HTML から確認できるようにする。
生成器は [sofel ADR 0073](adr/0073-rust-font-gallery.md) に従う Rust の開発用バイナリとし、Unicode スカラー値とグリフ ID、26.6 固定小数点の位置と整数画素、描画結果と登録済みの未対応状態を型で区別する。シェルの入口は Rust のテストを実行してから生成器を起動する。
sbix の単字 19 と合字 26 は固定版 FreeType と全画素を照合し、一覧の各欄に等倍で配置する（[sofel ADR 0072](adr/0072-color-gallery-acceptance.md)）。
CFF の 19・26 と可変 COLRv1 の 92・93 は固定版 HarfBuzz raster と原点・寸法・乗算済み BGRA の全画素を照合する。一覧 PNG の検査では、これらの2書体、sbix、OT-SVG の単字と文字列を個別に比較する。
文書だけの変更を除き、CI はプルリクエストと main への push で HTML と PNG を生成し、検査したコミット SHA 付きのアーティファクトを14日間保存する。
