# ADR 0003: クレートの層と依存の向き

ステータス: 採択（未実装）→C言語のAPIは [ADR 0015](0015-c-abi.md)、`sofel-unicode` の追加は [ADR 0019](0019-unicode-character-data.md)、`sofel-subset` の `sofel-unicode` への依存は [ADR 0022](0022-subsetting.md)

## 文脈

sofelは、シェーピングとラスタライズを1つの実装にまとめ、表の読み取りの結果を共有する（[ADR 0001](0001-project-goal-and-scope.md)）。
範囲は、フォントの読み取り、検証、シェーピング、輪郭、ヒンティング、ラスタライズ、カラーグリフ、書き出し、サブセット化、SVGとPDFとGPUへの出力に及ぶ。
形式は、sfntのほかにWOFF、WOFF2、Type1、CID、Type42、PFR、BDF、PCF、FNTを扱い、これらをsfntと同じ形で扱えるようにする。
DEFLATE、LZW、bzip2、Brotli、PNG、SVGのように、フォントの外の仕様の実装も含む。
共有の単位と依存の向きを最初に決めないと、これらの都合が1つの型に絡み合う。

HarfBuzzとFreeTypeは、どちらも可変のオブジェクトに遅延の初期化とキャッシュを持たせる。
HarfBuzzは `hb_face_t` に表とアクセラレータを、初回のアクセスで原子的に作って持たせる（`src/hb-machinery.hh`）。
FreeTypeは `FT_Face` と `FT_GlyphSlot` を、グリフを読み込むたびに書きかえる。
どちらも速いが、オブジェクトの状態が呼び出しの順序に依存し、スレッドの間での共有に注意が要る。

FreeTypeは、形式ごとのドライバが同じ `FT_Face` を作り、形式の違いを吸収する（`src/base/ftobjs.c`）。
Type1とCFFのcharstringの解釈とヒンティングは、`src/psaux/` に共通化されている。

## 実装すること

### ワークスペースとクレート

リポジトリ直下をCargoのワークスペースとし、次のクレートを置く。依存は表に書いたものだけとし、循環させない。

フォントに依存しないクレート：

| クレート | 役割 | 依存 |
|---|---|---|
| `sofel-geom` | 固定小数点の数（26.6、16.16、2.14）、輪郭と塗りの命令、それを受け取る `Pen` と `Painter` | なし |
| `sofel-inflate` | DEFLATE、zlib、gzipの展開 | なし |
| `sofel-lzw` | LZW（`.Z`）の展開 | なし |
| `sofel-bzip2` | bzip2の展開 | なし |
| `sofel-brotli` | Brotliの展開 | なし |
| `sofel-png` | PNGの展開 | `sofel-inflate` |
| `sofel-svg` | SVGの文書を解析し、輪郭と塗りの命令に変える。OpenTypeのSVG表に許される範囲を扱う | `sofel-inflate`（gzipで圧縮されたSVG文書のため）、`sofel-geom` |
| `sofel-raster` | 輪郭の塗りつぶし（アンチエイリアス、モノクロ、LCD、SDF）と、塗り（単色、グラデーション）の合成 | `sofel-geom` |
| `sofel-gpu` | 輪郭をGPU向けのデータに変え、シェーダーのソースを提供する | `sofel-geom` |
| `sofel-vector` | 輪郭と塗りの命令を、SVGとPDFに書き出す | `sofel-geom` |

フォントの形式を読むクレート。どれもバイト列の上にビューを重ね、コピーも確保もしない。各形式の検証器も持つ：

| クレート | 役割 | 依存 |
|---|---|---|
| `sofel-read` | sfntの表。TTCとOTC、Macのリソースの形式（dfont）からのsfntの取り出しを含む | なし |
| `sofel-type1` | Type1（PFA、PFB）、Multiple Masters、CIDキーのType1、Type42、AFM、PFM | なし |
| `sofel-pfr` | PFR | なし |
| `sofel-bitmapfont` | BDF、PCF、FNT、FON | なし |

フォントを扱うクレート：

