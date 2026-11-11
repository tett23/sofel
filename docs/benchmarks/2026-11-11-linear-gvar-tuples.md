# sofel [ADR 0047](../adr/0047-linear-gvar-tuple-iteration.md): gvar の組の線形走査

## 対象と方法

2026-11-11、Darwin arm64、`rustc 1.99.0 (b940084d7 2026-09-28)` で測った。変更前は commit `67821b6344ecadc008540b10931455e6c5c0a0e0`、変更後は同じ commit を起点とする `target/wt/adr0027-gvar-tuple-linear` の未コミット試作である。フォントは `target/testdata/harfbuzz/test/subset/data/fonts/RobotoFlex-Variable.ttf` の face 0、947 グリフを使った。

`/tmp/sofel-adr0047-box-bench` は、両側で同じ Rust ソースを使い、依存する `sofel-font` と `sofel-outline` の `path` だけを各 worktree へ切り替えた。`HbExtents::new(font.instance(&location))` を作り、`glyph_extents` をグリフ 0 から 946 まで順に呼ぶ。一回の全グリフ走査を16回繰り返し、最初の一回を捨てた15回の中央値を出した。箱の四つの値の和をチェックサムとして計算し、各走査間で一致を確認した。ビルドは両側とも次のコマンドで行った。

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/tmp/sofel-adr0047-bench-target \
  cargo build --release --manifest-path /tmp/sofel-adr0047-box-bench/Cargo.toml
```

実行コマンドは、`/tmp/sofel-adr0047-box-bench-baseline FONT wght500` と、変更後の `/tmp/sofel-adr0047-bench-target/release/sofel-adr0047-box-bench FONT wght500` である。`FONT` は上記フォントの絶対パスを指定した。既定位置には最後の引数 `default` を使った。

## 結果

| 位置 | 変更前の中央値 | 変更後の中央値 | 箱のチェックサム |
| --- | ---: | ---: | ---: |
| 既定位置 | 41 µs | 44 µs | 両側 1,052,122 |
| `wght=500` | 30,285 µs | 3,660 µs | 両側 1,076,492 |

`wght=500` では変更後の全グリフ走査時間が約8.3分の1になった。三回の追加実行で得た中央値は、変更前が 30,314、30,064、30,336 µs、変更後が 3,669、3,846、3,626 µs だった。すべてチェックサムは 1,076,492 だった。既定位置の差は 3 µs であり、この計測だけから回帰と判定しない。

変更後の全947個の箱を、試作の `tools/hb-dump/build.sh` で作った `/tmp/sofel-hb-adr0047` の `--extents --variations=wght=500 FONT` と行単位で比較した。両側947行で、`diff -u` の差分は0行だった。チェックサムだけで値の一致を判断していない。

この測定は一つのフォントと二つの位置に限る。`/tmp` の計測器と実行ファイルはリポジトリへ含めない。
