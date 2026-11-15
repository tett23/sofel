# ADR 0079: adr-guard を v0.13.1 に上げ、ADR のステータスの行を新形式に移す

Status: accepted-pending

## 文脈

sofel は、コミット済みの ADR と記録がステータスの行とリンクを除いて変わらないことを、adr-guard で検査している。
検査は、git の pre-commit の hook（`exec adr-guard pre-commit --allow-status --allow-links`）と、CI の二か所で行う。
CI では、`adr` ジョブが `tett23/adr-guard@624df60fe8956ef52b3a275198ee938175324496 # v0.10.0` の action を使う。
さらに、`rust` ジョブの「Install adr-guard for protection regression tests」の手順が、`version=v0.10.0` のリリースのアーカイブを取得して検証する。
その adr-guard は、[ADR 0032](0032-documentation-only-ci.md) の I05 のテスト（`scripts/tests/test-ci-guard.sh`）が使う。
adr-guard の版を決めた ADR はない。版は、最初の ADR を足したコミットと、[ADR 0032](0032-documentation-only-ci.md) を実装したコミットで、ワークフローに直接書いた。

adr-guard は、v0.10.0 の後に v0.11.0、v0.12.0、v0.13.0、v0.13.1 を出した。

| 版 | 変更 |
| --- | --- |
| v0.11.0 | Codex の PreToolUse hook `adr-guard codex-hook` を足した（[tett23/adr-guard ADR 0021](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0021-codex-hook.md)） |
| v0.12.0 | ステータスの行の新形式と、状態の遷移の検査を足した（[tett23/adr-guard ADR 0022](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md)）。修正の連なりをたどる `adr-guard trace` を足した（[tett23/adr-guard ADR 0023](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0023-trace-amendments.md)） |
| v0.13.0 | 移行のスクリプトに `--dry-run` を足し、移せない ADR を全件数えるようにした（[tett23/adr-guard ADR 0024](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0024-migration-dry-run-and-error-count.md)）。新形式に `superseded-pending`、変更の種類 `kind`（`replace`、`supplement`、`note`）と注記 `note`、`https://` の修正先を足した（[tett23/adr-guard ADR 0025](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0025-extend-canonical-status-amendments.md)）。旧形式の参照の読み方を広げた（[tett23/adr-guard ADR 0026](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0026-read-more-legacy-references.md)、[tett23/adr-guard ADR 0027](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0027-read-legacy-replaced-status.md)） |
| v0.13.1 | 旧形式の部分の名前から、`ADR NNNN` の直前の修飾子、先頭の `。`、末尾の `→` を除くようにした。参照の後ろに部分の名前の続きがある行と、括弧が部分の名前をまたぐ行は、移行スクリプトが「部分名を決められません」と報告し、移せない ADR として数える（[tett23/adr-guard ADR 0028](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0028-fix-legacy-part-names.md)） |

新形式のステータスの行は `Status: <トークン>` である。
トークンは `proposed`、`accepted`、`accepted-pending`、`superseded-pending`、`rejected`、`discarded`、`deprecated`、または `superseded [<変更の JSON 配列>]` のいずれかとする。
変更の要素は `{"part":<部分の名前か null>,"by":[<リポジトリのルートからのパス>]}` である。

[tett23/adr-guard ADR 0022](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md) によれば、v0.12.0 からは、追加した ADR と、ステータスの行を変えた ADR に新形式が要る。
pre-commit の hook と `check-range` は状態の遷移も検査し、遷移の検査は `--allow-status`、`ADR_GUARD_ALLOW`、トレーラーでは免除されない。
旧形式の行は、変えないかぎりそのまま読める。

## 調査した事項

この節の確認は、手元の adr-guard 0.13.1（`adr-guard -V` が `adr-guard 0.13.1` を出す）と、adr-guard の v0.13.1 のソースの `scripts/migrate-adr-status`（以下、移行スクリプト。リリースのアーカイブには入らない）で行った。

### action とリリース

- action の入力は v0.10.0 と v0.13.1 で同じである。tett23/adr-guard のクローンで `git diff --quiet v0.10.0 v0.13.1 -- action.yml` が終了コード 0 で終わった。
- タグ `v0.13.1` はコミット `ab55769152ed0f8a3465ee3435a2df43ab9d067f` を指す（tett23/adr-guard のクローンで `git rev-parse v0.13.1^{commit}`）。このコミットの `Cargo.toml` の版は `0.13.1` で、action は `version` を省くとこの版のリリースを取得する。
- v0.13.1 のリリースの `SHA256SUMS` は `adr-guard-v0.13.1-x86_64-unknown-linux-gnu.tar.gz` の行を持つ（`curl -fsSL https://github.com/tett23/adr-guard/releases/download/v0.13.1/SHA256SUMS`）。「Install adr-guard for protection regression tests」の手順は、`version` の値を変えるだけで、同じ名前の規則でアーカイブを取得できる。

