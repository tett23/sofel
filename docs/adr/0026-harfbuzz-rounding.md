# ADR 0026: HarfBuzzの丸めと浮動小数点の写し方（`roundf` の訂正）

ステータス: 採択

## 文脈

いくつかのADRは、HarfBuzzの値を写すときの丸めを「`roundf`（半分を0から遠い側へ丸める）」と書いた。

- [ADR 0016](0016-font-and-vertical-metrics.md)：拡大縮小の `em_multf` を `roundf(v × multf)` とし、`roundf` は自前で書くとした。`x_mult` を「`x_scale × 0x10000 / upem`（64bit、0へ切り捨て）」とした。
- [ADR 0021](0021-cff-outlines.md)：CFFの箱の各端を「まず `float` への変換と同じ規則で丸め、次に半分を0から遠い側へ丸める（`roundf`）」とした。
- [ADR 0022](0022-subsetting.md)：軸の固定などで、HarfBuzzと同じ丸め（`roundf`）を使うとした。
- [ADR 0023](0023-color-glyphs.md)：色のアルファを `roundf` で丸め、「`round` は0.5を0から遠い方へ丸める（`roundf` と同じ）」とした。

HarfBuzz 15.1.0は、`src/hb-algs.hh` で `roundf` をマクロで置きかえている。

```c
static inline double _hb_roundf (double x) { return floor (x + .5); }
static inline float  _hb_roundf (float x)  { return floor ((double) x + .5); }
#define roundf(x) _hb_roundf(x)
```

このため、HarfBuzzのソースの `roundf` は、Cの `roundf` ではない。
値を倍精度に広げてから0.5を足し、床関数をとる。半分の値は、正でも負でも、正の無限大の側へ丸まる（−2.5は−2、2.5は3）。Cの `roundf`（0から遠い側。−2.5は−3）とは、負の半分の値で結果が違う。`float` への変換も挟まない。

[ADR 0016](0016-font-and-vertical-metrics.md) と [ADR 0021](0021-cff-outlines.md) の実装では、HarfBuzzとの完全一致の突き合わせ（[ADR 0005](0005-conformance-tests.md)）でこの違いが見つかり、実装はHarfBuzzの実際の手順に合わせた。

- `sofel-font` の `float::roundf` は `floor(f64::from(v) + 0.5)` とした。
- `sofel-outline` のCFFの箱（`hb_cff.rs`）は、16.16の値に `0x8000` を足して16bit右へずらす整数の計算とした。16.16の値は倍精度で正確に表せるので、`floor(x + 0.5)` と常に等しい。
- `x_mult` は、HarfBuzzの `(int64_t) x_scale * 0x10000 / upem` で `upem` が `float` のため、割り算が `float` で行われ、結果を0へ切り捨てる。実装はこれに合わせた。

ADRの記述は実装と食い違ったままである。また、同じ丸めの関数が `sofel-font` に閉じていて、これから `sofel-color`（[ADR 0023](0023-color-glyphs.md)）と `sofel-subset`（[ADR 0022](0022-subsetting.md)）も同じものが要る。

## 実装すること

### 決まり

- sofelがHarfBuzzの値を写す箇所の「`roundf`」は、すべて `hb-algs.hh` の `_hb_roundf` の意味とする。引数が `float` なら倍精度に広げ、0.5を足し、床関数をとる。結果は倍精度の値とし、整数にするときは、HarfBuzzが使う変換（`hb_clamp_to`、`(int)` の切り捨てなど）をその箇所ごとに写す。
- Cの `roundf`、`lroundf`、`nearbyint` の意味が要る箇所（FreeTypeの値を写すときなど）は、関数の名前で区別する。
- HarfBuzzの式の中の型（`int`、`int64_t`、`float`、`double`）の混ざり方は、C++の通常の算術変換に従って写す。例えば、整数と `float` の割り算は `float` で行う。ADRに式を書くときは、型を添える。
- 浮動小数点の演算は、HarfBuzzと同じ精度の四則演算と、上の床関数だけとする。FMA（`mul_add`）は使わない。

### 置き場所

