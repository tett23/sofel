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

## wasm

- ライブラリのクレートは `#![no_std]`（必要なら `alloc`）とする（[ADR 0004](adr/0004-wasm-and-no-std.md)）。
- CIで、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テスト（wasmtime）を行う。
- JavaScriptからの呼び出しの層は持たない。

## 正しさの検査

- テストのフォントと期待値は同梱せず、`testdata.lock` に書いたコミットのHarfBuzz（15.1.0）とFreeType（2.15.0）を `scripts/fetch-testdata.sh` で取得して使う（[ADR 0002](adr/0002-license-and-reuse.md)、[0005](adr/0005-conformance-tests.md)）。
- シェーピング（AATを含む）は、HarfBuzzの `test/shape/data/*/tests/*.tests` の期待値と比べる。
- サブセット化は、HarfBuzzの `test/subset/` の期待するフォントと、バイト単位で比べる。
- 輪郭、ラスタライズ（各形式）、埋め込みビットマップ、sfnt以外の形式は、FreeTypeの出力を書き出す `tools/ft-dump/` と比べる。輪郭は完全一致を求める。sfnt以外の形式のテストのフォントは、その形式を実装するまえに、ADRで `testdata.lock` に加える。
- カラーグリフ、SVGとPDF、GPU、WOFF、検証器、展開器の基準は、それぞれのADRで定める。
- 適合テストは `cargo test -p sofel-conformance` で走らせる。期待する失敗の一覧に無い失敗と、一覧にあるテストの成功は、ともにテストの失敗とする。

## 性能の計測

- 比較の相手は、HarfBuzz 15.1.0とFreeType 2.15.0とする（[ADR 0006](adr/0006-performance-measurement.md)）。シェーピング、サブセット化、COLRv1の描画はHarfBuzzと、輪郭、ラスタライズ、埋め込みビットマップ、WOFF2の展開、sfnt以外の形式はFreeTypeと比べる。
- 計測はワークスペースの外の `bench/` で行い、ネイティブとwasm（wasmtime）の両方で計測する。
- 目標は、各項目でsofelの時間が比較の相手以下であること。
- 結果は `docs/benchmarks/` に記録し、コミット後は変更しない。

## ライセンス

- sofelはMITとする。FreeTypeのコードを流用する場合はFTLを選び、文書でFreeType Projectを明示する（[ADR 0002](adr/0002-license-and-reuse.md)）。Slugを流用する場合は、READMEにクレジットを載せる。
- 流用元の著作権表示とライセンスは `THIRD_PARTY_NOTICES.md` に載せ、流用したファイルの先頭に流用元を書く。
