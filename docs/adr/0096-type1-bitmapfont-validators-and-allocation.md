# ADR 0096: `sofel-type1` と `sofel-bitmapfont` の検証器と、確保を許す場合の追加（[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md)の修正）

Status: superseded [{"part":"不変条件 I2、I4 とテスト設計（確保の確かめ方）","by":["docs/adr/0098-allocation-free-validator-modules.md"]},{"part":"確保をやめるもの（metrics.rs の説明）","by":["docs/adr/0098-allocation-free-validator-modules.md"]}]

## 文脈

[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) は、`sofel-type1` と `sofel-bitmapfont` に、元のバイト列のままでは読めないものを作るときだけ確保を許し、検証器には確保を許さないとした（I2）。
実装（PR #369）で、[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) のとおりには進められない点が2つ見つかった（2026-11-18）。

1. 2つのクレートには検証器の関数が無く、I2 の確かめの対象が無い。一方、[ADR 0003](0003-crate-layers.md) は、形式を読むクレートが各形式の検証器を持つことを求めている。検証器が無いことのほうが、[ADR 0003](0003-crate-layers.md) とのずれである。
2. `sofel-type1` に、[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) が確保を許した場合（復号の結果、PFBの区画をつないだ結果、AFMとPFMの解析の結果）に入らない確保がある。

この ADR は、1 について検証器を足すことを決め、2 について確保を許す場合を足すか、確保をやめるかを決める。

## 調査した事項

- `rg -n 'pub fn validate|fn validate' crates/sofel-type1/src crates/sofel-bitmapfont/src` は、`crates/sofel-type1/src/type42.rs:350` の非公開の `validate_sfnt` だけを返す。公開の検証器は無い。
- `sofel-validate`（`crates/sofel-cli/src/validate.rs:113`、`:135`、`:157`、`:159`、`:165`）は、`WinFont::parse_with_offset`、`BdfFont::parse_with_offset`、`Type1Source::parse_pfa_with_offset`、`parse_pfb_with_offset`、`Charstrings::parse_with_offset` を呼んで、これらの形式を確かめている。どれも解析の結果を確保する。
- [ADR 0003](0003-crate-layers.md) の `docs/adr/0003-crate-layers.md:42` は、形式を読むクレートについて「各形式の検証器も持つ」と書き、`:68` は、検証器が確保をせずに誤りを1つずつ呼び出し側に渡すと書く。
- [ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) が許した場合に入らない `sofel-type1` の確保：
  - 演算子のスタック：`crates/sofel-type1/src/metrics.rs:35`（`let mut stack = Vec::new();`）。
  - CharStringsとSubrsの表：`crates/sofel-type1/src/charstrings.rs:23-24`（`glyphs: Vec<(&[u8], &[u8])>`、`subrs: Vec<Option<&[u8]>>`。どちらも元のデータを借りた参照の一覧）。
  - PFBの区画の一覧：`crates/sofel-type1/src/pfb.rs:39`、`:56`。
  - Multiple Mastersの軸と重み：`crates/sofel-type1/src/multiple_masters.rs:66-67`、`:189`。
  - CIDのFont DICTの一覧：`crates/sofel-type1/src/cid.rs:46`、`:123`。
  - Type42のCharStrings（グリフ名とグリフID）の表：`crates/sofel-type1/src/type42.rs:232`。
- 実装（PR #369）は、これらの関数に、何を確保するかをrustdocで書いた。コードは変えていない。

## 実装すること

### 検証器

- `sofel-type1` と `sofel-bitmapfont` に、[ADR 0003](0003-crate-layers.md) のとおり、各形式の検証器を足す。対象は、PFAとPFB（Type1）、CIDキーのType1、Type42、AFM、PFM、BDF、PCF、FNTとFONである。
- 検証器は確保をしない。見つけた誤りを、場所（バイトの位置）と理由とともに、呼び出し側の関数に1つずつ渡す（`sofel-read` の検証器と同じ形）。
- 暗号化された部分（`eexec`）の検証は、復号した結果を確保せずに、復号しながら読む。
- [ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) の I2 は、この検証器に当たる。
- `sofel-validate` は、解析の関数ではなく、この検証器を呼ぶ。報告の形は [ADR 0008](0008-validate-command-and-freetype-comparison.md) のとおりとする。

### 確保を許す場合（[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) の追加）

[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) の確保を許す場合に、次を加える。

- 長さがファイルの中身で決まる、解析した構造の一覧。元のバイト列の上では、目的の要素を位置で引けないもの（名前や番号で引くための表、辞書の一覧）に限る。上の調査の CharStringsとSubrsの表、PFBの区画の一覧、Multiple Mastersの軸と重み、CIDのFont DICTの一覧、Type42のCharStringsの表が当たる。

次は、確保をやめる。

- 大きさの上限が仕様で決まるもの。AFMとPFMの解析の演算子のスタック（`metrics.rs:35`）は、決まった大きさの配列にする。

### ADRのステータス

この ADR の実装のプルリクエストで、[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) のステータスの行を `superseded-pending [{"part":"実装すること（確保してよい場合）","by":["docs/adr/0096-type1-bitmapfont-validators-and-allocation.md"],"kind":"supplement"},{"part":"不変条件 I2（対象の検証器）","by":["docs/adr/0096-type1-bitmapfont-validators-and-allocation.md"],"kind":"supplement"}]` に改める（[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) の残りと一緒に実装を終えたら `superseded [...]`）。

## 実装しないこと

- 解析の関数（`parse_*`）の確保をやめること（[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) のとおり）。
- `sofel-read` と `sofel-pfr` に確保を許すこと。

## 不変条件

- I1: `sofel-type1` と `sofel-bitmapfont` の各形式に、公開の検証器の関数がある。
- I2: その検証器は、どの入力でも確保しない。
- I3: 検証器が誤りを報告しない入力は、対応する解析の関数が受け付ける。
- I4: `metrics.rs` の演算子のスタックは確保しない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 正しいPFA、PFB、CIDキーのType1、Type42、AFM、PFM、BDF、PCF、FNT、FON | 誤りなし、確保なし | I1, I2, I3 |
| 空のバイト列、1バイト、区画の途中で切れたPFB、`eexec` の途中で切れたPFA | 誤りを報告、確保なし | I2 |
| 演算子のスタックがちょうど上限、上限を1つ超えるAFM | 上限ちょうどは受け付け、超えると誤り。確保なし | I4 |

## テスト設計

実装のまえに、次のテストを書いて失敗させる。テストの名前は `adr0096_i1_...` の形とする。

- I1：各形式の検証器を呼ぶテスト（いまは関数が無いのでビルドが失敗する）。
- I2、I4：確保の回数を数えるグローバルなアロケータを使い、検証器と演算子のスタックの解析の前後で回数が変わらないことを確かめる（[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) I2 のテストもこれで書く）。
- I3：テストのデータの各形式のファイルと、それを切ったり壊したりした写しで、検証器の結果と解析の関数の結果を比べる。
- ファジング：各形式の検証器を `fuzz/` の対象に足し、panicせず確保しないことを確かめる。

## トレードオフ

- 検証器と解析の関数の2つを持つので、形式の読み方を2か所に書くことになる。I3 のテストで食い違いを見つける。
- 確保を許す場合を広げるので、`sofel-type1` の解析は確保を前提にした作りのまま残る。代わりに、名前で引く表を毎回元のバイト列から探しなおす費用を避けられる。

## 参考文献

- [ADR 0003](0003-crate-layers.md)、[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md)、[ADR 0008](0008-validate-command-and-freetype-comparison.md)
- PR #369 の実装の報告（2026-11-18）
