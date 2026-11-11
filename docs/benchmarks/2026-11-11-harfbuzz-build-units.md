# 計測用 HarfBuzz の翻訳単位の4分割

2026年11月11日に、[ADR 0038](../adr/0038-partition-benchmark-harfbuzz-units.md) の提案をローカルで比較した。
基準の commit は `7707f3f499a74afe6ed9687e1ed6f7d2e20aa296` で、実装用 worktree の基点 `92d9dc086f8c2fb6371898cb627545b07bbd3ba1` と `bench/build-c.sh`、`bench/build.rs`、`scripts/build-bench-c.sh`、`bench/wasi-sdk.lock` の内容は同じだった。
変更後の CI は実行していない。

## ビルドの条件と結果

Apple M1 Max、メモリ64 GiB、macOS 27.0（26A428）、native の Apple clang 21.0.0、wasi-sdk 34.0の clang 23.1.0を使った。
HarfBuzz のソースは `testdata.lock` の `3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372` に固定したものを使った。
ソースと SDK は取得済みで、両方式とも各ターゲットの C オブジェクトを毎回作り直した。
4分割、単一翻訳単位の順に各1回続けて測り、ほかの担当の大きなビルドと重ならないよう調整した。

native と wasm は、どちらの方式でも並列に起動した。
各ターゲット内ではコンパイラを一つずつ起動した。
`-O2 -DNDEBUG`、C++11、例外と RTTI を無効にする指定、include ディレクトリ、wasm だけの `HB_NO_MT` は同じである。
4分割は元の74個の include を19、18、19、18行に分け、各組をコンパイルして同じアーカイブへ収めた。

| 方式 | 時間 | 子孫を含む合計 RSS の最大観測値 |
| --- | ---: | ---: |
| 単一翻訳単位 | 51.40秒 | 3,186,528 KiB |
| 4翻訳単位 | 68.78秒 | 2,005,680 KiB |

4分割で合計 RSS の最大観測値は37.1%減り、時間は17.38秒増えた。
実装PRの基点では、先にマージされた可変フォントの比較のためにFreeTypeとHarfBuzzへ `-ffp-contract=off` を追加している。上のRSSと時間はその追加前に測った値であり、新しいフラグの下での増減は未測定である。実装PRでは新しいフラグのままnativeとwasmのライブラリをビルドした。
時間は `/usr/bin/time -p` の `real`、RSS は `scripts/measure-process-tree.sh` の `peak_tree_rss_kib` を使った。
RSS は共有ページを重複して数え、採取の間の瞬間的な最大値を取り逃すため、物理メモリの最大使用量とは異なる。

```sh
/usr/bin/time -p sh scripts/measure-process-tree.sh sh scripts/build-bench-c.sh
/usr/bin/time -p sh scripts/measure-process-tree.sh sh target/performance-evidence/build-c-split-pair.sh
```

## ソース、シンボルと出力の照合

4個の翻訳単位を番号順に連結し、元の `harfbuzz-subset.cc` と `cmp` で一致することを両ターゲットで確認した。
同じコンパイラとフラグの `-dM -E` で前処理し、機能に関わる `HB_NO_*`、`HAVE_*` と `HB_EXPERIMENTAL_API` の定義を比較した。
native には `HB_NO_SETLOCALE` と `HB_NO_SUBSET_DEPEND` の2個、wasm にはさらに `HB_NO_MT` を含む3個があり、各ターゲットの元の翻訳単位と分割後の4個すべてで定義と値が一致した。
これは対象にしたマクロの比較であり、翻訳単位内のすべてのローカルなマクロ状態の同一性を意味しない。
native は `nm -gU`、wasm は wasi-sdk の `llvm-nm --defined-only --extern-only` でアーカイブの外部シンボルを読んだ。
native の先頭のアンダースコアを考慮して `hb_*` の名前を抽出すると、どちらのターゲットでも両方式は580個で、集合は一致した。

同じ Rust のソースと `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_DEBUG=0` を使い、単一翻訳単位と4分割のアーカイブへそれぞれリンクした。
native と wasm の両方式で `--once` の全照合は終了コード0となり、各ターゲットの174行の実行項目のログも両方式で一致した。
wasm は Wasmtime 50.0.1の `run -W exceptions=y --dir target/testdata` で実行した。
これは利用する全計測項目の照合であり、HarfBuzz の全 API について意味の同一性を証明するものではない。

実装用 worktree でも `sh scripts/build-bench-c.sh` で native と wasm をビルドした。
その全80個の `.o` は、照合に使った試行版の対応する `.o` と SHA-256 が一致した。
wasm の4個の `.a` はバイト列が一致し、native の3個の `.a` は ar のメンバーの日時欄だけを除くとバイト列が一致した。
実装用 worktree の基点で Rust の依存一式をビルドし直した値ではなく、照合済みの C オブジェクトが最終のビルドスクリプトでも再現することを確認したものである。

