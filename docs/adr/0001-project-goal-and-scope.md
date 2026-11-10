# ADR 0001: HarfBuzzとFreeTypeを統合したフォントエンジンをRustで書く

ステータス: 採択（未実装）→C言語のAPIは [ADR 0015](0015-c-abi.md)

## 文脈

テキストを画面や紙に出すには、文字列をグリフの列と位置に変えるシェーピングと、グリフを指定のサイズで画素に変えるラスタライズが要る。
C/C++の世界では、前者をHarfBuzz、後者をFreeTypeが担う。
HarfBuzzは、ほかにサブセット化、可変フォントの固定（instancing）、ラスタライズ、SVGとPDFへの書き出し、GPUでの描画も持つ（15.0でraster、vector、gpuのライブラリが安定版になった）。

2つのライブラリは、同じフォントの表を別々に読む。
cmap、hmtx、可変フォントの軸の正規化（fvar、avar）、HVARによる送り幅の補正、glyfとCFFのグリフの位置などである。
HarfBuzzはhb-ftでFreeTypeを使い、FreeTypeの自動ヒンタはHarfBuzzを使うため、ビルドにも循環がある（FreeTypeは2.14から、HarfBuzzを実行時にdlopenする方式を用意している）。

Rustでは、HarfBuzzのシェーピング部分の移植であるHarfRustと、FreeTypeの役割を担うfontations（read-fonts、skrifa）がある。
HarfRustのREADMEは、主なフォントでHarfBuzzより25%未満遅いと述べている。

sofelは、2つの役割を1つのRustの実装にまとめ、表の読み取りと、その結果から作るキャッシュをシェーピングとラスタライズで共有する。
共有によって二重の読み取りをなくすことと、HarfBuzzとFreeTypeが使う高速化の手法を最初から取り入れることで、両者以上の速さを目指す。

sofelの主な利用者の1つはGolemである。
GolemはSVGとPDFを出力し、古い環境や古いフォントでもビルドできることを求める。
そのため、旧来のヒンティング、モノクロのラスタライズ、埋め込みビットマップに加え、Type1やBDFなどsfnt以外の古い形式も範囲に含める。
FreeTypeはこれらの形式を、sfntと同じ `FT_Face` の形で扱えるようにしている。
絵文字は、今のテキストでは欠かせないため、最初から範囲に含める。

## 実装すること

### 目標

- HarfBuzzとFreeTypeの機能を、1つのRustの実装で提供する。範囲は次の「対象とする範囲」で定める。
- 高速性を最優先の品質とする。速さの基準と計測の方法は [ADR 0006](0006-performance-measurement.md) で定める。
- 正しさの基準は、HarfBuzzとFreeTypeの出力とする。基準と検査の方法は [ADR 0005](0005-conformance-tests.md) で定める。
- wasmへのビルドを前提とする（[ADR 0004](0004-wasm-and-no-std.md)）。
- すべて自分で書く。HarfBuzz、FreeType、HarfRust、fontationsなどのコードとテストは、ライセンスの条件を満たす範囲で流用してよい（[ADR 0002](0002-license-and-reuse.md)）。ライブラリのクレートは外部のクレートに依存しない。

### 対象とする範囲

最終的に、次をsofelの範囲とする。各項目は、実装に入るまえに個別のADRで設計を定める。

- フォントの形式
  - sfnt（TrueType、OpenType/CFF、CFF2）と、そのコレクション（TTC/OTC）。
  - WOFFとWOFF2の読み込み。展開に要るDEFLATE（zlib）とBrotliの展開器を自分で書く。
  - Macのリソースの形式（dfontと、データフォークに置いたスーツケース）に入ったsfnt。
  - PostScriptの形式：Type1（PFA、PFB）、Type1のMultiple Masters、CIDキーのType1、Type42。Type1のメトリクスを補うAFMとPFM。
  - BitstreamのPFR。
  - ビットマップのフォント：BDF、PCF、WindowsのFNTとFON。
  - gzip、LZW（`.Z`）、bzip2で包んだフォントの展開。X11のPCFなどで使われる。
  - sfnt以外の形式も、sfntと同じ形（文字からグリフへの対応、メトリクス、輪郭またはビットマップ）で扱えるようにする。シェーピングは、GSUBやGPOSを持たないフォントとして行う（AFMのカーニングがあれば使う）。
