# ADR 0018: 輪郭のpp1.xのずらし（幻の点）

ステータス: 採択（未実装）

## 文脈

[ADR 0011](0011-glyf-outlines.md) は、幻の点（phantom points）を範囲の外とした。
その結果、FreeTypeとの突き合わせで、輪郭とラスタライズの期待する失敗が448件残っている（`conformance/expected-failures-freetype.txt` のうち理由に `pp1.x` を書いたもの）。
[ADR 0013](0013-benchmark-harness.md) の計測でも、glyfの見出しの `xMin` とhmtxの左のサイドベアリングが違うグリフ（Robotoで52、Comfortaaで6）を、ラスタライズの照合から除いている。

FreeType 2.15.0は、glyfのグリフを読むと、次のように輪郭をずらす（`src/truetype/ttgload.c`）。

- `tt_loader_set_pp`：グリフの見出しの箱（`bbox`）とhmtxの左のサイドベアリング（`left_bearing`）から、`pp1.x = bbox.xMin − left_bearing` とする。`pp2.x = pp1.x + advance`、`pp3.y = bbox.yMax + top_bearing`、`pp4.y = pp3.y − vadvance`。
- 拡大縮小するときは、幻の点も `FT_MulFix` で輪郭の点と同じ倍率をかける。ヒンティングしなければ丸めない。
- 複合グリフでは、部品を読むたびに幻の点が部品のものに置きかわる。`USE_MY_METRICS` の無い部品のあとは、読む前の幻の点に戻す。`USE_MY_METRICS` の部品では、部品の幻の点が残る。
- `TT_Load_Glyph` の最後に、`pp1.x` が0でなければ、輪郭全体を `−pp1.x` だけ横に動かす（`FT_Outline_Translate`）。`head` の `flags` のビット1（左のサイドベアリングが `xMin` と等しい）によらない。
- 左のサイドベアリングは `tt_face_get_metrics` で引く。グリフIDが `numberOfHMetrics` 未満なら長い記録から、以上ならサイドベアリングの配列から、配列が表の外なら0。hmtxが無ければ0。これは [ADR 0010](0010-font-foundation.md) のHarfBuzzの方針（送り幅）とは別の、FreeTypeの読み方である。

HarfBuzzも、glyfのグリフの箱の `x_bearing` を左のサイドベアリングにして、同じずらしを箱に反映している（`src/OT/glyf/GlyphHeader.hh`）。
FreeTypeと同じ輪郭を出すことは、[ADR 0011](0011-glyf-outlines.md) と [ADR 0012](0012-antialiased-rasterizer.md) の基準（完全一致）であり、Golemがグリフの箱をインクの箱として使うとき（[ADR 0016](0016-font-and-vertical-metrics.md)）の一貫性にも要る。

## 実装すること

- `sofel-outline` の `load` で、FreeTypeと同じく幻の点の `pp1.x` を求め、0でなければ読みこんだ全点を `−pp1.x` だけ動かす。`OutlineInfo` の `cbox` も動かしたあとの点から求める。
  - 単純なグリフ：見出しの `xMin` と、FreeTypeの読み方での左のサイドベアリングから求める。拡大縮小するときは `pp1.x` に `FT_MulFix`（`sofel_geom::mul_fix`）で横の倍率をかける。
  - 複合グリフ：FreeTypeの `load_truetype_glyph` と同じ順で、部品ごとに幻の点を保存と復元し、`USE_MY_METRICS` の部品の幻の点を残す。部品の変換と位置合わせが幻の点にどう効くか（`TT_Process_Composite_Component` で部品の点と一緒に動くか）は、FreeTypeのコードに従い、突き合わせで確かめる。
  - 空のグリフ（loca上の長さが0）：点が無いので動かさない。
- 幻の点の残りの3つ（`pp2`、`pp3`、`pp4`）は、この ADR では求めない。メトリクスは [ADR 0016](0016-font-and-vertical-metrics.md) のHarfBuzzの方針で `sofel-font` が返す。ヒンティング（7段階目）と可変フォント（3段階目）で幻の点が要るときに、そのADRで足す。
- `sofel-font` に、FreeTypeの `tt_face_get_metrics` と同じ読み方で左のサイドベアリングを返す内部の関数を足し、`sofel-outline` から使う。HarfBuzzの方針の `advance` とは別の関数とし、名前で区別する（例：`ft_left_side_bearing`）。
- 実装のプルリクエストで次を行う。
  - `conformance/expected-failures-freetype.txt` から、`pp1.x` を理由とする448件を消す。消した項目が一致することを `sofel-conformance` で確かめる（一覧にあるテストの成功はテストの失敗になる、[ADR 0005](0005-conformance-tests.md)）。
  - `bench/src/main.rs` のラスタライズの照合から、`shifted_glyphs` による除外を消す。
  - [ADR 0011](0011-glyf-outlines.md) のステータスを「採択→幻の点のpp1.xは [ADR 0018](0018-phantom-point-shift.md)」と改め、`docs/specifications.md` の輪郭の節を改める。
  - `docs/benchmarks/` に、除外なしの照合で取った記録を足す。

