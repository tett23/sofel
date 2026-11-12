# ADR 0010: `sofel-font` の土台（sfntのフォント、文字からグリフ、メトリクスの方針）

ステータス: 採択→可変フォントの座標と `sofel-font` の `alloc` は [ADR 0027](0027-variable-fonts.md)

## 文脈

[ADR 0003](0003-crate-layers.md) は、`sofel-font` を、形式を判別して1つのフォントを形式によらない形で表す層とした。
`sofel-outline`、`sofel-shape` などは `sofel-font` を借用し、表のバイト列を直接読まない。
[ADR 0007](0007-sfnt-read-foundation.md) は、壊れた表や範囲外の入力に対する既定の値の方針を、`sofel-read` ではなく `sofel-font` で決めるとした。

2段階目（[ADR 0001](0001-project-goal-and-scope.md)）でglyfの輪郭を取り出すには、フォントを開き、グリフの数と `unitsPerEm` を知り、glyfとlocaにたどり着く層が要る。
`sofel-render`（[ADR 0012](0012-antialiased-rasterizer.md)）で文字を画像にするには、文字からグリフへの対応も要る。

HarfBuzzとFreeTypeは、ここで方針が異なる。

- cmapのサブテーブルの選び方：HarfBuzzは (3,0)、(3,10)、(0,6)、(0,4)、(3,1)、(0,3)、(0,2)、(0,1)、(0,0)、(1,0)、(1,任意) の順に探す（`src/hb-ot-cmap-table.hh` の `find_best_subtable`）。記号フォント（(3,0)）とMacのエンコーディングでは、OS/2の表を見て文字を付けかえる。FreeTypeは、Unicodeの文字の表のうち32bitのものを優先して選ぶ。
- hmtxの範囲外：HarfBuzzは、グリフの数以上のグリフIDに0を、hmtxが無いフォントには `unitsPerEm / 2` を返す（`src/hb-ot-hmtx-table.hh`）。FreeTypeは、最後の送り幅を返す（`src/sfnt/ttmtx.c`）。
- グリフの数：どちらも、`MAXP` があればそれを、無ければ `maxp` を使う（HarfBuzz `src/hb-ot-maxp-table.hh`、FreeType `src/sfnt/ttload.c:792`）。

文字からグリフへの対応と送り幅は、シェーピングで使う。シェーピングの正しさの基準はHarfBuzzである（[ADR 0005](0005-conformance-tests.md)）。

## 実装すること

### クレート

- `crates/sofel-font` を作る。`#![no_std]`、確保なし、`#![forbid(unsafe_code)]`。依存は `sofel-read` と `sofel-geom` だけとする（[ADR 0003](0003-crate-layers.md) の表のうち、この段階で要るもの）。
- `sofel-read` を `sofel_font::read` として再公開する。`sofel-outline` などは、表のビューをこれを通して使う。これにより、[ADR 0003](0003-crate-layers.md) の依存の表（`sofel-outline` は `sofel-font` と `sofel-geom` に依存する）を保つ。

### `Font`

- `Font::new(data: &[u8], index: u32) -> Result<Font, FontError>` は、sfntとTTC/OTCを判別し、`index` 番目のフォントを開く。単体のsfntでは `index` は0だけを受け付ける。
- 作るときに、次を一度だけ読み、値として持つ。表の数とcmapの記録の数に比例する時間で終わり、確保をしない。
  - `head`（無い、または読めなければ `FontError`）
  - グリフの数：`MAXP` があればその `numGlyphs`、無ければ `maxp` の `numGlyphs`。どちらも無ければ0とする。
  - 横書きのメトリクス：`HHEA` と `HMTX` の組があればそれを、無ければ `hhea` と `hmtx` の組を使う。読めなければ「メトリクスなし」とする。
  - 文字の対応に使うcmapのサブテーブル：HarfBuzzの順で探し、読めるもののうち最初のものとする。異体字シーケンスのサブテーブル（(0,5) のフォーマット14か15）も別に持つ。
- `Font` は不変の値で、`Copy` と `Send` と `Sync` を満たす（[ADR 0003](0003-crate-layers.md)）。キャッシュは持たない。

### 方針

| 問い合わせ | 方針 |
|---|---|
| `num_glyphs()` | 上のグリフの数 |
| `units_per_em()` | `head` の値。16未満か16384を超える値でもそのまま返す |
| `nominal_glyph(c)` | 選んだサブテーブルで引く。グリフの数以上のIDと、対応なしは `None` |
| `variation_glyph(c, vs)` | 異体字シーケンスのサブテーブルで引き、`Default` なら `nominal_glyph(c)` を返す |
| `advance(gid)` | HarfBuzzと同じ。グリフの数以上なら0。メトリクスが無ければ `unitsPerEm / 2` |
| `table(tag)` | 表のバイト列。輪郭の層が使う |

