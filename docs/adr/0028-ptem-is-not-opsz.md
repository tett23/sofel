# ADR 0028: `--font-ptem` は `opsz` を決めない（[ADR 0020](0020-shaping-foundation.md)の訂正）

ステータス: 提案

## 文脈

[ADR 0020](0020-shaping-foundation.md) は、HarfBuzzの `hb-shape` の `--font-ptem`（`hb_font_set_ptem`）が、可変フォントの `opsz` の座標を決めるものとして書いた。

- 期待する失敗の分類 `variations` の説明に「`--font-ptem` による `opsz`」を含めた。
- 「後回しにしたもの」の可変フォントのシェーピングに「`--font-ptem` による `opsz`」を含めた。

HarfBuzz 15.1.0の `src/` には、`ptem` から `opsz` の座標を決める処理が無い。
`opsz` という文字列は、CFFの直列化の引数の名前（`opszr`）にしか現れない。
`hb_font_set_ptem` は値を `hb_font_t` に入れるだけで、その値を読むのはAATの `trak`（`src/hb-aat-layout-trak-table.hh`）である。
`opsz` の座標を文字の大きさから決めるのは、HarfBuzzではなく呼び出し側（CSSの `font-optical-sizing` を実装するブラウザなど）の仕事である。

`conformance/expected-failures.txt` の `variations` の263行のうち、`in-house/aat-trak.tests` の7行は `--font-ptem` だけを使う `trak` の行で、可変フォントの行ではない。
[ADR 0027](0027-variable-fonts.md) の下書きの調べで、この誤りが見つかった。

## 実装すること

- [ADR 0020](0020-shaping-foundation.md) の分類の決まりを、次のとおり読みかえる。
  - `variations`：可変フォント（`--variations`、`--named-instance`、FeatureVariations、ItemVariationStoreの補正）。`--font-ptem` は含めない。
  - `aat`：AATと古い `kern` 表、`--font-ptem` による `trak`（[ADR 0020](0020-shaping-foundation.md)のとおり）。
- `conformance/expected-failures.txt` の `in-house/aat-trak.tests` の7行を、`variations` から `aat` に分類しなおす。分類の理由が正しいことは、[ADR 0020](0020-shaping-foundation.md)の実装のテスト（分類ごとに原因が当てはまることを確かめるテスト）で確かめる。
- `docs/specifications.md` の分類の説明から「`--font-ptem` による `opsz`」を除く。
- `sofel-shape` コマンドの `--font-ptem` の扱いは変えない。[ADR 0020](0020-shaping-foundation.md) のとおり、`trak` を実装する（AATのADR）まで、使い方の誤りとして終了コード2で終わる。
- この ADR の実装のプルリクエストで、[ADR 0020](0020-shaping-foundation.md) のステータスの行に「→`--font-ptem` の分類は [ADR 0028](0028-ptem-is-not-opsz.md)」を足す。

## 実装しないこと

- `ptem` から `opsz` を決める機能。HarfBuzzにも無く、文字の大きさから座標を決める規則は呼び出し側の方針による。必要になれば、呼び出し側の補助として別のADRで判断する。
- `trak` の実装（AATのシェーピングのADR）。

## テスト設計

- 分類の理由を確かめる既存のテストで、`in-house/aat-trak.tests` の7行が `aat` の原因（`trak` 表を持つフォントと `--font-ptem`）に当てはまり、`variations` の原因（`fvar` を持つフォントと座標の指定）に当てはまらないことを確かめる。7行を `variations` のまま残すと、このテストが失敗することを一度確かめる。
- HarfBuzzの `src/` に `ptem` から `opsz` を決める処理が無いことは、テストにしない（取得したソースの版は `testdata.lock` で固定されており、版を上げるときにこのADRを見直す）。

## トレードオフ

- 誤りの訂正だけで、[ADR 0020](0020-shaping-foundation.md)の他の決定は変えない。7行の分類が変わるだけなので、[ADR 0027](0027-variable-fonts.md) の実装で消す `variations` の行の数が256行になる。

## 参考文献

- HarfBuzz `src/hb-font.cc`（`hb_font_set_ptem`）、`src/hb-aat-layout-trak-table.hh`、`util/font-options.hh`（`--font-ptem`）、`test/shape/data/in-house/tests/aat-trak.tests`（15.1.0）
- CSS Fonts Module Level 4, `font-optical-sizing` https://www.w3.org/TR/css-fonts-4/#font-optical-sizing-def
