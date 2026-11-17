# ADR 0081: クレートの置き場所を [ADR 0003](0003-crate-layers.md) の表に戻し、WOFF の依存を足す

Status: accepted-pending

## 文脈

[ADR 0003](0003-crate-layers.md) は、クレートとその依存を表で決めた。
依存は表に書いたものだけとし、クレートを足す、分ける、まとめるときは ADR で判断する。
ファイルを読むのは `sofel-cli` か呼び出し側であり、小さな道具のバイナリは `sofel-cli` に置く。

2026-11-17 の ADR 違反の監査（指摘 M1）は、実装がこの表と四か所で食い違うことを見つけた。
一つ目は、`sofel-woff` が表にない `sofel-read` と `sofel-write` に依存することである。
二つ目は、LZW と bzip2 の展開が、表の `sofel-lzw` と `sofel-bzip2` ではなく、`sofel-cli` の中にあることである。
三つ目は、PFR の読み取りが表の `sofel-pfr` ではなく `sofel-read` に、輪郭が `sofel-outline` ではなく `sofel-font` にあることである。
四つ目は、ライブラリのクレートである `sofel-woff` がバイナリを持ち、ファイルを読むことである。

一つ目は、表のほうが実装に追いついていない。
[ADR 0070](0070-webfont-encoding.md) は、`sofel-woff` に sfnt を WOFF と WOFF2 に書き出す API を足した。
書き出しには、sfnt の表の目録を読み、チェックサムを計算し、sfnt を組み立てる処理が要る。
この処理は `sofel-read` と `sofel-write` にすでにある。

残りの三つは、実装のほうが表から外れている。
LZW と bzip2 の展開が `sofel-cli` の中にあると、wasm や Golem のようなライブラリの利用者は、`.Z` と `.bz2` のフォントを展開できない。
PFR だけが、ほかの形式（Type1 の `sofel-type1` と `sofel-outline`、ビットマップの形式の `sofel-bitmapfont`）と違う分け方になっている。

## 調査した事項

次の事実は、origin/main（`aba0d646`）で確かめた。

| 事項 | 根拠 |
| --- | --- |
| `sofel-woff` の依存は `sofel-brotli`、`sofel-inflate`、`sofel-read`、`sofel-write` | `crates/sofel-woff/Cargo.toml:10-14`、`scripts/deps.txt:18-21` |
| `sofel-woff` は `sofel_write::build_sfnt` と `sofel_write::checksum`、`sofel_read` の sfnt の目録と `loca` を使う | `crates/sofel-woff/src/woff.rs:55`、`src/lib.rs:7`、`src/lib.rs:11`、`src/woff2/read.rs:240`、`:413` |
| LZW の展開は `crates/sofel-cli/src/font_input/lzw.rs`（122行）、bzip2 の展開は `crates/sofel-cli/src/font_input/bzip2.rs`（298行）にある | `wc -l` |
| `sofel-lzw`、`sofel-bzip2`、`sofel-pfr` のクレートは無い | `ls crates` |
| PFR の読み取りと検証の位置は `crates/sofel-read/src/pfr.rs`（380行）にある | `wc -l` |
| PFR の輪郭（`PfrFace::draw`）と埋め込みビットマップ（`PfrFace::bitmap`）は `crates/sofel-font/src/pfr.rs`（418行）にある | `crates/sofel-font/src/pfr.rs:76`、`:296` |
| PFR を使うのは `sofel-font`（`Font::from_pfr`）、`sofel-cli` の `validate.rs`、`glyph.rs`、`render.rs` | `rg 'pfr::|from_pfr|PfrFace' crates` |
| `sofel-woff` は `[[bin]]` を持ち、`src/main.rs`（64行）が `std::fs` でファイルを読む | `crates/sofel-woff/Cargo.toml:16`、`src/main.rs:3-7` |
| `scripts/deps.txt` は、無いクレート `sofel-pfr` への依存を許している | `scripts/deps.txt:24` |

## 実装すること

### [ADR 0003](0003-crate-layers.md) の表を直す

- `sofel-woff` の役割を「WOFF と WOFF2 を sfnt のバイト列に戻し、sfnt を WOFF と WOFF2 に書き出す」とする。
- `sofel-woff` の依存を `sofel-inflate`、`sofel-brotli`、`sofel-read`、`sofel-write` とする。
- 依存は循環しない。`sofel-write` の依存は `sofel-read` だけであり、`sofel-read` は依存を持たない。

### 実装を [ADR 0003](0003-crate-layers.md) の表に合わせる

- `sofel-lzw` と `sofel-bzip2` のクレートを作り、`sofel-cli` の `font_input/lzw.rs` と `font_input/bzip2.rs` の展開をそれぞれに移す。どちらも `#![no_std]`（必要なら `alloc`）とし、依存を持たない。`sofel-cli` はこの二つに依存して、包みの判別と展開の呼び出しだけを持つ。
- `sofel-pfr` のクレートを作り、`sofel-read` の `pfr.rs`（読み取りと検証の位置）を移す。`sofel-pfr` は依存を持たない。
- PFR の輪郭（`PfrFace::draw` の charstring の解釈）を `sofel-outline` に移す。PFR の埋め込みビットマップ（`PfrFace::bitmap`）を `sofel-bitmap` に移し、`sofel-bitmap` の役割に PFR を加える。
- `sofel-font` は `sofel-pfr` に依存し、`Font::from_pfr` とフェイスの文字対応と送りを返す。
- `sofel-woff` のバイナリを `crates/sofel-cli/src/bin/sofel-woff.rs` に移し、`sofel-woff` はライブラリだけにする。コマンドの名前と引数（[ADR 0070](0070-webfont-encoding.md) の `sofel-woff --format woff|woff2 INPUT -o OUTPUT`）は変えない。
- `scripts/deps.txt` を、直した表と同じ依存にする。

