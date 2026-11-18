# ADR 0093: `sofel-shape`、`sofel-color`、`sofel-subset` から `sofel-geom` への依存を表に足す（[ADR 0003](0003-crate-layers.md)の修正）

Status: proposed

## 文脈

[ADR 0003](0003-crate-layers.md) は、クレートの依存を表に書いたものだけとした。
2026-11-18 のADR違反の監査で、`sofel-shape` が `sofel-geom` に依存しているのに、[ADR 0003](0003-crate-layers.md) の表にも、それを足す修正のADRにも書かれていないことが見つかった（指摘2）。
`sofel-color` と `sofel-subset` から `sofel-geom` への依存は、[ADR 0026](0026-harfbuzz-rounding.md) で決めたが、[ADR 0003](0003-crate-layers.md) のステータスの記録に [ADR 0026](0026-harfbuzz-rounding.md) が無い。

## 調査した事項

- [ADR 0003](0003-crate-layers.md) の表（`docs/adr/0003-crate-layers.md:57`）で、`sofel-shape` の依存は `sofel-font` だけである。[ADR 0020](0020-shaping-foundation.md) は、依存を `sofel-font` と `sofel-unicode` とした。
- `crates/sofel-shape/Cargo.toml:12` に `sofel-geom` がある。使う箇所は `crates/sofel-shape/src/outline.rs:6-7`（`sofel_geom::float::{floor, hb_clamp_to_i32}`、`sofel_geom::{Command, PointF}`）である。`outline.rs` は、シェーピングの結果を輪郭として返すときに、HarfBuzzと同じ浮動小数点の丸め（[ADR 0026](0026-harfbuzz-rounding.md)）と、輪郭の命令を使う。
- `crates/sofel-subset/Cargo.toml:14` に `sofel-geom` がある。`sofel-color` の `Cargo.toml` には `sofel-geom` が無い。
- `scripts/deps.txt` の30行、45行、46行が、それぞれ `sofel-shape sofel-geom`、`sofel-color sofel-geom`、`sofel-subset sofel-geom` を許す。
- [ADR 0026](0026-harfbuzz-rounding.md) の `docs/adr/0026-harfbuzz-rounding.md:44` は、`sofel-font`、`sofel-outline`、`sofel-color`、`sofel-subset` が `sofel_geom::float` を使うと決めた。
- [ADR 0003](0003-crate-layers.md) のステータスの行の変更の配列に、[ADR 0020](0020-shaping-foundation.md) と [ADR 0026](0026-harfbuzz-rounding.md) は無い。

## 実装すること

- [ADR 0003](0003-crate-layers.md) の表の依存を、次のとおり改める。

| クレート | 依存（改めたあと） | 足す依存の根拠 |
| --- | --- | --- |
| `sofel-shape` | `sofel-font`、`sofel-unicode`、`sofel-geom` | `sofel-unicode` は [ADR 0019](0019-unicode-character-data.md)。`sofel-geom` は、シェーピングの結果の輪郭と、HarfBuzzの丸め（[ADR 0026](0026-harfbuzz-rounding.md)）のため（この ADR） |
| `sofel-color` | `sofel-outline`、`sofel-bitmap`、`sofel-svg`、`sofel-geom` | [ADR 0026](0026-harfbuzz-rounding.md) |
| `sofel-subset` | `sofel-font`、`sofel-write`、`sofel-unicode`、`sofel-geom` | `sofel-unicode` は [ADR 0022](0022-subsetting.md)。`sofel-geom` は [ADR 0026](0026-harfbuzz-rounding.md) |

- `scripts/deps.txt` は、いまのまま（上の表と同じ依存を許している）とする。
- `docs/specifications.md` の依存の図と表を、上の表に合わせる。
- この ADR を採択したら、[ADR 0003](0003-crate-layers.md) のステータスの行の変更の配列に、`{"part":"ワークスペースとクレート（sofel-shape、sofel-color、sofel-subset の依存）","by":["docs/adr/0093-geom-dependencies-of-shape-color-subset.md"]}` を足す。[ADR 0020](0020-shaping-foundation.md) のステータスの行には、`{"part":"実装すること（sofel-shape の依存）","by":["docs/adr/0093-geom-dependencies-of-shape-color-subset.md"]}` を足す。

## 実装しないこと

- 依存を減らすこと（`sofel-shape` の輪郭の処理を別のクレートに移すことなど）。
- `sofel-color` の `Cargo.toml` に `sofel-geom` を足すこと。表は依存を許すだけで、求めない。

## 不変条件

- I1: `scripts/deps.txt` が許す依存の組は、[ADR 0003](0003-crate-layers.md) の表と、それを改めたADRの表を合わせたものと等しい。
- I2: どのライブラリのクレートの通常の依存も、`scripts/deps.txt` が許す組に含まれる。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| いまの `scripts/deps.txt` | ADRの表と等しい | I1 |
| `sofel-shape` の `Cargo.toml` から `sofel-geom` を外したもの | I2 は成り立つ（許すだけなので） | I2 |
| `deps.txt` から `sofel-shape sofel-geom` を消したもの | `scripts/check-deps.sh` が失敗する | I2 |

## テスト設計

- I1：ADRの表から許す依存の組を作り、`scripts/deps.txt` と比べる検査を、`scripts/tests/` のテストとして足す。テストの名前は `ADR 0093 I1` を含む。表の組は、この ADR と [ADR 0003](0003-crate-layers.md)、[ADR 0019](0019-unicode-character-data.md)、[ADR 0022](0022-subsetting.md)、[ADR 0026](0026-harfbuzz-rounding.md)、[ADR 0081](0081-crate-layout-corrections.md) の表から書いた期待の一覧をテストに置き、一覧と `deps.txt` を比べる。
- I2：既存の `scripts/check-deps.sh` のテストに、`deps.txt` から1行消した一時のファイルで失敗することを確かめる例を足す（`ADR 0093 I2`）。

## トレードオフ

- ADRの表と `deps.txt` を照らす一覧をテストに置くため、依存を変えるADRを書くたびに、その一覧も直す手間がある。代わりに、ADRに無い依存を `deps.txt` に足しても、テストで見つかる。

## 参考文献

- [ADR 0003](0003-crate-layers.md)、[ADR 0020](0020-shaping-foundation.md)、[ADR 0026](0026-harfbuzz-rounding.md)
- 2026-11-18 のADR違反の監査（`auditor`、コミット `dcc1ea66`）の指摘2
