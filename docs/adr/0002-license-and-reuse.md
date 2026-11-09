# ADR 0002: ライセンスと、既存の実装とテストの流用

ステータス: 採択

## 文脈

sofelはすべて自分で書くが、既存の実装とテストは、ライセンスの条件を満たす範囲で流用してよい（[ADR 0001](0001-project-goal-and-scope.md)）。
流用の候補と、そのライセンスは次のとおりである（各リポジトリのライセンスのファイルで確かめた）。

| 候補 | ライセンス | 条件 |
|---|---|---|
| HarfBuzz | Old MIT（`COPYING`） | すべての複製に、著作権表示と2つの段落を含める |
| HarfBuzzの `src/ms-use/` | MIT（Microsoft） | 著作権表示と許諾の文を含める |
| FreeType | FreeType License（FTL）とGPLv2の選択 | FTLを選ぶ場合、製品の文書でFreeType Projectを明示する（いわゆる宣伝条項）。FTLはGPLv2と両立しない |
| HarfRust | MIT | 著作権表示と許諾の文を含める |
| fontations | MITとApache-2.0の選択 | MITを選べば、著作権表示と許諾の文を含める |
| Slug（GPUでの描画の方式とシェーダー） | MITとApache-2.0の選択。READMEは、特許をパブリックドメインに献呈したと述べる | MITを選べば、著作権表示と許諾の文を含める。READMEは、配布するソフトウェアで使う場合にクレジットの表記を求める |
| HarfBuzzのテスト：`test/shape/data/aots` | Apache-2.0（Adobe） | 著作権表示、ライセンスの写し、変更の明示 |
| HarfBuzzのテスト：`test/shape/data/text-rendering-tests` | Apache-2.0（Unicode） | 同上 |
| HarfBuzzのテスト：`test/shape/data/in-house` のフォント | `test/COPYING` はOFL 1.1が「多くのフォントに」当てはまるとだけ述べる | フォントごとのライセンスを確かめられない |

## 実装すること

### sofelのライセンス

- sofelのライセンスはMITとする。リポジトリ直下に `LICENSE` を置く。
- FreeTypeのコードを流用するときは、FTLを選ぶ。GPLv2は選ばない。

### 流用したときの表示

- リポジトリ直下に `THIRD_PARTY_NOTICES.md` を置き、流用元ごとに著作権表示とライセンスの全文を載せる。
- FreeTypeを流用した時点で、READMEとリリースノートに、FTLの推奨する次の文を載せる。`<year>` は流用元のFreeTypeの版の年とする。

  ```
  Portions of this software are copyright © <year> The FreeType Project (https://freetype.org). All rights reserved.
  ```

- Slugを流用した時点で、READMEにSlugとその作者のクレジットを載せる。

- 流用したコードを含むファイルの先頭に、流用元（リポジトリ、版またはコミット、パス）とライセンスをコメントで書く。移植や改変をした場合もそう書く。
- 新しい流用元を加えるときは、そのライセンスを確かめ、この表に当てはまらなければADRを作成する。

### テストのデータ

- テストのフォントはsofelのリポジトリに同梱しない。
- HarfBuzzとFreeTypeのリポジトリを、固定したコミットで取得し、そのデータを読んでテストする。取得と固定の方法は [ADR 0005](0005-conformance-tests.md) で定める。
- テストの期待値の書かれたファイル（HarfBuzzの `*.tests` など）も、同じく取得して読む。

## 実装しないこと

- GPLの流用元を使うこと。sofelをGPLにしない。
- テストのフォントの同梱。ライセンスを確かめられないものを再配布しない。
- 流用の自動検出（ライセンスの表示の漏れを機械で見つける仕組み）。レビューで確かめる。

## テスト設計

コードを生まないため、自動テストは無い。次を確かめる。

- このADRの採択後、`LICENSE` と `THIRD_PARTY_NOTICES.md`（流用の無い時点では見出しと説明だけ）が置かれていること。
- 流用したコードを含むプルリクエストのレビューで、ファイルの先頭のコメントと `THIRD_PARTY_NOTICES.md` の追記があること。

## トレードオフ

- MITにすると、FTLの宣伝条項がMITの条件に加わる。sofelの利用者は、FreeType由来の部分を含む限り、文書でFreeType Projectを明示する必要がある。MIT単独より条件が増えるが、FreeTypeのヒンティングやラスタライズの知見を使える利点を取る。
- FTLを選ぶため、sofelはGPLv2のプログラムに組み込めない（GPLv3とは両立する）。
- RustのクレートではMITとApache-2.0の選択が慣例だが、MIT単独とする。Apache-2.0の特許条項の利点よりも、ライセンスの組み合わせを単純に保つことを優先する。
- テストのフォントを同梱しないため、テストの実行に外部のリポジトリの取得が要る。

## 参考文献

- HarfBuzz `COPYING`、`src/ms-use/COPYING`、`test/COPYING`、`test/shape/data/aots/COPYING`、`test/shape/data/text-rendering-tests/COPYING`（15.1.0）
- FreeType `LICENSE.TXT`、`docs/FTL.TXT`（2.15.0）
- HarfRust `LICENSE`（0.15.0）
- fontations `LICENSE-MIT`、`LICENSE-APACHE`
- Slug `LICENSE`、`README.md` https://github.com/EricLengyel/Slug
