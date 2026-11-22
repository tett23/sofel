# 自作のビットマップフォント

これらはテスト用に一から作った最小の入力であり、sofel と同じライセンスで配布する。
BDF は手書きのテキストで、PCF はその BDF を変換したもの、FNT と FON は固定値から組んだものである。
コマンドはリポジトリのルートで実行する。

## mono.bdf

手で書いた BDF テキストで、そのまま読める。
8 × 8 の `A` を一つ持ち、ビットマップの行は `18 24 42 7E 42 42 42 00` である。

## order.bdf

手で書いた BDF テキストで、そのまま読める。
文字コードと格納順の違いを確かめるための入力である。

## gray2.bdf

手で書いた BDF テキストで、そのまま読める。
2 ビットの階調を持つ。

## gray4.bdf

手で書いた BDF テキストで、そのまま読める。
4 ビットの階調を持つ。

## gray8.bdf

手で書いた BDF テキストで、そのまま読める。
8 ビットの階調を持つ。

## mono.fnt

`tests/support/generate_winfnt.rs` の `mono()` が、Windows FNT 2.0 のヘッダーと8 × 8の `A` を組む。
次のコマンドで、FNT と FON の4ファイルを再生成する。
出力先を変えれば、同梱ファイルを上書きせずに比較できる。

```sh
rustc --edition 2024 crates/sofel-bitmapfont/tests/support/generate_winfnt.rs -o /tmp/sofel-generate-winfnt
/tmp/sofel-generate-winfnt /tmp/sofel-winfnt-fixtures
cmp /tmp/sofel-winfnt-fixtures/mono.fnt crates/sofel-bitmapfont/tests/data/mono.fnt
```

## mono3.fnt

`tests/support/generate_winfnt.rs` の `version3()` が `mono()` の字形を FNT 3.0 のヘッダーと32ビットのオフセットで組む。
`mono.fnt` の節のコマンドで再生成する。

## two.fon

`tests/support/generate_winfnt.rs` の `ne()` が NE のリソース目録と二つの FNT を組む。
一つめは `mono.fnt`、二つめは文字コード66の塗りつぶした正方形である。
`mono.fnt` の節のコマンドで再生成する。

## mono-pe.fon

`tests/support/generate_winfnt.rs` の `pe()` が PE のリソース目録に `mono.fnt` を格納する。
`mono.fnt` の節のコマンドで再生成する。

## mono.pcf

`mono.bdf` を bdftopcf 1.1.2 で変換した。

```sh
bdftopcf -p4 -u1 -m -M -o /tmp/mono.pcf crates/sofel-bitmapfont/tests/data/mono.bdf
cmp /tmp/mono.pcf crates/sofel-bitmapfont/tests/data/mono.pcf
```

## mono_swap.pcf

`mono.bdf` を bdftopcf 1.1.2 で変換した。
ビット順とバイト順を逆にし、4バイトの走査単位を使う。

```sh
bdftopcf -p4 -u4 -m -L -o /tmp/mono_swap.pcf crates/sofel-bitmapfont/tests/data/mono.bdf
cmp /tmp/mono_swap.pcf crates/sofel-bitmapfont/tests/data/mono_swap.pcf
```

## order_lsb.pcf

`order.bdf` を bdftopcf 1.1.2 で変換した。
パディングと走査単位を1バイトとし、ビットとバイトをLSB順に並べる。

```sh
bdftopcf -p1 -u1 -l -L -o /tmp/order_lsb.pcf crates/sofel-bitmapfont/tests/data/order.bdf
cmp /tmp/order_lsb.pcf crates/sofel-bitmapfont/tests/data/order_lsb.pcf
```
