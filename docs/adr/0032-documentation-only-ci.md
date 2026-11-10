# ADR 0032: 文書だけの変更ではCIのビルドとテストを省く

ステータス: 採択→省略する文書の判定は [ADR 0034](0034-ci-document-input-classification.md)

## 文脈

現在の `.github/workflows/ci.yml` は、すべての pull request と main への push でビルドとテストを実行する。
文書だけを変えた場合も、Rust、FreeType、HarfBuzz、wasmのビルドとfuzzを待つ必要がある。
文書だけの変更では、ADRと記録の保護を実行し、プロダクトのビルドとテストを省く。

一方、コミット済みの監査と性能計測の記録は、文書であっても変更してはいけない。
現在の `adr` ジョブの adr-guard v0.10.0 は、ADRだけでなく `docs/audit/`、`docs/audits/`、`docs/benchmark/`、`docs/benchmarks/` 以下も保護する。
そのため、workflow全体をパスのフィルタで止めず、重いジョブだけを条件付きにする。

## 実装すること

### 文書だけの変更の定義

変更したすべてのパスが次のいずれかに当てはまる場合を、文書だけの変更とする。
パスはリポジトリ直下からの相対パスで、大文字と小文字を区別する。

| パス | 範囲 |
|---|---|
| `docs/` 以下 | 拡張子を問わず、文書、図、監査記録、性能計測の記録を含む |
| `README.md`、`CONTRIBUTING.md` | リポジトリ直下と各サブディレクトリの同名ファイル |

追加、変更、削除を同じ規則で分類する。
移動は削除と追加として扱い、移動前と移動後の両方のパスを調べる。
`README.rs`、`README.md.sh`、`Cargo.toml`、`scripts/`、`.github/`、lockファイルなどは省略対象に含めない。
一つでも省略対象外のパスがあれば、従来のビルドとテストをすべて実行する。

### 変更の判定

`changes` ジョブを追加し、Gitの差分から `docs_only` を文字列の `true` または `false` として出力する。
判定用の `scripts/ci-docs-only.sh` はBashとGitだけを使い、Rustや外部のパス判定actionを導入しない。
`git diff --no-renames --name-only -z` の結果をNUL区切りのまま読み、空白、改行、ワイルドカード文字を含むファイル名も一つのパスとして扱う。
パスをシェルのコードとして評価しない。

pull request では、イベントに記録された base SHA と head SHA の merge-base から head までを比較する。
main への push では、イベントの before SHA と after SHA を比較する。
`HEAD^` だけとの比較や、APIが返すファイル一覧の件数制限に依存しない。
チェックアウトは `fetch-depth: 0` とし、イベントのSHAを環境変数経由で渡す。

差分が空の場合と、beforeがゼロSHAの場合は `docs_only=false` とする。
必要なコミットまたはmerge-baseを取得できない場合も、省略せず `false` と理由を出力する。
Gitの差分処理に失敗した場合は `changes` を失敗させ、成功したCIとして扱わない。

### ジョブの実行条件

`rust`、`conformance`、`wasm`、`fuzz`、`bench`、`capi-wasm` は `changes` に依存させ、`docs_only == 'false'` の場合だけ実行する。
条件はジョブ単位に置き、文書だけの場合にはmatrixの各ビルド環境の準備も行わない。
省略対象のジョブ名と、実行する場合の既存の検査内容は維持する。

`adr` は `changes` の結果に依存させず、すべての対象イベントで実行する。
既存の `check-range`、ADRリンク、ステータス、番号の重複の検証を維持する。
ADRのステータスとリンクの例外は維持するが、監査と計測の記録にはその例外を適用しない。
新しい記録の追加は許可し、コミット済みの記録の変更、削除、移動は拒否する。
このADRはadr-guardのコミット範囲の検査方式や緊急時の例外を変更しない。

`ci-result` ジョブを `always()` で実行し、`changes` と `adr` の成功を必須にする。
通常の変更では六つのビルドとテストのジョブがすべて成功したことを必須にする。
文書だけの場合は六つがすべて `skipped` であることを確認する。
依存ジョブの失敗、キャンセル、想定外の省略、判定値の欠落は失敗にする。
ブランチ保護で必須チェックを設定する場合は、この `ci-result` と `adr` を指定する。

実装時にはREADMEと仕様書に、CIを省くパスと常に実行する検証を記載する。
本ADRの提案段階ではworkflowを変更しない。