### 文書

- [docs/specifications.md](../specifications.md) のクレートの構成図と、PFR、LZW、bzip2、WOFF の節を、移した後の置き場所と API の名前に直す。
- README のクレートの一覧を直す。
- `crates/sofel-capi/unmapped.txt` の PFR の行を、移した後の API の名前に直す。
- 採択の後に、[ADR 0003](0003-crate-layers.md) のステータスに、「ワークスペースとクレート」の節の `sofel-woff` の行をこの ADR が置き換えたことを記録する。

## 実装しないこと

- 移す処理の振る舞いの変更。展開、PFR の読み取り、輪郭、ビットマップ、`sofel-woff` コマンドの出力と終了コードは変えない。
- PFR の C API。今と同じく Rust の API だけとする（[ADR 0065](0065-legacy-faces-and-webfonts.md)）。
- 形式ごとのフェイスを共通の操作につなぐこと（[ADR 0065](0065-legacy-faces-and-webfonts.md) の残り）。

## 不変条件

- `I1`: `scripts/check-deps.sh` は、`scripts/deps.txt` に無い依存を持つクレートがあれば失敗し、`scripts/deps.txt` は「ワークスペースとクレート」の節の表（この ADR で直したもの）と同じ依存を許す。
- `I2`: `sofel-lzw`、`sofel-bzip2`、`sofel-pfr` は `#![no_std]` で、ワークスペースの外にもワークスペースの中にも依存を持たない。
- `I3`: 移す前と後で、同じ入力に対する展開の結果、PFR の読み取りと検証の位置、輪郭の命令、ビットマップの画素、`sofel-woff` コマンドの標準出力、出力のファイル、終了コードが一致する。
- `I4`: ライブラリのクレート（`sofel-cli`、`sofel-capi`、`sofel-conformance` を除く）は、`[[bin]]` を持たない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| `deps.txt` に無い依存を一つ足したクレート | `check-deps.sh` が失敗する | I1 |
| 直した表どおりのワークスペース | `check-deps.sh` が通る | I1 |
| 空の `.Z`、1ブロックの `.Z`、壊れた `.bz2` | 移す前と同じ結果とエラー | I3 |
| フェイスが2つの PFR、切り詰めた PFR | 移す前と同じフェイス、輪郭、画素、検証の位置 | I3 |
| `sofel-woff --format woff2 - -o -`、存在する出力への失敗 | 移す前と同じ出力と終了コード | I3 |
| ライブラリのクレートの `Cargo.toml` | `[[bin]]` が無い | I4 |

## 受けいれ条件

- `A1`: 実装のプルリクエストの CI で、`check-deps.sh`、`cargo test`、wasm の単体テスト（[ADR 0004](0004-wasm-and-no-std.md)）、適合テストが通る。wasm の単体テストの対象に `sofel-lzw`、`sofel-bzip2`、`sofel-pfr` を加える。

## テスト設計

- 移す処理の今あるテスト（`sofel-cli` の LZW と bzip2 のテスト、`sofel-read` と `sofel-font` の PFR のテスト、`sofel-woff` の `tests/encode.rs`、`sofel-cli/tests/pfr_validate.rs`）は、処理と一緒に移し、名前を変えずに通す。これが `I3` を確かめる。
- `I1` は、`scripts/tests/test-check-deps.sh` に、表に無い依存を足した一時のワークスペースで失敗する場合を足して確かめる。今あるテストがすでに確かめていれば、それを使う。
- `I2` と `I4` は、`scripts/check-deps.sh` に、ライブラリのクレートの `Cargo.toml` に `[[bin]]` が無いこと、三つのクレートの `lib.rs` に `#![no_std]` があることの検査を足し、`scripts/tests/test-check-deps.sh` でその検査が失敗する場合を確かめる。テストの名前は `ADR 0081 I2` と `ADR 0081 I4` で始める。
- 実装のまえに、`I2` と `I4` のテストを書いて失敗させる。

## トレードオフ

- クレートが三つ増え、`Cargo.toml` とワークスペースの管理が増える。代わりに、ライブラリの利用者が LZW、bzip2、PFR を使え、形式ごとの分け方が揃う。
- PFR の輪郭とビットマップの公開の API の置き場所が変わる（`sofel_font::PfrFace::draw` から `sofel-outline` へ、`bitmap` から `sofel-bitmap` へ）。今の利用者はリポジトリの中の `sofel-cli` だけであり、同じ変更で直す。
- `sofel-woff` を読み取りと書き出しの両方のクレートとしたため、読み取りだけを使う利用者も `sofel-write` を依存に持つ。分けるとクレートが一つ増えるので、今は分けない。

## 参考文献

- [ADR 0003](0003-crate-layers.md)、[ADR 0004](0004-wasm-and-no-std.md)、[ADR 0065](0065-legacy-faces-and-webfonts.md)、[ADR 0070](0070-webfont-encoding.md)
- 2026-11-17 の ADR 違反の監査（指摘 M1、会話で報告。`docs/audits/` には記録していない）
