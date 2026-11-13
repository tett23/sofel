# ADR 0075: GSUB と GPOS の機能タグを C から列挙する

ステータス: 採択

## 文脈

Golem の PDF 組版では、同じスタイルで並ぶグリフ列について、指定が有効な OpenType 機能を該当する文字の書体がどれも持たない場合に診断を出す。
sofel に切り替えた後も、フォントを開くときに置換表（GSUB）と位置調整表（GPOS）の機能タグを収集できるようにする。

## 調査した事項

Golem の `modules/golem-font/cbits/golem/golem_font.c` にある `golem_font_feature_tags` は、GSUB、GPOS の順に HarfBuzz の `hb_ot_layout_table_get_feature_tags` を呼ぶ。
HarfBuzz は指定された表の FeatureList（機能レコードの配列）から、開始位置と容量に応じてタグを返す。
sofel の `crates/sofel-read/src/layout/mod.rs` には既にインデックスを受け取る `FeatureList::tag` がある。

## 実装すること

[ADR 0059](0059-layout-sanitizer-diagnostics.md) に従ってシェーピングと同じ検査を通った表を使い、Rust と C に表単位の問い合わせを公開する。
Rust では `sofel_font::Font::feature_tags(&self, table: LayoutTable, start: u32, out: &mut [Tag]) -> (usize, usize)` が格納数と総数をこの順で返す。
公開する列挙型 `sofel_font::LayoutTable` の値は `Gsub` と `Gpos` とする。
文字体系や言語で絞り込まず、レコードの順と重複を保つ。
表がない場合と検査で表全体を破棄した場合は空列として成功する。

```c
int32_t sofel_font_feature_tags(const sofel_font_t *font, uint32_t table_tag,
                                uint32_t start, uint32_t *tags, uint32_t capacity,
                                uint32_t *written, uint32_t *total);
```

`total` は開始位置によらない総数、`written` は今回コピーした個数とする。
この関数では [ADR 0062](0062-layout-query-device-adjustments.md) の部分取得に合わせ、残りのタグが容量を超えても部分列を返して成功する。
[ADR 0015](0015-c-abi.md) の可変長出力規約に対するこの関数固有の例外として、ヘッダにも部分取得の規約を明記する。
容量が 0 のときは `tags` に NULL を渡して総数だけを読める。
必須ポインタが NULL、または容量が正で `tags` が NULL の場合は `SOFEL_ERROR_NULL`、それ以外で表タグが GSUB、GPOS のどちらでもない場合は `SOFEL_ERROR_INVALID_ARGUMENT` を返す。
エラー時は出力を書き換えない。

## データ構造とアルゴリズム

Rust 側では検査済みの FeatureList を借用し、開始位置を総数以下に収めてから残数を計算する。
残数と容量の小さい方だけを既存のタグアクセサで読み出す。

## 実装しないこと

二つの表を連結する順序とタグを集合にする処理は呼び出し側で決める。
タグを列挙しても、特定の文字列でその機能が適用されるかは判定しない。

## 不変条件

1. 成功時の総数と出力列は、同じ表、開始位置、容量を与えたHarfBuzz 15.1.0 の総数と出力列に等しい。
2. 成功時は出力配列の先頭から `written` 個だけを書き、エラー時は個数も配列も変更しない。

| 入力（表のレコード順） | 開始位置、容量 | 期待する総数、出力 |
| --- | --- | --- |
| 表なし、破棄、または空 | 0、1 | 0、空 |
| kern | 0、0 | 1、空 |
| kern | 0、1 | 1、kern |
| kern、kern、vert | 0、2 | 3、kern と kern |
| kern、kern、vert | 2、1 | 3、vert |
| kern | 1 または UINT32_MAX、1 | 1、空 |

## テスト設計

実装に先立ち、`inv1_feature_tags` で両方の表に異なるタグを置いたフォント、片方の表だけが壊れたフォント、境界表の入力を HarfBuzz と照合する。
`inv2_feature_tags_outputs` は C から呼び、短い配列の前後に置いた番兵、容量 0、必須ポインタの NULL、対象外の表タグとエラー時の出力を検査する。

## トレードオフ

部分取得を選ぶと、Golem は所有するバッファを再利用できる。
呼び出し側は総数と取得数を区別する必要がある。

## 参考文献

- [HarfBuzz 15.1.0 の OpenType レイアウト問い合わせ](https://github.com/harfbuzz/harfbuzz/blob/15.1.0/src/hb-ot-layout.cc)
- [Golem のフォント処理](https://github.com/tett23/golem/blob/85bc0377851b80a6e4477bca5ce6cacb99edb157/modules/golem-font/cbits/golem/golem_font.c)
