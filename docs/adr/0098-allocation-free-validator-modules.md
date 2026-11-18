# ADR 0098: 検証器が確保しないことを、アロケータではなくモジュールの検査で確かめる（[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md)と[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md)の修正）

Status: proposed

## 文脈

[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) は、`sofel-type1` と `sofel-bitmapfont` に、確保しない検証器を足すと決めた。
確保しないことは、確保の回数を数えるグローバルなアロケータをテストに置いて確かめるとした（I2、I4）。
[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) の I2 のテストも、同じ形である。
実装を始めたところ、このテストはどのクレートにも置けないことが分かった（2026-11-18）。
また、[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) は、`metrics.rs` の関数の説明を誤っていた。

メンテナーは、確保しないことを、アロケータで数えるのではなく、コードの形で確かめる案を選んだ（2026-11-18）。

## 調査した事項

- アロケータを差しかえるには、`unsafe impl GlobalAlloc` が要る。
- ワークスペースの `Cargo.toml:15` は `unsafe_code = "forbid"` とする。`crates/sofel-type1/Cargo.toml:10` の `[lints] workspace = true` により、この決まりは統合テストにもかかる。
- 実装のエージェントが `crates/sofel-type1/tests/` に一時のテストを置き、`cargo test -p sofel-type1` を動かした。結果は `error[E0453]: allow(unsafe_code) incompatible with previous forbid` だった（2026-11-18 の報告。ファイルは消してある）。
- [ADR 0003](0003-crate-layers.md) の `:84` と [ADR 0015](0015-c-abi.md) の `:35`、`:42`、`:111` は、`unsafe` を `sofel-capi` のポインタの扱いだけに限る。
- `scripts/check-deps.sh:114-128` は、`scripts/deps.txt` の `!no_alloc <crate>` の行のクレートの `src/` に、`alloc` クレートの取りこみと使用が無いことを検査する（[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) I1）。`deps.txt:121-122` が `sofel-read` と `sofel-pfr` をこの対象にしている。
- `crates/sofel-type1/src/metrics.rs:33-35` の `parse_metrics` は、復号した Type 1 の charstring から、最初の `hsbw` か `sbw` を読む関数である。演算子のスタックを `Vec` で確保し、上限の検査は `:108` の `stack.len() > 24` にある。`crates/sofel-type1/src/afm.rs` と `pfm.rs` に、`stack` を含む行は無い。[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) の `:47`、`:69` は、この関数を AFM と PFM の解析のものと書いていて、誤りである。

## 実装すること

### 確保しないモジュール

- `sofel-type1` と `sofel-bitmapfont` に、確保しないモジュールのディレクトリ `src/noalloc/` を置く。[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) の検証器と、`metrics.rs` の charstring の読みとりは、このディレクトリの中に置く。公開は、クレートのほかの場所からの再公開でよい。
- `src/noalloc/` の中のコードは、次の2つを守る。
  - `alloc` の名前を使わない。`alloc::`、`Vec`、`Box`、`String`、`vec!`、`format!`、`.to_vec(`、`.to_string(`、`.to_owned(`、`.collect` を書かない。
  - クレートのほかのモジュールを呼ばない。`use` と経路に書けるのは、`core::`、`crate::noalloc::`、`self::`、`super::`（`src/noalloc/` の中を指すものだけ）と、確保しない依存のクレート（`deps.txt` の `!no_alloc` のもの）だけとする。
- この2つを守れば、`src/noalloc/` のコードは確保できない。確保する型の名前を書けず、確保するコードを呼べないからである。呼び出し側が渡す関数（誤りを受けとる関数など）の中の確保は、呼び出し側のものとし、検証器の確保に数えない。
- 解析の関数（`parse_*`）は、`src/noalloc/` の処理を呼んでよい。読み方を1か所にし、[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) I3（検証器が誤りを出さない入力は、解析の関数も受け付ける）を作りから成り立たせるためである。

### 検査

- `scripts/deps.txt` に `!no_alloc_dir <crate> <dir>` の行を足し、`sofel-type1 src/noalloc` と `sofel-bitmapfont src/noalloc` を書く。
- `scripts/check-deps.sh` は、`!no_alloc_dir` の行のディレクトリについて、上の2つの決まりを検査する。ディレクトリが無いときも失敗にする。

