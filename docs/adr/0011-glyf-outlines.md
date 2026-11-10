# ADR 0011: glyfの輪郭（`sofel-read` のglyfとloca、`sofel-outline`）

ステータス: 採択→幻の点のpp1.xは [ADR 0018](0018-phantom-point-shift.md)

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の2段階目は、glyfの輪郭を取り出し、1グリフを画像にできるところまでである。
輪郭は、ヒンティングの有無にかかわらず、FreeTypeと座標が完全に一致することを求める（[ADR 0005](0005-conformance-tests.md)）。
この段階ではヒンティングをしない。

FreeType 2.15.0は、ヒンティングをしないTrueTypeの輪郭を次のように作る（`src/truetype/ttgload.c`、`src/truetype/ttpload.c`、`src/base/ftoutln.c`、`src/base/ftobjs.c`）。

- 大きさ：`FT_Set_Pixel_Sizes` は、`x_scale = FT_DivFix(ppem << 6, unitsPerEm)`（yも同じ）とする。`FT_LOAD_NO_HINTING` では、この丸めていない倍率を使う。`FT_LOAD_NO_SCALE` では拡大縮小しない。
- 単純なグリフ：各点を `FT_MulFix(x, x_scale)`、`FT_MulFix(y, y_scale)` とし、丸めない。フラグ0x40（`OVERLAP_SIMPLE`）は最初の点だけを見て、輪郭に重なりの印を付ける。フラグ0x80は、大文字の `GLYF` を使うフォント（ISO OFF第5版）でだけ三次曲線の制御点を表す。
- 合成グリフ：成分を拡大縮小してから、2x2の変換（F2Dot14を4倍して16.16にする）をかける。位置は、XYの値なら `SCALED_COMPONENT_OFFSET` のとき `FT_Hypot` で拡大してから倍率をかけ、点の対応なら拡大縮小済みの点の差とする。ヒンティングをしないときは `ROUND_XY_TO_GRID` を無視する。再帰が100を超えるか、同じグリフに戻ると誤りとする。最初の成分の `OVERLAP_COMPOUND` で重なりの印を付ける。成分のグリフIDは、`GLYF` のフォントでフラグ0x2000のとき24bitである。
- 表の選び方：`GLYF` があれば `GLYF` と `LOCA` を、無ければ `glyf` と `loca` を使う。`LOCA` も、短い形式（2倍した16bit）と長い形式（32bit）は `head.indexToLocFormat` で決まる。
- locaの解釈（`tt_face_get_location`）：始まりがglyfの外なら長さ0。終わりがglyfの外なら、最後のグリフに限ってglyfの終わりまでに切り、ほかは長さ0。終わりが始まりより前なら、glyfの終わりまでとする。locaの項目がグリフの数+1より少なく、ファイルに余地が無ければ、グリフの数を減らす。
- 命令への変換（`FT_Outline_Decompose`）：暗黙の曲線上の点は `(a + b) / 2`（0へ向けて切り捨て）。輪郭の最初の点が曲線上にないとき、最後の点が曲線上ならそこから始め、そうでなければ最初と最後の点の中点から始める。各輪郭は、始点へ戻る線か曲線で閉じる。

## 実装すること

### `sofel-read`：glyfとloca

