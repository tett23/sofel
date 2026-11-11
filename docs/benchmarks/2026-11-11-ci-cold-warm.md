# [ADR 0033](../adr/0033-ci-latency-and-ligature-iteration.md) の CI 完了時間とメモリの実測

2026年11月11日に、[ADR 0033](../adr/0033-ci-latency-and-ligature-iteration.md) の変更を [PR 52](https://github.com/tett23/sofel/pull/52) で確認した。
同じ head `7707f3f499a74afe6ed9687e1ed6f7d2e20aa296` の [run 42885929437](https://github.com/tett23/sofel/actions/runs/42885929437) を2回実行し、両方とも全ジョブが成功した。
ジョブ合計時間ではなく、各 attempt の `run_started_at` から最後のジョブの `completed_at` までを workflow の完了時間とした。

## 条件とキャッシュ

attempt 1 は新しいキャッシュの世代 `adr0033-v1` を使った最初の実行で、以下では cold と呼ぶ。
適合テストの Rust の成果物、両 OS の C 参照ツール、計測用 C ライブラリはキャッシュがなく、ビルドを実行した。
同じ run 内で先に保存されたテストデータを後のジョブが復元した例はあるため、全ジョブの全キャッシュがなかったという意味ではない。

attempt 2 は全ジョブの再実行で、以下では warm と呼ぶ。
両 OS の適合テストの Rust の成果物と C 参照ツール、計測用 C ライブラリが、それぞれの完全なキーで復元された。
計測用 C ライブラリと C 参照ツールのビルドは省かれた。
テストと照合は実行した。

## workflow とジョブの時間

時刻は GitHub の API が返す UTC の秒単位の値を使った。
開始待ちは、attempt の開始から各ジョブの開始までの差であり、ジョブの実行時間に含めない。

| 対象 | cold の開始待ち | cold の実行時間 | warm の開始待ち | warm の実行時間 |
| --- | ---: | ---: | ---: | ---: |
| workflow 全体 | 対象外 | 512秒 | 対象外 | 359秒 |
| conformance macOS | 3秒 | 509秒 | 5秒 | 354秒 |
| conformance Linux | 3秒 | 325秒 | 109秒 | 234秒 |
| bench | 2秒 | 330秒 | 5秒 | 217秒 |

cold は `2026-11-10T19:30:24Z` から `19:38:56Z`、warm は `19:39:36Z` から `19:45:35Z` だった。
どちらも最後に終わったのは macOS の適合テストである。
warm は cold より153秒、29.9%短かった。
これは同じコードのキャッシュ状態の違いを含む2回の観測であり、変更前と変更後の厳密な比較ではない。

warm の実行中には別 PR の CI との runner の競合があり、Linux の適合テストは109秒後に開始した。
ただし、最後に終わった macOS の適合テストの開始待ちは5秒だった。
Linux は workflow の終了より16秒早く完了し、この回の最長経路にはならなかった。

## 子孫プロセスの合計 RSS

`scripts/measure-process-tree.sh` は、起動したコマンドと子孫の RSS の合計を採取し、採取の後に0.1秒待って繰り返した。
表はその最大観測値である。
共有ページを各プロセスで重複して数えるため、物理メモリの使用量ではない。
採取の間の瞬間的な最大値と、計測したステップの外のプロセスは含まれない。

| 対象 | cold | warm |
| --- | ---: | ---: |
| bench の native と wasm の C ビルド | 2,637,348 KiB | キャッシュにより未実行 |
| Linux の hb-dump ビルド | 2,088,256 KiB | キャッシュにより未実行 |
| macOS の hb-dump ビルド | 1,560,896 KiB | キャッシュにより未実行 |
| Linux の適合テスト | 1,586,136 KiB | 1,621,000 KiB |
| macOS の適合テスト | 1,640,688 KiB | 2,432,256 KiB |

適合テストには初回の Rust のビルドも含まれる。
macOS の適合テストは warm の方が大きい値を示した。
二つの実行ファイルの重なり方や runner の状態の影響を分離する計測はしていないため、この差の原因は未確認である。
[ローカルの先行計測](2026-11-11-ci-latency-and-ligatures.md) で見た2並列による9.8%の RSS 増加が、CI でも同じになるとは判断しない。

## 再取得と根拠

ジョブ時刻は、次の API から取得した JSON で確認した。
各 attempt の開始時刻を使うため、再実行でも変わらない `created_at` は時間の起点にしなかった。

```sh
gh api repos/tett23/sofel/actions/runs/42885929437/attempts/1
gh api repos/tett23/sofel/actions/runs/42885929437/attempts/1/jobs
gh api repos/tett23/sofel/actions/runs/42885929437/attempts/2
gh api repos/tett23/sofel/actions/runs/42885929437/attempts/2/jobs
gh run view 42885929437 --attempt 1 --log
gh run view 42885929437 --attempt 2 --log
```

生の JSON とログは作業ディレクトリの `target/performance-evidence/ci-{cold,warm}-{run,jobs}.json` と `ci-{cold,warm}.log` に保存した。
cold の RSS はログの2640、20389、20779、50230、50620行、warm の RSS は847、55063行にある。
warm の C キャッシュ復元は457、1546、54673行、Rust の適合テストのキャッシュ復元は186、54401行で確認した。
これらの `target/` 以下の生ログはリポジトリには含めず、上記の GitHub run も根拠として示す。
