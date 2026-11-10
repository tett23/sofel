# sofel

HarfBuzz（シェーピング、サブセット化）とFreeType（グリフの読み込み、ヒンティング、ラスタライズ）の役割を1つにまとめた、Rustのフォントエンジン。
フォントの表の読み取りと、その結果から作るキャッシュをシェーピングとラスタライズで共有し、両者より速いことを目指す（[ADR 0001](docs/adr/0001-project-goal-and-scope.md)）。
WOFFとWOFF2、Type1やBDFなどの古い形式、AAT、絵文字（COLR、埋め込みビットマップ、SVGのグリフ）、古いフォントのためのヒンティングとラスタライズ、表の検証、SVGとPDFとGPUへの出力までを範囲とする。

- 高速性を最優先にする。HarfBuzzとFreeTypeを相手に、ネイティブとwasmの両方で計測する（[ADR 0006](docs/adr/0006-performance-measurement.md)）。
- wasm（`wasm32-unknown-unknown`）へのビルドを前提とし、ライブラリは `no_std` で書く（[ADR 0004](docs/adr/0004-wasm-and-no-std.md)）。
- 外部のクレートに依存せず、すべて自分で書く。正しさは、HarfBuzzとFreeTypeの出力と突き合わせて確かめる（[ADR 0005](docs/adr/0005-conformance-tests.md)）。

## 状態

1段階目（表の読み取りと検証の土台）を実装している。
`sofel-read` は、sfntの目録、TTC/OTC、head、maxp/MAXP、hhea/HHEA、hmtx/HMTX、cmap（フォーマット0〜15）を読み、各表の検証器を持つ（[ADR 0007](docs/adr/0007-sfnt-read-foundation.md)）。
範囲と実装の順序は [ADR 0001](docs/adr/0001-project-goal-and-scope.md)、クレートの構成は [ADR 0003](docs/adr/0003-crate-layers.md) を参照。
仕様は [docs/specifications.md](docs/specifications.md) にまとめる。
FreeTypeとHarfBuzzとの性能の比較は [docs/benchmarks/](docs/benchmarks/) に記録する（最新は [2段階目](docs/benchmarks/2026-11-10-stage2.md)）。

| クレート | 役割 |
|---|---|
| [`sofel-geom`](crates/sofel-geom) | FreeTypeと同じ結果を返す固定小数点の演算と、輪郭を受け渡す `Pen` |
| [`sofel-read`](crates/sofel-read) | sfntの表の読み取りと検証。コピーも確保もせず、どの入力でもpanicしない |
| [`sofel-font`](crates/sofel-font) | フォントを一度だけ読み、グリフの数、文字からグリフへの対応、送り幅をHarfBuzzの方針で返す |
| [`sofel-outline`](crates/sofel-outline) | glyfとGLYFの輪郭を、ヒンティングなしでFreeTypeと同じ座標で `Pen` に渡す |
| [`sofel-raster`](crates/sofel-raster) | FreeTypeのsmoothのラスタライザと同じ計算で、8bitのアンチエイリアスの画像を作る |
| [`sofel-cli`](crates/sofel-cli) | 小さな道具（`sofel-validate`、`sofel-render`） |
| [`sofel-conformance`](crates/sofel-conformance) | FreeTypeとの突き合わせのテスト（`cargo test` では走らない） |

### `sofel-render`

1グリフをヒンティングなしで描き、PGM（P5、白地に黒）を標準出力に書く（[ADR 0012](docs/adr/0012-antialiased-rasterizer.md)）。FreeTypeの `FT_Render_Glyph` と同じ画像になる。

```sh
cargo run --release --bin sofel-render -- --ppem 48 --char U+0041 font.ttf > a.pgm
```

### `sofel-validate`

sfntとTTC/OTCの各表を検証する（[ADR 0008](docs/adr/0008-validate-command-and-freetype-comparison.md)）。誤りを1件1行で出し、`--json` ではJSON Linesで出す。

```sh
cargo run --release --bin sofel-validate -- font.ttf
cargo run --release --bin sofel-validate -- --json font.ttf
```

終了コードは、`Error` が無ければ0、あれば1、使い方の誤りか読めないファイルがあれば2。出力の形とJSONのキーは `sofel-validate --help` で示す。

## 開発

クローン後、gitのhookを有効化する。コミット済みのADRは [adr-guard](https://github.com/tett23/adr-guard) で保護しており、手元に adr-guard が要る。

```sh
git config core.hooksPath .githooks
```

設計判断は `docs/adr/` にADRとして記録する。

ツールチェーンは `rust-toolchain.toml` で固定している。テストは次のとおり走らせる。

```sh
cargo test
cargo clippy --workspace --all-targets -- -D warnings
sh scripts/check-deps.sh   # クレートの依存が ADR 0003 の表に沿うか（jq が要る）
sh scripts/run-tests.sh    # シェルのスクリプトのテスト
```

FreeTypeとの突き合わせ（[ADR 0005](docs/adr/0005-conformance-tests.md)、[0008](docs/adr/0008-validate-command-and-freetype-comparison.md)）には、固定したコミットのHarfBuzzとFreeTypeの取得と、`ft-dump` のビルド（Cのコンパイラが要る）が要る。期待する失敗は `conformance/expected-failures-freetype.txt` に理由を添えて書く。

```sh
sh scripts/fetch-testdata.sh
sh tools/ft-dump/build.sh
cargo test --release -p sofel-conformance
```

wasmでは、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テストを確かめる（[ADR 0004](docs/adr/0004-wasm-and-no-std.md)）。単体テストには [wasmtime](https://wasmtime.dev/) が要る。

```sh
cargo build --release --target wasm32-unknown-unknown -p sofel-read -p sofel-geom -p sofel-font -p sofel-outline -p sofel-raster
CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime cargo test --target wasm32-wasip1 -p sofel-read -p sofel-geom -p sofel-font -p sofel-outline -p sofel-raster
```

性能の計測（[ADR 0013](docs/adr/0013-benchmark-harness.md)）は、ワークスペースの外の `bench/` で行う。テストのデータの取得のあと、FreeTypeとHarfBuzzを静的ライブラリにビルドする（C++のコンパイラが要る。wasmでは `bench/fetch-wasi-sdk.sh` がwasi-sdkを取得する）。`--once` は各項目を1回だけ走らせ、結果の照合だけを行う。記録は `docs/benchmarks/` に置く。

```sh
sh bench/build-c.sh
cargo run --release --manifest-path bench/Cargo.toml            # Markdownの表を書く
sh bench/build-c.sh wasm32-wasip1
cargo build --release --manifest-path bench/Cargo.toml --target wasm32-wasip1
wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm
```

ファジングには、nightlyのツールチェーンと [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) が要る。対象は `cmap`、`metrics`、`font`。

```sh
cd fuzz
cargo +nightly fuzz run cmap -- -max_total_time=60
```

## License

MIT（[LICENSE](LICENSE)、[ADR 0002](docs/adr/0002-license-and-reuse.md)）。流用したコードの著作権表示は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) に載せる。

FreeTypeの計算を移植した部分を含むクレート（`sofel-geom` など）は、FreeType License（FTL）にも従う（`MIT AND FTL`）。FTLの求めにより、次の文を載せる。

> Portions of this software are copyright © 2026 The FreeType Project (https://freetype.org). All rights reserved.
