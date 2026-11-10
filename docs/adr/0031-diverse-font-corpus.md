# ADR 0031: ライセンスを確認した多機能フォントを検証に追加する

ステータス: 採択（未実装）

## 文脈

現在の汎用フォント集合はHarfBuzzのsubset用とshape用のフォントから収集する。
`crates/sofel-conformance/src/fonts.rs` と同じ範囲を固定コミットで数えると653ファイルあり、SHA-256で重複を除くと651種類になる。
TTCの各フェイスを別に数えると654フェイスである。
一方、ベンチマークは固定した少数のフォントを使い、色の検証はAPI用とfuzz用も追加するなど、検証ごとに読む範囲が異なる。

既存の小さな回帰テストに加えて、実用サイズのフォントと異なる機能の組み合わせを検証する。
単にファイル数を増やすのでなく、文字体系、輪郭の形式、可変フォント、色、数式、縦書き、コレクションを補完する集合を選ぶ。
テーブルの存在は実行経路を通った証拠にはならないため、フォントと検証ケースを対にして追加する。

[ADR 0002](0002-license-and-reuse.md) と [ADR 0005](0005-conformance-tests.md) の外部テストデータの方針を、本ADRで選んだ配布元にも広げる。
フォントのバイナリは引き続きsofelのリポジトリに同梱しない。

## 選んだ基準

次の基準を順に適用する。
ライセンスを確認できない候補は、機能が多くても採用しない。

1. 配布元の固定版に、当該フォントに適用されるライセンス本文と著作権表示がある。試験での使用、複製、変更を許可するOFL 1.1またはApache-2.0を今回の対象とする。
2. 生成済みのバイナリを固定コミットまたは版とSHA-256で再取得できる。ビルド環境によるフォント自体の差を持ち込まない。
3. 現物のテーブル、軸、scriptタグを確認し、検証目的を具体的に書ける。宣伝文や書体名だけで機能を判断しない。
4. 既存の入力と違う文字体系、形式、軸の相互作用、実用的なサイズ、共有テーブルなどを検証できる。複数の役割を兼ねるものを優先する。
5. 同じ役割では、ファイル数と取得量が少ない候補を優先する。巨大なフォントは、コレクションや大きなグリフ数など小さな入力で代替しにくい役割がある場合に選ぶ。

## 実装すること

### 追加するフォント

次表のテーブルと軸は、2026年11月11日に取得したバイナリのSFNT目録とfvarから確認した。
GSUBのscriptタグも現物から読み取った。
ここでの「確認」は構造の存在の確認であり、sofelとの一致をすでに確認した意味ではない。

