# ADR 0092: CFFの `random` の区間の確かめの食い違いを直す（[ADR 0082](0082-cff-random-interval-check.md)の修正）

Status: proposed

## 文脈

[ADR 0082](0082-cff-random-interval-check.md) は、CFFの `random` を区間 (0, 1] としてたどる検証器の確かめを決めた。
2026-11-18 のADR違反の監査で、[ADR 0082](0082-cff-random-interval-check.md) の中に、実装がどちらにも従えない食い違いが2つ見つかった。
この ADR は、その2つだけを直す。

[ADR 0082](0082-cff-random-interval-check.md) のほかの決定は変えない。たとえば、[ADR 0082](0082-cff-random-interval-check.md) は待っている枝の数に上限を書いておらず、各整数を別の枝としてたどり、たどる順で最初の診断だけを報告すると決めている。
いまの実装はそれらに従っていないが、実装を[ADR 0082](0082-cff-random-interval-check.md) に合わせて直す（実装の直しは、この ADR の範囲の外の作業である）。

## 調査した事項

- [ADR 0082](0082-cff-random-interval-check.md) の本文（`docs/adr/0082-cff-random-interval-check.md:70`）は、`div` を「除数の区間が0を含まなければ区間の演算で、除数の端が0の側に開いていれば結果の端を無限大にする」と決めている。区間 (0, 1] は0を含まないので、この決まりでは `div` の除数にしても0での `div` にならず、結果の端が無限大になる。
- 同じ ADR の不変条件の境界の表（同 `:118`）は、「区間 (0, 1] で `div` の除数にする」の期待を `CffRandomUnchecked`（0での `div`）としている。本文と表は両立しない。
- 同じ ADR の診断の表（同 `:81`）は、`CffRandomUnchecked` の当てはまるときとして、0での `div`、負の数の `sqrt`、スタックの深さを超える `index` の位置、負かスタックの深さを超える `roll` の数、無限大の端を `mul`、`div`、位置に使うこと、実行の数が100000を超えることを並べ、「`reason` は、この4つを区別する」と書く。並べた場合は4つに分けられないので、`reason` の値を一つに決められない。
- いまの実装（`crates/sofel-read/src/diag.rs:725-738`）は `reason` に5つの値（`DivisionByZero`、`NegativeSqrt`、`StackPosition`、`InfiniteBound`、`Limit`）を持つ。
- 監査の記録：2026-11-18 の `auditor`（Fable）の報告の指摘3と指摘4。

## 実装すること

### `div` の除数の決まり（[ADR 0082](0082-cff-random-interval-check.md) の不変条件の表の訂正）

本文の決まり（`:70`）を正とし、不変条件の表の `:118` の行を次の2行に置きかえる。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 区間 (0, 1] を `div` の除数にする（例 `1 random div`） | `div` では診断は無い。結果の区間の上端は無限大になる | [ADR 0082](0082-cff-random-interval-check.md) I4 |
| 0を含む区間を `div` の除数にする（例 `1 random 0.5 sub div`。除数は (−0.5, 0.5]） | `CffRandomUnchecked`（`DivisionByZero`） | [ADR 0082](0082-cff-random-interval-check.md) I4 |

### `reason` の値（[ADR 0082](0082-cff-random-interval-check.md) の診断の表の訂正）

`CffRandomUnchecked` の `reason` は、次の5つとする。番号はC ABIの値である。

| 値 | 名前 | 当てはまるとき |
| --- | --- | --- |
| 0 | `DivisionByZero` | 幅を持つ区間が、0での `div` を含む |
| 1 | `NegativeSqrt` | 幅を持つ区間が、負の数の `sqrt` を含む |
| 2 | `StackPosition` | 幅を持つ区間が、スタックの深さを超える `index` の位置、または負かスタックの深さを超える `roll` の数を含む |
| 3 | `InfiniteBound` | 無限大の端を持つ区間を、`mul`、`div`、または位置に使う |
| 4 | `Limit` | すべての枝を合わせた実行の数が100000を超える |

- `Limit` は、実行の数の上限だけで決まる。待っている枝の数などの実装の都合で、上限より前に `Limit` を報告しない（[ADR 0082](0082-cff-random-interval-check.md) の本文のとおり）。

### ADRのステータス

この ADR の実装のプルリクエストで、[ADR 0082](0082-cff-random-interval-check.md) のステータスの行を `superseded [{"part":"不変条件（区間 (0, 1] を div の除数にする行）","by":["docs/adr/0092-cff-random-check-corrections.md"]},{"part":"実装すること（CffRandomUnchecked の reason）","by":["docs/adr/0092-cff-random-check-corrections.md"]}]` に改める。

## 実装しないこと

- [ADR 0082](0082-cff-random-interval-check.md) のほかの決定の変更。
- 診断の種類の変更。

## 不変条件

- I1: 区間 (0, 1] を `div` の除数にしたグリフで、その `div` は `CffRandomUnchecked` を報告しない。
- I2: 0を含む幅を持つ区間を `div` の除数にしたグリフは、`CffRandomUnchecked`（`DivisionByZero`）を報告する。
- I3: `CffRandomUnchecked` の `reason` は上の表の5つのどれかで、その数値はC ABIの値と等しい。
- I4: 枝を合わせた実行の数が100000以下のグリフは、`reason` が `Limit` の診断を報告しない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| `1 random div drop endchar` | 診断なし | I1 |
| `1 random 0.5 sub div drop endchar` | `DivisionByZero` | I2 |
| `random` と `ifelse` の組を17個重ね、枝を合わせた実行の数が100000以下 | `Limit` は無い | I4 |
| 枝を合わせた実行の数がちょうど100000 | `Limit` は無い | I4 |
| 枝を合わせた実行の数が100001 | `Limit` | I3, I4 |
| C ABIで5つの `reason` を読む | 値は0〜4 | I3 |

## テスト設計

実装のまえに、次のテストを書いて失敗させる。テストの名前は `adr0092_i1_...` の形とする。

- I1、I2：[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方でCFFのフォントを作り、検証器の診断を確かめる。
- I4：`random` と `ifelse` を17個以上重ねたグリフで、枝を合わせた実行の数を数えて100000以下であることを確かめてから、`Limit` が出ないことを確かめる。いまの実装は待っている枝の上限（16）で `Limit` を出すので、このテストは直す前に失敗する。
- I3：C ABIのテストで、各 `reason` の値を確かめる。
- いまの `adr0082_i4_limit_counts_every_branch` は、実行の数の上限に届く前に待っている枝の上限で終わる。実行の数が100000を超えることで `Limit` になることを確かめる形に直す。

## トレードオフ

- 待っている枝の数に上限を置かないため、枝を積む領域は実行の数の上限（100000）から決まる大きさになる。`sofel-read` は確保をしない（[ADR 0003](0003-crate-layers.md)）ので、その領域の持ち方（固定の大きさの領域、たどりなおしなど）は実装で決める。どの持ち方でも[ADR 0082](0082-cff-random-interval-check.md) の決まりを満たせないと分かったら、修正のADRを書く。

## 参考文献

- [ADR 0082](0082-cff-random-interval-check.md)
- 2026-11-18 のADR違反の監査（`auditor`、コミット `dcc1ea66`）の指摘1、3、4、5
