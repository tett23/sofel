# ADR 0086: ヒンタのモジュールの設定をロード指定で受け取る

Status: accepted-pending

## 文脈

FreeType は、モジュールごとの設定（property）で、ヒンティングの結果を変えられる。
アプリケーションは `FT_Property_Set` で、自動ヒンタのステムの太らせ、x-height の引き上げ、既定の書記体系などを変える。

sofel は、これらの設定を FreeType の既定の値に固定している。
ロードマップの「[ADR 0064](0064-freetype-hinting-and-render-modes.md): 自動ヒンタの残り」は、設定を受け取る API が無いと書いている。
この ADR は、どの設定を受け取るか、どこで受け取るかを決める。

## 調査した事項

### FreeType 2.15.0 の設定

`include/freetype/ftdriver.h` の `@property:` の節は 13 ある（`grep -n '@property:'`）。
そのうち `warping` は、FreeType から取り除かれた設定の記録である（`ftdriver.h:1288-1300`）。
SDF の `overlaps`、`flip_sign`、`flip_y` は、`@property:` の節を持たず、コードにだけある（`src/sdf/ftsdfrend.c:91`、`:101`、`:111`）。
各モジュールの設定の受け口は次のとおりである（`grep -n 'ft_strcmp( property_name'`）。

| 設定 | 受け取るモジュール | 受け口 | 既定の値 |
| --- | --- | --- | --- |
| `no-stem-darkening` | autofitter、cff、type1、t1cid | `src/autofit/afmodule.c:253`、`src/base/ftpsprop.c:172` | TRUE（`afmodule.c:404`、`src/cff/cffobjs.c:1147`、`src/type1/t1objs.c:609`） |
| `darkening-parameters` | autofitter、cff、type1、t1cid | `afmodule.c:191`、`ftpsprop.c:51` | `CFF_CONFIG_OPTION_DARKENING_PARAMETER_*`（`afmodule.c:406`、`cffobjs.c:1149`、`t1objs.c:611`） |
| `increase-x-height` | autofitter（フェイスごと） | `afmodule.c:172`。フェイスの値に入れる（`afmodule.c:187`） | `AF_PROP_INCREASE_X_HEIGHT_MAX` で、値は 0（無効）（`src/autofit/afglobal.c:388`、`afglobal.h:90`） |
| `fallback-script` | autofitter | `afmodule.c:118` | `AF_STYLE_FALLBACK`（`afmodule.c:402`） |
| `default-script` | autofitter | `afmodule.c:156` | `AF_SCRIPT_DEFAULT`（`afmodule.c:403`） |
| `glyph-to-script-map` | autofitter（読むだけ） | `afmodule.c:294` | なし |
| `hinting-engine` | cff、type1、t1cid | `ftpsprop.c:114` | `FT_HINTING_ADOBE`（`cffobjs.c:1145`） |
| `random-seed` | cff、type1、t1cid | `ftpsprop.c:198` | なし |
| `interpreter-version` | truetype | `src/truetype/ttdriver.c:82` | 未確認 |
| `spread`、`overlaps` | sdf、bsdf | `src/sdf/ftsdfrend.c:69`、`:111` | 未確認 |
| `no-long-family-names` | pcf | `src/pcf/pcfdrivr.c:666` | 未確認 |
| `svg-hooks` | ot-svg | `src/svg/ftsvg.c:168` | なし |

- `increase-x-height` は、値が 0 でなく、ppem が 6（`AF_PROP_INCREASE_X_HEIGHT_MIN`、`afglobal.h:89`）以上で値以下のときだけ効く（`src/autofit/aflatin.c:1236-1243`）。
- `darkening-parameters` は、x が負、x が昇順でない、y が負か 500 を超える値を `Invalid_Argument` で拒む（`afmodule.c:236-240`）。
- `hinting-engine` は、`CFF_CONFIG_OPTION_OLD_ENGINE` か `T1_CONFIG_OPTION_OLD_ENGINE` でビルドしたときだけ効き、それ以外では何もしない（`ftdriver.h:381-386`）。二つのマクロは、`ftoption.h:902` と `:944` で既定では無効である。

### sofel のいまの API

- `crates/sofel-outline/src/lib.rs:138` の `HintMode` は、`None`、`NativeV35`、`NativeV40`、`Auto` を選ぶ。`interpreter-version` は、この選択で受け取っている。
- `crates/sofel-raster/src/outline_sdf.rs:260` は、SDF の `spread` を引数で受け取る。
- [ADR 0082](0082-cff-random-interval-check.md) は、`random` の種を外から変える手段を持たないと決めた。
- [tools/ft-dump/ft-dump.c](../../tools/ft-dump/ft-dump.c) の `FT_Property_Set` の呼び出しは、truetype の `interpreter-version` の一つだけである（`ft-dump.c:1774`）。autofitter、cff、type1、t1cid の設定は与えていない。

## 実装すること

