# CI の待ち時間と合字候補の走査の計測（2026-11-11）

[ADR 0033](../adr/0033-ci-latency-and-ligature-iteration.md) の実装差分を、専用 worktree `target/wt/performance` で計測した。
適合テストの2並列実行は42.1%、計測用 C ライブラリの2並列ビルドは49.8%、Roboto の英単語のシェーピングは22.9%短くなった。
これらは手元のステップごとの値であり、変更後の GitHub Actions workflow 全体の時間は未測定である。

## 環境と比較の範囲

- 機械: Apple M1 Max、メモリ64 GiB。
- OS: macOS 27.0、build 26A428、arm64。
- Rust: rustc 1.99.0（b940084d7、2026-09-28）、LLVM 23.1.1。
- ネイティブの C と C++: Apple clang 21.0.0（clang-2100.4.1.1）。既存スクリプトの `-O2` を使った。
- wasm の C と C++: wasi-sdk 34.0。取得済みの同じ SDK を両方のビルドに使った。
- 基準コミット: `10de31aad8ea699f9dd6b1e7bdd96f3019eddfe2`。
- 比較対象: `testdata.lock` の FreeType と HarfBuzz。取得済みの同じディレクトリを使用した。
- 時間の比較に使う Rust のビルド: `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0`。適合テストの release の debug 情報は既定値の0、ネイティブの計測用バイナリには `CARGO_PROFILE_RELEASE_DEBUG=0` を指定した。
- wasm の検証: Wasmtime 50.0.1（c307ec81d）。

適合テストと C ビルドの計測中は、ほかの作業の担当者に新たな重い Cargo の実行を待ってもらった。
専用 worktree のビルド成果物を使い、共有したテストデータと SDK は書き換えていない。
OS のバックグラウンド処理は止めていない。

RSS は、起動したコマンドのプロセスツリーに属するプロセスの値を足した。
`ps -axo pid=,ppid=,rss=` で取得し、各回の取得の後に0.1秒待って繰り返した。
表は合計 RSS の最大観測値であり、瞬間的な最大値を保証しない。
共有ページはプロセスごとに数えるため、実際の物理メモリの使用量と一致するとは限らない。

## 変更前の CI

