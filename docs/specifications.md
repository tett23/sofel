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
- 行分割、双方向テキスト、フォントの選択、C言語のAPIは扱わない。

## 構成

Cargoのワークスペースで、依存は次のとおりとする（[ADR 0003](adr/0003-crate-layers.md)）。矢印の先が依存先である。

```
フォントに依存しないクレート
  sofel-geom
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
  sofel-shape   → sofel-font
  sofel-outline → sofel-font, sofel-geom
  sofel-bitmap  → sofel-font, sofel-png
  sofel-color   → sofel-outline, sofel-bitmap, sofel-svg
  sofel-write   → sofel-read
  sofel-subset  → sofel-font, sofel-write
  sofel-cli     → 上のすべて（道具のバイナリ群）
```

- 輪郭と塗りは、`sofel-geom` の `Pen` と `Painter` で受け渡す。`sofel-raster`、`sofel-gpu`、`sofel-vector` はフォントの型を知らない。
- 形式のビューと検証器は、形式を読むクレートだけに置く。検証器は確保をせず、誤りを呼び出し側の関数に渡す。charstringの解釈とヒンティングは `sofel-outline` が行い、Type1とCFFで共有する。
- `sofel-font` が形式を判別し、形式によらない形でフォントを表す。圧縮で包んだフォントの展開は、`sofel-font` に渡すまえに呼び出し側が行う（`sofel-cli` は自動で展開する）。
- ライブラリの値は不変で、内部可変性を使わない。キャッシュ（シェーピングの計画など）は、呼び出し側が持つ値とする。
- 公開の値はすべて `Send` と `Sync` を満たす。ライブラリの中でスレッドを作らない。
- ライブラリのクレートは `#![forbid(unsafe_code)]` とし、外部のクレートに依存しない。
- ライブラリは `&[u8]` を受け取るだけで、ファイルを読まない。
- 検査には、必要に応じて形式手法（Kani、Verus、Creusotなど）を使ってよい。道具はライブラリの依存に入れない。

## 表の読み取り（`sofel-read`）

- 基本の型は `Tag`、`GlyphId`（`u32`。24bitのグリフIDを扱う）、`Fixed`、`F2Dot14`、`LongDateTime` とし、生の値だけを持つ（[ADR 0007](adr/0007-sfnt-read-foundation.md)）。
- 表のビューは、作るときにヘッダと配列の範囲を一度だけ検査する。作れないほど壊れていれば `Err(ReadError)` を返す。作れたビューの読み取りは、範囲外の入力に `None` を返し、既定の値で埋めない。既定の値の方針は `sofel-font` で決める。
- どの入力でもpanicしない。clippyの `indexing_slicing`、`arithmetic_side_effects` などを拒否にして強制し、ファジングで確かめる。
- 読める形式と表：sfnt（`0x00010000`、`OTTO`、`true`、`typ1`）、TTC/OTC、head、maxp、MAXP、hhea、HHEA、hmtx、HMTX、cmap（フォーマット0、2、4、6、8、10、12、13、14、15）。
- 検証器は各表の `validate(data, report)` で、`Diagnostic`（タグ、オフセット、重大度、種類）を1つずつ渡す。重大度は `Error`（ビューを作れない、または読み取りの結果が仕様の意図と異なる）と `Warning`（仕様に反するが読み取りに影響しない）とする。`Error` の無い表は、ビューを作れ、範囲内の読み取りが `None` を返さない。

## 固定小数点と `Pen`（`sofel-geom`）

- `F26Dot6`、`Fixed`（16.16）、`F2Dot14` と、FreeTypeと同じ結果を返す `mul_fix`、`div_fix`、`mul_div`、`mul_div_no_round`、`vector_length`（CORDIC）、`transform` を持つ（[ADR 0009](adr/0009-geom-fixed-point-and-pen.md)）。
- 輪郭は `Pen`（`move_to`、`line_to`、`quad_to`、`cubic_to`、`close`）で受け渡す。命令の並びは `FT_Outline_Decompose` と同じ約束に従う。`Point` の単位（26.6かフォントの単位）は作る側が決める。

## フォント（`sofel-font`）