| ID | フォント | 確認した主な構造 | 選定理由と検証ケース |
|---|---|---|---|
| arabic | Noto Sans Arabic | glyf、gvar、HVAR、MVAR、avar、GSUB、GPOS、GDEF、wdth/wght | arabの接続形とマーク位置をRTLで比較する。幅と太さを変えて輪郭とメトリクスも比較する |
| devanagari | Noto Sans Devanagari | glyf、gvar、HVAR、avar、GSUBのdeva/dev2、GPOS、GDEF、wdth/wght | Indicの並べ替え、結合、マーク位置を比較する |
| khmer | Noto Sans Khmer | glyf、gvar、HVAR、avar、GSUBのkhmr、GPOS、GDEF、wdth/wght | Khmerの音節と並べ替えを比較する |
| myanmar | Noto Sans Myanmar | glyf、gvar、HVAR、avar、GSUBのmym2、GPOS、GDEF、wdth/wght | Myanmarのkinzi、下付き、マーク位置を比較する |
| balinese | Noto Sans Balinese | glyf、gvar、HVAR、avar、GSUBのbali、GPOS、GDEF、wght | USEのシェーピングを比較する |
| hebrew | Noto Sans Hebrew | glyf、gvar、HVAR、avar、GSUBのhebr、GPOS、GDEF、wdth/wght | RTLのマーク重なりと位置を比較する。Arabicとは別のscript経路を通す |
| source-serif | Source Serif 4 Variable Roman | CFF2、BASE、HVAR、MVAR、avar、GSUB、GPOS、GDEF、wght/opsz | CFF2のblend、サイズ軸、BASEとメトリクスを比較する |
| stix-math | STIX 2 Math 2.0.2 | CFF、MATH、GSUB、GPOS、GDEF、math | MATHの定数、グリフ情報、伸長部品の読み取りと検証、静的CFFの輪郭を調べる |
| source-han | Source Han Sans 2.005R TTC | 45フェイス。先頭と末尾にCFF、BASE、VORG、vhea、vmtx、GSUB、GPOS、GDEF。両フェイスのグリフ数65535 | 大きなCJKコレクションと共有テーブルを読む。全フェイスの選択、縦横メトリクス、縦書きシェーピングを比較する |
| amstelvar | Amstelvar avar2 Roman | avar 2.0、92軸、gvar、HVAR、MVAR、GSUB。avarのaxisIndexMapとVarStoreの両方がある | 多軸の座標変換と軸の相互作用を比較する。alpha版として実用フォントとは区別する |
| color-cff | samples-cff_colr_1.otf | CFF、COLR v1、CPAL、GSUB | CFF輪郭とCOLRペイントの組み合わせを比較する |
| color-sbix | samples-sbix.ttf | sbix、GSUB | ビットマップの取得とメトリクスを比較する |
| color-svg | samples-untouchedsvg.ttf | SVG、GSUB | SVG文書の範囲と取得を検証する。SVGの描画は検証対象に含めない |

追加は13ファイル、TTCを含めて57フェイスを予定する。
加えて、既存HarfBuzzの `test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf` を可変COLRのケースに使う。
このファイルはcolor-fontsの同名ファイルとSHA-256が同じため、新規追加の数に含めない。
現物は53,096バイト、44軸、COLR v1、CPAL、HVAR、gvarを持つ。
SHA-256は `ad575a09d6748aebcb3b90ffd384c64d5c64b5fd9927967e7cd7cc0d70c98d34` である。

Source Han Sansは117,351,964バイトと大きいため、同じ配布のOTF全書体や地域別の全パッケージは追加しない。
一つのTTCから全フェイスを列挙するが、全フェイスの全グリフをすべての座標で描画する積は作らない。

### 固定する取得元とパス

次の表のコミットは短縮せず固定する。
Google Fontsの各フォントのライセンスは、そのフォントと同じディレクトリの `OFL.txt` である。
Source Serifは `LICENSE.md`、STIXは `OFL.txt` を保存する。
いずれもOFL 1.1であり、著作権表示を含む。

| 取得元 | コミット | フォントのパス |
|---|---|---|
| `google/fonts` | `5bb28733cec313b63fbd415b574b5ae35c8aa8b8` | `ofl/notosansarabic/NotoSansArabic[wdth,wght].ttf` |
| 同上 | 同上 | `ofl/notosansdevanagari/NotoSansDevanagari[wdth,wght].ttf` |
| 同上 | 同上 | `ofl/notosanskhmer/NotoSansKhmer[wdth,wght].ttf` |
| 同上 | 同上 | `ofl/notosansmyanmar/NotoSansMyanmar[wdth,wght].ttf` |
| 同上 | 同上 | `ofl/notosansbalinese/NotoSansBalinese[wght].ttf` |
| 同上 | 同上 | `ofl/notosanshebrew/NotoSansHebrew[wdth,wght].ttf` |
| `adobe-fonts/source-serif` | `80d3f8894c09c937bebfa9011247d2e1c79fd6f4` | `VAR/SourceSerif4Variable-Roman.otf` |
| `stipub/stixfonts` | `bea1dc9a3cc2d8b57e412dfd0f448a84382dfe46` | `archive/STIXv2.0.2/OTF/STIX2Math.otf` |
| `googlefonts/amstelvar-avar2` | `8727e9b81b944be8b9b9188a19ba9cf19b867d95` | `Fonts/variable/Amstelvar[...,GRAD,opsz,wdth,wght].ttf` |
| `googlefonts/color-fonts` | `e830e042b0e93fd5b0197a7fe9f0398f6d87f467` | `fonts/samples-cff_colr_1.otf` |
| 同上 | 同上 | `fonts/samples-sbix.ttf` |
| 同上 | 同上 | `fonts/samples-untouchedsvg.ttf` |