## HarfBuzz 側の実行時間

参照として測る HarfBuzz 自身の時間は、同じソースとフラグでも翻訳単位の境界で変わりうる。
`--filter "shaping en-words.txt"` と `--filter "subset ("` で代表項目を選び、既存の harness の3 warm-up、30標本の中央値と中央絶対偏差を採った。
native は単一翻訳単位、4分割の順、wasm は逆の順に一回ずつ実行した。
次の表は HarfBuzz の一呼び出しの時間で、変化率が正なら4分割の方が遅い。

| 環境 | フォント | 項目 | 単一翻訳単位（µs） | 4分割（µs） | 変化率 |
| --- | --- | --- | ---: | ---: | ---: |
| native | Roboto-Regular | shaping en-words.txt | 7203.3 ±60.7 | 6972.4 ±75.7 | -3.21% |
| native | Roboto-Regular | first shaping en-words.txt | 47.6 ±0.7 | 46.4 ±0.4 | -2.52% |
| native | Roboto-Regular | subset (42 glyphs, retain-gids) | 126.0 ±2.3 | 121.4 ±1.7 | -3.65% |
| native | Roboto-Regular | subset (2322 characters) | 135.7 ±1.6 | 128.2 ±1.4 | -5.53% |
| native | SourceHanSans-Regular_subset | subset (1918 glyphs, retain-gids) | 13834.9 ±240.4 | 13492.3 ±434.9 | -2.48% |
| native | SourceHanSans-Regular_subset | subset (2322 characters) | 13131.4 ±111.2 | 12952.1 ±405.4 | -1.37% |
| native | NotoNastaliqUrdu-Bold | subset (5 glyphs, retain-gids) | 129.2 ±2.9 | 122.7 ±0.8 | -5.03% |
| native | NotoNastaliqUrdu-Bold | subset (2322 characters) | 139.6 ±1.4 | 131.2 ±1.0 | -6.02% |
| wasm | Roboto-Regular | shaping en-words.txt | 12078.2 ±47.6 | 12285.9 ±87.3 | +1.72% |
| wasm | Roboto-Regular | first shaping en-words.txt | 62.4 ±0.5 | 62.3 ±0.5 | -0.16% |
| wasm | Roboto-Regular | subset (42 glyphs, retain-gids) | 150.1 ±2.3 | 157.2 ±2.2 | +4.73% |
| wasm | Roboto-Regular | subset (2322 characters) | 168.4 ±1.8 | 166.5 ±2.5 | -1.13% |
| wasm | SourceHanSans-Regular_subset | subset (1918 glyphs, retain-gids) | 17587.6 ±195.7 | 17432.1 ±304.1 | -0.88% |
| wasm | SourceHanSans-Regular_subset | subset (2322 characters) | 16875.6 ±211.4 | 17087.7 ±245.2 | +1.26% |
| wasm | NotoNastaliqUrdu-Bold | subset (5 glyphs, retain-gids) | 170.0 ±2.0 | 169.0 ±2.0 | -0.59% |
| wasm | NotoNastaliqUrdu-Bold | subset (2322 characters) | 185.2 ±3.8 | 178.9 ±1.0 | -3.40% |

native の8項目は4分割の方が短く、wasm は短い項目と長い項目があった。
最大の増加は wasm の Roboto の42 glyphs のサブセット化で4.73%だった。
各方式の実行は一回で、実行順や負荷の影響は分離していない。
全項目の参照時間が同一だとは判断せず、今後この条件で計測するときには翻訳単位の変更を明記する。

## workflow の完了時間

[変更前の CI 実測](2026-11-11-ci-cold-warm.md) の cold は workflow 全体512秒、最後に終わった macOS の適合テスト509秒、`bench` 330秒だった。
`bench` 内の C ビルドは122秒で、最長ジョブと `bench` の実行時間の差は179秒だった。
ローカルの増分17.38秒はこの差より小さいが、Linux runner のコンパイラ、CPU、開始待ちとジョブの重なりは異なる。
CI の最長経路が変わらないかは、変更後の CI を測るまで仮説として扱う。

変更前の warm では計測用 C ライブラリの完全なキーのキャッシュを復元し、C ビルドを省いた。
分割するスクリプトもキャッシュキーに含めるため、変更後の初回は再ビルドになる。
採択後は cold と warm を分け、workflow 開始から最終ジョブ終了までの時間と子孫 RSS を確認する。

## 生の記録

試行用 worktree の `target/performance-evidence/` に、`hb-split4.stderr`、`hb-monolith-repeat.stderr`、各ターゲットの include とシンボルの一覧、`--once` のログ、`hb-split4-reference-times.json` と元の計測表を保存した。
生ログと試行用ビルドディレクトリはリポジトリに含めない。
実装用 worktree の `target/verification/feature-macros.json` には前処理のコマンドと各翻訳単位の定義、`object-comparison.json` には最終の C オブジェクトの照合結果を保存した。