- 表の読み取り
  - sfntの表の目録、cmap、head、hhea、hmtx、vhea、vmtx、maxp、OS/2、name、post、VORG、STAT、fvar、avar（v1、v2）、gvar、cvar、HVAR、VVAR、MVAR、GDEF、GSUB、GPOS、BASE、MATH、kern、COLR、CPAL、VARC、EBLC、EBDT、CBLC、CBDT、sbix、SVG、gasp、hdmx、fpgm、prep、cvt。
  - AATの表：morx、mort、kerx、ankr、trak、feat、ltag。
  - ISO OFF第5版の24bitのグリフID（65536グリフ超）を含む。
- 表の検証器
  - 各表の構造の誤り（範囲外のオフセット、矛盾する数、不正な値）を、場所と理由つきで報告する。FreeTypeのotvalid、gxvalidに相当する。sfnt以外の形式も同じく検証する。
  - 表や形式の読み取りを足すときは、同じものの検証器も足す。デバッグの手段を開発の初めから持つためである。
- シェーピング
  - HarfBuzzのOTシェーパーと同じ処理。正規化、文字体系ごとのシェーパー（arabic、hangul、hebrew、indic、khmer、myanmar、thai、USEほか）、GSUB、GPOS、フォールバックのマーク配置、クラスタの扱い。
  - AATのシェーピング（morx、kerx、trak）。
- グリフの輪郭
  - glyf（二次曲線、三次曲線）、CFF、CFF2、VARC。可変フォントの補間を含む。
  - Type1とCIDのcharstring、Multiple Mastersの補間、PFRのグリフ。
- ヒンティング
  - TrueTypeバイトコードのインタプリタ。FreeTypeのv35（旧来のヒンティング）とv40の両方。
  - CFF、Type1、CIDのヒンティング（FreeTypeのpsauxとpshinter相当）と、自動ヒンタ。
- ラスタライズ
  - アンチエイリアス（FreeTypeのsmooth相当）、モノクロ（raster相当）、LCD向けのサブピクセル（LCDフィルタとHarmony相当）、SDF（輪郭からと、ビットマップから）。
- カラーグリフと絵文字
  - COLRv0、COLRv1の描画（グラデーションと合成を含む）。
  - 埋め込みビットマップ（EBDT、CBDT、sbix）。PNGの展開器を自分で書く。
  - SVGのグリフ（OpenTypeのSVG表）。OpenType仕様がSVGの表に許す範囲のSVGを描画する。
- フォントの書き出し
  - sfntの表の書き出し（オフセットのあふれを解く再配置を含む）。
  - サブセット化（HarfBuzzの `hb-subset` 相当）。
  - 可変フォントの固定（instancing）。軸の固定と、範囲の絞り込み。
- ベクトルとGPUへの出力
  - グリフの列を、SVGとPDFに書き出す（HarfBuzzの `libharfbuzz-vector` 相当）。PDFへのフォントの埋め込みは、そのADRで定める。
  - GPUでの描画のためのデータとシェーダー（HarfBuzzの `libharfbuzz-gpu` 相当、Slugの方式）。sofelは輪郭をGPU向けのデータに変え、シェーダーのソース（WGSL、GLSL、MSL、HLSL）を提供する。GPUの装置やAPIは呼び出し側が扱う。
- 小さな道具
  - シェーピングの結果をHarfBuzzの `hb-shape` と同じ形で出すコマンド、グリフを画像にするコマンド、フォントを検証するコマンド、サブセット化のコマンドなど。UNIX哲学にもとづき、1つの道具は1つのことをする。

### 実装の順序

依存の少ない層から積む。各段階は、前の段階の検査が通ってから始める。
各段階で、その段階で読む表の検証器も作る。

1. 表の読み取りと検証の土台（sfntの目録、境界の検査、cmap、head、maxp、hhea、hmtx）と、検証のコマンド。
2. glyfの輪郭と、アンチエイリアスのラスタライズ。1グリフを画像にできるところまで。
3. 可変フォント（fvar、avar、gvar、HVAR）。
4. シェーピングの土台（バッファ、クラスタ、正規化、GSUB、GPOS）と、文字体系ごとのシェーパー。
5. カラーグリフと絵文字（COLR、CPAL、CBDT、sbix、EBDT、SVGのグリフ）と、PNGとDEFLATEの展開器。
6. CFF、CFF2。
7. ヒンティング（TrueTypeのv35とv40、CFF、自動ヒンタ）と、モノクロ、LCD、SDFのラスタライズ。
8. 古い形式（Type1、CID、Type42、PFR、BDF、PCF、FNT、dfont）と、そのヒンティング、gzip、LZW、bzip2の展開器。
9. WOFF、WOFF2と、Brotliの展開器。
10. AATのシェーピング。
11. フォントの書き出し、サブセット化、可変フォントの固定。
12. SVGとPDFへの書き出し、GPUでの描画。
13. VARCなど、残りの表。