Amstelvarのライセンスは同じコミットの `OFL.txt` で、OFL 1.1である。
上のAmstelvarのファイル名の `...` は省略記号ではなく、実際のファイル名の一部である。

color-fontsは同じコミットの `LICENSE` がApache-2.0である。
samplesの元SVGはリポジトリ本体の通常ファイルであり、別のライセンスやNOTICEによる例外は見つからなかった。
READMEはsamplesの生成元をそのSVGと説明しているため、samplesの生成フォントにもリポジトリのApache-2.0が適用されると判断する。
この判断は配布元の構成に基づくもので、各バイナリに個別のライセンス文があるという意味ではない。
別サブモジュールのNotoとTwemojiを生成元とするフォントは、今回の選定に含めない。

Source Han Sansはリリース `2.005R` の `01_SourceHanSans.ttc.zip` から `SourceHanSans.ttc` だけを取り出す。
アーカイブのSHA-256は `a024cf1759494847cd47aae4379bcb3dc530017c709f3f503ee0ed918dd92952` である。
ライセンスはリリースのコミット `6c709ca72d3d7c46ab42ebecc1a26e7d69595a37` の `LICENSE.txt` を使う。
同ファイルはOFL 1.1で、Reserved Font NameとしてSourceを指定する。
SHA-256は `fcac737e761ec63dbfbdce11030a1780161920d80315edba9c8beff1c2bac5a2` である。

| ID | バイト数 | バイナリのSHA-256 |
|---|---:|---|
| arabic | 844676 | `63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e` |
| devanagari | 641944 | `14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e` |
| khmer | 359788 | `f37a8431a0c5d5ed2f81a767417546aca576a81fb7eff9c924d46aecf828f2ca` |
| myanmar | 784832 | `7abbbfbe2514105d7ce94937aee3feb2ba89b73a256c8b77b5866bd9b83e32ec` |
| balinese | 193412 | `ccca9d3492b86930d8e0272d1e06d9e2e11147489b04d9858aa58e766cc30374` |
| hebrew | 112640 | `7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88` |
| source-serif | 1884148 | `867b73c6a954a4a64616906d179f94572a748790a1d022ebeeff07f56ea0221a` |
| stix-math | 808220 | `56cc42241cccc86d6d570134aa918fe8ac714f4b340d9916282fcc53c592d546` |
| source-han | 117351964 | `1382ea173f57af2ef9de32b41ce24d9c33fe69b8f4f2eae7cf05e64a4efd8aef` |
| amstelvar | 1641636 | `86386aa8d3daa442899b2cd6a76bb4e186002b9e9eacbde79986993a0de616d0` |
| color-cff | 2972 | `57761c533abdfec9d0fa8f35a5633209bcc3bf7536011d6836e9c6f8038392c7` |
| color-sbix | 19608 | `0fd0a23379b0e982db8bef5f9a50cf7960a6ee3504778a9a9c039bde4d2f573d` |
| color-svg | 5972 | `ab86aec653bdd9d412e2d6670ba9485fd56a4e89dcd3c034ae43abca921c7e25` |

### データ構造と取得

追加集合は既存の `testdata.lock` と分けた `fonts.lock` で管理する。
一行を一つの取得ファイルとし、タブ区切りで `id`、HTTPS URL、SHA-256、バイト数、保存先相対パスを持つ。
フォントだけでなくライセンスも登録する。
GitHubのraw URLはコミットを含め、リリースアーカイブは版とSHA-256を固定する。
Google Fonts全体や制作ソースの全履歴をCIでcloneしない。

