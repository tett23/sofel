# ADR 0078: FreeType のサイドベアリング補正を参照実装に合わせる

ステータス: 採択

## 文脈

[ADR 0027](0027-variable-fonts.md) は、`Instance::ft_left_side_bearing` と `ft_top_side_bearing` に HVAR と VVAR の差分を適用すると定めた。この二つの問い合わせは FreeType の `tt_face_get_metrics` が返す水平および垂直のサイドベアリングを表す。参照版の FreeType 2.15.0 では、HVAR と VVAR の送り幅の差分が適用されても、二つのサイドベアリングの差分は適用されない。

## 調査した事項

固定版 FreeType 2.15.0（`37515cf56ba3349656049edbed89ffc6f6466f2d`）の `src/truetype/ttdriver.c` では、`tt_service_metrics_variations` の `hadvance_adjust` と `vadvance_adjust` は関数を指し、`lsb_adjust` と `tsb_adjust` は `NULL` である。`src/sfnt/ttmtx.c` の `tt_face_get_metrics` は、各コールバックが存在するときだけ補正する。

公開値も、差分を持つ合成フォントで確かめた。`crates/sofel-font/tests/var.rs` の `font_with_hvar` に、グリフ0を外側0・内側1の差分 −31 に写す LSB マップを足した。`font_with_vvar` には、グリフ0を外側0・内側0の差分 +21 に写す TSB マップを足した。両フォントの `fvar` は `wght` 軸の既定値0、最大値1を持つ。`Hvar::lsb_map().map(0) == 1`、`Vvar::tsb_map().map(0) == 0` と、最大位置での `item_delta::<Ft>` がそれぞれ −31、+21 になることを Rust の一時テストで確かめた。

合成フォントの生成には、HVAR のヘッダーの LSB マップオフセットを `20 + store.len()` とし、ストアの後に形式0・1項目のバイト列 `00 00 00 01 01` を足した。VVAR では、TSB マップオフセットを `24 + store.len() + 5` とし、既存の VORG マップの後に `00 00 00 01 00` を足した。一時テストの `probe_ft_side_bearing_fixture` は、この二つの関数で作ったバイト列を下のパスに `std::fs::write` で保存した。

次のコマンドで一時テストと公開値の照合を実行した。`ft-dump` はリポジトリの `tools/ft-dump/build.sh` で固定版 FreeType からビルドしたものである。

```sh
cargo test -p sofel-font --test var probe_ft_side_bearing_fixture -- --nocapture
/private/tmp/sofel-0027-cff2-ft-dump --metrics --variations=wght=1 /private/tmp/sofel-hvar-sidebearing-probe.ttf | rg '^advance [01] |^bearing [01] '
/private/tmp/sofel-0027-cff2-ft-dump --metrics --variations=wght=1 /private/tmp/sofel-vvar-sidebearing-probe.ttf | rg '^v_advance 0 |^bearing 0 '
```

一時テストは1件成功した。HVAR フォントでは送り幅がグリフ0で500から521、グリフ1で501から470に変わり、サイドベアリングはそれぞれ0と1のままだった。VVAR フォントでは縦の送り幅がグリフ0で500から521に変わり、上のサイドベアリングは0のままだった。`ft-dump --metrics` の出力は、最大位置で `advance 0 521`、`advance 1 470`、`bearing 0 0 -`、`bearing 1 1 -`、`v_advance 0 521`、`bearing 0 0 0` だった。

### 参照フォントの再現

先の `cargo test` は調査時だけ追加した一時テストを実行した記録であり、現在の作業ツリーではそのテストを削除している。参照フォントは次の16進ダンプから再生成できる。ダンプは前述の `font_with_hvar` と `font_with_vvar` で作ったファイルの実際のバイト列である。

HVAR フォント（600バイト、SHA-256 `19d06985f66d16c8fea59d190e48896f90df5df1405a35dcb1154a8f8f535881`）：

