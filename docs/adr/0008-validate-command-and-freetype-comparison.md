# ADR 0008: 検証のコマンドと、FreeTypeとの突き合わせの土台

ステータス: 採択（未実装）

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の1段階目には、表の読み取りと検証の土台（[ADR 0007](0007-sfnt-read-foundation.md)）に加え、検証のコマンドが含まれる。
また、正しさの基準はHarfBuzzとFreeTypeの出力であり、テストのデータは固定したコミットで取得する（[ADR 0005](0005-conformance-tests.md)）。
1段階目で読む表（head、maxp、hhea、hmtx、cmap）は、FreeTypeの公開のAPIで中身を取り出せるため、ここで突き合わせの仕組みを作る。

FreeTypeは、Cのコンパイラだけでビルドできる。`docs/INSTALL.ANY` が、コンパイルするファイルの一覧を示している。
FreeTypeは、読み込みの時点でcmapのサブテーブルを検証し、壊れたものを捨てる（`src/sfnt/ttcmap.c` の「broken cmap sub-table ignored」）。
FreeTypeの公開のAPIでは、次のものを取り出せる。

- `FT_Get_Sfnt_Table`：head、hhea、maxpの構造体（`TT_Header`、`TT_HoriHeader`、`TT_MaxProfile`）。
- `FT_Get_CMap_Format`、`FT_Set_Charmap`、`FT_Get_First_Char`、`FT_Get_Next_Char`：各サブテーブルの対応の組。
- `FT_Get_Advances` と `FT_LOAD_NO_SCALE`：hmtxの送り幅。

hmtxの左のサイドベアリングを生の値で取り出す公開のAPIは無い。

## 実装すること

### 検証のコマンド `sofel-validate`

- `crates/sofel-cli` を作り、バイナリ `sofel-validate` を置く。`sofel-cli` は `std` を使う。
- `sofel-validate <ファイル>...` は、各ファイルをsfntかTTC/OTCとして読み、`sofel-read` の検証器がある表をすべて検証する。
- 誤りは1件を1行で、標準出力に次の形で出す。`<フォント番号>` はコレクションのときだけ付ける。

  ```
  <パス>[#<フォント番号>]: <タグ>+0x<オフセット>: <error|warning>: <説明>
  ```

- `--json` を付けると、誤り1件をJSONのオブジェクト1つとし、1行に1つずつ出す（JSON Lines）。CIなどの機械での処理のためである。オブジェクトは次のキーを持つ。

  | キー | 値 |
  |---|---|
  | `path` | ファイルのパス（文字列） |
  | `font` | コレクションの中のフォント番号（数）。コレクションでなければ `null` |
  | `table` | 表のタグ（4文字の文字列） |
  | `offset` | 表の先頭からのオフセット（数） |
  | `severity` | `"error"` か `"warning"` |
  | `kind` | 誤りの種類の識別子（`Diagnostic` の種類の名前をsnake_caseにしたもの。例：`segments_not_sorted`） |
  | `message` | 人が読む説明（文字列） |
  | `details` | 種類ごとの値（期待した値と実際の値など）のオブジェクト |

  - 誤りが1件も無ければ、何も出さない。
  - JSONの書き出しは自分で書く（文字列のエスケープはRFC 8259に従う）。外部のクレートには依存しない。
  - `kind` の識別子と `details` のキーは、出力の約束の一部とする。変える場合は、リリースノートの「変更」に書く。
- 終了コードは、`Error` が1件も無ければ0、1件以上あれば1とする。使い方の誤り（未知のオプション、ファイルの指定なし）と、読めないファイルがあった場合は2とする。読めないファイルがあっても、ほかのファイルは検証を続ける。`--json` の有無で終了コードは変わらない。
- 使い方の誤りと読めないファイルの報告は、`--json` の有無にかかわらず、標準エラー出力に人が読む形で出す。
- 検証器の無い表は黙って飛ばす。
- `--help` で、両方の出力の形、`--json` のキー、終了コードを示す。