[CI run 42875555732](https://github.com/tett23/sofel/actions/runs/42875555732) の開始は2026-11-10 18:28:32 UTC、最後のジョブの終了は18:39:07 UTCだった。
workflow の開始から最後のジョブの終了まで635秒である。
run のコミットは `b4d489a092e2bb97a82c7af7e539029c5842901b` で、上の基準コミットがマージした PR の head である。

| ジョブ | 開始から終了まで |
| --- | ---: |
| conformance（macos-latest） | 631秒 |
| conformance（ubuntu-latest） | 478秒 |
| bench | 452秒 |
| fuzz の最長（math） | 161秒 |
| wasm | 126秒 |
| rust | 78秒 |
| capi-wasm | 40秒 |
| adr | 8秒 |

最後に終わった macOS の `conformance` では、FreeType の参照ツールのビルドが12秒、HarfBuzz の参照ツールのビルドが86秒だった。
適合テストのステップは406秒で、ログ内の Rust のビルドは約62秒、実行は約343秒だった。
その後のシェーピングのハッシュのステップは24秒だった。
`bench` では、ネイティブと wasm の C ビルドのステップに244秒かかっていた。

このため、macOS の適合テストを先に短縮し、次の最長ジョブになりうる `bench` のビルドも対象にした。
ジョブ数は増やしていない。
workflow の完了時間を、各ジョブの時間の合計から推定していない。

時刻は次の API から取得した。

```sh
gh api repos/tett23/sofel/actions/runs/42875555732
gh api repos/tett23/sofel/actions/runs/42875555732/jobs --paginate
```

## 適合テストの実行

同じ変更後の実行ファイルと参照ツールを使い、runner の引数だけを1から2へ変えた。
Rust のビルドは前もって済ませ、両方の回で Cargo のコンパイルは発生していない。
この比較には、合字の最適化の前後の差は含めていない。

```sh
CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 cargo test --release -p sofel-conformance --no-run
CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 sh scripts/test-conformance.sh 1
CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 sh scripts/test-conformance.sh 2
```

`SOFEL_FT_DUMP` と `SOFEL_HB_DUMP` は、専用 worktree でビルドした同じ参照ツールの絶対パスを指定した。
テストの実行ファイルの libtest のスレッド数は、両方の回で1である。
テスト自身が明示的に作るスレッドは変えていない。

| 実行ファイルの並列数 | wall-clock | 合計 RSS の最大観測値 | 終了コード |
| --- | ---: | ---: | ---: |
| 1 | 215.979秒 | 1,771,104 KiB（1.69 GiB） | 0 |
| 2 | 125.116秒 | 1,943,920 KiB（1.85 GiB） | 0 |

wall-clock は42.1%短縮し、RSS の最大観測値は9.8%増えた。
両方とも33個の libtest と doctest の結果行があり、合計65テストが成功、失敗0、ignore 0だった。
出力から取り出したテスト名の多重集合も一致した。
HarfBuzz の shape の5850ケースと、輪郭532841項目、ラスタライズ532833項目も両方で実行した。

## キャッシュがない場合の C ビルド

取得したソースと SDK を再利用し、C のコンパイル成果物は再利用しない条件で比較した。
`bench/build-c.sh` は毎回オブジェクトを作り直すため、二回ともネイティブと wasm のすべての C ライブラリを再コンパイルした。
ネットワークからソースや SDK を取得する時間は含まない。

直列のコマンドは次のとおり。

```sh
sh bench/build-c.sh
sh bench/build-c.sh wasm32-wasip1
```

並列の回は、同じ二つのコマンドを background で起動し、両方を `wait` した。
最終実装では、この手順を `sh scripts/build-bench-c.sh` にまとめた。
出力ディレクトリは、ネイティブと wasm で別である。

| ビルド | wall-clock | 合計 RSS の最大観測値 | 終了コード |
| --- | ---: | ---: | ---: |
| 直列 | 97.904秒 | 1,684,576 KiB（1.61 GiB） | 0 |
| 2並列 | 49.154秒 | 3,190,144 KiB（3.04 GiB） | 0 |

wall-clock は49.8%短縮し、RSS の最大観測値は89.4%増えた。
コンパイラの同時起動を2本に限り、各ビルドの内部の直列処理は変えていない。
このメモリの値は Apple clang の値であり、CI の GCC の上限を示すものではない。
採択後の Linux CI では、子プロセスを含む合計 RSS と wall-clock を確認する必要がある。

キャッシュがある場合には、入力とツールチェーンが一致した C の成果物を復元する。
変更後の GitHub Actions 上のキャッシュの保存と復元は未測定であり、キャッシュによる workflow 全体の短縮率は記載しない。

## シェーピング

変更前と変更後を、同じネイティブの C ライブラリにリンクした release の実行ファイルとして保存した。
変更後は、合字の候補の `Vec` への収集を除き、必要な候補だけを iterator から読む。
両方を交互に三回実行した。
各回の計測方法は [ADR 0013](../adr/0013-benchmark-harness.md) の30標本、中央値と中央絶対偏差による。

```sh
target/performance-evidence/sofel-bench-before --filter 'shaping en-words.txt'
target/performance-evidence/sofel-bench-lazy --filter 'shaping en-words.txt'
```

| 回 | 変更前（µs） | 変更後（µs） | 同じ回の HarfBuzz、前／後（µs） |
| --- | ---: | ---: | ---: |
| 1 | 38042.2 ±261.8 | 29328.3 ±190.8 | 7037.4／7053.0 |
| 2 | 38026.3 ±259.8 | 29095.9 ±83.5 | 7073.7／7043.2 |
| 3 | 38251.5 ±163.4 | 29395.7 ±190.5 | 7047.1／7093.2 |

三回の中央値の中央値は、38.0422 ms から29.3283 msへ22.9%短くなった。
初回のシェーピングは、変更前19.8〜20.1 µs、変更後19.6〜20.0 µsだった。
この初回の差は小さいため、高速化の成果には数えない。
すべての回で、HarfBuzz との出力の照合が通った。

## 正しさの検査

- `sh scripts/run-tests.sh`: 成功。追加した runner の並列数、全件実行と失敗伝播、二つの C ビルドの成功と失敗の全組を含む。
- `cargo clippy --workspace --all-targets -- -D warnings`: 成功。
- `cargo test --release -p sofel-conformance`: 変更前の基準で全65テスト成功。
- `sh scripts/test-conformance.sh 1` と `2`: 変更後の全65テストが両方で成功。
- `cargo test --release -p sofel-shape`: 成功。シェーピングの固定ハッシュとの一致を含む。
- `adr0033_i4_ligature_candidate_order_and_unreadable_candidates`: 旧実装と変更後の両方で成功。空の候補、候補の優先順、null と範囲外の候補、1文字しかない入力を含む。
- `cargo run --release --manifest-path bench/Cargo.toml -- --once`: 全項目の照合が成功。
- `cargo test --release --target wasm32-wasip1 -p sofel-shape --test shape --test shape_hash`: Wasmtime 50.0.1 で20テスト成功。境界の回帰テストと固定ハッシュとの一致を含む。
- wasm の計測用バイナリの `--once`: Wasmtime 50.0.1（`-W exceptions=y --dir target/testdata`）で全項目の照合が成功。

生のログと計測の JSON は、この worktree の `target/performance-evidence/` に保存した。
この記録は、CI 全体の変更後の実測を含まない。
CI のキャッシュなしの回、同じ入力でキャッシュを使う回、Linux の同時 C ビルドの RSS は、採択後の別の記録に残す。
