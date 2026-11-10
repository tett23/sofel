# sofel

HarfBuzz（シェーピング、サブセット化）とFreeType（グリフの読み込み、ヒンティング、ラスタライズ）の役割を1つにまとめた、Rustのフォントエンジン。
フォントの表の読み取りと、その結果から作るキャッシュをシェーピングとラスタライズで共有し、両者より速いことを目指す（[ADR 0001](docs/adr/0001-project-goal-and-scope.md)）。
WOFFとWOFF2、Type1やBDFなどの古い形式、AAT、絵文字（COLR、埋め込みビットマップ、SVGのグリフ）、古いフォントのためのヒンティングとラスタライズ、表の検証、SVGとPDFとGPUへの出力までを範囲とする。

- 高速性を最優先にする。HarfBuzzとFreeTypeを相手に、ネイティブとwasmの両方で計測する（[ADR 0006](docs/adr/0006-performance-measurement.md)）。
- wasm（`wasm32-unknown-unknown`）へのビルドを前提とし、ライブラリは `no_std` で書く（[ADR 0004](docs/adr/0004-wasm-and-no-std.md)）。
- 外部のクレートに依存せず、すべて自分で書く。正しさは、HarfBuzzとFreeTypeの出力と突き合わせて確かめる（[ADR 0005](docs/adr/0005-conformance-tests.md)）。

## 状態

2段階目（glyfの輪郭とアンチエイリアスのラスタライズ）まで実装している。
`sofel-read` は、sfntの目録、TTC/OTC、head、maxp/MAXP、hhea/HHEA、hmtx/HMTX、cmap（フォーマット0〜15）、loca、glyf（[ADR 0007](docs/adr/0007-sfnt-read-foundation.md)、[0011](docs/adr/0011-glyf-outlines.md)）と、OS/2、name、post、vhea/VHEA、vmtx/VMTX、VORG（[ADR 0016](docs/adr/0016-font-and-vertical-metrics.md)）、BASE（[ADR 0017](docs/adr/0017-base-baselines.md)）、CFFとCFF2（[ADR 0021](docs/adr/0021-cff-outlines.md)）、GDEF、GSUB、GPOS（[ADR 0020](docs/adr/0020-shaping-foundation.md)）を読み、各表の検証器を持つ。
範囲と実装の順序は [ADR 0001](docs/adr/0001-project-goal-and-scope.md)、クレートの構成は [ADR 0003](docs/adr/0003-crate-layers.md) を参照。
仕様は [docs/specifications.md](docs/specifications.md) にまとめる。
FreeTypeとHarfBuzzとの性能の比較は [docs/benchmarks/](docs/benchmarks/) に記録する（最新は [2段階目](docs/benchmarks/2026-11-10-stage2.md)）。
ライブラリの使い方は [docs/api.md](docs/api.md) にまとめる（各項目の説明はrustdoc）。

| クレート | 役割 |
|---|---|
| [`sofel-geom`](crates/sofel-geom) | FreeTypeと同じ結果を返す固定小数点の演算と、輪郭を受け渡す `Pen` |
| [`sofel-unicode`](crates/sofel-unicode) | Unicode 18.0.0の文字の性質（General_Category、Script、正規分解と合成、Vertical_Orientationなど。[ADR 0019](docs/adr/0019-unicode-character-data.md)） |
| [`sofel-read`](crates/sofel-read) | sfntの表の読み取りと検証。コピーも確保もせず、どの入力でもpanicしない |
| [`sofel-font`](crates/sofel-font) | フォントを一度だけ読み、グリフの数、文字からグリフへの対応、送り幅、フォント全体と縦書きのメトリクス、BASEの基準線とその代わりの値、名前とグリフ名（postとCFFのcharset）をHarfBuzzの方針で返し、HarfBuzzと同じ拡大縮小（`HbScale`）をする。HarfBuzzのCFFとCFF2のアクセラレータ（`HbCff1`、`HbCff2`）を持つ。PostScript名と `FT_Face` の値はFreeTypeの方針 |
| [`sofel-outline`](crates/sofel-outline) | glyfとGLYFの輪郭を、ヒンティングなしでFreeTypeと同じ座標で `Pen` に渡す。HarfBuzzと同じグリフの箱（glyf、CFF2、CFF。`HbExtents`、[ADR 0021](docs/adr/0021-cff-outlines.md)）を求める |
| [`sofel-raster`](crates/sofel-raster) | FreeTypeのsmoothのラスタライザと同じ計算で、8bitのアンチエイリアスの画像を作る |
| [`sofel-cli`](crates/sofel-cli) | 小さな道具（`sofel-validate`、`sofel-render`） |
| [`sofel-capi`](crates/sofel-capi) | C ABI（ヘッダは [`sofel.h`](crates/sofel-capi/include/sofel.h)、[ADR 0015](docs/adr/0015-c-abi.md)） |
| [`sofel-conformance`](crates/sofel-conformance) | FreeTypeとHarfBuzzとの突き合わせのテスト（`cargo test` では走らない） |

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
sh scripts/check-deps.sh   # クレートの依存が ADR 0003 の表に沿い、sofel-capi 以外が unsafe を禁じているか（jq が要る）
sh scripts/run-tests.sh    # シェルのスクリプトのテスト
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p sofel-geom -p sofel-unicode -p sofel-read -p sofel-font -p sofel-outline -p sofel-raster -p sofel-cli -p sofel-capi
```

C ABI（[ADR 0015](docs/adr/0015-c-abi.md)）のテストには、Cのコンパイラと `nm` と jq が要る。Cのテストの期待値 `crates/sofel-capi/tests/c/fixture.h` はRustのAPIから作り、`cargo test` が最新であることを確かめる（`SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test fixture` で書きなおす）。

```sh
sh scripts/test-capi.sh        # ネイティブで走らせ、公開のシンボルとヘッダの一致も確かめる
sh scripts/test-capi.sh tsan   # ThreadSanitizer
sh scripts/test-capi.sh asan   # AddressSanitizer
sh scripts/test-capi.sh wasm   # wasi-sdk で wasm32-wasip1 にビルドし、wasmtime で走らせる
sh scripts/check-capi.sh       # ライブラリの公開の関数がすべてC ABIにあるか、除外の理由があるか
```

ライブラリのクレートは公開の項目すべてにrustdocを書く（`#![warn(missing_docs)]`）。使い方の手引きは [docs/api.md](docs/api.md)。

