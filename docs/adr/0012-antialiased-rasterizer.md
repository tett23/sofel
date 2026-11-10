# ADR 0012: アンチエイリアスのラスタライズ（`sofel-raster`）と `sofel-render`

ステータス: 採択

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の2段階目の目標は、1グリフを画像にできることである。
ラスタライズの結果の許容差は、ラスタライズのADRで決める（[ADR 0005](0005-conformance-tests.md)）。

FreeType 2.15.0のsmoothのラスタライザ（`src/smooth/ftgrays.c`、`src/smooth/ftsmooth.c`）は次のように動く。

- 26.6の座標を4倍して24.8にする（`PIXEL_BITS` は8）。
- 画素ごとの「セル」に、被覆の量（`cover`）と面積（`area`）を積む（`FT_INTEGRATE`）。セルは、行ごとにxの順の連結リストとして、固定の大きさの領域（16384バイト）か、見積もりが大きければヒープに置く。足りなければ、行を半分の帯に分けて描きなおす。
- 線分（`gray_render_line`）は、64bitの積と、`FT_UDIV(a, b) = (a * (0xFFFFFFFF / b)) >> 32` という近似の割り算で、セルをまたぐ位置を求める。
- 二次曲線（`gray_render_conic`）は、制御点のずれから分割の数を2のべきで決め、前進差分で線分に分ける。三次曲線（`gray_render_cubic`）は、制御点のずれが半画素を超える間、二等分する。
- 掃き出し（`gray_sweep`）は、行ごとに `cover` を累積し、`area >> 9` を被覆率とする。非ゼロ規則では符号を反転して255で止め、偶奇規則では下位8bitを使う。
- ビットマップの位置と大きさ（`ft_glyphslot_preset_bitmap`）は、すべての点の最小と最大の箱を画素の格子へ外向きに丸めて決める（`bitmap_left`、`bitmap_top`、幅、行数）。幅か高さが0x10000以上か、`ppem` の16倍を超えると描かない。
- 輪郭に重なりの印（glyfの `OVERLAP_SIMPLE`、`OVERLAP_COMPOUND`）があると、座標を4倍して4×4の副標本で描き、`(coverage + 8) / 16` を足し合わせて255で止める（`ft_smooth_raster_overlap`）。

セルを連結リストで持つのは、帯ごとの描画に固定の大きさの領域を使い回すためである。
セルに積む値（`cover` と `area`）は、格納の仕方によらない。
同じ演算で同じ値を積めば、密な配列に積んでも同じ被覆率が出る。

## 実装すること

### `sofel-raster`

- `crates/sofel-raster` を作る。`#![no_std]` と `alloc`、`#![forbid(unsafe_code)]`。依存は `sofel-geom` だけとする（[ADR 0003](0003-crate-layers.md)）。
- `Rasterizer` は `sofel_geom::Pen` を実装し、26.6の命令の列を自分の作業領域に記録する。呼び出し側が持ち、グリフをまたいで再利用する。
- `render(&mut self, fill: FillRule, overlap: bool) -> Bitmap<'_>` は、記録した命令の制御の箱から位置と大きさを決め、8bitの被覆率の画像を返す。`Bitmap` は `left`、`top`、`width`、`rows` と、上の行から並べた画素の列を持つ。
- 数値の計算はFreeTypeと同じとする：24.8への変換、`gray_render_line` の手順と `FT_UDIV`、二次曲線の前進差分、三次曲線の二等分の基準、`FT_FILL_RULE`、重なりのときの4×4の副標本と足し合わせ。
- 格納だけを変える：セルを連結リストではなく、帯の幅×高さの密な配列（`cover` と `area` の組）に積む。帯の高さは、配列が上限（64KiB）に収まるように決め、それを超える高さのグリフは帯ごとに命令の列を描きなおす。掃き出しは、帯の行ごとに左から累積する。
- 描かない場合（幅か高さが0x10000以上）は、空の `Bitmap` を返す。`ppem` の16倍を超える場合の扱いは、`ppem` を知る `sofel-render` が決める。
- FreeTypeの計算を移植するため、[ADR 0002](0002-license-and-reuse.md) に従って流用元とFTLを表示する。

### 許容差

- FreeTypeとの差は、画素ごとに0とする（完全一致）。位置と大きさも一致を求める。
- 速さのために計算を変えたくなった場合は、許容差を変えるADRを作成する。

### `sofel-render`