### 作業ツリーの検査

このブランチの作業ツリーで、`adr-guard links`、`adr-guard check-status`、`adr-guard check-numbers` は、どれも終了コード 0 で終わった。
`adr-guard links --fix --include-committed` は、どのファイルも書きかえなかった。

### I05 のテスト

`scripts/tests/test-ci-guard.sh` は、v0.13.1 では失敗する。
このテストは、一時リポジトリに `ステータス: 提案` の ADR をコミットし、`ステータス: 採択` に変えたコミットを `check-range --allow-status --allow-links` が通すことを確かめる。
手元の adr-guard 0.13.1 で実行すると、「I05: ADR status update rejected」を出して終了コード 1 で終わった。
同じ手順を手で行うと、`check-range` は「新形式のステータス行は `Status: ` で始めてください」を出した（この手順は 0.13.0 で確かめた）。

一方、テストの二つのステータスの行を `Status: proposed` と `Status: accepted` に置きかえた写しは、手元の adr-guard 0.13.1 で `ok: ADR 0032 I05 record protection` を出して終了コード 0 で終わった。
置きかえたのはこの二行だけで、ほかの検査（ADR の本文の変更、削除、移動と、記録の追加、変更、削除、移動）は変えていない。

### pre-commit の hook

このリポジトリの写しで、v0.13.1 の pre-commit の hook は次のように振る舞った。

| ステージした変更 | 結果 |
| --- | --- |
| `ステータス: 提案` の新しい ADR | 拒否（「新形式のステータス行は `Status: ` で始めてください」） |
| `Status: accepted` の新しい ADR | 拒否（「許可されていない状態遷移: Absent -> accepted」） |
| `Status: proposed` の新しい ADR | 通る |

### ステータスの行の移行

移行スクリプトの `--dry-run` は、移せる ADR を 78 件、移せない ADR を 0 件と数え、終了コード 0 で終わった（このほかに、`Status: proposed` のこの ADR を移行済みの 1 件と数えた）。
「部分名を決められません」と報告された ADR はなかった。
`採択（未実装）` の 3 件は `accepted-pending` に、[ADR 0022](0022-subsetting.md) の `採択（未実装）→丸めは [ADR 0026](0026-harfbuzz-rounding.md)` は `superseded-pending` になる。
v0.13.0 の移行スクリプトと比べて、出す新しい行は変わらなかった。

移行の全体は、このリポジトリの写しで確かめた。
写しには、この ADR の草稿を置いてコミットし、`--write` を実行した。
その後の `--dry-run` は「移行できる: 0件、移行済み: 79件、移行できない: 0件」を出した。
変更をコミットすると pre-commit の hook が通り、`adr-guard check-range --allow-status --allow-links <元の HEAD> HEAD`、`check-status`、`links`、`check-numbers` は、どれも終了コード 0 で終わった。
移行の後の 79 件は、`accepted` が 59 件、`superseded` が 15 件、`accepted-pending` が 3 件、`superseded-pending` が 1 件、`proposed` が 1 件だった。
`adr-guard unimplemented` は、移行の前の作業ツリーで実行したときと同じ 4 件（[ADR 0022](0022-subsetting.md)、[ADR 0061](0061-subset-axis-instancing.md)、[ADR 0064](0064-freetype-hinting-and-render-modes.md)、[ADR 0065](0065-legacy-faces-and-webfonts.md)）を出した。

## 実装すること

### CI

- `.github/workflows/ci.yml` の `adr` ジョブの action を `tett23/adr-guard@ab55769152ed0f8a3465ee3435a2df43ab9d067f # v0.13.1` にする。入力（`base`、`head`、`allow-status: true`、`allow-links: true`）は変えない。
- 「Install adr-guard for protection regression tests」の手順の `version=v0.10.0` を `version=v0.13.1` にする。アーカイブの名前と取得先の URL の組み立てと、`SHA256SUMS` での検証は変えない。手順の前のコメントに `docs/adr/0079` を足す。

### I05 のテスト

- `scripts/tests/test-ci-guard.sh` の一時リポジトリの ADR のステータスの行を、`ステータス: 提案` から `Status: proposed` に、`ステータス: 採択` から `Status: accepted` にする。ほかの検査は変えない。
- 直す前のテストが v0.13.1 で失敗することは、調査した事項で確かめた。このテストを CI の版より先に直し、手元の v0.13.1 で通ることを確かめてから、CI の版を上げる。

### ステータスの行の移行

この ADR を `Status: proposed` でコミットした後に、リポジトリのルートで、adr-guard の v0.13.1 のソースの `scripts/migrate-adr-status` を `--dry-run` で実行する。
移せない ADR が 0 件であることを確かめてから、`--write` で書く。
移せない ADR が出たときは、そのステータスの行を手で新形式に書く。

### 文書

