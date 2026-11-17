# ADR 0080: 計測の記録を CI で取る

Status: accepted-pending

## 文脈

[ADR 0006](0006-performance-measurement.md) は、性能の計測の方法と記録の形を決めた。
同じ ADR の「実装しないこと」は、CI での性能の計測を除いた。
理由は、CI の機械では計測の揺れが大きいことである。
[ADR 0013](0013-benchmark-harness.md) も、CI では `--once` で照合だけを行い、時間は記録しないとした。
そのため、`docs/benchmarks/` の記録は、これまですべて開発者の手元の機械で取ってきた。

しかし、手元の機械で記録を取る運用は、二つの点で行き詰まった。
一つ目は、手元の機械の負荷である。
[ADR 0014](0014-raster-sweep-touched-cells.md) の記録を取るために、2026-11-10 に負荷が下がるのを待ったが、10時から13時22分まで負荷平均は105から283の間にとどまり、記録を取れなかった。
[ロードマップ](../roadmap.md)の「負荷の高いマシンで取った記録」も、取り直しが残っている。
二つ目は、手元の機械への依存である。
記録の値は、開発者の機械の種類と、その時に動いているほかの処理に左右される。
CI のランナーは、毎回、決まったイメージから始まり、開発者の手元の処理とは別の機械で動く。

この ADR は、CI で計測の記録を取る仕組みを決める。
退行による CI の失敗は、引き続き決めない。

## 調査した事項

次の事実は、2026-11-15 の main の CI の実行（run 43769767140）の `bench` ジョブのログで確かめた。

| 事項 | 値 |
| --- | --- |
| ランナーのイメージ | `ubuntu-24.04`、版 `20261111.370.1` |
| OS | Ubuntu 24.04.5 LTS |
| `--once` のネイティブの手順（ビルドを含む） | 62秒（22:34:52 から 22:35:54） |
| `--once` の wasm の手順（ビルドを含む） | 123秒（22:35:54 から 22:37:57） |
| ネイティブの `--once` が標準エラーに出した項目の行 | 343行 |

計測の枠組みの既定値は、`bench/src/harness.rs` の `Config::default` で、1回の標本が10ミリ秒以上、ウォームアップの標本が3つ、標本が30個である。
したがって、1つの項目の1つの実装の計測は、少なくとも33回×10ミリ秒＝0.33秒かかる。
343項目の各実装を合わせた計測の時間は、計っていない（未確認）。
1項目が1つの実装だけだとしても、下限は約113秒である。

手元の機械の負荷は、2026-11-10 に `uptime` で次のとおり確かめた。

| 時刻 | 1分の負荷平均 |
| --- | ---: |
| 10:07 | 174.12 |
| 10:38 | 234.22 |
| 12:22 | 104.75 |
| 12:52 | 187.72 |
| 13:22 | 282.78 |

GitHub のホストするランナーが、実行ごとに同じ CPU の機械に割り当てられるかは、確かめていない（未確認）。

## 実装すること

### 記録を書くスクリプト

`scripts/bench-record.sh` を足す。
このスクリプトは、ネイティブと wasm の計測を走らせ、記録の Markdown を一つ書く。

- 引数は、`--subject <主題>`（必須）、`--filter <文字列>`（省略可）、`--out <ディレクトリ>`（省略時は `docs/benchmarks`）とする。
- 主題は、英小文字と数字をハイフンでつないだ形（`^[a-z0-9]+(-[a-z0-9]+)*$`）に限る。
- `--filter` は、ネイティブと wasm の両方の計測にそのまま渡す。
- ネイティブの計測は `cargo run --release --manifest-path bench/Cargo.toml --` で、wasm の計測は `wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm` で走らせる。
  テストのため、この二つのコマンドは環境変数 `SOFEL_BENCH_NATIVE` と `SOFEL_BENCH_WASM` で差し替えられる。
- 記録のファイルの名前は、`<YYYY-MM-DD>-<主題>-ci-<ランナー>.md` とする。
  日付は UTC で、ランナーは `linux-x64` か `macos-arm64` のように、OS と CPU の種類をハイフンでつないだものとする。
  OS は `uname -s` を小文字にしたもの（`darwin` は `macos` とする）、CPU の種類は `uname -m` の `x86_64` を `x64` に、`aarch64` と `arm64` を `arm64` にしたものとする。
- 記録の本文は、「環境」「ネイティブ」「wasm」の節をこの順に持つ。
  「ネイティブ」と「wasm」の節には、計測が標準出力に書いた表をそのまま置く。
- 「環境」の節には、次の値を書く。
  取れなかった値は、行を省かずに `未取得` と書く。