役割は `conformance/fonts.tsv` に分け、タブ区切りでフォントID、取得ファイルID、アーカイブ内メンバー名、展開後パス、SHA-256、ライセンスID、対象機能を記録する。
直接取得するフォントのメンバー名は `-` とし、アーカイブの場合は固定したメンバー名を指定する。
対象機能はコンマ区切りの識別子とし、空欄、未知の識別子、重複IDを拒否する。
IDと相対パスにはタブと改行を許可せず、空行と行頭が `#` のコメント以外は規定の列数を要求する。
メンバー名を固定し、絶対パス、`..`、シンボリックリンクを拒否する。
保存先相対パスにも絶対パスと `..` を許可せず、既存のシンボリックリンクをたどらない。
取得先は `target/testdata/extra-fonts/` とし、検証済みのファイルだけを最終パスに移す。
キャッシュがあってもハッシュと必須ライセンスを確認する。
オフライン指定では一つでも欠けていれば失敗させる。

### ライセンスの扱い

OFLのフォントには、著作権表示とライセンス全文を取得時とキャッシュ保存時に添える。
SourceのようなReserved Font Nameを持つフォントの変更版を、その名称のまま再配布しない。
subset試験の生成物は一時ディレクトリに置き、CIの配布artifactに含めない。
フォント自体をsofelのMITライセンスで配布しない。

Apache-2.0のフォントにはライセンス全文と配布元にある著作権表示を添える。
変更を再配布する場合には変更を明示し、配布元のNOTICEが追加された版に更新する場合はその内容も確認する。
既存可変COLRフォントを利用する際も、上記color-fontsの固定版のライセンスと由来を対応づけて保存する。

READMEには取得コマンドとライセンスの保存場所を記載し、`THIRD_PARTY_NOTICES.md` には各配布元、固定版、著作権表示、ライセンス全文、本文の取得先を記載する。
同じライセンス本文をまとめて載せる場合も、各フォントの著作権表示と対応関係を残す。
sofel本体のライセンスは変えない。

### 検証への組み込み

新しい集合は専用のテストとして追加し、既存HarfBuzzのケースを置き換えない。
各フェイスでテーブルの読み取り、validate、cmap、メトリクスを検証する。
cmapは既定座標で、Unicodeスカラー値の全範囲（0から0x10FFFFのうちサロゲートを除く）のnominal glyph IDをHarfBuzzと比較する。
欠けた文字のglyph IDは0として比較し、variation selectorの組はこの試験には含めない。

輪郭とグリフメトリクスは、同じ最大128グリフの集合を使う。
まずグリフ0、末尾、登録した全座標でケースのシェーピング結果に現れるグリフの和集合を必須集合とする。
必須集合が128を超えた場合はケースの登録時にエラーとする。
グリフ数をN、Kをmin(N, 128)とし、Kが2以上ならi=0からK-1の順に `floor(i * (N - 1) / (K - 1))` を候補にする。
必須集合にない候補をその順に加え、集合の大きさがKになった時点で止める。
Kが1ならグリフ0だけ、Kが0なら空集合とし、比較ツールと同じ空フォントのエラーを確認する。
最後にグリフID順に整列して実行する。

グリフメトリクスは横advance、縦advance、縦origin、extentsをHarfBuzz互換APIとhb-dumpで比較する。
スケールはx/yともunits-per-emとし、[ADR 0016](0016-font-and-vertical-metrics.md) の欠けた値の扱いに従う。
フォント全体の横と縦のascender、descender、line gapも同じ条件で比較する。
FreeType互換APIの輪郭とadvanceは、既存のft-dumpとの比較と同じ非ヒンティング条件を使う。
HarfBuzzとFreeTypeの値を直接同一視せず、各互換APIを対応する比較相手と照合する。

シェーピングのケースはUnicodeコードポイント列、script、language、direction、feature、座標を明記する。
既定featureだけのケースに加え、任意featureの有効と無効を対で比較する。
各シェーピング対象フォントで、そのfeatureまたは座標によってHarfBuzzの結果が実際に変わるケースを少なくとも一つ含め、表があるだけの検証にしない。
可変フォントは既定座標、各軸単独の最小と最大、全軸最小と全軸最大を含める。
数値の丸めと比較方法は [ADR 0027](0027-variable-fonts.md)、[ADR 0029](0029-variation-deltas.md)、[ADR 0030](0030-variation-evaluation-corrections.md) に従う。

