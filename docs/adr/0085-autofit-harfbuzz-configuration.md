# ADR 0085: HarfBuzz を使う構成の自動ヒンタを選べるようにする

Status: proposed

## 文脈

FreeType の自動ヒンタは、ビルドの構成で二つの振る舞いを持つ。
`FT_CONFIG_OPTION_USE_HARFBUZZ` を定義しない構成は、cmap から引いた文字だけで、グリフに書記体系と style を割り当てる。
定義した構成は、GSUB の機能（小型大文字、上付き、下付きなど）でたどれるグリフにも style を割り当て、blue zone の文字列をシェーピングしてから読む。

sofel の自動ヒンタは、[ADR 0064](0064-freetype-hinting-and-render-modes.md) に従い、参照ビルドと同じ HarfBuzz なしの構成を移植した。
ロードマップの「[ADR 0064](0064-freetype-hinting-and-render-modes.md): 自動ヒンタの残り」は、HarfBuzz を使う構成について、照合の手段を決める ADR が要ると書いている。
この ADR は、その構成を sofel で選べるようにし、照合の手段を決める。

sofel は特定の利用者に向けた範囲に絞らず、FreeType と HarfBuzz の機能をすべて写すことを目標にしている（[ADR 0001](0001-project-goal-and-scope.md)）。
そのため、参照ビルドが持たない構成でも、FreeType がビルドの構成として提供するものは対象にする。

## 調査した事項

### FreeType 2.15.0 の構成

- `include/freetype/config/ftoption.h:293` は `/* #define FT_CONFIG_OPTION_USE_HARFBUZZ */` で、既定では無効である。
- `ftoption.h:318` の `FT_CONFIG_OPTION_USE_HARFBUZZ_DYNAMIC` は、HarfBuzz を実行時に `dlopen` で読む構成である。`FT_CONFIG_OPTION_USE_HARFBUZZ` が無いと効かない（`ftoption.h:303`）。
- `src/autofit/afshaper.c` は、HarfBuzz を使う構成で次の関数を呼ぶ（`grep -o -E 'hb\( *[a-z_]+ *\)'` で数えた）。
  - `hb_ot_layout_collect_lookups`、`hb_ot_layout_lookups_substitute_closure`、`hb_ot_layout_lookup_collect_glyphs`、`hb_ot_layout_lookup_would_substitute`
  - `hb_shape`、`hb_buffer_*`、`hb_set_*`、`hb_ot_tags_from_script_and_language`、`hb_font_set_scale`
- `src/autofit/afadjust.c` も、HarfBuzz を使う構成で `hb_ot_layout_collect_lookups` と `hb_set_*` を呼び、アクセントの調整のための逆引きを GSUB からも作る。
- `src/autofit/afgsub.c` は 704 行である（`wc -l`）。
- `ft_hb_enabled` で分かれる箇所は、`afshaper.c` と `afadjust.c` のほかに次がある（`ft-hb.c:169` が定義）。
  - `aflatin.c:97`、`:345`、`:1095`
  - `afcjk.c:104`、`:307`、`:577`
  - `afglobal.c:364`、`:426`

### sofel の参照ビルドと移植

- [tools/ft-dump/build.sh](../../tools/ft-dump/build.sh) は C のコンパイラだけでビルドする（`build.sh:2-3`）。`autofit/autofit.c` を含め、HarfBuzz を定義する `-D` もリンクする `-l` も持たない。
  三つめの引数で `--webfont-reference` と `--filtered-lcd` の別の構成を作る仕組みがある。
- [tools/ft-dump/ft-dump.c](../../tools/ft-dump/ft-dump.c) は、グリフごとの style を出す `--auto-styles` をすでに持つ（`ft-dump.c:2037`）。
- `crates/sofel-outline/src/autofit/mod.rs` の冒頭の注釈は、移植元を `afshaper.c` の HarfBuzz なしの経路とする。
- [scripts/deps.txt](../../scripts/deps.txt) で、`sofel-outline` が依存するのは `sofel-font` と `sofel-geom` だけで、`sofel-shape` には依存しない。GSUB の閉包は `sofel-subset` の `layout/closure.rs` にあり、シェーピングは `sofel-shape` にある。`sofel-shape` は `sofel-subset` に依存しない。

### Golem の構成

Golem（`tett23/golem` の `dc8bf92e`）の `modules/golem-font/golem-font.cabal:65` は、FreeType を `-DFT2_BUILD_LIBRARY -DFT_CONFIG_MODULES_H=<golem_ftmodule.h>` でビルドし、`FT_CONFIG_OPTION_USE_HARFBUZZ` を定義しない。
Golem は、この ADR の構成を使わない。

## 実装すること

### 構成の選択

- 自動ヒンタのロード指定に、style の割り当て方として「cmap だけ」（既定）と「GSUB も使う」の二つを置く。既定は FreeType の既定の構成と同じ「cmap だけ」とする。
- 「GSUB も使う」は、FreeType を `FT_CONFIG_OPTION_USE_HARFBUZZ` でビルドしたときの振る舞いを写す。写す範囲は、`afshaper.c` と `afadjust.c` と、「調査した事項」に挙げた `ft_hb_enabled` の分岐のすべてである。

### 層の分け方