- `Font::new(data, index)` は、sfntとTTC/OTCのフォントを開き、head、グリフの数、横書きのメトリクス、文字の対応に使うcmapのサブテーブルを一度だけ読む（[ADR 0010](adr/0010-font-foundation.md)）。不変で、キャッシュを持たない。
- グリフの数は `MAXP` を `maxp` より優先する。メトリクスは `HHEA`/`HMTX` を `hhea`/`hmtx` より優先する。
- cmapのサブテーブルは、HarfBuzzと同じ順（(3,0)、(3,10)、(0,6)、(0,4)、(3,1)、(0,3)、(0,2)、(0,1)、(0,0)、(1,0)、(1,任意)）で、読めるものを選ぶ。グリフの数以上のグリフIDは対応なしとする。記号フォントとMacのエンコーディングの付けかえは、まだ行わない。
- 送り幅はHarfBuzzと同じく、グリフの数以上なら0、メトリクスが無ければ `unitsPerEm / 2` とする。

## 輪郭（`sofel-outline`）

- glyfと `GLYF` の単純なグリフと合成グリフの輪郭を、ヒンティングをせずに、FreeTypeと同じ座標で `Pen` に渡す（[ADR 0011](adr/0011-glyf-outlines.md)）。`GLYF` があれば `GLYF` と `LOCA` を使い、そのときだけフラグ0x80を三次曲線、0x2000を24bitの成分のグリフIDとして扱う。
- 倍率は `FT_Set_Pixel_Sizes` と同じく `div_fix(ppem << 6, unitsPerEm)` で、点は `mul_fix` で拡大縮小し、丸めない。locaの解釈と合成の位置の計算はFreeTypeに合わせる。
- 作業領域（`Scratch`）は呼び出し側が持つ。

## ラスタライズ（`sofel-raster`）

- 8bitのアンチエイリアスの画像を、FreeTypeのsmoothのラスタライザと同じ数値の計算で作る（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。セルは帯ごとの密な配列（上限64KiB）に積む。重なりの印のある輪郭は4×4の副標本で描く。FreeTypeとの差は0（完全一致）とする。

## 道具

- `sofel-render [--face N] --ppem P (--glyph G | --char U+XXXX) FILE`：1グリフをヒンティングなしで描き、PGM（P5）を標準出力に書く。位置は注釈の行 `# left <x> top <y>` に書く（[ADR 0012](adr/0012-antialiased-rasterizer.md)）。
- `sofel-validate <ファイル>...`：sfntとTTC/OTCの各表を検証する（[ADR 0008](adr/0008-validate-command-and-freetype-comparison.md)）。1件を1行で `<パス>[#<フォント番号>]: <タグ>+0x<オフセット>: <error|warning>: <説明>` の形で出す。`--json` では、1件を1行のJSONのオブジェクト（JSON Lines）で出す。キーは `path`、`font`、`table`、`offset`、`severity`、`kind`、`message`、`details`。`kind` と `details` のキーは出力の約束の一部とする。終了コードは、`Error` が無ければ0、あれば1、使い方の誤りか読めないファイルがあれば2で、`--json` の有無で変わらない。

## wasm

- ライブラリのクレートは `#![no_std]`（必要なら `alloc`）とする（[ADR 0004](adr/0004-wasm-and-no-std.md)）。
- CIで、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テスト（wasmtime）を行う。ツールチェーンは `rust-toolchain.toml` でstableの版を固定する（[ADR 0007](adr/0007-sfnt-read-foundation.md)）。
- JavaScriptからの呼び出しの層は持たない。

## 正しさの検査

- テストのフォントと期待値は同梱せず、`testdata.lock` に書いたコミットのHarfBuzz（15.1.0）とFreeType（2.15.0）を `scripts/fetch-testdata.sh` で取得して使う（[ADR 0002](adr/0002-license-and-reuse.md)、[0005](adr/0005-conformance-tests.md)）。
- シェーピング（AATを含む）は、HarfBuzzの `test/shape/data/*/tests/*.tests` の期待値と比べる。
- サブセット化は、HarfBuzzの `test/subset/` の期待するフォントと、バイト単位で比べる。
- head、hhea、maxp、cmap、hmtxの送り幅は、`tools/ft-dump/` の出力と比べる。FreeTypeが捨てたcmapのサブテーブルは、`sofel-read` の検証器が `Error` を報告しなければならない（[ADR 0008](adr/0008-validate-command-and-freetype-comparison.md)）。`ft-dump` は自分で書いたCのプログラムで、取得したFreeTypeのソースから `tools/ft-dump/build.sh` でビルドし、FreeTypeの公開のAPIだけを使う。`ft-dump` の正しさは、値の分かっている小さなフォントで自動のテストにより確かめる。
  - 項目は `<フォント>#<フェイス>:<項目>` で識別する。項目は、`open`、`head`、`hhea`、`maxp`、`cmap:<プラットフォーム>:<エンコーディング>[:<n>]`、`cmap-validate:<プラットフォーム>:<エンコーディング>[:<n>]`、`hmtx`。
  - `open` は「FreeTypeが開けるフェイスは、sofelも表の目録を読める」の一方向だけを確かめる。FreeTypeは描けるフェイスだけを開く（glyfかCFFが要る）ため、逆向きは比べない。FreeTypeが開けないフェイスは、ほかの項目も比べない。
  - `maxp` は、FreeTypeが読み込みの時点で行う調整（`maxFunctionDefs` を64以上に、`maxTwilightPoints` を0xFFFB以下に。`src/sfnt/ttload.c`）を、sofelの生の値に当ててから比べる。
  - cmapの対応は、FreeTypeの `FT_Get_Next_Char` と同じく、グリフIDがFreeTypeの `num_glyphs` 未満のものだけを比べる。cmap表に無いプラットフォーム7（Adobe）のエンコーディングは、FreeTypeがPostScriptのグリフ名から合成したものなので比べない。
  - HarfBuzz 15.1.0のテストのフォント653個で、4569項目がすべて一致している（期待する失敗は無い）。