- `Loca`：`loca` と `LOCA` のビュー。`indexToLocFormat` を引数にとる。`offset(i)` は生のオフセット（短い形式は2倍した値）を返す。
- `Glyph`：glyfの1グリフのビュー。`numberOfContours`、`xMin`〜`yMax`、単純なグリフの輪郭の終点の配列、命令の長さ、フラグと座標の復号のイテレータ（点ごとに `(x, y, on_curve, flag)` を返す。フラグの繰り返しと座標の差分を展開する）、合成グリフの成分のイテレータ（フラグ、グリフID、引数、変換）を持つ。`extended`（`GLYF` であるか）を引数にとり、0x80と0x2000の扱いを変える。
- 確保をしない。復号は、呼ばれたときにバイト列を前から読む。
- 検証器：
  - `loca`：項目の数（グリフの数+1）に足りない（Error）、オフセットがglyfの外（Error）、昇順でない（Error）、短い形式の上限の超過。
  - `glyf`：各グリフについて、ヘッダの不足、フラグや座標が長さを超える（Error）、終点の配列が昇順でない（Error）、成分のグリフIDがグリフの数以上（Error）、予約のフラグ（Warning）。
  - `sofel-validate` の対象に加え、`validate::sfnt` が `head`、`maxp`/`MAXP` と組み合わせて呼ぶ（[ADR 0007](0007-sfnt-read-foundation.md)、[0008](0008-validate-command-and-freetype-comparison.md)）。

### `sofel-outline`

- `crates/sofel-outline` を作る。`#![no_std]` と `alloc`、`#![forbid(unsafe_code)]`。依存は `sofel-font` と `sofel-geom` だけとする（[ADR 0003](0003-crate-layers.md)）。
- `Scale`：`Unscaled`（フォントの単位）か、`Scaled { x: Fixed, y: Fixed }`（26.6を出す倍率）。`Scale::pixels(font, x_ppem, y_ppem)` は `FT_Set_Pixel_Sizes` と同じ倍率を作る。
- `Scratch`：点、フラグ、輪郭の終点、合成の途中の状態を入れる作業領域。呼び出し側が持ち、グリフをまたいで再利用する。確保は、足りないときに伸ばす分だけとする（[ADR 0003](0003-crate-layers.md) の「キャッシュは呼び出し側の値」）。
- `draw(font, glyph, scale, scratch, pen) -> Result<OutlineInfo, OutlineError>`：
  - 上のFreeTypeの手順のとおりに、単純なグリフと合成グリフの点を作り、`FT_Outline_Decompose` と同じ命令の列を `pen` に渡す。
  - `OutlineInfo` は、重なりの印と、すべての点（曲線上にない点を含む）の最小と最大（FreeTypeの `FT_Outline_Get_CBox` と同じ箱）を持つ。
  - 誤り（グリフIDの範囲外、壊れたグリフ、合成の再帰の上限）は `OutlineError` として返す。途中まで `pen` に渡したかどうかは問わない。
- 速さ：グリフのバイト列を1回だけ前から読み、`Scratch` に点を展開しながら拡大縮小する。拡大縮小しないとき（`Unscaled`）は、乗算を省く。
- FreeTypeの手順を移植するため、[ADR 0002](0002-license-and-reuse.md) に従って流用元とFTLを表示する。

### FreeTypeとの突き合わせ

- `ft-dump` に `--outline <SIZE>` を足す（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。`<SIZE>` は `unscaled` か画素数。各フェイスの全グリフについて、`FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP`（`unscaled` では加えて `FT_LOAD_NO_SCALE`）で読み、`FT_Outline_Decompose` の命令を書き出す。読めないグリフは誤りの番号を書く。重なりの印と制御の箱も書く。
- `sofel-conformance` に、glyfか `GLYF` を持つフェイスについて、`unscaled`、16画素、48画素で、命令の列、重なりの印、制御の箱がFreeTypeと完全に一致することを確かめるテストを足す。項目は `<フォント>#<フェイス>:outline:<SIZE>:<グリフ>` とする。
  - FreeTypeとsofelがともに誤りとするグリフは一致とみなす。片方だけが誤りなら不一致とする。
  - 可変フォントは既定のインスタンス（変化量なし）だけを比べる。
- 期待する失敗の一覧は、[ADR 0008](0008-validate-command-and-freetype-comparison.md) と同じ `conformance/expected-failures-freetype.txt` とする。

## 実装しないこと