### テストのデータの取得

- `testdata.lock` を置き、HarfBuzz 15.1.0（`3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`）とFreeType 2.15.0（`37515cf56ba3349656049edbed89ffc6f6466f2d`）を書く（[ADR 0005](0005-conformance-tests.md)）。
  - 1行に1つ、`<名前> <URL> <コミット>` の形とする。
- `scripts/fetch-testdata.sh` は、`testdata.lock` の各行について、`target/testdata/<名前>/` にそのコミットだけを浅く取得する。
  - すでにそのコミットがあれば、何もしない。
  - 環境変数 `SOFEL_TESTDATA_OFFLINE=1` のときは取得をせず、すべてがそろっていなければ失敗する。CIで、キャッシュから戻したデータが完全かを確かめるためである。
  - 読む `testdata.lock` と取得先のディレクトリは、引数で変えられる（テストのため）。
  - POSIXのシェルで書き、macOSのBSDのコマンドでも動かす。

### `ft-dump`

- `ft-dump` は自分で書く。FreeTypeのデモの道具（ft2demosの `ftdump`）は使わない。FreeTypeからは、ライブラリの公開のAPIだけを使う。
- [ADR 0005](0005-conformance-tests.md) のテスト設計は、`ft-dump` の出力を「FreeTypeの `ftdump` やCの小さな確認のコードと照らして」確かめるとしていた。これを、下の「テスト設計」の、値の分かっている小さなフォントによる自動のテストに置きかえる。
- `tools/ft-dump/ft-dump.c` と、それをビルドする `tools/ft-dump/build.sh` を置く。
- `build.sh` は、`target/testdata/freetype/` のソースから `docs/INSTALL.ANY` の一覧のファイルと `ft-dump.c` を `cc` でコンパイルし、`target/ft-dump/ft-dump` を作る。
  - FreeTypeの設定（`ftoption.h`）は既定のまま使う。
  - 外部のライブラリ（zlibを除く。zlibはFreeTypeの同梱のものを使う）には依存しない。
- `ft-dump <ファイル>` は、ファイルのすべてのフォントについて、次を行ごとのテキストで標準出力に書く。
  - head、hhea、maxpの各フィールド（`FT_Get_Sfnt_Table`）。
  - FreeTypeが受け入れたcmapのサブテーブルごとに、プラットフォーム、エンコーディング、フォーマットと、対応の組（`FT_Get_First_Char`、`FT_Get_Next_Char`）。
  - 全グリフの送り幅（`FT_Get_Advances`、`FT_LOAD_NO_SCALE`）。
- 出力の形は `ft-dump.c` の先頭のコメントに書き、`sofel-conformance` はその形に合わせて書き出す。

### 突き合わせのテスト

- `crates/sofel-conformance` を作る。`default-members` に含めず、`cargo test -p sofel-conformance` で走らせる（[ADR 0005](0005-conformance-tests.md)）。
- 対象のフォントは、取得したHarfBuzzの `test/shape/data/*/fonts/` と `test/subset/data/fonts/` のすべてのsfntとする。
- 各フォントについて、`ft-dump` の出力と、`sofel-read` で同じ形に書き出した結果を、項目ごとに比べる。項目は次のとおり。
  - `head`、`hhea`、`maxp`：フィールドの値。
  - `cmap:<プラットフォーム>:<エンコーディング>`：対応の組の集合。
  - `hmtx`：送り幅。グリフIDが `numGlyphs` 未満の範囲だけを比べる。
- サブテーブルの検証の照合も行う。
  - FreeTypeが受け入れたサブテーブルは、`sofel-read` でビューを作れること。
  - FreeTypeが捨てたサブテーブル（`ft-dump` の出力に無いもの）は、`sofel-read` の検証器が `Error` を報告すること。