- ヒンティングのロード指定に、次の設定を持たせる。省いたときは FreeType の既定の値とする。
  - `no-stem-darkening` と `darkening-parameters`。自動ヒンタと、CFF、Type1、CID のネイティブヒンタで別々に持つ。FreeType がモジュールごとに持つためである。
  - 自動ヒンタの `increase-x-height`、`fallback-script`、`default-script`。
- 自動ヒンタのキャッシュから、グリフごとの style を読む API を置く（`glyph-to-script-map` に当たる）。
- 設定は、フェイスやプロセスの状態に置かず、ロードの呼び出しごとに値で渡す。同じ入力と同じ指定から、いつも同じ結果を返すためである。
- 照合のため、ft-dump に、設定を `FT_Property_Set` で与えるオプションを足す。
- `sofel-cli` と `sofel-capi` から、同じ設定を指定できるようにする。
- FreeType が `Invalid_Argument` で拒む値は、sofel も同じ条件で拒み、ロードをしない。
- [docs/specifications.md](../specifications.md) の、モジュールの設定を既定の値に固定すると書く箇所と、[docs/roadmap.md](../roadmap.md) の「モジュールの設定」の項を、実装と同じ変更で直す。

## 実装しないこと

- `hinting-engine`。既定の構成では何もしないためである。
- `random-seed`。[ADR 0082](0082-cff-random-interval-check.md) の決まりに従う。
- `FREETYPE_PROPERTIES` の環境変数。ロードの結果を引数だけで決めるためである。[ADR 0082](0082-cff-random-interval-check.md) も、`random` の種を環境変数で変える手段を持たないと決めている。
- `interpreter-version`、SDF の `spread`。いまの API で受け取っている。
- SDF の `overlaps`、`flip_sign`、`flip_y`、PCF の `no-long-family-names`、`svg-hooks`。ヒンティングの設定ではないので、それぞれの機能の ADR で扱う。

## 不変条件

- I1: 設定を省いたロードの結果は、この ADR の前のロードの結果と同じである。
- I2: 同じフェイス、グリフ、軸座標、サイズ、ターゲット、設定で、輪郭点、タグ、輪郭終端、送りが、同じ設定を `FT_Property_Set` で与えた FreeType と一致する。
- I3: 二つのロードで設定が違っても、互いの結果に影響しない（設定はロードの呼び出しの外に残らない）。
- I4: FreeType が `Invalid_Argument` で拒む設定の値は、sofel も拒み、ロードの結果を返さない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 設定を省く | 前と同じ結果 | I1 |
| `no-stem-darkening` を FALSE、`darkening-parameters` を既定 | FreeType と一致する | I2 |
| `darkening-parameters` の4点がすべて同じ x | FreeType と一致する | I2 |
| `darkening-parameters` の x が昇順でない、y が 501 | 設定を拒む | I4 |
| `darkening-parameters` の y がちょうど 500 | 受けて、FreeType と一致する | I2 |
| `increase-x-height` が 0（無効） | FreeType と一致する | I2 |
| `increase-x-height` が 10 で、ppem が 5、6、10、11 | FreeType と一致する（6 と 10 だけで効く） | I2 |
| `default-script` を cmap に無い書記体系にする | FreeType と一致する | I2 |
| 太らせありと無しのロードを交互に呼ぶ | 各ロードが単独で呼んだ結果と同じ | I3 |

## 受けいれ条件

- A1: [ADR 0064](0064-freetype-hinting-and-render-modes.md) の自動ヒンタと PostScript 系のネイティブヒンタの適合テストのコーパスで、設定の組を三つ以上（太らせあり、x-height の引き上げあり、既定の書記体系を変える）選び、全ターゲットのスロットとビットマップが FreeType と一致する。

## テスト設計

- `adr0086_i1_...`: 既存の適合テストを、設定を省いた指定と、既定の値を明示した指定の両方で動かし、結果を比べる。
- `adr0086_i2_...`: ft-dump の設定のオプションで FreeType の結果を取り、境界の表の各行を比べる。
- `adr0086_i3_...`: 一つのキャッシュで、設定の違うロードを交互に呼び、単独で呼んだ結果と比べる。
- `adr0086_i4_...`: 拒む値の各行で、sofel がエラーを返すことと、ft-dump の `FT_Property_Set` が `Invalid_Argument` を返すことを確かめる。

## トレードオフ

- 設定をロード指定に入れるので、呼ぶ側はロードのたびに設定を渡す。FreeType のようにライブラリの状態に置くより引数が増える一方、同じ入力から同じ結果を返す性質を保てる。
- モジュールごとに別の設定を持つので、自動ヒンタと CFF で太らせを揃えたい呼び出しでは二つを指定する。FreeType と同じ単位で照合できる。

## 参考文献

- FreeType 2.15.0: `include/freetype/ftdriver.h`、`src/autofit/afmodule.c`、`src/base/ftpsprop.c`、`src/cff/cffobjs.c`
- [ADR 0064: ヒンティングと描画モードを独立に指定する](0064-freetype-hinting-and-render-modes.md)
- [ADR 0082: CFFの `random` の決まりと、区間による参照の確かめ](0082-cff-random-interval-check.md)
