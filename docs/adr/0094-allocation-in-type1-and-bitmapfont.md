# ADR 0094: `sofel-type1` と `sofel-bitmapfont` に確保を許す（[ADR 0003](0003-crate-layers.md)の修正）

Status: proposed

## 文脈

[ADR 0003](0003-crate-layers.md) は、フォントの形式を読むクレート（`sofel-read`、`sofel-type1`、`sofel-pfr`、`sofel-bitmapfont`）を、バイト列の上にビューを重ね、コピーも確保もしないものとした。
2026-11-18 のADR違反の監査で、`sofel-type1` と `sofel-bitmapfont` が確保していること、それを許すADRが無いことが見つかった（指摘7）。

この2つのクレートが読む形式は、元のバイト列のままでは中身を読めない部分を持つ。

## 調査した事項

- [ADR 0003](0003-crate-layers.md) の `docs/adr/0003-crate-layers.md:42` が「どれもバイト列の上にビューを重ね、コピーも確保もしない」と書く。
- `crates/sofel-type1/src/lib.rs:5` と `crates/sofel-bitmapfont/src/lib.rs:5` に `extern crate alloc;` がある。`Vec<` か `alloc::` を含む行の数は、`sofel-type1` で `afm.rs` 2、`face.rs` 5、`pfm.rs` 2、`pfb.rs` 3、`cipher.rs` 4、`charstrings.rs` 6、`source.rs` 3、`sofel-bitmapfont` で `lib.rs` 3、`pcf.rs` 4、`winfnt.rs` 2 である（`rg -c 'Vec<|alloc::'`）。
- `sofel-type1` の確保の例：`cipher.rs:35` の復号の関数は `Option<Vec<u8>>` を返す。Type1の `eexec` の部分とCharStringは暗号化されていて、復号しないと読めない。PFBは区画に分かれていて、続けて読むには区画をつなぐ必要がある。
- `sofel-bitmapfont` の確保の例：`lib.rs:42` の `pub bitmap: Vec<u8>`、`lib.rs:51` の `glyphs: Vec<Glyph>`。BDFはテキストの16進数でビットマップを書くので、バイト列の上のビューでは画素を表せない。
- `sofel-read` と `sofel-pfr` には確保が無い（監査の報告）。

## 実装すること

- [ADR 0003](0003-crate-layers.md) の「フォントの形式を読むクレート」の決まりのうち、「コピーも確保もしない」を、`sofel-read` と `sofel-pfr` だけに当てる。
- `sofel-type1` と `sofel-bitmapfont` には、`alloc` による確保を許す。確保してよいのは、元のバイト列のままでは読めないものを作るときだけとする。
  - `sofel-type1`：`eexec` とCharStringの復号の結果、PFBの区画をつないだ結果、AFMとPFMの解析の結果（文字列の値と表）。
  - `sofel-bitmapfont`：テキストや圧縮した形で書かれたビットマップを画素に直した結果、グリフの一覧（BDFの解析の結果、PCFとFNTの表を並べなおした結果）。
- 確保する関数の説明（rustdoc）に、何を確保するかを書く。
- `#![no_std]` と `extern crate alloc` は続ける（[ADR 0004](0004-wasm-and-no-std.md)）。
- 検証器は、[ADR 0003](0003-crate-layers.md) のとおり、確保をせずに誤りを1つずつ呼び出し側に渡す。この ADR は検証器には確保を許さない。
- `docs/specifications.md` のクレートの説明を改める。
- この ADR を採択したら、[ADR 0003](0003-crate-layers.md) のステータスの行の変更の配列に `{"part":"ワークスペースとクレート（フォントの形式を読むクレートの確保）","by":["docs/adr/0094-allocation-in-type1-and-bitmapfont.md"]}` を足す。

## 実装しないこと

- `sofel-type1` と `sofel-bitmapfont` を、確保しない作り（呼び出し側の配列に書く形）に作りなおすこと。
- `sofel-read` と `sofel-pfr` に確保を許すこと。

## 不変条件

- I1: `sofel-read` と `sofel-pfr` のライブラリのコードに、`extern crate alloc` と `alloc::` が無い。どちらも `#![no_std]` なので、`alloc` を取りこまなければ `Vec`、`Box`、`String` は使えない。
- I2: `sofel-type1` と `sofel-bitmapfont` の検証器の関数は確保しない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| いまの `sofel-read` と `sofel-pfr` | `extern crate alloc` と `alloc::` が無い | I1 |
| `sofel-read` に `extern crate alloc;` を足した一時の変更 | 検査が失敗する | I1 |
| `sofel-type1` と `sofel-bitmapfont` の検証器を、確保すると失敗するアロケータの下で走らせる | 確保せずに終わる | I2 |

## テスト設計

- I1：`scripts/check-deps.sh` に、`sofel-read` と `sofel-pfr` の `src/` に `extern crate alloc` と `alloc::` が無いことを確かめる検査を足す。わざと `extern crate alloc;` を足した一時の写しで失敗することを、`scripts/tests/` のテストで確かめる（名前に `ADR 0094 I1` を含む）。
- I2：テストのバイナリで、確保の回数を数えるグローバルなアロケータを使い、検証器の呼び出しの前後で回数が変わらないことを確かめる（`adr0094_i2_...`）。

## トレードオフ

- 形式を読むクレートのうち2つが確保するので、wasmなどの確保の費用が気になる利用者は、`sofel-type1` と `sofel-bitmapfont` を使うと確保が起きる。代わりに、復号や画素の展開を呼び出し側が持つ配列で行う作りより、APIが簡単になる。
- 確保してよい場合を「元のバイト列のままでは読めないもの」に限ったため、ほかの場合に確保を足すときは、この ADR を改めるADRが要る。

## 参考文献

- [ADR 0003](0003-crate-layers.md)、[ADR 0004](0004-wasm-and-no-std.md)
- Adobe Type 1 Font Format（`eexec` の暗号）
- 2026-11-18 のADR違反の監査（`auditor`、コミット `dcc1ea66`）の指摘7
