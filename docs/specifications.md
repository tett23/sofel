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
- 読める形式と表：sfnt（`0x00010000`、`OTTO`、`true`、`typ1`）、TTC/OTC、head、maxp、MAXP、hhea、HHEA、hmtx、HMTX、cmap（フォーマット0、2、4、6、8、10、12、13、14、15）、loca、LOCA、glyf、GLYF（[ADR 0011](adr/0011-glyf-outlines.md)）。glyfの検証器は、各グリフをlocaの項目から次の項目までとして読み、locaの壊れた箇所はlocaの検証器が報告する。
- 検証器は各表の `validate(data, report)` で、`Diagnostic`（タグ、オフセット、重大度、種類）を1つずつ渡す。重大度は `Error`（ビューを作れない、または読み取りの結果が仕様の意図と異なる）と `Warning`（仕様に反するが読み取りに影響しない）とする。`Error` の無い表は、ビューを作れ、範囲内の読み取りが `None` を返さない。

## 固定小数点と `Pen`（`sofel-geom`）

- `F26Dot6`、`Fixed`（16.16）、`F2Dot14` と、FreeTypeと同じ結果を返す `mul_fix`、`div_fix`、`mul_div`、`mul_div_no_round`、`vector_length`（CORDIC）、`transform` を持つ（[ADR 0009](adr/0009-geom-fixed-point-and-pen.md)）。
- 輪郭は `Pen`（`move_to`、`line_to`、`quad_to`、`cubic_to`、`close`）で受け渡す。命令の並びは `FT_Outline_Decompose` と同じ約束に従う。`Point` の単位（26.6かフォントの単位）は作る側が決める。

## フォント（`sofel-font`）

- `Font::new(data, index)` は、sfntとTTC/OTCのフォントを開き、head、グリフの数、横書きのメトリクス、文字の対応に使うcmapのサブテーブルを一度だけ読む（[ADR 0010](adr/0010-font-foundation.md)）。不変で、キャッシュを持たない。
- グリフの数は `MAXP` を `maxp` より優先する。メトリクスは `HHEA`/`HMTX` を `hhea`/`hmtx` より優先する。
- cmapのサブテーブルは、HarfBuzzと同じ順（(3,0)、(3,10)、(0,6)、(0,4)、(3,1)、(0,3)、(0,2)、(0,1)、(0,0)、(1,0)、(1,任意)）で、読めるものを選ぶ。グリフの数以上のグリフIDは対応なしとする。記号フォントとMacのエンコーディングの付けかえは、まだ行わない。
- 送り幅はHarfBuzzと同じく、表が覆うグリフ（`numberOfHMetrics` と表の長さから決まる。`numberOfHMetrics` がグリフの数より大きい壊れたフォントでは、グリフの数以上のIDも含む）には表の値を、その外には0を返す。hmtxのデータが全く無ければ `unitsPerEm / 2`（`unitsPerEm` が16〜16384の外なら1000とみなす）を返す。hheaが無くhmtxがあるフォントでは、長いメトリクスが0個なので、すべて0になる。
- `ft_left_side_bearing` は、輪郭の `pp1.x` のために、FreeTypeの `tt_face_get_metrics` と同じ読み方で左のサイドベアリングを返す（[ADR 0018](adr/0018-phantom-point-shift.md)）。送り幅（HarfBuzzの方針）とは別の読み方である。`numberOfHMetrics` 未満のグリフIDは長い記録から（記録が表に収まらなければ0）、以上はサイドベアリングの配列から（グリフの数によらず、表の外なら0。最後の送り幅が読めなければ0）引く。hheaかhmtxが無ければ0。表は、`GLYF` があれば `HHEA` と `HMTX`、無ければ `hhea` と `hmtx`（`hhea` が無ければ `HHEA` と `HMTX`）を使う。

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
  - ラスタライズの照合は、すべてのグリフで行う（[ADR 0018](adr/0018-phantom-point-shift.md) で、FreeTypeと同じく輪郭をpp1.xだけずらすようになったため）。
  - HarfBuzzの輪郭は、glyfを分解する命令の列がFreeTypeと違うため、命令の数を照合せず、時間だけを計る。
- FreeTypeはcmapの検証に `setjmp` を使うため、wasmでは例外処理の提案（`-mllvm -wasm-enable-sjlj`、`wasmtime -W exceptions=y`）でビルドし、走らせる。
- 2段階目の項目：文字からグリフ（FreeType、HarfBuzz）、送り幅（FreeType、HarfBuzz）、輪郭（拡大縮小なしと48画素。FreeType、HarfBuzz）、ラスタライズ（12、24、48、96画素。FreeType）。
- CIでは、ネイティブとwasmの両方を `--once` で走らせ、落ちないことと結果の照合だけを確かめる。
- 目標は、各項目でsofelの時間が比較の相手以下であること。
- 結果は `docs/benchmarks/` に記録し、コミット後は変更しない。

## ライセンス

- sofelはMITとする。FreeTypeのコードを流用する場合はFTLを選び、文書でFreeType Projectを明示する（[ADR 0002](adr/0002-license-and-reuse.md)）。Slugを流用する場合は、READMEにクレジットを載せる。
- 流用元の著作権表示とライセンスは `THIRD_PARTY_NOTICES.md` に載せ、流用したファイルの先頭に流用元を書く。