- テストの識別子は `<フォントのパス>#<フォント番号>:<項目>` とし、期待する失敗の一覧 `conformance/expected-failures-freetype.txt` で管理する（[ADR 0005](0005-conformance-tests.md)）。一覧には、失敗の理由を1行のコメントで添える。
- テストのデータか `ft-dump` が無いときは、取得とビルドの手順を示して失敗する。
- 検証のコマンドの照合のため、同じフォントのすべてについて `sofel-validate` を実行し、panicせずに終わることを確かめる。

### 依存の向きの検査

- クレートが2つ以上になるので、`scripts/check-deps.sh` を置く（[ADR 0003](0003-crate-layers.md)）。
  - 検査するワークスペースと `deps.txt` は、引数で変えられる（テストのため）。
  - 各クレートの直接の依存を `cargo metadata --format-version 1 --no-deps` から取り出す。
  - それが `scripts/deps.txt`（[ADR 0003](0003-crate-layers.md) の表を、1行に `<クレート> <依存先>` の形で写したもの）に無い依存を含めば失敗する。
  - `cargo metadata` の出力（JSON）の読み取りには `jq` を使う。`jq` はCIのランナーにある。
- `sofel-cli` と `sofel-conformance` は `deps.txt` で、ほかのすべてのクレートへの依存を許す。

### スクリプトのテスト

- シェルのスクリプトのテストを `scripts/tests/test-<スクリプト名>.sh` に置き、`scripts/run-tests.sh` ですべて走らせる。各テストはPOSIXのシェルで書き、一時ディレクトリの中だけで動き、失敗すれば0以外で終わる。

### CI

- 突き合わせのジョブを足す。LinuxとmacOSのランナーの両方で、次を行う。
  1. `testdata.lock` の内容をキーに、`target/testdata/` をキャッシュから戻す。
  2. キャッシュが当たったときは `SOFEL_TESTDATA_OFFLINE=1` で、外れたときは通常で、`scripts/fetch-testdata.sh` を走らせる。
  3. `tools/ft-dump/build.sh` で `ft-dump` をビルドする。
  4. `cargo test -p sofel-conformance` を走らせる。
- 既存のRustのジョブ（[ADR 0007](0007-sfnt-read-foundation.md)）に、`scripts/check-deps.sh` と `scripts/run-tests.sh` を足す。

## 実装しないこと

- `sofel-validate` の重大度の絞り込みのオプション（`--errors-only` など）。必要になったら加える。
- hmtxの左のサイドベアリングの突き合わせ。FreeTypeが生の値を公開しないため、単体テストで確かめる（[ADR 0007](0007-sfnt-read-foundation.md)）。
- FreeTypeのotvalidとの照合。この段階の表はotvalidの対象（BASE、GDEF、GPOS、GSUB、JSTF、MATH）でないため、それらの表を読む段階で加える。
- シェーピングの適合テスト。シェーピングの段階のADRで加える。
- `ft-dump` のWindowsでのビルド。
- fontToolsの `ttx` などPythonの道具による照合。`ft-dump` の正しさは、値の分かっている小さなフォントで確かめる。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。すべて自動で走り、CIで実行する。

- `sofel-validate`（`sofel-cli` の結合テスト）：テストのためのバイト列のフォント（正しいもの、`Error` を含むもの、`Warning` だけを含むもの、TTC）と、存在しないパスを一時ディレクトリに置いて実行し、次を確かめる。
  - 人が読む形の出力の行。
  - `--json` の出力の各行が、正しいJSONで、上の表のキーと値を持つこと。JSONの検査には、テストの中で書いた小さなJSONの解析器を使う。
  - `message` に引用符、改行、制御文字、非ASCIIの文字を含むときのエスケープ。
  - 終了コード（0、1、2）が `--json` の有無で変わらないこと。読めないファイルがあっても、ほかのファイルの結果が出ること。
