# ADR 0050: OpenType タグから HarfBuzz と同じスクリプト値を返す

ステータス: 採択

## 文脈

[ADR 0020](0020-shaping-foundation.md) は、OpenType のスクリプトタグから HarfBuzz と同じスクリプトを求める API を定めた。
呼び出し側が区別したいのは、HarfBuzz が変換を終えた時点の4バイトの値である。
たとえば OpenType タグ `aran` の変換結果は `Aran` である。

## 調査した事項

`sofel-shape` の `ot_tag_to_script` と `ot_tags_to_script_and_language` は、どちらも変換結果を `script_from_raw` に渡す。
この関数は値がゼロなら `None` を返す。
それ以外では `Script::from_iso15924` を呼び、元の4バイトを復元できなければ `Script::UNKNOWN` を返す。

`sofel-unicode` の `Script` は4バイトを格納する。
`from_iso15924` は文字列を解釈するための構築子であり、たとえば `Aran` を `Arab` に、`Qaai` を `Zinh` に変換する。

HarfBuzz 15.1.0 の `hb_ot_tag_to_script` は、末尾が `2` または `3` のタグを対応表で引く。
対応表にないタグの戻り値は `Zzzz` である。

## 実装すること

`sofel-unicode` に、変換済みの4バイトをそのまま格納する構築子を設ける。
`sofel-shape` はゼロを先に処理し、それ以外の変換結果をこの構築子で返す。

実装時に、`Script` と `tag()` の API 文書に OpenType 由来の値域を記す。
[API ガイド](../api.md) と[仕様書](../specifications.md) の `UNKNOWN` に関する記述を更新する。

## 実装しないこと

ISO 15924 の文字列を受け取る `Script::from_iso15924` の規則と、言語タグの変換は対象外とする。

## 不変条件

1. 任意の OpenType タグについて、`hb_ot_tag_to_script` の戻り値がゼロなら `ot_tag_to_script` は `None` を返す。
   それ以外なら、返った `Script` の `tag()` は戻り値のビッグエンディアン表現と一致する。
2. 任意のスクリプトタグと任意の言語タグ（省略を含む）について、`ot_tags_to_script_and_language` の第1戻り値は、同じスクリプトタグを渡した `ot_tag_to_script` の戻り値と一致する。

| OpenType タグ | 期待するスクリプト |
| --- | --- |
| `DFLT` | `None` |
| `latn` | `Latn` |
| `aran` | `Aran` |
| `qaai` | `Qaai` |
| `ab!d` | `Ab!d` |
| `LATN` | `LATN` |
| `dev3` | `Deva` |
| `abc2` | `Zzzz` |
| `zzzz` | `Zzzz`（`Some(Script::UNKNOWN)`） |

## テスト設計

`inv1_script_tag_bytes` で表の各入力を変換し、4バイトまたは `None` を照合する。
`inv2_script_with_language` で同じ入力に対して言語タグを省略した場合と指定した場合を試し、単独変換と比較する。
実装前に両テストが `aran` などで失敗することを確かめるため、第2のテストでも表の期待値と照合する。

HarfBuzz 15.1.0 の `hb_ot_tag_to_script` に表の各入力を渡し、テストに使う期待値を照合する。
既存の `script_tags_that_name_no_iso15924_script_are_unknown` も表に合わせて更新する。

## トレードオフ

呼び出し側は HarfBuzz が区別する値を識別できる。
その分、OpenType から得た `Script::tag()` を表示するコードは、英字以外のバイトも扱う必要がある。

## 参考文献

- [HarfBuzz 15.1.0 のタグ変換](https://github.com/harfbuzz/harfbuzz/blob/15.1.0/src/hb-ot-tag.cc)
- [OpenType タグとスクリプトの変換](../../crates/sofel-shape/src/tag.rs)
- [Script の定義と ISO 15924 の文字列の解釈](../../crates/sofel-unicode/src/lib.rs)