- README の開発の節の adr-guard の記述に、v0.13.1 以降が要ることと、ADR のステータスの行を新形式（`Status: <トークン>`）で書き、新しい ADR を `Status: proposed` でコミットすることを書く。
- `adr-guard links --fix --include-committed` を実行し、直すべき参照があれば直す。

## 実装しないこと

- `.githooks/pre-commit` のコマンドとオプションは変えない。v0.10.0 から引数は変わっていないため。
- Codex の hook（`adr-guard codex-hook`）は足さない。このリポジトリは Codex の hook の設定を持たないため。
- action の固定の仕方（コミットの SHA と版のコメント）は変えない。
- ADR の本文は変えない。変えるのはステータスの行だけとする。[ADR 0032](0032-documentation-only-ci.md) の文脈と参考文献にある v0.10.0 の記述も、当時の事実として残す。
- adr-guard の版を決めた ADR がないため、ほかの ADR のステータスを、この ADR による修正にはしない。
- 移行スクリプトをこのリポジトリに取り込まない。移行は一度だけの作業のため。

## 不変条件

なし。設定の値と、一度だけ行う移行の手順を決める ADR で、sofel のクレートとスクリプトの振る舞いを変えないため。
[ADR 0032](0032-documentation-only-ci.md) の I05 の性質（ADR のステータスだけの変更は通り、本文と記録の変更、削除、移動は失敗する）は変えない。

## テスト設計

- `sh scripts/run-tests.sh` で、直した `scripts/tests/test-ci-guard.sh` が手元の adr-guard 0.13.1 で通ることを確かめる。
- 移行の後に、移行スクリプトの `--dry-run` が「移行できない: 0件」を出し、`grep -l '^ステータス:' docs/adr/*.md` が何も出さないことを確かめる。
- 手元の adr-guard 0.13.1 で、`adr-guard links`、`adr-guard check-status`、`adr-guard check-numbers` が終了コード 0 で終わり、移行の変更をステージした状態で `.githooks/pre-commit` が通ることを確かめる。
- コミットの後、`adr-guard check-range origin/main HEAD --allow-status --allow-links` が終了コード 0 で終わることを確かめる。
- `adr-guard unimplemented` が、移行の前と同じ 4 件を出すことを確かめる。
- pull request の CI で、`adr` ジョブ（v0.13.1 の action）と、`rust` ジョブのスクリプトのテスト（v0.13.1 のアーカイブ）が通ることを確かめる。

## トレードオフ

- 新しい ADR は、まず `Status: proposed` でコミットし、採択は別のコミットでステータスの行を変えて記録することになる。最初から `ステータス: 採択` でコミットする手順は使えなくなるが、採択や却下が状態の遷移として検査される。
- I05 のテストは、新形式のステータスの行だけを試す。旧形式の行の変更が拒否されることは、adr-guard 自身のテストに任せる。
- 手元の adr-guard が v0.12.0 より古いと、新形式の検査と状態の遷移の検査が手元では効かず、CI で初めて失敗に気づく。README に必要な版を書いて補う。

## 参考文献

- [tett23/adr-guard v0.13.1 のリリース](https://github.com/tett23/adr-guard/releases/tag/v0.13.1)
- [tett23/adr-guard v0.13.0 のリリース](https://github.com/tett23/adr-guard/releases/tag/v0.13.0)
- [tett23/adr-guard v0.13.1 の action](https://github.com/tett23/adr-guard/blob/ab55769152ed0f8a3465ee3435a2df43ab9d067f/action.yml)
- [tett23/adr-guard ADR 0021: Codex の PreToolUse hook で、コミット済みのADRと記録の編集を拒否する](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0021-codex-hook.md)
- [tett23/adr-guard ADR 0022: 状態と遷移を別々の代数的データ型で表す](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0022-status-transitions.md)
- [tett23/adr-guard ADR 0023: trace で元の決定から修正ADRをたどる](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0023-trace-amendments.md)
- [tett23/adr-guard ADR 0024: 移行スクリプトに --dry-run を設け、失敗を最後まで数える](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0024-migration-dry-run-and-error-count.md)
- [tett23/adr-guard ADR 0025: 新形式のステータス行に、実装待ちの修正、補足、注記、別のリポジトリの修正先を加える](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0025-extend-canonical-status-amendments.md)
- [tett23/adr-guard ADR 0026: 旧形式のステータス行の修正先を、リンク先と裸の番号からも読む](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0026-read-more-legacy-references.md)
- [tett23/adr-guard ADR 0027: 旧形式の `置換（…）` を、修正を受けた状態として読む](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0027-read-legacy-replaced-status.md)
- [tett23/adr-guard ADR 0028: 旧形式の部分名から修飾子と区切りの記号を除き、決められない部分名を移行しない](https://github.com/tett23/adr-guard/blob/v0.13.1/docs/adr/0028-fix-legacy-part-names.md)