- 期待する失敗の一覧の判定：一覧に無い失敗と、一覧にあるテストの成功が、ともにテストの失敗になることを、小さな偽の結果で確かめる（[ADR 0005](0005-conformance-tests.md)）。
- `ft-dump` の出力の解析：手で書いた出力のテキストを、項目ごとの値に正しく分けられることを確かめる。
- `ft-dump` の正しさ（`sofel-conformance` のテスト）：値の分かっている小さなフォント（[ADR 0007](0007-sfnt-read-foundation.md) の単体テストと同じ組み立て方で、head、hhea、maxp、cmapのフォーマット4と12、hmtxを持つもの。FreeTypeが捨てるはずの壊れたcmapのサブテーブルを持つものも作る）を一時ファイルに書き、`ft-dump` の出力が、テストに手で書いた期待の出力と一致することを確かめる。
- `scripts/fetch-testdata.sh`（`scripts/tests/test-fetch-testdata.sh`）：一時ディレクトリに2つのコミットを持つgitのリポジトリを作り、その `file://` のURLと古いほうのコミットを書いた `testdata.lock` で次を確かめる。
  - 取得した作業ツリーのHEADが、指定のコミットであること。
  - 元のリポジトリを消したあとの2回目の実行が、成功すること（取得しなおさないこと）。
  - `SOFEL_TESTDATA_OFFLINE=1` で、そろっているときは成功し、取得先を消したときは失敗すること。
- `tools/ft-dump/build.sh`：CIのLinuxとmacOSのランナーで、突き合わせのジョブがビルドする。ビルドの失敗はジョブの失敗になる。
- `scripts/check-deps.sh`（`scripts/tests/test-check-deps.sh`）：一時ディレクトリに2つのクレートを持つワークスペースを作り、`deps.txt` に合う依存では成功し、`deps.txt` に無い依存を足すと失敗することを確かめる。
- CIのキャッシュ：キャッシュが当たったときに `SOFEL_TESTDATA_OFFLINE=1` で取得のスクリプトを走らせるため、キャッシュの中身が欠けていればジョブが失敗する。

## トレードオフ

- `ft-dump` はCのプログラムで、FreeTypeのソースからビルドするため、CIと手元にCのコンパイラが要る。代わりに、FreeTypeの版を `testdata.lock` で固定でき、システムのFreeTypeの版に左右されない。
- FreeTypeが捨てたサブテーブルを `Error` とする照合は、FreeTypeの検証の基準（`FT_VALIDATE_DEFAULT`）に `sofel-read` の `Error` の基準を合わせることになる。FreeTypeより厳しい誤りを `Error` にしたい場合は、照合の向きを「FreeTypeが捨てた ⇒ `Error`」の一方向に限っているため、そのまま加えられる。
- 突き合わせのジョブをLinuxとmacOSの両方で走らせるため、CIの時間とmacOSのランナーの費用が増える。代わりに、`ft-dump` のビルドとFreeTypeの挙動を両方の環境で確かめられる。
- `--json` の出力の約束（`kind` の識別子と `details` のキー）を保つ必要があり、検証器の誤りの種類の名前を気軽に変えられなくなる。
- `scripts/check-deps.sh` は `jq` に依存する。`cargo metadata` のJSONをシェルだけで読むのは脆いため、`jq` を受け入れる。
- 対象のフォントはHarfBuzzのテストのフォント（大半がサブセット化された小さなもの）に限られる。大きなフォントや古いフォントの検査は、後の段階で `testdata.lock` を広げるADRで扱う。

## 参考文献

- FreeType `docs/INSTALL.ANY`、`src/sfnt/ttcmap.c`、`include/freetype/freetype.h`、`include/freetype/tttables.h`、`include/freetype/ftadvanc.h`（2.15.0）
- HarfBuzz `test/shape/data/`、`test/subset/data/fonts/`（15.1.0）
- RFC 8259（JSON）、JSON Lines https://jsonlines.org/
- jq https://jqlang.org/