### [ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) の読みかえ

- I2（検証器は確保しない）は、「検証器は `src/noalloc/` の中にあり、そのディレクトリは `check-deps.sh` の検査を通る」と読みかえる。
- I4 と、確保をやめるものの節の「AFMとPFMの解析の演算子のスタック（`metrics.rs:35`）」は、「Type 1 の charstring の `hsbw` と `sbw` を読む演算子のスタック（`metrics.rs:35`）」と読みかえる。上限は、いまの検査と同じ24とし、決まった大きさの配列にする。
- 境界の表の「演算子のスタックがちょうど上限、上限を1つ超えるAFM」は、「オペランドが24個と25個の charstring」と読みかえる。期待は、24個は受け付け、25個は誤りとする。
- テスト設計の、確保の回数を数えるアロケータのテストは、下のテスト設計に置きかえる。

### [ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) の読みかえ

- I2 のテスト（確保の回数を数えるアロケータ）も、下のテスト設計の検査に置きかえる。

### ADRのステータス

この ADR の実装のプルリクエストで、ステータスの行を次のとおり改める。

- [ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md)：`superseded [{"part":"不変条件 I2、I4 とテスト設計（確保の確かめ方）","by":["docs/adr/0098-allocation-free-validator-modules.md"]},{"part":"確保をやめるもの（metrics.rs の説明）","by":["docs/adr/0098-allocation-free-validator-modules.md"]}]`。[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) の残りの実装がまだなら、`superseded-pending` とする。
- [ADR 0094](0094-allocation-in-type1-and-bitmapfont.md)：[ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) が決めた配列に、`{"part":"テスト設計（I2）","by":["docs/adr/0098-allocation-free-validator-modules.md"]}` を足す。

## 実装しないこと

- `unsafe` を許すクレートを足すこと。
- 確保の回数を、実行して数えること。
- `src/noalloc/` の外のコード（解析の関数）の確保をやめること（[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md) のとおり）。

## 不変条件

- I1: `!no_alloc_dir` の行の各ディレクトリは存在し、その中のファイルに、`alloc` の名前（上の一覧）が無い。
- I2: `!no_alloc_dir` の行の各ディレクトリの中のファイルの `use` と経路は、上の許す一覧のものだけである。
- I3: [ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) の検証器と `parse_metrics` の演算子のスタックの処理は、`!no_alloc_dir` のディレクトリの中にある。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| いまの `deps.txt` とソース（実装の後） | `check-deps.sh` が通る | I1, I2, I3 |
| `src/noalloc/` の1つのファイルに `let v: Vec<u8> = Vec::new();` を足した写し | `check-deps.sh` が失敗する | I1 |
| `src/noalloc/` の1つのファイルに `use crate::cipher::decrypt;` を足した写し | `check-deps.sh` が失敗する | I2 |
| `!no_alloc_dir` のディレクトリを消した写し | `check-deps.sh` が失敗する | I1 |
| オペランドが24個と25個の charstring | 24個は受け付け、25個は誤り | I3 |

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- I1、I2：`scripts/tests/test-check-deps.sh` に、表の写しの例を足す（名前に `ADR 0098 I1`、`ADR 0098 I2` を含む）。
- I3：`sofel-type1` と `sofel-bitmapfont` の各検証器と `parse_metrics` を、`src/noalloc/` の経路から呼ぶテスト（`adr0098_i3_...`）。オペランドが24個と25個の charstring のテストも置く。

## トレードオフ

- 確保しないことを、名前と経路の検査で確かめるので、検査の一覧に無い確保の仕方（新しい `alloc` の型など）を見逃しうる。代わりに、`unsafe` を足さずに済み、実行していない経路の確保も見つかる。
- `src/noalloc/` からクレートのほかのモジュールを呼べないので、解析の関数と共有する処理は `src/noalloc/` に移すことになる。

## 参考文献

- [ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md)、[ADR 0094](0094-allocation-in-type1-and-bitmapfont.md)、[ADR 0003](0003-crate-layers.md)、[ADR 0015](0015-c-abi.md)
- 2026-11-18 の [ADR 0096](0096-type1-bitmapfont-validators-and-allocation.md) の実装の報告