- ヒンティング（[ADR 0001](0001-project-goal-and-scope.md) の7段階目）と、それに伴う `ROUND_XY_TO_GRID` や命令の実行。
- 可変フォントの補間（gvar。3段階目）。
- 幻の点（phantom points）と、グリフの読み込みによるメトリクスの計算。送り幅は `sofel-font` の `advance` を使う（[ADR 0010](0010-font-foundation.md)）。
- CFF、CFF2、VARCの輪郭（6段階目と13段階目）。
- 埋め込みビットマップ（5段階目）。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。フォントは、[ADR 0007](0007-sfnt-read-foundation.md) のテストの組み立て方にglyfとlocaの組み立てを足して作る。

- `Loca`：短い形式と長い形式、`LOCA` の24bitのグリフの数に対応する項目の数で、オフセットを確かめる。
- `Glyph` の復号：フラグの繰り返し、x/yの短い形式と符号、同じ値の省略、0x40、0x80（`extended` の有無で意味が変わる）を、手で組んだバイト列で確かめる。範囲外を読むバイト列で、イテレータが途中で止まり、panicしないことを確かめる。
- 成分のイテレータ：引数の4つの形式（8/16bit、符号つきXY/符号なしの点番号）、3種の変換、0x2000の24bitのグリフIDを確かめる。
- 検証器：各誤りについて、期待する種類、重大度、オフセットが報告されることを確かめる。[ADR 0007](0007-sfnt-read-foundation.md) の不変条件（Errorが無ければ読み取りが成功する）を、変異させた入力で確かめる。
- `draw`：手で組んだグリフで、命令の列を確かめる。
  - 曲線上の点だけの輪郭、二次曲線の制御点が連続する輪郭（暗黙の点と0への切り捨て）、最初の点が曲線上にない2つの場合、1点だけの輪郭、空のグリフ。
  - 三次曲線（`GLYF` のときだけ）。
  - 合成グリフ：XYの位置、点の対応、拡大、xyの拡大、2x2、`SCALED_COMPONENT_OFFSET`、入れ子、再帰の上限、自己参照。
  - 拡大縮小した座標が `mul_fix` と一致すること。
- 実際のフォントでの完全一致は、上の `sofel-conformance` のテストで確かめる。`ft-dump --outline` の正しさは、値の分かっているグリフを持つ小さなフォントで、出力が手で書いた期待の出力と一致することを確かめる（[ADR 0008](0008-validate-command-and-freetype-comparison.md) と同じ方式）。
- ファジング：`fuzz/` に、任意のバイト列のフォントの全グリフで `draw` を呼ぶ対象を足し、panicしないことを確かめる。
- wasm：`sofel-read` と `sofel-outline` の単体テストを `wasm32-wasip1` で通す。

## トレードオフ

- locaの解釈と合成の位置の計算をFreeTypeに合わせるため、HarfBuzzの描画（`hb_font_draw_glyph`）と、壊れたフォントで結果が違うことがある。正しさの基準として、輪郭はFreeTypeを選ぶ（[ADR 0005](0005-conformance-tests.md)）。
- 三次曲線を `GLYF` のフォントに限るのはFreeTypeの解釈で、小文字の `glyf` でフラグ0x80を使うフォントは二次曲線として描く。
- `Scratch` を呼び出し側が持つため、APIの使い方が1段増える。グリフごとの確保を避け、スレッドごとに作業領域を分けられる利点を取る。
- 輪郭の取り出しの速さは、[ADR 0013](0013-benchmark-harness.md) の計測の道具ができてから測る。

## 参考文献

- FreeType `src/truetype/ttgload.c`、`src/truetype/ttpload.c`、`src/sfnt/sfobjs.c`、`src/base/ftoutln.c`、`src/base/ftobjs.c`（2.15.0）
- HarfBuzz `src/OT/glyf/`（15.1.0）
- OpenType Specification：glyf、loca https://learn.microsoft.com/en-us/typography/opentype/spec/
- ISO/IEC 14496-22 第5版（`GLYF`、`LOCA`、三次曲線のフラグ）