| クレート | 役割 | 依存 |
|---|---|---|
| `sofel-woff` | WOFFとWOFF2をsfntのバイト列に戻す | `sofel-inflate`、`sofel-brotli` |
| `sofel-font` | 形式を判別し、1つのフォントと可変フォントのインスタンス（正規化した座標）を表す。文字からグリフへの対応、メトリクス、軸の正規化など、シェーピングと描画が共有する情報を、形式によらない形で返す | `sofel-read`、`sofel-type1`、`sofel-pfr`、`sofel-bitmapfont`、`sofel-geom` |
| `sofel-shape` | シェーピング（OT、AAT、GSUBとGPOSを持たないフォント） | `sofel-font` |
| `sofel-outline` | グリフの輪郭（glyf、CFF、CFF2、VARC、Type1とCIDのcharstring、PFR）、可変フォントとMultiple Mastersの補間、ヒンティング。輪郭は `Pen` に渡す | `sofel-font`、`sofel-geom` |
| `sofel-bitmap` | ビットマップのグリフ（EBDT、CBDT、sbix、BDF、PCF、FNT） | `sofel-font`、`sofel-png` |
| `sofel-color` | カラーグリフ（COLR、カラーのビットマップ、SVGのグリフ）を、塗りの命令として `Painter` に渡す | `sofel-outline`、`sofel-bitmap`、`sofel-svg` |
| `sofel-write` | sfntの表の書き出しと、オフセットのあふれを解く再配置 | `sofel-read` |
| `sofel-subset` | サブセット化と、可変フォントの固定（sfntだけ） | `sofel-font`、`sofel-write` |
| `sofel-cli` | 小さな道具のバイナリ群（`sofel-shape`、`sofel-render`、`sofel-validate`、`sofel-subset` など） | 上のすべて |

- 輪郭と塗りの受け渡しは、`sofel-geom` の `Pen` と `Painter` で行う。`sofel-raster`、`sofel-gpu`、`sofel-vector` はフォントの型を知らず、`sofel-geom` だけを知る。
- 形式を読むクレートは、構造の読み取りと検証器だけを持つ。charstringの解釈やヒンティングは `sofel-outline` が行い、Type1とCFFで共有する（FreeTypeの `psaux` と同じ分け方）。
- ほかのクレートは、フォントのバイト列を直接読まない。形式を読むクレートのビューを通す。
- 検証器は、確保をせずに、見つけた誤りを呼び出し側の関数に1つずつ渡す（場所と理由）。誤りを集める、表示するのは呼び出し側（`sofel-cli` の `sofel-validate` など）である。
- 表や形式の読み取りを足すときは、同じプルリクエストでその検証器も足す（[ADR 0001](0001-project-goal-and-scope.md)）。
- 圧縮で包んだフォント（WOFF、WOFF2、gzip、LZW、bzip2）の展開は、`sofel-font` に渡すまえに呼び出し側が行う。`sofel-font` は展開済みのバイト列だけを受け取る。`sofel-cli` は包みを判別して自動で展開する。
- 各クレートは、最初に必要になった段階（[ADR 0001](0001-project-goal-and-scope.md) の実装の順序）で作る。空のクレートを先に作らない。クレートを足す・分ける・まとめる場合は、ADRで判断する。

### 状態とキャッシュ

- ライブラリの値は不変とする。内部可変性（`Cell`、`RefCell`、`OnceCell`、原子変数、`Mutex`）を使わない。
- キャッシュは、呼び出し側が持つ明示的な値とする。例えば、シェーピングの計画（HarfBuzzの `hb_shape_plan_t` 相当）は `sofel-shape` が作って返し、呼び出し側が持って再利用する。
- 表の読み取りは、ビューを作るだけなので遅延させない。重い前処理（lookupごとの集合の要約など）は、それを使う計画の値を作るときに行う。
- 共有の情報は `sofel-font` の値として作り、`sofel-shape`、`sofel-outline`、`sofel-bitmap`、`sofel-subset` はそれを借用する。例えば、正規化した座標から求めた送り幅は、シェーピングとラスタライズで同じ値を使う。
- すべての公開の値は `Send` と `Sync` を満たす。並列化は呼び出し側に任せ、ライブラリの中でスレッドを作らない。

### コードの方針

- 関数型のスタイルを優先する。関数は入力から出力を返し、引数を書きかえない。ただし、ラスタライズのセルの蓄積や、シェーピングのバッファの書きかえのように、計測で必要と示せる内側の繰り返しでは、関数の中に閉じた可変の変数を使ってよい。
- ライブラリのクレートは `#![forbid(unsafe_code)]` とする。`unsafe` が速さのために要る場合は、計測の結果（[ADR 0006](0006-performance-measurement.md)）を添えたADRで、範囲を限って許す。
- ライブラリのクレートは、外部のクレートに依存しない（[ADR 0001](0001-project-goal-and-scope.md)）。
- Rustのedition 2024、stableのツールチェーンでビルドする。