## 実装しないこと

- `pp2`、`pp3`、`pp4` と、幻の点から求めるメトリクス（FreeTypeの `horiAdvance` など）。
- ヒンティングによる幻の点の丸め（`TT_Hint_Glyph`）と、v40の互換の処理。
- 可変フォントで動く幻の点（gvar、HVAR、VVAR）。
- `FT_LOAD_NO_SCALE` 以外の読み込みの旗（`FT_LOAD_VERTICAL_LAYOUT` など）による違い。

## 後回しにしたもの

この ADR の実装が済んだら、次のADRを起こす（どちらも [ADR 0001](0001-project-goal-and-scope.md) の段階のADRに含めてよい）。

- **ヒンティングの幻の点**（7段階目）：4つの幻の点を命令の実行の対象に入れ、`TT_Hint_Glyph` の丸め、v40の互換の処理、ヒンティングした送り幅（`horiAdvance`）を求める。
- **可変フォントの幻の点**（3段階目）：gvarで4つの幻の点を動かし、HVARとVVARが無いフォントの送り幅と縦の原点を幻の点から求める（HarfBuzzの `get_v_origin_with_var_unscaled`、FreeTypeの `TT_Vary_Apply_Glyph_Deltas`）。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- **単体テスト**（[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方でフォントを作る）：
  - `xMin` と左のサイドベアリングが等しい単純なグリフで、輪郭が動かないこと。
  - 違う単純なグリフで、拡大縮小なしと48画素で、輪郭が `−pp1.x`（拡大縮小では `mul_fix` をかけた値）だけ動くこと。`cbox` も動くこと。
  - `numberOfHMetrics` 以上のグリフIDで、サイドベアリングの配列から引くこと、配列が足りなければ0とすること、hmtxが無ければ0とすること。
  - 複合グリフで、`USE_MY_METRICS` の無い部品だけのとき、複合グリフ自身の見出しとサイドベアリングで動くこと。`USE_MY_METRICS` の部品があるとき、その部品の幻の点で動くこと。
  - 空のグリフ。
- **FreeTypeとの突き合わせ**：`sofel-conformance` の輪郭（拡大縮小なし、16画素、48画素）とラスタライズ（12、24、48画素）で、`pp1.x` の期待する失敗が消え、全体が一致すること。
- **計測**：[ADR 0013](0013-benchmark-harness.md) の `--once` の照合が、除外なしで通ること。CIの `bench` ジョブで確かめる。
- **HarfBuzzとの整合**：[ADR 0016](0016-font-and-vertical-metrics.md) の `glyf_extents` の `x_bearing` と、拡大縮小なしの `cbox.x_min` が、単純なグリフで等しいことを、HarfBuzzのテストのフォントの全グリフで確かめる。

## トレードオフ

- 左のサイドベアリングに、HarfBuzzの方針とは別のFreeTypeの読み方を持つ。輪郭はFreeTypeとの完全一致を基準とするため（[ADR 0011](0011-glyf-outlines.md)）、読み方を分ける。範囲の外のグリフIDの扱いが2つあることを、関数の名前で示す。
- 4つの幻の点のうち1つだけを求める。残りはいま使う所が無く、求めると読み込みの費用が増える。ヒンティングと可変フォントのADRで、残りを足す手間が残る。
- 複合グリフの幻の点の扱いは、FreeTypeの実装の細部（部品の変換との関係）に依存する。仕様に書かれていないため、突き合わせのテストのフォントに無い組み合わせでは、食い違いが残る可能性がある。

## 参考文献

- FreeType `src/truetype/ttgload.c`（`tt_loader_set_pp`、`load_truetype_glyph`、`TT_Process_Simple_Glyph`、`TT_Process_Composite_Component`、`TT_Load_Glyph`）、`src/sfnt/ttmtx.c`（`tt_face_get_metrics`）（2.15.0）
- HarfBuzz `src/OT/glyf/GlyphHeader.hh`（`get_extents_without_var_scaled`）（15.1.0）
- OpenType仕様 glyf https://learn.microsoft.com/typography/opentype/spec/glyf 、head（`flags` のビット1） https://learn.microsoft.com/typography/opentype/spec/head
