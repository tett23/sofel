# ADR 0076: FreeType と同じ輪郭でグリフを縁取る

ステータス: 採択

## 文脈

文字の縁を塗るには、線幅と角の形を指定して元のグリフから塗りつぶし用の輪郭を作る。
縁だけを塗る用途と、外側または内側の境界を新たなグリフとして塗る用途に同じ処理を使う。

## 調査した事項

FreeType 2.15.0 の `src/base/ftstroke.c` では、`FT_Glyph_Stroke` と `FT_Glyph_StrokeBorder` がともに元の輪郭を閉じたパスとして処理する。
後者は元の輪郭全体について外側を決め、内側の指定を受けると反対側を取り出す。
`FT_Stroker_Set` はマイター制限（鋭い角の先端をどこまで伸ばすかを決める倍率）を 16.16 固定小数点の 1 以上に丸める。
sofel の `crates/sofel-geom/src/pen.rs` は、直線、二次曲線、三次曲線を描画命令として渡す `Pen` を定めている。

## 実装すること

`sofel-outline` に読み込み済みの輪郭を縁取る Rust API を置き、[ADR 0015](0015-c-abi.md) の C ABI にも公開する。
Rust では `sofel_outline::Scratch::stroke(&self, style: StrokeStyle, border: StrokeBorder, pen: &mut impl Pen) -> Result<(), StrokeError>` を公開する。
`StrokeStyle` は半径、角の接合方式、マイター制限をまとめ、`StrokeBorder` は出力の種類を選ぶ。
C の入口 `sofel_outline_stroke` は読み込み済みの輪郭作業領域、半径、角の接合方式、マイター制限、出力の種類、既存のペンのコールバックと利用者データを受け取る。
成功時は生成した輪郭をペンに渡す。

```c
int32_t sofel_outline_stroke(const sofel_outline_scratch_t *scratch,
                             int32_t radius, uint32_t join, int32_t miter_limit,
                             uint32_t border, sofel_move_to_func_t move_to,
                             sofel_line_to_func_t line_to, sofel_quad_to_func_t quad_to,
                             sofel_cubic_to_func_t cubic_to, sofel_close_func_t close,
                             void *user);
```

半径は線幅の半分で、元の輪郭の座標と同じ単位の非負整数とする。
接合方式は round、bevel、miter variable、miter fixed の四つから選び、C ではこの順に 0 から 3 とする。
マイター制限は符号付き 16.16 で受け取り、FreeType と同じ下限を使う。

出力の種類は、線の両側を含む縁全体、外側の境界、内側の境界の三つとし、C ではこの順に 0 から 2 とする。
元の輪郭と送り幅は変更しない。
作業領域か五つのコールバックのいずれかが NULL の場合は `SOFEL_ERROR_NULL` を返す。
それ以外で半径が負の場合と列挙値が未知の場合は `SOFEL_ERROR_INVALID_ARGUMENT` を返す。
利用者データには NULL を許す。
Rust の `StrokeError` は負の半径を `InvalidRadius`、表現範囲の超過を `TooLarge` で返す。

## データ構造とアルゴリズム

元の輪郭点、曲線の種類、各輪郭の終端から左右の境界を固定小数点で組み立てる。
曲線の分割、角の接合、整数への丸め、生成した輪郭の順序は固定版 FreeType のストローカに合わせる。
片側の出力では元の輪郭全体の向きを FreeType と同じ規則で判定してから境界を選ぶ。
穴を囲む輪郭も同じ側の選択に従う。
出力を一時領域で完成させ、出力座標が符号付き 32 ビットに収まらない場合、または出力の点数か輪郭数が 65535 を超えた場合は `SOFEL_ERROR_TOO_LARGE` に対応するエラーを返す。

## 実装しないこと

この提案は閉じたグリフ輪郭を扱う。
開いたパスと端点の形を指定する汎用ストローカ、ビットマップ画像の膨張、描画色の指定は範囲外とする。

## 不変条件

1. 指定が有効で、参照実装が成功し、結果が前述の範囲に収まるときは成功し、出力の描画命令列は FreeType の対応する操作の結果を `FT_Outline_Decompose` で分解し、各輪郭の末尾に sofel の `close` を補った列と一致する。
2. 同じ元の輪郭から別の設定で続けて呼んでも、各結果は元の輪郭から一回ずつ生成した結果と等しい。
3. 引数エラーまたは表現範囲超過のとき、ペンの呼び出し回数は 0 で、元の輪郭を引き続き使える。

| 入力 | 期待する結果 |
| --- | --- |
| 空輪郭 | 成功し、描画命令は 0 件 |
| 三角形一つ、半径 0 と 1 | 各半径で条件 1 を満たす |
| 鋭角、マイター制限が 1 の直前、ちょうど 1、直後 | 下限適用後の各接合方式で条件 1 を満たす |
| 穴のある輪郭と全輪郭の向きを反転したもの | 三つの出力指定それぞれで条件 1 と 2 を満たす |
| 半径が負、または出力座標が上限を 1 超える | 条件 3 を満たす |

## テスト設計

参照ツール `tools/ft-dump` のビルドに FreeType の `base/ftstroke.c` を加え、縁取り後の輪郭を描画命令列として出力できるようにする。
`inv1_stroke_reference` で同じ整数座標の直線、二次曲線、三次曲線を固定版 FreeType と sofel に渡す。
縁全体は `FT_Glyph_Stroke`、片側は `FT_Glyph_StrokeBorder` を照合先とし、閉じた輪郭用の端点設定は butt に固定する。
境界表に加え、連続する同一点、鋭角のマイターが切り替わる前後も含めて命令の種類、順序、全座標を比較する。
`inv2_stroke_reuse` は処理前後の元の輪郭も比較する。
`inv3_stroke_failure` は C の入口で不正な設定と範囲超過を与え、出力の途中通知がないことを調べる。
これらのテストを先に追加して失敗を確認してから実装する。

## トレードオフ

一時領域には縁取り後の点数に応じたメモリを要する。
輪郭をすべて生成してから通知するため、描画側はエラー時に途中まで受け取った命令を取り消す処理を持たずに済む。

## 参考文献

- [FreeType の Glyph Stroker](https://freetype.org/freetype2/docs/reference/ft2-glyph_stroker.html)
- [FreeType のストローカ実装](https://github.com/freetype/freetype/blob/VER-2-15-0/src/base/ftstroke.c)