| 値 | 取り方 |
| --- | --- |
| ランナーのイメージと版 | 環境変数 `ImageOS` と `ImageVersion`（ランナーがこの名前で渡すかは未確認） |
| OS の名前と版 | Linux は `/etc/os-release`、macOS は `sw_vers` |
| CPU の名前 | Linux は `lscpu`、macOS は `sysctl -n machdep.cpu.brand_string` |
| 論理 CPU の数 | Linux は `nproc`、macOS は `sysctl -n hw.ncpu` |
| メモリ | Linux は `/proc/meminfo`、macOS は `sysctl -n hw.memsize` |
| 計測の直前の負荷平均 | `uptime` |
| Rust の版 | `rustc -V` |
| C と C++ のコンパイラの版 | `cc --version` と `c++ --version` の最初の行 |
| wasmtime の版 | `wasmtime --version` |
| wasi-sdk の版 | `bench/wasi-sdk.lock` |
| FreeType と HarfBuzz の版 | `testdata.lock` |
| sofel のコミット | `git rev-parse HEAD` |
| 実行の URL | 環境変数 `GITHUB_SERVER_URL`、`GITHUB_REPOSITORY`、`GITHUB_RUN_ID` |

- ネイティブか wasm の計測が 0 以外の終了コードで終わったとき（照合の失敗を含む）は、記録のファイルを書かず、その終了コードで終わる。

### ワークフロー

`.github/workflows/bench.yml` を足す。

- 起動は、手動（`workflow_dispatch`）だけとする。入力は `subject`（必須）と `filter`（省略可）である。
- ランナーは、`ubuntu-latest` と `macos-latest` の二つとし、それぞれで1つのジョブを走らせる。
- 準備の手順（ツールチェーン、テストのデータ、wasi-sdk、wasmtime、計測用の C ライブラリ）は、`ci.yml` の `bench` ジョブと同じものを使う。
- ジョブは、`scripts/bench-record.sh` で記録を書き、記録を artifact（名前は `bench-record-<ランナー>`）として上げ、同じ内容をジョブの要約（`GITHUB_STEP_SUMMARY`）にも書く。
- ジョブの権限は `contents: read` とし、リポジトリには書き込まない。
- ジョブの時間の上限は120分とする。

### 記録の取り込み

記録は、プルリクエストで `docs/benchmarks/` に取り込む。
取り込む人は、`gh run download <実行の ID> --pattern 'bench-record-*'` で artifact を取り、ファイルを変えずに `docs/benchmarks/` に置く。
取り込んだ記録は、ほかの記録と同じく、コミットの後は変えない。

### 文書

- [docs/specifications.md](../specifications.md) の「性能の計測」の節に、CI での記録の取り方を書く。
- README の開発の節に、ワークフローの起動と記録の取り込みの手順を書く。
- `A2` の記録を取り込んだ後に、[ロードマップ](../roadmap.md)の「RobotoFlex（重なりのある輪郭）のラスタライズ」と「負荷の高いマシンで取った記録」の項を、取り込んだ記録に合わせて直す。
- 採択の後に、[ADR 0006](0006-performance-measurement.md) と [ADR 0013](0013-benchmark-harness.md) のステータスに、それぞれの「実装しないこと」の節をこの ADR が置き換えたことを記録する。

## 実装しないこと

- CI での性能の退行による失敗。記録を取るだけで、比の値でジョブを失敗させない。
- 予算（`docs/budgets.md`）。最適化に取りかかるときに、別の ADR で決める。
- プルリクエストや main への push を契機にした計測。計測は手動で起動する。
- ワークフローによる記録のコミットやプルリクエストの作成。リポジトリへの書き込みの権限を持たせないためである。
- セルフホストのランナー。
- 異なる実行の記録どうしの比較の自動化。
- 手元の機械での計測の禁止。手元の記録も、これまでどおり取ってよい。

## 不変条件

`scripts/bench-record.sh` について、次が成り立つ。