未実装の機能は、ケースIDと原因を個別に記録する。
不一致を自動で期待する失敗として登録せず、原因を調べる。
予期しない成功も失敗にし、実装後に除去する。
取得、ハッシュ、ライセンス、フェイス列挙の失敗は機能の未実装として分類できない。

CIでは既存のnative conformanceとwasmの決定性検証にこの集合を接続する。
文書だけの変更では [ADR 0032](0032-documentation-only-ci.md) の実装後の条件に従い取得と実行を省く。
テストデータ不足による黙った省略は認めない。
ベンチマークは各役割の代表ケースを `--once` の正しさの照合に含め、速度の測定結果は新規の `docs/benchmarks/` の記録に保存する。

## 選ばなかった基準と候補

| 見送るもの | 理由 |
|---|---|
| OSに付属する商用フォント、無償ダウンロードだけをうたうフォント | CIへの複製と変更の許諾を確認できることを必須とする。価格をライセンスの代わりにしない |
| ライセンスのないミラーや出所不明のフォント | 元の配布物と許諾の対応を確認できない |
| Noto Naskh Arabic、Noto Serif Devanagari | 現物とOFLは確認したが、初期集合では文字体系と可変機能がNoto Sansの候補と重なる。新しい不具合や別のlookupの検証目的ができた場合に追加する |
| Source Serif 4 Regularの静的OTF | 可変CFF2を優先する。静的CFFはSTIXとSource Hanで補う |
| `STIXTwoMath.input.ttf` | 生成途中の入力を実用配布フォントの代わりに使わず、配布済みOTFを固定する |
| Source Hanの全ウェイト、全地域の別ファイル | TTCと役割が重なり、取得量と実行量が増える |
| Amstelvar avar2のItalicとreferenceの2書体 | Italicの90軸はRomanの92軸と役割が重なる。referenceの2書体はavar 1.0であり、今回のavar 2.0の目的には使わない |
| color-fontsのNotoとTwemoji由来の生成物 | 今回確認したリポジトリ本体のライセンスだけで別サブモジュール由来の許諾を済ませない |
| 同じバイナリを別の取得元から再追加すること | SHA-256が同じ入力を種類数に数えない。既存フォントの検証対象の拡大として扱う |
| 人気、見た目、総グリフ数だけでの選択 | 実行する機能の違いを示せない。巨大さはSource Hanのように具体的な検証目的がある場合だけ採用する |

## 実装しないこと

- 新しいフォントを追加したことだけを理由にsofelの対応機能を増やしたと表記すること。
- フォントの自動更新、システムフォントの走査、Google Fonts全体の取得。
- 外部フォントのバイナリまたはsubsetのsofelリポジトリへの同梱。
- 本ADRで数式組版、SVG描画、未実装のshaperを新しく実装すること。
- 既存コーパス全体のライセンスを、本ADRで遡って確認済みと扱うこと。

## 不変条件

1. **I01**: 登録した全フォントは、固定された取得元、SHA-256、バイト数、対応するライセンスの記録を持つ。
2. **I02**: フォントまたはライセンスの欠落とハッシュ不一致は、オンラインでもオフラインでも検証成功にならない。
3. **I03**: 取得に失敗したファイルは検証済みキャッシュのパスに現れず、アーカイブ展開は指定した保存先の外に書き込まない。
4. **I04**: フォントIDとフェイス番号から検証対象が一意に決まり、同一SHA-256の再登録は新しい種類として数えない。
5. **I05**: 各採用理由には実行するケースまたは構造検証が対応し、未実行の機能を検証済みと報告しない。
6. **I06**: 比較対象と期待する失敗の一覧が同じなら、nativeとwasmで同じケースIDの結果を得る。比較値の一致条件は各機能の既存ADRに従う。