FreeTypeとの突き合わせ（[ADR 0005](docs/adr/0005-conformance-tests.md)、[0008](docs/adr/0008-validate-command-and-freetype-comparison.md)）には、固定したコミットのHarfBuzzとFreeTypeの取得と、`ft-dump` のビルド（Cのコンパイラが要る）が要る。Vertical_Orientationの照合には、`scripts/fetch-ucd.sh` で取得したUCDのファイルが要る。HarfBuzzとのメトリクスとUnicodeの文字の性質の突き合わせ（[ADR 0016](docs/adr/0016-font-and-vertical-metrics.md)、[ADR 0019](docs/adr/0019-unicode-character-data.md)）には、HarfBuzzの `src/harfbuzz.cc` を取りこんだ `hb-dump` のビルド（C++のコンパイラが要る）が要る。期待する失敗は `conformance/expected-failures-freetype.txt`、`conformance/expected-failures-otvalid.txt`（[ADR 0020](docs/adr/0020-shaping-foundation.md) のotvalidとの照合）と `conformance/expected-failures-harfbuzz.txt` に理由を添えて書く。

```sh
sh scripts/fetch-testdata.sh
sh scripts/fetch-ucd.sh
sh tools/ft-dump/build.sh
sh tools/hb-dump/build.sh
cargo test --release -p sofel-conformance
```

Unicodeの文字の表（`crates/sofel-unicode/src/tables.rs` と `crates/sofel-font/src/tables/arabic_pua.rs`）は生成してコミットする（[ADR 0019](docs/adr/0019-unicode-character-data.md)）。`scripts/fetch-ucd.sh` が `ucd.lock` のUCDのファイルを `target/ucd/` に取得してSHA-256を確かめ、`tools/gen-unicode/` が表を書く。アラビア文字の私用領域の表は、`scripts/fetch-testdata.sh` で取得したHarfBuzzのファイルから作る。CIは作りなおした表とコミットしたものの一致を確かめる。

```sh
sh scripts/fetch-ucd.sh
sh scripts/fetch-testdata.sh
cargo run --release --manifest-path tools/gen-unicode/Cargo.toml
cargo test --manifest-path tools/gen-unicode/Cargo.toml
```

`sofel-shape` の言語のタグの表（`crates/sofel-shape/src/tag_table.rs`）は、`scripts/fetch-testdata.sh` で取得したHarfBuzzの `src/hb-ot-tag-table.hh` から `tools/gen-ot-tag/` が書き、コミットする（[ADR 0020](docs/adr/0020-shaping-foundation.md)）。CIは作りなおした表とコミットしたものの一致を確かめる。

```sh
sh scripts/fetch-testdata.sh
cargo run --release --manifest-path tools/gen-ot-tag/Cargo.toml
cargo test --manifest-path tools/gen-ot-tag/Cargo.toml
```

wasmでは、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テストを確かめる（[ADR 0004](docs/adr/0004-wasm-and-no-std.md)）。単体テストには [wasmtime](https://wasmtime.dev/) が要る。

```sh
cargo build --release --target wasm32-unknown-unknown -p sofel-read -p sofel-geom -p sofel-unicode -p sofel-font -p sofel-outline -p sofel-raster -p sofel-capi
CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime cargo test --target wasm32-wasip1 -p sofel-read -p sofel-geom -p sofel-unicode -p sofel-font -p sofel-outline -p sofel-raster
```

性能の計測（[ADR 0013](docs/adr/0013-benchmark-harness.md)）は、ワークスペースの外の `bench/` で行う。テストのデータの取得のあと、FreeTypeとHarfBuzzを静的ライブラリにビルドする（C++のコンパイラが要る。wasmでは `bench/fetch-wasi-sdk.sh` がwasi-sdkを取得する）。`--once` は各項目を1回だけ走らせ、結果の照合だけを行う。記録は `docs/benchmarks/` に置く。

```sh
sh bench/build-c.sh
cargo run --release --manifest-path bench/Cargo.toml            # Markdownの表を書く
sh bench/build-c.sh wasm32-wasip1
cargo build --release --manifest-path bench/Cargo.toml --target wasm32-wasip1
wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm
```

ファジングには、nightlyのツールチェーンと [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) が要る。対象は `cmap`、`metrics`、`font`、`outline`、`os2`、`name`、`post`、`vorg`、`base`、`gdef`、`gsub`、`gpos`。

```sh
cd fuzz
cargo +nightly fuzz run cmap -- -max_total_time=60
```

## License

MIT（[LICENSE](LICENSE)、[ADR 0002](docs/adr/0002-license-and-reuse.md)）。流用したコードの著作権表示は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) に載せる。

Unicode Character Databaseから生成した表を含む `sofel-unicode` は、Unicode License v3にも従う（`MIT AND Unicode-3.0`）。

FreeTypeの計算を移植した部分を含むクレート（`sofel-geom` など）は、FreeType License（FTL）にも従う（`MIT AND FTL`）。FTLの求めにより、次の文を載せる。

> Portions of this software are copyright © 2026 The FreeType Project (https://freetype.org). All rights reserved.