- `I1`: 計測が二つとも終了コード 0 で終わったとき、記録のファイルは一つだけ書かれ、その名前は `<YYYY-MM-DD>-<主題>-ci-<ランナー>.md` の形である。
- `I2`: 記録の本文は、「環境」「ネイティブ」「wasm」の節をこの順に持ち、「ネイティブ」と「wasm」の節は、それぞれの計測の標準出力を変えずに含む。
- `I3`: 「環境」の節は、「実装すること」の表のすべての値の行を持ち、取れなかった値は `未取得` と書く。
- `I4`: ネイティブか wasm の計測が 0 以外の終了コードで終わったとき、スクリプトはその終了コードで終わり、記録のファイルを書かない。
- `I5`: 主題が `^[a-z0-9]+(-[a-z0-9]+)*$` に合わないとき、または `--subject` が無いとき、スクリプトは計測を走らせずに終了コード 2 で終わる。
- `I6`: `--filter` を与えたとき、ネイティブと wasm の両方の計測に同じ文字列が `--filter` として渡る。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 主題 `stage2`、計測は二つとも成功 | `<日付>-stage2-ci-<ランナー>.md` が一つ書かれ、3つの節を持つ | I1, I2 |
| 主題 `a`（一文字） | 記録が書かれる | I1 |
| 主題 `Stage2`、`stage2-`、`-x`、空 | 計測を走らせず、終了コード 2 | I5 |
| `--subject` が無い | 計測を走らせず、終了コード 2 | I5 |
| ネイティブが終了コード 1 | 終了コード 1、記録は書かれない | I4 |
| ネイティブは成功、wasm が終了コード 3 | 終了コード 3、記録は書かれない | I4 |
| `uname -s` が `Darwin`、`uname -m` が `arm64` | 名前の末尾が `-ci-macos-arm64.md` | I1 |
| `uname -s` が `Linux`、`uname -m` が `x86_64` | 名前の末尾が `-ci-linux-x64.md` | I1 |
| `lscpu` と `sysctl` が無い | CPU の名前の行が `未取得` | I3 |
| `--filter raster` | 二つの計測に `--filter raster` が渡る | I6 |
| `--filter` 無し | 二つの計測に `--filter` が渡らない | I6 |
| 計測の標準出力が空 | 節は作られ、中身は空 | I2 |

## 受けいれ条件

- `A1`: 実装のプルリクエストをマージした後に、main でワークフローを `subject` を `stage2-rerun`、`filter` を空にして起動し、二つのランナーのジョブが成功して、二つの artifact ができる。
- `A2`: `A1` の二つの記録を、変えずに `docs/benchmarks/` に取り込むプルリクエストがマージされる。

## テスト設計

実装のまえに、`scripts/tests/test-bench-record.sh` を書いて失敗させる。

- 計測のコマンドを `SOFEL_BENCH_NATIVE` と `SOFEL_BENCH_WASM` で、決まった表を出して決まった終了コードで終わる偽のコマンドに差し替え、不変条件の境界の表のすべての行を確かめる。
  テストの名前は、`ADR 0080 I1: ...` の形とする。
- 偽のコマンドは、受け取った引数をファイルに書き、`I6` ではそれを読んで確かめる。
- ランナーの名前は、決まった値を出す偽の `uname` を `PATH` の先に置いて確かめる。
- `I3` の `未取得` は、`PATH` を、`lscpu` と `sysctl` を含まない一時のディレクトリだけにして確かめる。
- 出力のディレクトリは一時のディレクトリとし、`I4` と `I5` ではそこにファイルが無いことを確かめる。
- テストは `scripts/run-tests.sh` から走らせ、CI の `rust` ジョブで動く。

自動のテストにしないものは、次のとおり確かめる。

- ワークフロー：`A1` のとおり、マージの後に main で起動して確かめる。起動の前に、`actionlint` があれば `.github/workflows/bench.yml` を検査する。
- 記録の取り込み：`A2` のとおり確かめる。

## トレードオフ

- CI のランナーは、ほかの利用者の仮想機械と同じ物理の機械で動くことがあり、計測が揺れると考える（未確認）。
  計測の道具は、1つの項目で、比較の相手と sofel を同じプロセスの中で続けて計る（`bench/src/main.rs`）。
  そのため、比は時間の絶対値より揺れに強いと考える（推測）。
  記録には中央絶対偏差と、計測の直前の負荷平均を残し、比を読むときの目安とする。
- ランナーが実行ごとに同じ CPU を使うとは限らない（未確認）。
  記録に CPU の名前を書き、違う CPU の記録の時間を直接は比べない。
- 手動で起動するため、退行は自動では見つからない。
  起動の手間は、プルリクエストごとの計測で CI の時間を延ばすことより小さいと判断した。
- 計測の全体の時間は、まだ計っていない（未確認）。
  上限の120分を超えるなら、`filter` で項目を分けて起動する。
- macOS のランナーを使うことで、これまでの手元の記録（Apple M1 Max）と同じ arm64 の記録も取れる。
  ただし、CPU と OS の版は手元と違うため、手元の記録とは直接は比べない。

## 参考文献

- [ADR 0006](0006-performance-measurement.md)、[ADR 0013](0013-benchmark-harness.md)、[ADR 0014](0014-raster-sweep-touched-cells.md)、[ADR 0045](0045-foundation-completion-criteria.md)
- GitHub Docs「Workflow syntax for GitHub Actions」（`workflow_dispatch`、`permissions`、`timeout-minutes`）https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax
- GitHub Docs「Store and share data with workflow artifacts」https://docs.github.com/en/actions/tutorials/store-and-share-data
- GitHub CLI「gh run download」https://cli.github.com/manual/gh_run_download