- `sofel-outline` は `sofel-shape` に依存しない。
- GSUB を読んで作る結果は、データとして `sofel-outline` の自動ヒンタに渡す。
  - グリフごとの style の割り当て（`af_shaper_get_coverage` の結果）
  - 各 style の blue zone の文字列をシェーピングしたグリフ列（`af_shaper_get_cluster` の結果）
  - アクセントの調整の逆引き（`afadjust.c` の GSUB の部分）
- このデータを作る関数は `sofel-shape` に置く。`afshaper.c` が使うシェーピング（`hb_shape`）が `sofel-shape` にあるためである。
- `afshaper.c` が使う lookup の閉包（`hb_ot_layout_lookups_substitute_closure`）は、`sofel-shape` に書く。`sofel-shape` に `sofel-subset` の依存を足さないためである。
- `sofel-cli` と `sofel-capi` は、二つをつないで「GSUB も使う」構成のロードを提供する。
- [docs/specifications.md](../specifications.md) の自動ヒンタの節と、[docs/roadmap.md](../roadmap.md) の「HarfBuzz を使う構成」の項を、実装と同じ変更で直す。

### 照合の手段

- [tools/ft-dump/build.sh](../../tools/ft-dump/build.sh) の三つめの引数に `--harfbuzz` を足し、`FT_CONFIG_OPTION_USE_HARFBUZZ` を定義して、固定した HarfBuzz 15.1.0 を静的にリンクした ft-dump を作る。HarfBuzz は `harfbuzz.cc` を `$CXX` でコンパイルするので、この構成だけは C++ のコンパイラも要る。
- グリフごとの style は、既存の `--auto-styles` を `--harfbuzz` の ft-dump で動かして比べる。
- 適合テストは、この ft-dump と、sofel の「GSUB も使う」構成を比べる。比べる項目は、[ADR 0064](0064-freetype-hinting-and-render-modes.md) の不変条件1と2と同じである。

## 実装しないこと

- `FT_CONFIG_OPTION_USE_HARFBUZZ_DYNAMIC` の、実行時に HarfBuzz を読む仕組み。sofel は自前の GSUB を使うので、読み込みの成否で振る舞いが変わる経路を持たない。
- HarfBuzz 15.1.0 以外の HarfBuzz と組んだ FreeType との一致。

## 不変条件

- I1: 「cmap だけ」の構成のロード結果は、この ADR の前のロード結果と同じである。
- I2: 「GSUB も使う」の構成では、同じフェイス、グリフ、軸座標、サイズ、ターゲットで、輪郭点、タグ、輪郭終端、送りが、`--harfbuzz` の ft-dump の自動ヒンタと一致する。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| GSUB の無いフェイス | 「GSUB も使う」で FreeType と一致する | I2 |
| `smcp` で小型大文字に置き換わるグリフ（cmap に無い） | 小型大文字の style で FreeType と一致する | I2 |
| `sups` と `subs` で置き換わるグリフ | 上付きと下付きの style で FreeType と一致する | I2 |
| GSUB の lookup が一つも機能から参照されない | 「cmap だけ」と同じ割り当てで FreeType と一致する | I2 |
| アクセントの付いた合字で、GSUB でだけたどれるもの | `afadjust.c` の調整が FreeType と一致する | I2 |

## 受けいれ条件

- A1: [ADR 0064](0064-freetype-hinting-and-render-modes.md) の自動ヒンタの適合テストのコーパスのすべてで、「GSUB も使う」構成の全ターゲットのスロットとモノクロのビットマップが、`--harfbuzz` の ft-dump と一致する。期待する失敗を置くときは、理由を一件ずつ書く。
- A2: CI で `--harfbuzz` の ft-dump を作り、A1 の比較を動かす。

## テスト設計

- `adr0085_i1_...`: 既存の自動ヒンタの適合テストを、「cmap だけ」を明示した指定でも動かし、結果が変わらないことを確かめる。
- `adr0085_i2_...`: `smcp`、`sups`、`subs` を持つ合成のフォントと、コーパスの実フォントで、`--harfbuzz` の ft-dump の `--auto-hint` の出力と比べる。
- `sofel-shape` の、style の割り当てを作る関数の単体テストは、`--harfbuzz` の ft-dump の `--auto-styles` の出力と比べる。

## トレードオフ

- 層を守るため、GSUB の結果をデータとして渡す。呼ぶ側が二つのクレートをつなぐ手間が増える一方、`sofel-outline` に `sofel-shape` の依存を足さずに済む。
- 照合のための ft-dump の構成が一つ増え、CI で HarfBuzz を静的にリンクするビルドが要る。
- lookup の閉包を `sofel-shape` にも書くので、`sofel-subset` の閉包と似たコードが二つになる。二つを一つにまとめるかは、別の ADR で決める。
- GSUB の無いフェイスでも、HarfBuzz を使う構成は blue zone の文字列を `hb_shape` に通す。そのため、二つの構成が同じ結果になるとは決めず、どちらも FreeType と比べる。

## 参考文献

- FreeType 2.15.0: `include/freetype/config/ftoption.h`、`src/autofit/afshaper.c`、`src/autofit/afadjust.c`、`src/autofit/afgsub.c`
- [ADR 0064: ヒンティングと描画モードを独立に指定する](0064-freetype-hinting-and-render-modes.md)
- [ADR 0003: クレートの層と依存の向き](0003-crate-layers.md)