### 形式手法

- 検査には、必要に応じて形式手法を導入してよい。例えば、モデル検査器（Kaniなど）で、形式の読み取りが入力によらず範囲外を読まないこと、桁あふれしないことを示す。証明つきの検証器（Verus、Creusotなど）で、展開器や固定小数点の演算の性質を示す。
- 形式手法の道具は、検査のためだけに使い、ライブラリのクレートの依存には入れない。ライブラリのコードに書く注釈（属性やマクロ）が要る場合は、`cfg` で通常のビルドから外す。
- 導入する道具と、示す性質は、対象のクレートのADRのテスト設計に書く。

## 実装しないこと

- C言語のAPI（[ADR 0001](0001-project-goal-and-scope.md)）。
- フォントのファイルの読み込みと、メモリへの割り付け（mmap）。ライブラリは `&[u8]` を受け取るだけとし、ファイルを読むのは `sofel-cli` か呼び出し側である。Type1のAFMのように別のファイルに分かれた情報も、呼び出し側が読んでバイト列で渡す。
- グリフのキャッシュ（ラスタライズ済みのビットマップの保持）。FreeTypeの `ftcache` 相当は、必要になったら別に判断する。
- GPUの装置の管理とAPIの呼び出し（[ADR 0001](0001-project-goal-and-scope.md)）。

## テスト設計

- ワークスペースの構成は、各クレートを作るADRのテストで確かめる。
- 依存の向き：クレートを2つ以上作った段階で、各クレートの `cargo tree` が表の依存に反しないことを確かめるスクリプトを置き、CIで実行する。
- `unsafe` の禁止：`#![forbid(unsafe_code)]` により、コンパイルで確かめる。
- `Send` と `Sync`：公開の主な型について、`fn assert_send_sync<T: Send + Sync>() {}` を呼ぶテストを置く。
- 検証器：表や形式の読み取りを足すプルリクエストで、わざと壊したバイト列について、検証器が期待する場所と理由を報告するテストを置く。
- 形式の判別：`sofel-font` を作るプルリクエストで、各形式の先頭のバイト列から、形式を正しく判別するテストを置く。形式を足すたびに加える。
- 形式手法：導入したクレートのADRに従う。

## トレードオフ

- 内部可変性を使わないため、HarfBuzzのように「初回のアクセスで作って以後は使い回す」ことが、ライブラリの中ではできない。呼び出し側が計画などの値を持つ必要があり、APIの使い方がやや増える。代わりに、値の状態が呼び出しの順序に依存せず、スレッドの間で自由に共有できる。
- クレートが23に分かれる。依存が絡み合わないことと、展開器やラスタライザをフォントの外でも使えることを優先し、数の多さを受け入れる。
- `sofel-font` は、sfntと古い形式の違いを吸収するため、形式ごとの分岐を持つ。sfntだけを扱う場合より抽象が1段増え、sfntの速さを損なわないかを計測で確かめる必要がある。
- 圧縮の展開を呼び出し側に任せるため、ライブラリの利用者は包みの判別と展開を自分で書く必要がある。代わりに、`sofel-font` が展開器に依存せず、展開のためのメモリの確保が利用者に見える。
- 検証器を読み取りと同時に作るため、各段階の作業が増える。代わりに、壊れたフォントや自分の誤りを、早い段階で場所つきで見つけられる。
- `unsafe` を禁止するため、境界の検査が残る。HarfBuzzは一度検査した表を検査なしで読む。性能の差が計測で問題になれば、範囲を限って許すADRを作成する。形式手法で範囲外の読み取りが無いことを示せた箇所は、その候補になる。

## 参考文献

- HarfBuzz `src/hb-machinery.hh`、`src/hb-object.hh`、`src/hb-shape-plan.cc`、`src/hb-gpu.cc`、`src/hb-vector*.cc`、`src/hb-raster*.cc`（15.1.0）
- FreeType `include/freetype/freetype.h`、`src/base/ftobjs.c`、`src/psaux/`、`src/otvalid/`、`src/gxvalid/`（2.15.0）
- Kani Rust Verifier https://github.com/model-checking/kani
- Verus https://github.com/verus-lang/verus
- Creusot https://github.com/creusot-rs/creusot
- Rich Hickey "Simple Made Easy"（Strange Loop 2011） https://www.infoq.com/presentations/Simple-Made-Easy/