```sh
xxd -r -p > /private/tmp/sofel-hvar-sidebearing-probe.ttf <<'HEX'
000100000009008000030010485641523ff2417a0000009c0000003b636d6170
001eedb5000000d80000005c66766172777a69920000013400000024676c7966
00000000000001580000000068656164ce3c603e000001580000003668686561
07ab03da0000019000000024686d74782776005e000001b4000000606c6f6361
00000000000002140000003a6d617870001c5000000002500000000600010000
0000001400000000000000360000000000010000000c00010000001600010001
00004000400000020001000100000015ffe10000000101000000000200030001
000000140003000a000000340004002000000004000400010000005affff0000
0041ffffffc0000100000000000c000000000028000000000000000200000041
0000005a000000010001f6000001f6000000001b000100000010000200010014
0000000877676874000000000000000000010000000001000001000000028000
000000005f0f3cf5000b03e800000000b2d05e0000000000b8c63f00ffceff38
03b60320000100090002000000000000000100000320ff38005a04b0ffe2ffd8
044c00010000000500000000000000000000001401f4000001f5000101f60002
01f7000301f8000401f9000501fa000601fb000701fc000801fd000901fe000a
01ff000b0200000c0201000d0202000e0203000f020400100205001102060012
02070013ffecffebffeaffe9ffe8ffe7ffe6ffe5000000000000000000000000
0000000000000000000000000000000000000000000000000000000000000000
0000000000000000000000000000000000005000001c0000
HEX
```

VVAR フォント（704バイト、SHA-256 `b0d0c6baf865f607088491a3ab5adf0cada36fd491fb7e84e0272ed6e0137eb9`）：

```sh
xxd -r -p > /private/tmp/sofel-vvar-sidebearing-probe.ttf <<'HEX'
00010000000c008000030040564f524702bd0000000000cc0000000856564152
411040bd000000d400000042636d6170001eedb5000001180000005c66766172
777a69920000017400000024676c796600000000000001980000000068656164
ce3c603e00000198000000366868656107ab03da000001d000000024686d7478
2776005e000001f4000000606c6f636100000000000002540000003a6d617870
001c500000000290000000067668656107ab03c70000029800000024766d7478
01f40000000002bc000000040001000002bc0000000100000000001800000000
0000003d000000000000003800010000000c0001000000160001000100004000
4000000100010001000000150000000100000000010000000000000200030001
000000140003000a000000340004002000000004000400010000005affff0000
0041ffffffc0000100000000000c000000000028000000000000000200000041
0000005a000000010001f6000001f6000000001b000100000010000200010014
0000000877676874000000000000000000010000000001000001000000028000
000000005f0f3cf5000b03e800000000b2d05e0000000000b8c63f00ffceff38
03b60320000100090002000000000000000100000320ff38005a04b0ffe2ffd8
044c00010000000500000000000000000000001401f4000001f5000101f60002
01f7000301f8000401f9000501fa000601fb000701fc000801fd000901fe000a
01ff000b0200000c0201000d0202000e0203000f020400100205001102060012
02070013ffecffebffeaffe9ffe8ffe7ffe6ffe5000000000000000000000000
0000000000000000000000000000000000000000000000000000000000000000
0000000000000000000000000000000000005000001c0000000100000320ff38
005a04b0ffe2ffd8044c00010000000500000000000000000000000101f40000
HEX
```

固定版の参照ツールをビルドしてから、次のコマンドで公開値を再確認する。`scripts/fetch-testdata.sh` は `freetype.lock` に固定されたソースを取得する。

```sh
sh scripts/fetch-testdata.sh
sh tools/ft-dump/build.sh
shasum -a 256 /private/tmp/sofel-hvar-sidebearing-probe.ttf /private/tmp/sofel-vvar-sidebearing-probe.ttf
target/ft-dump/ft-dump --metrics --variations=wght=1 /private/tmp/sofel-hvar-sidebearing-probe.ttf | rg '^advance [01] |^bearing [01] '
target/ft-dump/ft-dump --metrics --variations=wght=1 /private/tmp/sofel-vvar-sidebearing-probe.ttf | rg '^v_advance 0 |^bearing 0 '
```