## 実装しないこと

- workflowの `paths-ignore` で文書変更の実行全体を止めること。
- Markdown全体を拡張子だけで省略対象にすること。
- コードの変更箇所に応じてクレートごとのテストを省くこと。
- 文書だけの変更でRustのインストール、ビルド、単体テスト、文書のビルド、fuzz、フォントの取得、ベンチマークを実行すること。
- 監査と計測の記録の本文を修正可能にすること。

## 不変条件

1. **I01**: 正常に取得した空でない差分について、`docs_only` が `true` であることと、差分のすべてのパスが省略対象であることは同値である。
2. **I02**: 移動のいずれか一方のパスが省略対象外なら `docs_only=false` である。
3. **I03**: 判定不能または空の差分が `docs_only=true` になることはない。
4. **I04**: 文書だけの変更では六つのビルドとテストのジョブはすべて省略され、`adr` は実行される。
5. **I05**: 既存の保護対象の記録を通常のコミットで変更、削除、移動した場合、文書だけの変更であっても `adr` は失敗する。新規記録の追加はその理由では失敗しない。
6. **I06**: `ci-result` の成功は、判定と保護の成功、および判定に対応する六つのジョブの結果を必要とする。

| 境界の入力 | 判定または結果 |
|---|---|
| 差分なし | 通常CI |
| `docs/specifications.md` 一つ | 文書だけ |
| `README.md` または `CONTRIBUTING.md` 一つ | 文書だけ |
| `crates/sofel-read/README.md` 一つ | 文書だけ |
| 文書300個とコード1個 | 通常CI。先頭300個だけで判定しない |
| `docs/example.rs` から `crates/example.rs` への移動 | 通常CI |
| `docs/a.md` から `docs/b.md` への移動 | 文書だけ。ADRや記録なら別途保護検証 |
| 文書パスに空白または改行を含む | パスを分割せず判定 |
| コミット済みの `docs/benchmarks/result.svg` の変更 | ビルドを省き、保護検証で失敗 |
| 新規の `docs/audits/report.md` | ビルドを省き、通常の文書検証 |
| 不明なbefore、merge-baseなし | 通常CI |
| Git差分処理の失敗 | `changes` と `ci-result` が失敗 |

## テスト設計

判定用スクリプトのテストは、一時Gitリポジトリを作るシェルスクリプトとして実装する。
まずI01からI03のテストを追加して失敗を確かめ、その後で判定を実装する。
テスト名には `I01` などの番号を含める。
既存のスクリプト検査に組み込み、判定スクリプト自身を変えたPRで実行する。
文書だけのPRでは、この自動テストも実行しない。

I01からI03は境界表のパス、複数コミットのPR、複数コミットのpush、baseが進んだPR、移動、ゼロSHAを検証する。
I05は一時リポジトリにADRと四種類の記録ディレクトリのファイルをコミットし、追加、変更、削除、移動をadr-guardで検証する。
ADRのステータスだけの変更が通ることと、同じ変更を記録に加えると失敗することも検証する。

I04とI06は実装PRの通常CIと、実装後の文書だけのPRのActions結果で確認する。
後者では、`changes`、`adr`、`ci-result` の成功と六つのジョブの省略を確認する。
集約のテストでは失敗、キャンセル、判定値欠落、想定外の `skipped` を入力し、成功として扱わないことを確かめる。

## トレードオフ

判定と集約のジョブを増やすため、コードを変えたPRには短い準備時間が加わる。
一方、文書だけのPRでは外部ライブラリのビルドと全テストを待たず、記録の保護を維持できる。

`docs/` 以下を拡張子によらず文書として扱うため、実行に必要なソースやテストデータをその下に置かない運用が必要になる。
将来その配置が必要になった場合は、省略対象の規則を先に変更する。
READMEに実行可能なコードを埋め込んでコンパイル対象にする場合も同様である。

## 参考文献

- sofel `.github/workflows/ci.yml`（`7a6f8c6e41588a0c525e070cfd536158621aa0d2`）。
- [adr-guard v0.10.0 action](https://github.com/tett23/adr-guard/blob/624df60fe8956ef52b3a275198ee938175324496/action.yml)。
- [GitHub Actions: workflowの起動とパスフィルタ](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow)。
- [GitHub Actions: ジョブの条件と省略時の結果](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-jobs-with-conditions)。