- `sofel-font` の `float` モジュールの関数（`floor`、`floorf`、`ceilf`、`truncf`、`_hb_roundf` に当たる関数、`hb_clamp_to` に当たる変換）を、`sofel-geom` の公開のモジュール `sofel_geom::float` に移す。`sofel-font`、`sofel-outline`、`sofel-color`、`sofel-subset` は、すべてここを使う。[ADR 0003](0003-crate-layers.md) の依存の向き（どれも `sofel-geom` に依存してよい）に沿う。
- `_hb_roundf` に当たる関数は、引数の型ごとに名前を分ける（例えば `hb_roundf(f64) -> f64` と `hb_roundf32(f32) -> f64`）。説明に、HarfBuzzの定義と、Cの `roundf` と違う点を書く。
- `sofel-outline` のCFFの箱の整数の計算は、`floor(x + 0.5)` と常に等しいことを説明に書いて残す（16.16の整数の計算のほうが速く、倍精度を通さない）。等しいことはテストで確かめる。
- 公開の関数になるので、[ADR 0015](0015-c-abi.md) の決まりに従う。C ABIでは使い道が無いため、`crates/sofel-capi/unmapped.txt` に理由を添えて書く。

### ADRのステータス

この ADR の実装のプルリクエストで、次のステータスの行を改める。

- [ADR 0016](0016-font-and-vertical-metrics.md)：「採択→`roundf` と `x_mult` の写し方は [ADR 0026](0026-harfbuzz-rounding.md)」
- [ADR 0021](0021-cff-outlines.md)：「採択→箱の丸めは [ADR 0026](0026-harfbuzz-rounding.md)」
- [ADR 0022](0022-subsetting.md)、[ADR 0023](0023-color-glyphs.md)：いまのステータスに「→丸めは [ADR 0026](0026-harfbuzz-rounding.md)」を足す。
- `docs/specifications.md` の丸めの記述を、この ADR に合わせる。

## 実装しないこと

- 丸めの結果を変えること。実装はすでにHarfBuzzと一致しているので、この ADR は記述を正し、関数の置き場所を1つにするだけである。
- FreeTypeの丸め（`FT_MulFix` などの固定小数点の丸め）の変更。[ADR 0009](0009-geom-fixed-point-and-pen.md) のとおりとする。

## テスト設計

実装のまえに、次のテストを書いて失敗させる（移す前の関数の場所では、`sofel_geom::float` が無いので失敗する）。

- **丸めの単体テスト**：`hb_roundf` と `hb_roundf32` について、−2.5→−2、−1.5→−1、−0.5→0（符号つきの0の扱いも、HarfBuzzの `floor` の結果と同じにする）、0.5→1、2.5→3、整数、0.49999997（`float` で0.5未満の最大の値）、2^23以上と2^52以上の値、NaNと無限大を確かめる。Cの `roundf` との違いが出る値（負の半分）を必ず含める。
- **HarfBuzzとの突き合わせ**：`tools/hb-dump/` に、`float` と `double` の値の列を読んで `roundf` を通した値を書く `--roundf` を足し、上の値と乱数の値（決まった種で100万個。半分の値を多く含むように作る）で、`sofel_geom::float` の結果と完全一致することを確かめる。
- **CFFの整数の計算**：`hb_cff.rs` の整数の丸めと `hb_roundf` が、16.16の全範囲の境界の値（各整数の±0.5の前後、最小と最大）と乱数の値で等しいことを確かめる。
- **`x_mult`**：`upem` を16、1000、1024、2048、16384、`x_scale` を負の値を含む範囲で動かし、`hb-dump` の `x_mult` と等しいことを確かめる（[ADR 0016](0016-font-and-vertical-metrics.md) の拡大縮小の突き合わせに足す）。
- **既存の突き合わせ**：[ADR 0016](0016-font-and-vertical-metrics.md)、[ADR 0017](0017-base-baselines.md)、[ADR 0021](0021-cff-outlines.md) のHarfBuzzとの突き合わせが、移したあとも同じ結果で通ることを確かめる。

## トレードオフ

- `sofel-geom` に、固定小数点のほかに浮動小数点の補助を置く。`sofel-geom` は「固定小数点の数と輪郭の命令」の層だが（[ADR 0003](0003-crate-layers.md)）、ここに置けば `sofel-font` と `sofel-color` と `sofel-subset` が同じ関数を共有でき、同じ丸めを3か所に書かずに済む。
- 関数の名前に `hb` を付けて、HarfBuzzの意味であることを示す。一般の丸めの関数として使われると、Cの `roundf` と違う結果に驚かれるためである。

## 参考文献

- HarfBuzz `src/hb-algs.hh`（`_hb_roundf`、`roundf` のマクロ）、`src/hb-font.hh`（`changed`、`em_multf`）、`src/hb-ot-cff1-table.cc`、`src/hb-ot-cff2-table.cc`（`get_extents`）（15.1.0）
- ISO/IEC 9899:2018（C17）7.12.9.6 `round`、7.12.9.2 `floor`
- ISO/IEC 14882:2017（C++17）7.4 Usual arithmetic conversions
