# シェーピングの適合テストで保持する行とフォントの実測

2026年11月11日、[ADR 0041](../adr/0041-compact-shape-comparison-results.md) を追加フォント導入後の `f4478dfebbae0bc1110bb011fffd22b99cfdbe93` で比較した。
変更を選んだきっかけは、直列化とラスタの参照出力を逐次比較にした後も、名前順の2並列テストの最大観測時に `harfbuzz_shape_dump` が1,726,896 KiBを使っていたことである。
その時点の合計は1,925,392 KiBで、同時に実行していた `harfbuzz_subset` は161,232 KiBだった。

## 入力と診断の照合

基点と変更後の `harfbuzz_shape_dump` に、同じHarfBuzzの参照ツール、フォントと入力テキストを与えた。
実行環境はApple M1 Max、メモリ64 GiB、macOS 27.0（26A428）、Rust 1.99.0で、ビルドには `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_DEBUG=0` を使った。
次のコマンドで通常の判定に加えて不一致の詳細も比較した。`TEST_BINARY` には各方式のreleaseの実行ファイルを指定する。

```sh
SOFEL_SHAPE_VERBOSE=1 SOFEL_SHAPE_SUGGEST=1 \
  TEST_BINARY --nocapture --test-threads=1
/usr/bin/time -p sh scripts/measure-process-tree.sh \
  TEST_BINARY --nocapture --test-threads=1
```

| モード | 比較件数 | 一致 | 不一致 |
| --- | ---: | ---: | ---: |
| `--layout-map` | 652 | 649 | 3 |
| `--shape` | 529,399 | 523,621 | 5,778 |
| `--shape-random` | 2,017 | 1,419 | 598 |

全6,379件の不一致について、識別子、行番号、コードポイント、HarfBuzzとsofelの出力と、カテゴリの件数を含む合計19,147行が変更前後で順序まで一致した。
照合した行を改行でつないだSHA-256は `bf026ec8530fb9e538599af4dd00e73413c46963bd22b3b856bb925c25b2d8ab` だった。
テストは期待する失敗の一覧で判定し、三つのモードとも成功した。

`--shape` では、不一致の5,765行が212,740 byteの `NotoSansDevanagari-Regular.ttf`、13行が1,564,904 byteの `NotoSansDuployan-Regular.otf` を参照した。
変更前の `judge` は各不一致行で `std::fs::read` を呼び、その結果をフォントのパスがキーの `BTreeMap` に集めていた。
不一致行数と実ファイルの大きさから計算すると、この組の読み込み対象の合計は1,246,789,852 byteである。
変更後はパスの重複を除いてから読むため、同じ組の読み込み対象は1,777,644 byteとなる。これは読み込む長さの合計で、ディスクI/Oの実測値とは区別する。

変更後は、比較したすべての行を数え、不一致の全行について判定と診断に必要なデータを持つ。
組の中の全行が一致するときは成功を表す識別子一件を渡す。
I1からI4に対応する先行テストは、実装前に `pair_cases` と `failure_fonts` が未定義で失敗し、実装後に3件とも成功した。
空行、全一致、全不一致、非ASCIIの混在、入力と両出力の行数の違い、フォント読み込み失敗の境界を確認した。
対象のClippyとワークスペースのフォーマット検査も成功した。

## 実時間とピークRSS

| 対象 | 変更前 | 変更後 |
| --- | ---: | ---: |
| シェーピングの適合テストの実時間 | 14.53秒 | 14.44秒 |
| 同テストの子孫を含むピークRSS | 1,772,256 KiB | 339,136 KiB |
| 適合全体の実時間 | 113.50秒 | 114.05秒 |
| 適合全体の子孫を含むピークRSS | 2,004,720 KiB | 1,470,320 KiB |

シェーピングの三つのモードを含む実行ファイル単体で、RSSの最大観測値は80.9%減った。
変更前後の順に各1回、同じマシンで別作業の短いreleaseビルドがあった時間帯に測ったため、時刻の差0.09秒から速度の変化は判断できない。
同じRSS観測スクリプトを使い、両回とも92標本、終了コード0だった。

適合全体は、直列化、ラスタ、輪郭の逐次比較を両方式に共通に適用したworktreeで走らせた。
Cargoが出した40本をターゲット名の昇順で2並列に投入し、各テストを `--nocapture --test-threads=1` で実行した。
実行ファイルは各測定前にビルドし、計測にはCargoによる更新の確認と最後のdoctestを含めた。
変更前102件、変更後105件のテストが成功し、照合報告139行は各実行ファイル内の順序も含めて一致した。
ピークRSSは26.7%減り、経過時間は0.55秒増えた。
観測回数は変更前651回、変更後653回だった。単体、全体ともRSSは共有ページを重複して数える採取時の合計である。

全体のピークを占めるプロセスも変わった。
変更前はシェーピングの比較が1,743,792 KiB、サブセット比較が227,040 KiBだった。
変更後はカラーグリフの比較 `harfbuzz_color` が1,392,784 KiB、同時実行中の `harfbuzz` が56,320 KiBだった。
一回ずつのローカル比較であり、GitHub Actionsのworkflow完了時間は未測定である。

## 生の記録

単体のworktreeの `target/evidence/` に、先行テストの `red.*`、実装後の `green.*`、詳細を比較した `{before,after}-verbose.stdout`、行数とハッシュを記した `diagnostics-equality.json`、単体計測の `{before,after}-suite.stderr` を保存した。
全体のworktreeの `target/evidence/` に、`fixed-{0040,0041}.stderr`、両版の実行ファイルごとのstdout、stderr、開始・終了時刻、ピークの全プロセスと `0041-full-equality.json` を保存した。