- `sofel-cli` にバイナリ `sofel-render` を足す。
- `sofel-render [--face N] --ppem P (--glyph G | --char U+XXXX) FILE` は、ヒンティングをせずに1グリフを描き、PGM（P5、8bitの灰色）を標準出力に書く。白地に黒で書く（被覆率を反転する）。
- `bitmap_left` と `bitmap_top` は、PGMの注釈の行（`# left <x> top <y>`）に書く。
- グリフの選び方は `sofel-font` の `nominal_glyph` に従う（[ADR 0010](0010-font-foundation.md)）。対応の無い文字、空のグリフ、描かない大きさのグリフでは、終了コード1で理由を標準エラー出力に書く。使い方の誤りは2とする。
- `ppem` の16倍を超える箱のグリフは、FreeTypeと同じく描かない。

### FreeTypeとの突き合わせ

- `ft-dump` に `--raster <PPEM>` を足す。各フェイスの全グリフを `FT_LOAD_NO_HINTING`、`FT_RENDER_MODE_NORMAL` で描き、`bitmap_left`、`bitmap_top`、幅、行数と、画素を16進で書き出す。
- `sofel-conformance` に、glyfか `GLYF` を持つフェイスについて、12、24、48画素で、FreeTypeと画像が完全に一致することを確かめるテストを足す。項目は `<フォント>#<フェイス>:raster:<PPEM>:<グリフ>` とする。輪郭が一致しない（[ADR 0011](0011-glyf-outlines.md) の項目が失敗する）グリフも、この項目で比べる。

## 実装しないこと

- モノクロ、LCD、SDFのラスタライズ（[ADR 0001](0001-project-goal-and-scope.md) の7段階目）。
- 塗り（グラデーション）の合成とカラーグリフ（5段階目）。
- 斜体や太字の合成（`FT_GlyphSlot_Embolden` など）。
- PNGなどPGM以外の画像の形式。
- SIMDでの掃き出し。計測（[ADR 0013](0013-benchmark-harness.md)）で必要と示せたら、スカラーの実装を残してADRで足す（[ADR 0004](0004-wasm-and-no-std.md)）。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- 線分：水平、垂直、対角、セルの境界をちょうど通る線、1画素より短い線、負の座標で、積む `cover` と `area` をFreeTypeの式を直接書いた参照の関数と比べる。`FT_UDIV` の近似の割り算が、参照と一致することを確かめる。
- 曲線：二次曲線の分割の数の境界（ずれが64の前後）、三次曲線の二等分の基準の境界で、出す線分の列を参照と比べる。
- 掃き出し：非ゼロ規則と偶奇規則、255で止まる場合、反転の場合を確かめる。
- 帯：配列の上限を小さくしたときと大きくしたときで、同じ画像になることを確かめる（帯の分け方が結果を変えないこと）。
- 重なり：重なる2つの四角で、4×4の副標本の結果を確かめる。
- 位置と大きさ：負の座標、ちょうど格子の上の箱、空の輪郭、0x10000以上の箱を確かめる。
- `sofel-render`：値の分かっている小さなフォントで、PGMの見出し、注釈、画素と、終了コード（0、1、2）を確かめる。
- 実際のフォントでの完全一致は、上の `sofel-conformance` のテストで確かめる。`ft-dump --raster` の正しさは、塗りつぶした四角など結果が手で計算できるグリフで確かめる。
- wasm：`sofel-raster` の単体テストを `wasm32-wasip1` で通す。

## トレードオフ

- 完全一致を求めるため、浮動小数点の面積の計算（font-rsなどの方式）やSIMDの累積など、より速い可能性のある方式を選べない。密な配列にすることで、FreeTypeの連結リストの探索と、領域が足りないときの描きなおしは省ける。速さがFreeTypeに届かなければ、計測の記録を添えて許容差を変えるADRを作成する。
- 帯の配列の上限を64KiBとするため、大きなグリフ（数百画素）では帯ごとに命令の列を描きなおす。FreeTypeも同じく描きなおす。
- 重なりのある輪郭は4×4の副標本で描くため、16倍の描画の費用がかかる。FreeTypeと同じである。
- PGMはほとんどの画像の閲覧の道具で開けるが、PNGより大きい。PNGの書き出しは、PNGの符号化器を要するため後に回す。

## 参考文献

- FreeType `src/smooth/ftgrays.c`、`src/smooth/ftsmooth.c`、`src/base/ftobjs.c`（`ft_glyphslot_preset_bitmap`）、`src/base/ftoutln.c`（`FT_Outline_Get_CBox`）（2.15.0）
- Netpbm PGM format https://netpbm.sourceforge.net/doc/pgm.html
