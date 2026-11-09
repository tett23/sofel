# ADR 0009: `sofel-geom` の固定小数点の演算と `Pen`

ステータス: 採択

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の2段階目は、glyfの輪郭とアンチエイリアスのラスタライズである。
輪郭は、FreeTypeと座標が完全に一致することを求める（[ADR 0005](0005-conformance-tests.md)）。
FreeTypeは、拡大縮小と変換を固定小数点で計算する。
一致させるには、同じ丸めの演算を同じ順序で行う必要がある。

FreeType 2.15.0の演算は次のとおりである（`include/freetype/internal/ftcalc.h`、`src/base/ftcalc.c`）。

- `FT_MulFix(a, b)`：`ab = (i64)a * b; ab += 0x8000 + (ab >> 63); return ab >> 16;`。0.5は0から遠い方へ丸める。
- `FT_DivFix(a, b)`：符号と絶対値に分け、`b > 0` なら `((a << 16) + (b >> 1)) / b`、`b == 0` なら `0x7FFFFFFF` とし、符号を戻す。
- `FT_MulDiv(a, b, c)`：絶対値で `(a * b + c / 2) / c`。`FT_MulDiv_No_Round` は切り捨てる。
- 合成グリフの成分の位置の拡大（`SCALED_COMPONENT_OFFSET`）には `FT_Hypot` を使う。これはCORDICによる `FT_Vector_Length` である（`src/base/fttrigon.c`）。
- 2x2の変換は `FT_Vector_Transform` で、`x' = MulFix(x, xx) + MulFix(y, xy)`、`y' = MulFix(x, yx) + MulFix(y, yy)` と計算する（`src/base/ftoutln.c`）。

輪郭を作る `sofel-outline` と、塗る `sofel-raster`、のちの `sofel-gpu` と `sofel-vector` は、輪郭の命令を受け渡す共通の型を要する。
[ADR 0003](0003-crate-layers.md) は、これを依存のない `sofel-geom` に置くと決めた。

## 実装すること

### クレート

- `crates/sofel-geom` を作る。`#![no_std]`、確保なし、`#![forbid(unsafe_code)]`、依存なし（[ADR 0003](0003-crate-layers.md)、[0004](0004-wasm-and-no-std.md)）。
- panicしないことを、[ADR 0007](0007-sfnt-read-foundation.md) と同じclippyのlintで強制する。演算のあふれは、FreeTypeと同じ結果になるよう、64bitの中間値と明示の折り返しで扱う。

### 数の型

- `F26Dot6(i32)`：26.6の固定小数点（画素の1/64）。
- `Fixed(i32)`：16.16の固定小数点。
- `F2Dot14(i16)`：2.14の固定小数点。`Fixed` への変換は `v << 2`（FreeTypeと同じ）。
- `sofel-read` の生の値の型（`sofel_read::Fixed` など）からの変換は、`sofel-font` で行う。`sofel-geom` は `sofel-read` に依存しない。

### 演算

FreeType 2.15.0と同じ結果を返す関数として書く。

- `mul_fix(a: i32, b: i32) -> i32`、`div_fix(a: i32, b: i32) -> i32`、`mul_div(a, b, c)`、`mul_div_no_round(a, b, c)`。
- `vector_length(x, y) -> i32`（`FT_Vector_Length`、`FT_Hypot`）。FreeTypeのCORDICの表と手順を移植する。
- `Matrix { xx, xy, yx, yy: Fixed }` と、`FT_Vector_Transform` と同じ `transform(point, matrix)`。
- FreeTypeのコードを移植するため、移植したファイルの先頭に流用元を書き、`THIRD_PARTY_NOTICES.md` にFTLの全文を、READMEにFTLの推奨するクレジットの文を載せる（[ADR 0002](0002-license-and-reuse.md)）。

### 点と `Pen`

- `Point { x: i32, y: i32 }`。単位は作る側が決める（拡大縮小した輪郭なら26.6、拡大縮小しない輪郭ならフォントの単位）。
- 輪郭を受け取る `Pen` トレイトを置く。`move_to(p)`、`line_to(p)`、`quad_to(c, p)`、`cubic_to(c1, c2, p)`、`close()` を持つ。
- 命令の並びは、FreeTypeの `FT_Outline_Decompose` が出すものと同じ約束に従う。輪郭ごとに `move_to` で始まり、始点へ戻る線か曲線を明示して `close` で終わる。
- 塗りの命令を受け取る `Painter` は、カラーグリフの段階のADRで足す。

## 実装しないこと

- 浮動小数点の点の型と演算。SVG、PDF、GPUへの出力の段階で要るなら、そのADRで足す。
- 角度と三角関数（`FT_Vector_Rotate`、`FT_Atan2` など）。要る段階で足す。
- 輪郭を確保して持つ型（命令の列の記録）。要るクレートが、自分の作業領域として持つ。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- 各演算：FreeTypeのCの式を `i128` でそのまま書いた参照の関数と、境界の値（0、±1、`i32::MIN`、`i32::MAX`、±0x8000の前後、`b == 0`）と、自分で書く疑似乱数（xorshift）による多数の入力で、結果が一致することを確かめる。
- `vector_length`：(0, 0)、軸の上の点、(3, 4) の倍数、`i32` の端の値で、FreeTypeの結果と一致することを確かめる。FreeTypeの結果は、[ADR 0008](0008-validate-command-and-freetype-comparison.md) の `ft-dump` に、引数で受け取った値の `FT_Vector_Length` を書き出す小さな動作を足して得る。突き合わせは `sofel-conformance` のテストとする。
- `transform`：単位行列、拡大、回転に近い値、負の値で、手で計算した値と一致することを確かめる。
- `Pen`：命令を記録する小さな `Pen` で、呼んだ順に記録されることを確かめる（後続のADRのテストの道具でもある）。
- wasm：同じテストを `wasm32-wasip1` で通す。

## トレードオフ

- FreeTypeと同じ丸めの演算に縛られるため、浮動小数点やSIMDに置きかえて速くすることはできない。輪郭の完全一致（[ADR 0005](0005-conformance-tests.md)）の代償として受け入れる。
- `Point` の単位を型で区別しないため、26.6とフォントの単位を取り違えても型の検査で見つからない。単位ごとに型を分けると、`Pen` が単位ごとに要り、ラスタライザやベクトル出力の実装が増える。単位は作る側の関数の名前と文書で示す。
- FreeTypeのCORDICの移植により、sofelの利用者にFTLのクレジットの表示の義務が生じる（[ADR 0002](0002-license-and-reuse.md)）。

## 参考文献

- FreeType `include/freetype/internal/ftcalc.h`、`src/base/ftcalc.c`、`src/base/fttrigon.c`、`src/base/ftoutln.c`（2.15.0）