- テストのデータは、CIでキャッシュが当たったとき `SOFEL_TESTDATA_OFFLINE=1` で取得のスクリプトを走らせ、キャッシュの中身が欠けていれば失敗させる。シェルのスクリプトのテストは `scripts/run-tests.sh` で走らせる。
- glyfの輪郭は、`ft-dump --outline` の出力と、拡大縮小なし、16画素、48画素で、命令の列、重なりの印、制御の箱が完全に一致することを確かめる（項目 `outline:<SIZE>:<グリフ>`、[ADR 0011](adr/0011-glyf-outlines.md)）。ラスタライズは、`ft-dump --raster` の出力と、12、24、48画素で画像が完全に一致することを確かめる（項目 `raster:<PPEM>:<グリフ>`、[ADR 0012](adr/0012-antialiased-rasterizer.md)）。
- 輪郭、ラスタライズ（各形式）、埋め込みビットマップ、sfnt以外の形式は、FreeTypeの出力を書き出す `tools/ft-dump/` と比べる。輪郭は完全一致を求める。sfnt以外の形式のテストのフォントは、その形式を実装するまえに、ADRで `testdata.lock` に加える。
- カラーグリフ、SVGとPDF、GPU、WOFF、検証器、展開器の基準は、それぞれのADRで定める。
- 適合テストは `cargo test -p sofel-conformance` で走らせる。期待する失敗の一覧に無い失敗と、一覧にあるテストの成功は、ともにテストの失敗とする。

## 性能の計測

- 比較の相手は、HarfBuzz 15.1.0とFreeType 2.15.0とする（[ADR 0006](adr/0006-performance-measurement.md)）。シェーピング、サブセット化、COLRv1の描画はHarfBuzzと、輪郭、ラスタライズ、埋め込みビットマップ、WOFF2の展開、sfnt以外の形式はFreeTypeと比べる。
- 計測はワークスペースの外の `bench/` で行い、ネイティブとwasm（wasmtime）の両方で計測する。HarfBuzzとFreeTypeは、取得したソースから `bench/build-c.sh` で静的ライブラリにする。wasmにはwasi-sdk（`bench/wasi-sdk.lock` で版とSHA-256を固定）を使う（[ADR 0013](adr/0013-benchmark-harness.md)）。
- 1回の標本を10ミリ秒以上にまとめ、ウォームアップを3つ捨てて30個の標本を取り、中央値と中央絶対偏差を出す。計測の前に、相手と同じ結果を出していることを確かめる。
- 2段階目の項目：文字からグリフ（FreeType、HarfBuzz）、送り幅（FreeType、HarfBuzz）、輪郭（拡大縮小なしと48画素。FreeType、HarfBuzz）、ラスタライズ（12、24、48、96画素。FreeType）。
- CIでは、ネイティブとwasmの両方を `--once` で走らせ、落ちないことと結果の照合だけを確かめる。
- 目標は、各項目でsofelの時間が比較の相手以下であること。
- 結果は `docs/benchmarks/` に記録し、コミット後は変更しない。

## ライセンス

- sofelはMITとする。FreeTypeのコードを流用する場合はFTLを選び、文書でFreeType Projectを明示する（[ADR 0002](adr/0002-license-and-reuse.md)）。Slugを流用する場合は、READMEにクレジットを載せる。
- 流用元の著作権表示とライセンスは `THIRD_PARTY_NOTICES.md` に載せ、流用したファイルの先頭に流用元を書く。