順序は、利用者の必要に応じて入れかえてよい。入れかえる場合は、依存する段階が先に済んでいることを確かめる。

## 実装しないこと

次は範囲に含めない。必要になったら、範囲を広げるADRを作成する。

- AppleのHVF。FreeTypeもApple自身のlibhvfを呼ぶだけで、形式の仕様が公開されていない。
- sfnt以外の形式の書き出しとサブセット化。サブセット化と書き出しはsfntだけを対象とする。
- WOFFとWOFF2への書き出し（DEFLATEとBrotliの圧縮器）。
- Graphite、wasmシェーパー、CoreText・DirectWrite・Uniscribeなどプラットフォームのシェーパー。
- 行分割、双方向テキストの並べ替え、フォントの選択とフォールバック。HarfBuzzとFreeTypeの範囲の外である。
- GPUの装置の管理とAPIの呼び出し（wgpu、Vulkan、Metalなど）。
- C言語のAPI（HarfBuzzやFreeTypeとの互換のABI）。

## テスト設計

この ADR はプロジェクトの範囲を定めるだけで、コードを生まない。
各段階のテストは、その段階のADRに書く。共通の基準は [ADR 0005](0005-conformance-tests.md) と [ADR 0006](0006-performance-measurement.md) に従う。

## トレードオフ

- 全部を自分で書くため、read-fontsやHarfRustの上に作るより作業量がはるかに大きい。代わりに、表の読み取りの結果とキャッシュの持ち方を、シェーピングとラスタライズの両方に合わせて自由に設計できる。速さを最優先にする以上、ここを他者の設計に委ねない。
- 範囲がHarfBuzzとFreeTypeのほぼ全体に及ぶ。DEFLATE、Brotli、PNG、SVGの描画、PDFの書き出しまで自分で書くため、フォントの外の仕様の実装も多い。実装の順序で、使える状態を早く作り、残りを積み上げる。
- 古いフォントのための機能（v35、モノクロ、埋め込みビットマップ、Type1やBDFなどの形式）は、新しいフォントだけを扱うなら要らない。Golemが古い環境と古いフォントを扱うため、保守の費用を受け入れる。
- sfnt以外の形式もsfntと同じ形で扱うため、フォントを表す層（[ADR 0003](0003-crate-layers.md) の `sofel-font`）が、形式ごとの違いを吸収する必要がある。sfntだけを前提にした設計より抽象が1段増える。
- HarfRustとfontationsは同じ目標の多くをすでに達成している。sofelの存在理由は統合による速さであり、[ADR 0006](0006-performance-measurement.md) の計測で優位を示せない場合は、方針を見直す。
- C言語のAPIを持たないため、既存のHarfBuzzやFreeTypeの利用者がそのまま置きかえることはできない。

## 参考文献

- HarfBuzz 15.1.0（`3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`） https://github.com/harfbuzz/harfbuzz
- FreeType 2.15.0（`VER-2-15-0`、`37515cf56ba3349656049edbed89ffc6f6466f2d`） https://gitlab.freedesktop.org/freetype/freetype
- HarfRust 0.15.0 https://github.com/harfbuzz/harfrust
- fontations（read-fonts 0.46.0、skrifa 0.49.0） https://github.com/googlefonts/fontations
- Slug https://github.com/EricLengyel/Slug
- ISO/IEC 14496-22 Open Font Format
- OpenType Specification https://learn.microsoft.com/en-us/typography/opentype/spec/
- WOFF File Format 1.0 https://www.w3.org/TR/WOFF/
- WOFF File Format 2.0 https://www.w3.org/TR/WOFF2/
- RFC 1950（ZLIB）、RFC 1951（DEFLATE）、RFC 1952（GZIP）、RFC 7932（Brotli）
- Adobe Type 1 Font Format、Adobe Technical Note #5014（CID-Keyed Font）、#5012（Type 42）、#5004（AFM）、#5005（BDF）
- FreeType `src/type1/`、`src/cid/`、`src/type42/`、`src/pfr/`、`src/bdf/`、`src/pcf/`、`src/winfonts/`、`src/base/ftrfork.c`、`src/gzip/`、`src/lzw/`、`src/bzip2/`（2.15.0）
- PNG Specification https://www.w3.org/TR/png/
- Apple TrueType Reference Manual https://developer.apple.com/fonts/TrueType-Reference-Manual/