- 記号フォントの文字の付けかえ（U+0000〜U+00FFをU+F000〜U+F0FFとして引く）、MacRomanへの変換、OS/2のフォントのページによるアラビア文字の付けかえは、OS/2の表を読む段階（シェーピングの段階）のADRで足す。それまでは、選んだサブテーブルをそのまま引く。
- 可変フォントのインスタンス（正規化した座標）は、3段階目のADRで `Font` に足すか、別の値とするかを決める。

### 速さ

- `nominal_glyph` と `advance` は、`Font` が持つビューから直接引き、`sofel-read` の二分探索のほかに処理を足さない。
- HarfBuzzの持つcmapのキャッシュ（直近の対応を覚える表）に相当するものは、呼び出し側が持つ値として、シェーピングの段階で必要と計測で示せたときに足す（[ADR 0003](0003-crate-layers.md)、[0006](0006-performance-measurement.md)）。

## 実装しないこと

- sfnt以外の形式（Type1、BDFなど）。[ADR 0001](0001-project-goal-and-scope.md) の8段階目で、`FontFile` の判別とあわせて足す。
- WOFFとWOFF2。呼び出し側が展開してから渡す（[ADR 0003](0003-crate-layers.md)）。
- 縦書きのメトリクス（vhea、vmtx、VORG）。要る段階で足す。
- OS/2、name、postの表の読み取り。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。フォントは、[ADR 0007](0007-sfnt-read-foundation.md) のテストの組み立て方で作る。

- 開く：単体のsfnt、TTCの各番号、範囲外の番号、headの無いフォント、壊れた目録で、成功と `FontError` を確かめる。
- グリフの数：`maxp` だけ、`MAXP` だけ、両方（`MAXP` が勝つ）、どちらも無い（0）の4つを確かめる。
- サブテーブルの選び方：HarfBuzzの順の各組について、それより優先度の低い記録と一緒に置いたときに、その記録が選ばれることを確かめる。読めないサブテーブルは飛ばして次を選ぶことを確かめる。
- `nominal_glyph`：グリフの数以上のIDを返すサブテーブルで `None` になることを確かめる。
- `variation_glyph`：`Default`、特定のグリフ、対応なしの3つを確かめる。
- `advance`：`numberOfHMetrics` 未満、以上でグリフの数未満、グリフの数以上（0）、メトリクスなし（`unitsPerEm / 2`）を確かめる。`HHEA`/`HMTX` が `hhea`/`hmtx` より優先されることを確かめる。
- `Send`、`Sync`、`Copy`：`fn assert<T: Send + Sync + Copy>()` を呼ぶテストを置く。
- wasm：同じテストを `wasm32-wasip1` で通す。
- 実際のフォントでの突き合わせは、輪郭とラスタライズ（[ADR 0011](0011-glyf-outlines.md)、[0012](0012-antialiased-rasterizer.md)）と、シェーピングの段階のHarfBuzzのテストで行う。

## トレードオフ

- 方針をHarfBuzzに合わせるため、FreeTypeの `FT_Get_Char_Index` や `FT_Get_Advance` と結果が違う場合がある。シェーピングの基準がHarfBuzzであるため、この段階からHarfBuzzに合わせる。FreeTypeと比べる輪郭とラスタライズは、グリフIDを入力にとるため、この方針の影響を受けない。
- 記号フォントの付けかえを後回しにするため、2段階目では記号フォント（Wingdingsなど）の文字が正しいグリフに対応しない場合がある。
- `Font` を作るときにcmapのサブテーブルを選ぶため、文字の対応を使わない用途（グリフIDだけで描く用途）にも選ぶ費用がかかる。費用は記録の数に比例し、輪郭の取り出しに比べて小さい。

## 参考文献

- HarfBuzz `src/hb-ot-cmap-table.hh`（`find_best_subtable`、`_hb_symbol_pua_map`）、`src/hb-ot-hmtx-table.hh`、`src/hb-ot-maxp-table.hh`（15.1.0）
- FreeType `src/sfnt/ttload.c`、`src/sfnt/ttmtx.c`、`src/base/ftobjs.c`（`FT_Get_Char_Index`、`find_unicode_charmap`）（2.15.0）
