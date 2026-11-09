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

| クレート | 役割 |
|---|---|
| [`sofel-read`](crates/sofel-read) | sfntの表の読み取りと検証。コピーも確保もせず、どの入力でもpanicしない |

## 開発

クローン後、gitのhookを有効化する。コミット済みのADRは [adr-guard](https://github.com/tett23/adr-guard) で保護しており、手元に adr-guard が要る。

```sh
git config core.hooksPath .githooks
```

設計判断は `docs/adr/` にADRとして記録する。

ツールチェーンは `rust-toolchain.toml` で固定している。テストは次のとおり走らせる。

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

wasmでは、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テストを確かめる（[ADR 0004](docs/adr/0004-wasm-and-no-std.md)）。単体テストには [wasmtime](https://wasmtime.dev/) が要る。

```sh
cargo build --release --target wasm32-unknown-unknown -p sofel-read
CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime cargo test --target wasm32-wasip1 -p sofel-read
```

ファジングには、nightlyのツールチェーンと [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) が要る。対象は `cmap`、`metrics`、`font`。

```sh
cd fuzz
cargo +nightly fuzz run cmap -- -max_total_time=60
```

## License

MIT（[LICENSE](LICENSE)、[ADR 0002](docs/adr/0002-license-and-reuse.md)）。流用したコードの著作権表示は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) に載せる。