| 境界の入力 | 結果 |
|---|---|
| 空の追加集合 | 取得設定のエラー。追加検証成功としない |
| 一つの有効なフォントとライセンス | 取得と登録が成功 |
| フォントだけ存在 | ライセンス欠落で失敗 |
| 同じSHA-256を二つのIDで登録 | 重複を報告して登録を拒否 |
| TTCの先頭と末尾フェイス | 両方を列挙して実行 |
| TTCのフェイス数と等しい番号 | 範囲外のエラー |
| 輪郭ケース128グリフ、129グリフ | 前者は実行、後者は登録エラー |
| 取得中断、1バイトだけ破損 | 最終パスに採用せず失敗 |
| `../outside` を含むアーカイブメンバー | 展開せず失敗 |
| 未実装の機能が一つ | 原因を確認した当該ケースだけ期待する失敗に分類 |
| 期待する失敗が成功に変わる | 一覧更新を求める失敗 |

## テスト設計

I01からI04は、小さなローカルの取得fixtureと一時ディレクトリを使うテストを先に書く。
ハッシュ不一致、ライセンス不足、重複、空集合、展開先逸脱を実装前に失敗させる。
テスト名には `I01` などの番号を入れる。
実フォントのダウンロードを単体テストの前提にしない。

I05は登録したすべての役割にケースがあることを検査し、比較ツールの出力を記録する。
シェーピングのfeature有無や非既定座標で結果が変わることも比較ツールで確認する。
MATHの構造検証を数式組版の一致と数えず、色の構造検証を描画の一致と数えない。

I06は固定したFreeTypeとHarfBuzzのnative結果と比較し、同じケースをwasmでも実行する。
初回追加PRには、ファイル数、SHA-256の一意数、フェイス数、ケース数、成功数、原因別の期待する失敗数、取得量、実行時間を記載する。
本ADRの提案時点では追加コーパスの実行時間とsofelの一致結果は未測定である。

## トレードオフ

機能を補完するフォントを少数ずつ選ぶため、書体全体の網羅性は保証しない。
一方、役割を持たない類似ファイルを増やさず、失敗の原因を検証ケースに対応させられる。

配布物とライセンスを固定するため、更新時にハッシュと選定理由を再確認する手間が生じる。
その手間を再現性のために受け入れ、フォントが変更されたこととsofelの回帰を区別する。

## 参考文献

- [OFL 1.1公式本文](https://openfontlicense.org/open-font-license-official-text/)。
- [Google Fonts固定版](https://github.com/google/fonts/tree/5bb28733cec313b63fbd415b574b5ae35c8aa8b8)。各候補の `ofl/<family>/OFL.txt`。
- [Source Serif固定版のライセンス](https://github.com/adobe-fonts/source-serif/blob/80d3f8894c09c937bebfa9011247d2e1c79fd6f4/LICENSE.md)。
- [STIX固定版のライセンス](https://github.com/stipub/stixfonts/blob/bea1dc9a3cc2d8b57e412dfd0f448a84382dfe46/OFL.txt)。
- [Amstelvar avar2の固定版](https://github.com/googlefonts/amstelvar-avar2/tree/8727e9b81b944be8b9b9188a19ba9cf19b867d95) の `OFL.txt`、`Fonts/variable/`。
- [color-fontsの固定版](https://github.com/googlefonts/color-fonts/tree/e830e042b0e93fd5b0197a7fe9f0398f6d87f467) の `LICENSE`、`README.md`、`font-srcs/samples/`、`fonts/`。
- [Source Han Sans 2.005R](https://github.com/adobe-fonts/source-han-sans/releases/tag/2.005R) と [対応するライセンス](https://github.com/adobe-fonts/source-han-sans/blob/6c709ca72d3d7c46ab42ebecc1a26e7d69595a37/LICENSE.txt)。
- sofel `crates/sofel-conformance/src/fonts.rs`、`crates/sofel-conformance/src/color.rs`、`bench/src/main.rs`、`bench/src/shape.rs`（`7a6f8c6e41588a0c525e070cfd536158621aa0d2`）。
