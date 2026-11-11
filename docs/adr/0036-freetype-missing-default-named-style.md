# ADR 0036: FreeType の既定名つきスタイルが欠けた fvar の扱い

ステータス: 採択

## 文脈

[ADR 0027](0027-variable-fonts.md) は `Font::ft_mm_var()` を `FT_Get_MM_Var` と同じ値にすると決めた。
同じ箇所は、既定位置の名前つきインスタンスが fvar に無く、name にサブファミリー名と PostScript 名があれば、FreeType が末尾に既定のインスタンスを合成すると書く。
この文では、必要な name レコードが欠けるときの `num_namedstyles` と `FT_Get_Default_Named_Instance` の値が定まらない。
ここで有効な name レコードは、FreeType が読み込んだ長さが正の文字列を持ち、Windows の platform 3で encoding 0または1、または Apple の platform 1で encoding 0のレコードを指す。

固定版 FreeType の `sfnt_init_face` は、既定位置のレコードが fvar に無い場合、name の内容を調べる前にスタイル数を1増やし、既定インスタンスの番号を記録する。
続く `TT_Get_MM_Var` は、増えた数を `num_namedstyles` とし、領域をゼロ初期化する。
必要な name レコードがそろう場合だけ、`TT_Get_MM_Var` は末尾の `strid`、`psid`、座標を設定する。
必要な name レコードが欠ける場合、末尾にはゼロ初期化した `strid`、`psid`、座標が残る。
固定版 FreeType の実機ケースでは、Roboto Variable の既定位置のレコードを変更して name ID 2と6を無効にすると、`FT_Get_MM_Var` は19スタイルを返し、`FT_Get_Default_Named_Instance` は19を返し、末尾の名前 ID と座標はゼロだった。
したがって、FreeType と同じ値を返すには、名前を構築できない末尾のスタイルも件数に含める必要がある。

## 実装すること

- `Font::ft_mm_var()` は、fvar に既定位置のレコードが無ければ、必要な name レコードの有無にかかわらず末尾のスタイルを件数に含める。
- name ID 17 または2と、name ID 6が有効なら、末尾の `strid`、`psid`、座標をそれぞれ選んだサブファミリー ID、6、軸の既定値とする。
- サブファミリーの ID は17を2より優先する。
- 必要な name レコードが欠けるなら、末尾の `strid`、`psid`、座標をゼロとする。
- `Font::ft_default_named_instance()` は、fvar に既定位置のレコードがあればその1始まりの番号を返し、無ければ末尾の1始まりの番号を返す。

## 実装しないこと

- HarfBuzz 側の `Font::named_instances()` に FreeType だけが数える末尾スタイルを加えない。
- `Location::named_instance` が末尾のゼロスタイルを選んだときの FreeType 側のエラーは、この ADR では決めない。

## 不変条件

1. **I01 件数**: fvar に既定位置のレコードが無ければ、`ft_mm_var().named_styles().count()` は fvar の `instanceCount + 1` に等しい。
2. **I02 末尾**: 必要な name レコードがそろう場合の末尾は既定値を持ち、そろわない場合の末尾はゼロを持つ。
3. **I03 番号**: `ft_default_named_instance()` は、fvar にある既定位置か追加された末尾の1始まりの番号に等しい。

| 境界の入力 | 件数 | 末尾の値 | 既定インスタンス番号 | 不変条件 |
|---|---:|---|---:|---|
| fvar のインスタンス0件、必要な name あり | 1 | 名前 ID と既定座標 | 1 | I01、I02、I03 |
| fvar のインスタンス1件、既定位置なし、必要な name あり | 2 | 名前 ID と既定座標 | 2 | I01、I02、I03 |
| fvar のインスタンス1件、既定位置なし、name ID 6なし | 2 | ゼロ | 2 | I01、I02、I03 |
| fvar のインスタンス1件、既定位置なし、name ID 6の文字列長0 | 2 | ゼロ | 2 | I01、I02、I03 |
| fvar のインスタンス1件、既定位置なし、name ID 6が platform 0だけにある | 2 | ゼロ | 2 | I01、I02、I03 |
| fvar のインスタンス1件、既定位置あり | 1 | fvar のレコード | 1 | I03 |

## テスト設計

- `i01_missing_default_counts_tail_without_names` は、必要な name がない場合も末尾の件数が増えることを確かめる。
- `i02_missing_names_leave_zero_tail` は、末尾の名前 ID と全座標がゼロになることを確かめる。
- `i02_zero_length_and_unsupported_platform_leave_zero_tail` は、名前 ID の存在だけでは末尾の値が埋まらないことを確かめる。
- `i03_default_named_instance_uses_one_based_tail` は、fvar の既定レコードありとなしの両方で番号を確かめる。
- `i02_named_default_uses_name_ids` は、name ID 17を2より優先し、ID6と既定座標を使うことを確かめる。
- 位置を選ぶときのエラーは、固定版 FreeType を実行してから別のテストで確かめる。

## トレードオフ

FreeType が名前を構築できない場合のゼロスタイルも公開するため、呼び出し側が利用可能な名前を持つとは限らない。
一方で、`FT_Get_MM_Var` が返す件数と各レコードの値を保てる。

## 参考文献

- FreeType 2.15.0、commit `37515cf56ba3349656049edbed89ffc6f6466f2d`、`src/sfnt/sfobjs.c` 720〜775行。
- FreeType 2.15.0、同 commit、`src/truetype/ttgxvar.c` 2840〜2854行と3041〜3092行。
- FreeType 2.15.0、同 commit、`src/base/ftutil.c` 48〜61行。
- FreeType 2.15.0、同 commit、`src/sfnt/sfdriver.c` 508〜513行と627〜653行。
