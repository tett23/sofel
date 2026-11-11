# ADR 0006: 性能の基準と計測の方法

ステータス: 採択→完了条件は [ADR 0045](0045-foundation-completion-criteria.md)

## 文脈

sofelは高速性を最優先の品質とし、HarfBuzzとFreeTypeの両方より速いことを目指す（[ADR 0001](0001-project-goal-and-scope.md)）。
wasmへのビルドは前提であり（[ADR 0004](0004-wasm-and-no-std.md)）、wasmでの速さもネイティブと同じく目標に含める。

目標を検証するには、同じ機械、同じフォント、同じ入力で、sofelと比較の相手を計測する必要がある。
HarfBuzzは、Google Benchmarkによる計測（`perf/benchmark-shape.cc`、`perf/benchmark-font.cc`）を持ち、フォントと文章の組を `perf/fonts/`、`perf/texts/`、`test/subset/data/fonts/` から読む。
HarfRustは、同じ計測をHarfBuzzとHarfRustの両方で走らせて比べている（HarfRustの `HARFBUZZ.md`）。

## 実装すること

### 比較の相手

- シェーピング、サブセット化、COLRv1の描画：HarfBuzz 15.1.0。フォントの関数は `ot`（HarfBuzz自身の実装）とする。
- グリフの輪郭の取り出し、ラスタライズ、埋め込みビットマップ、WOFF2の展開、古い形式：FreeType 2.15.0。
- SVGとPDFへの書き出しとGPUでの描画の比較の相手は、それぞれのADRで定める。
- 両者とも、[ADR 0005](0005-conformance-tests.md) の `testdata.lock` で取得したソースから、`release` 相当の最適化でビルドする。

### 計測の項目

フォントと文章は、取得したHarfBuzzの `perf/` と `test/subset/data/fonts/` から読む。同梱しない（[ADR 0002](0002-license-and-reuse.md)）。

- シェーピング：HarfBuzzの `perf/benchmark-shape.cc` と同じフォントと文章の組（ラテン、アラビア、ナスタアリーク、デーヴァナーガリー、デュプロワイエ、日本語など）。
- フォントの読み込み：フォントのバイト列から、最初のシェーピングの結果が出るまで（計画の作成を含む、冷えた状態）。
- 輪郭の取り出し：HarfBuzzの `perf/benchmark-font.cc` と同じフォント（可変フォントを含む）の全グリフ。
- ラスタライズ：同じフォントの全グリフを、12、24、48、96 ppemで。ヒンティングの有無の両方。アンチエイリアス、モノクロ、LCD、SDFの各形式。
- カラーグリフ：COLRv1、CBDT、sbixの絵文字のフォントで、よく使う絵文字の描画。比較の相手は、COLRv1はHarfBuzzの `libharfbuzz-raster`、埋め込みビットマップはFreeTypeとする。
- WOFF2の展開：WOFF2のフォントからsfntのバイト列に戻すまで。比較の相手はFreeTypeとする。
- 古い形式：Type1、CID、PFR、BDF、PCF、FNTのフォントの読み込みと、全グリフの輪郭の取り出しまたはビットマップの取り出し。比較の相手はFreeTypeとする。sfntと同じ `sofel-font` を通すことによる、sfntの速さへの影響も、シェーピングと輪郭の項目で確かめる（[ADR 0003](0003-crate-layers.md)）。
- サブセット化：HarfBuzzの `perf/benchmark-subset.cc` と同じフォントと、残す文字の数の組。可変フォントの固定を含む。

各項目は、該当する機能を実装した段階で加える。絵文字のフォントなど、取得したHarfBuzzに無いフォントを使う場合は、`testdata.lock` に加えるADRを作成する（[ADR 0005](0005-conformance-tests.md)）。

### 計測の道具

- ワークスペースの外に、計測のクレート `bench/` を置く。ライブラリのクレートの依存には入れない。
- 計測の枠組み（反復、ウォームアップ、中央値と中央絶対偏差の算出、結果の出力）は自分で書く。Criterionなどには依存しない。
- HarfBuzzとFreeTypeは、手で書いた最小限の `extern "C"` の宣言で呼ぶ。bindgenなどの生成器には依存しない。
- 同じ入力について、sofel、HarfBuzz、FreeTypeの時間と、比（sofel ÷ 相手）を出力する。

### wasmでの計測

- sofelを `wasm32-wasip1` に、HarfBuzzとFreeTypeをwasi-sdkで `wasm32-wasip1` にビルドし、同じ項目をwasmtimeで計測する。
- ブラウザでの計測は行わない。wasmtimeの数値をwasmの基準とする。

### 記録

- 計測の結果を `docs/benchmarks/<YYYY-MM-DD>-<主題>.md` に記録する。adr-guardにより、コミット後は変更できない。
- 記録には、機械（CPU、OS、メモリ）、ツールチェーンの版、sofelのコミット、比較の相手の版、計測の項目ごとの中央値と比を含める。
- 速さのための変更（`unsafe` やSIMDの導入を含む。[ADR 0003](0003-crate-layers.md)、[0004](0004-wasm-and-no-std.md)）は、変更前と変更後の計測の記録を添える。

### 目標

- 各項目で、sofelの時間がHarfBuzzまたはFreeTypeの時間以下であること（比が1.0以下）を目標とする。ネイティブとwasmの両方で同じ目標とする。
- 目標に届かない項目は、記録に残し、改善の対象とする。目標を満たさないことを理由に、機能の実装を止めない。

## 実装しないこと

- CIでの性能の計測と、退行による失敗。CIの機械は計測の揺れが大きいためである。CIでは、`bench/` がビルドできることと、各項目を1回ずつ走らせて落ちないことだけを確かめる。
- HarfRustとskrifaとの比較。比較の相手は、元のHarfBuzzとFreeTypeとする。必要になったら加える。
- メモリの使用量の計測。必要になったら別に判断する。
- ブラウザでの計測。

## テスト設計

- 計測の枠組みの統計：既知の数列について、中央値と中央絶対偏差が正しいことを単体テストで確かめる。
- 比較の相手の呼び出し：HarfBuzzで1つの文章をシェーピングし、グリフの数が `hb-shape` の出力と一致することを確かめるテストを置く。FreeTypeでも、1グリフの輪郭の点の数を確かめる。
- wasmでの計測：手元でwasmtimeで1項目を走らせ、ネイティブと同じ形の結果が出ることを一度確かめる。
- 最初の記録：`bench/` を作るプルリクエストで、最初の記録を `docs/benchmarks/` に置く。

## トレードオフ

- CIで性能の退行を止めないため、退行は手元の計測で見つける必要がある。速さを最優先にするのに自動の歯止めが無いのは弱いが、揺れの大きい計測で誤った失敗を出すより、記録を残す運用を優先する。
- HarfBuzzとFreeTypeをwasi-sdkでビルドする手間がかかる。代わりに、wasmでも同じ土俵で比べられる。
- 計測の枠組みを自分で書くため、Criterionの外れ値の検出や統計の検定は使えない。中央値と中央絶対偏差で足りるかは、最初の記録で確かめる。

## 参考文献

- HarfBuzz `perf/README.md`、`perf/benchmark-shape.cc`、`perf/benchmark-font.cc`、`perf/benchmark-subset.cc`、`perf/texts/COPYING`（15.1.0）
- HarfRust `HARFBUZZ.md`、`README.md`（0.15.0）
- wasi-sdk https://github.com/WebAssembly/wasi-sdk
- Wasmtime https://wasmtime.dev/