現在の `Instance::ft_left_side_bearing` と `ft_top_side_bearing` は、位置によらない `hmtx` と `vmtx` の値を返す。既存の実装は、この調査で得た参照値と一致する。

## 実装すること

[ADR 0027](0027-variable-fonts.md) の `sofel-font` の値に関する決定を、この ADR で変更する。`Instance::ft_left_side_bearing` と `ft_top_side_bearing` は、FreeType 2.15.0 の `tt_face_get_metrics` と同じく、HVAR の LSB マップと VVAR の TSB マップの差分を適用しない。`ft_h_advance` と縦の送り幅には、参照実装どおりの差分を適用する。

合成フォントの LSB と TSB マップを使う回帰テストを足す。`ft-dump --metrics` の送り幅とサイドベアリングを同じ位置で照合する。

## 実装しないこと

`FT_Load_Glyph` が輪郭の変化から求めるグリフスロットのメトリクスは、この ADR で扱わない。既存の輪郭と箱の比較は [ADR 0027](0027-variable-fonts.md) の範囲に残す。HVAR と VVAR の表の読み取りや、送り幅の補正も変更しない。

## 不変条件

1. I1: 有効なフォントとグリフおよび任意の位置について、`Instance::ft_left_side_bearing(glyph)` は同じフォントの既定位置での値と等しい。HVAR の LSB マップが非ゼロ差分を指しても等しい。
2. I2: 有効な縦メトリクスを持つフォントとグリフおよび任意の位置について、`Instance::ft_top_side_bearing(glyph, y_max)` は同じ `y_max` を与えた既定位置での値と等しい。VVAR の TSB マップが非ゼロ差分を指しても等しい。

| 境界 | LSB と TSB のマップ | 位置 | 期待値 |
|---|---|---|---|
| 0項目 | 空 | 既定 | 元の `hmtx` と `vmtx` の値 |
| 1項目 | グリフ0に非ゼロ差分 | 最大 | 元の `hmtx` と `vmtx` の値 |
| ちょうど収まる | 最後のグリフを最後の有効項目へ写す | 最大 | 元の `hmtx` と `vmtx` の値 |
| 1つあふれる | 最後のグリフより大きい番号を問い合わせる | 最大 | 既存の範囲外グリフの扱い |

## テスト設計

`adr0078_i01_hvar_lsb_map_keeps_freetype_bearing` は、非ゼロの LSB 差分を持つ HVAR フォントで I1 を検査する。最初に FreeType の `ft-dump --metrics` の送り幅が変わり、サイドベアリングが変わらないことを確かめる。次に、sofel の返り値を両方と照合する。

`adr0078_i02_vvar_tsb_map_keeps_freetype_bearing` は、非ゼロの TSB 差分を持つ VVAR フォントで I2 を検査する。既定値と最大位置を比べ、縦送り幅だけに差分が付くことを確かめる。両テストで、空のマップ、1項目、最後の有効項目と範囲外のグリフを扱う。

## トレードオフ

HVAR と VVAR の LSB・TSB マップは表のビューに残す。表の構造を読む API と、固定版 FreeType が公開するメトリクスを分けることで、`Instance` の値を参照実装に合わせる。

## 参考文献

- [FreeType 2.15.0 のメトリクスサービス](https://github.com/freetype/freetype/blob/VER-2-15-0/src/truetype/ttdriver.c)
- [FreeType 2.15.0 の `tt_face_get_metrics`](https://github.com/freetype/freetype/blob/VER-2-15-0/src/sfnt/ttmtx.c)
- [ADR 0027](0027-variable-fonts.md)
