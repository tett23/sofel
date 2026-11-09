# ADR 0005: HarfBuzzとFreeTypeの出力を正しさの基準にする

ステータス: 採択（未実装）→`ft-dump` の確かめ方は[ADR 0008](0008-validate-command-and-freetype-comparison.md)

## 文脈

sofelの正しさの基準は、HarfBuzzとFreeTypeの出力である（[ADR 0001](0001-project-goal-and-scope.md)）。
テストのフォントは同梱せず、外部のリポジトリを固定したコミットで取得して読む（[ADR 0002](0002-license-and-reuse.md)）。

HarfBuzzは、シェーピングのテストを `test/shape/data/*/tests/*.tests` に持つ。
1行が1つのテストで、`フォントのパス;hb-shapeのオプション;Unicodeの列;期待するグリフの列` の形である。
期待値は `hb-shape` の出力の形（`[uni06CC.fina=10+1655|...]`）で書かれているため、HarfBuzzを動かさずに比べられる。
`@font-funcs=ot,ft` のような `@` で始まる行は、後続の行への指示である。
対象のディレクトリは `in-house`（90ファイル）、`aots`（Adobe）、`text-rendering-tests`（Unicode）の3つである。

HarfBuzzは、サブセット化のテストを `test/subset/data/tests/*.tests`（104ファイル）に持つ。
フォント、プロファイル（`hb-subset` のオプション）、残す文字の組ごとに、期待する出力のフォントを `test/subset/data/expected/` に置き、出力がバイト単位で一致することを求める（`test/subset/run-tests.py`）。

FreeTypeには、グリフの輪郭とラスタライズの結果の期待値の集まりが無い。
skrifaは、FreeTypeとグリフ単位で突き合わせる道具（fontationsの `fauntlet`）で検査している。

sofelは段階を追って実装するため（[ADR 0001](0001-project-goal-and-scope.md)）、初めのうちは大半のテストが失敗する。
失敗を許しつつ、通ったテストが再び失敗しないことを保証する仕組みが要る。

## 実装すること

### テストのデータの取得

- `testdata.lock` に、取得するリポジトリのURLとコミットを書く。最初は次の2つとする。
  - HarfBuzz 15.1.0（`3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`）
  - FreeType 2.15.0（`37515cf56ba3349656049edbed89ffc6f6466f2d`）
- `scripts/fetch-testdata.sh` が `testdata.lock` を読み、`target/testdata/<名前>/` に、そのコミットだけを浅く取得する。POSIXのシェルで書き、macOSのBSDのコマンドでも動かす。
- 固定のコミットを上げるときは、`testdata.lock` を変えるプルリクエストで、期待する失敗の一覧（後述）の差分も合わせて見直す。

### シェーピングの適合テスト

- ワークスペースに、ライブラリとは別のクレート `sofel-conformance` を置く。`default-members` に含めず、`cargo test` では走らせない。`cargo test -p sofel-conformance` で走らせる。
- `*.tests` の各行を読み、`sofel-shape` でシェーピングし、`hb-shape` と同じ形で出力した文字列を期待値と比べる。
- テストの識別子は `<ディレクトリ>/<ファイル名>:<行番号>` とする。
- 期待する失敗の一覧を `conformance/expected-failures.txt` に置く。1行に1つの識別子を書く。
  - 一覧に無いテストが失敗したら、テストは失敗する（退行）。
  - 一覧にあるテストが通ったら、テストは失敗する。一覧から消すことを強いるためである。
- sofelがまだ解釈しない `hb-shape` のオプションを含むテストは、失敗として扱う（一覧に載る）。黙って飛ばさない。
- テストのデータが無いときは、`scripts/fetch-testdata.sh` を実行するよう案内して失敗する。
- `@font-funcs=` の指示は無視し、sofel自身のフォントの関数で比べる。
- AATのシェーピングも、同じ仕組みで `in-house` の `aat-*.tests` などと比べる。

### サブセット化の適合テスト

- `sofel-conformance` に、HarfBuzzの `test/subset/data/tests/*.tests` を読み、`sofel-subset` の出力が `test/subset/data/expected/` のフォントとバイト単位で一致することを確かめるテストを置く。
- 期待する失敗の一覧は、シェーピングと同じ仕組みで `conformance/expected-failures-subset.txt` に置く。
- 一致しないときの診断のため、表ごとに一致しない最初の位置を報告する。

### FreeTypeとの突き合わせ

- FreeTypeの出力を書き出す小さなCのプログラム `tools/ft-dump/` を置く。取得したFreeType 2.15.0のソースからビルドし、フォント、グリフID、ppem、ヒンティングの指定（なし、v35、v40、CFF、自動ヒンタ）、描画の形式（アンチエイリアス、モノクロ、LCD、SDF）を受け取って、輪郭（26.6の固定小数点の座標と命令）と、ラスタライズの結果（被覆率）を標準出力に書く。埋め込みビットマップ（EBDT、CBDT、sbix）のグリフは、展開したビットマップを書く。
- `sofel-conformance` に、同じ入力でsofelの出力を作り、`ft-dump` の出力と比べるテストを置く。比べるフォントは、取得したHarfBuzzのテストのフォントとする。
- sfnt以外の形式（Type1、CID、Type42、PFR、BDF、PCF、FNT、dfont）も、FreeTypeが読めるので `ft-dump` と比べる。`ft-dump` は、文字からグリフへの対応とメトリクスも書き出す。HarfBuzzのテストのフォントはsfntだけなので、これらの形式のフォントは、ライセンスを確かめたうえで `testdata.lock` に加えるADRを、その形式を実装するまえに作成する（[ADR 0002](0002-license-and-reuse.md)）。
- 輪郭は、ヒンティングの有無にかかわらず、座標が完全に一致することを求める。FreeTypeの固定小数点の演算は決定的であり、同じ演算を書けば一致させられるためである。
- ラスタライズの結果の許容差は、ラスタライズのADRで定める。
- 期待する失敗の一覧は、シェーピングと同じ仕組みで `conformance/expected-failures-freetype.txt` に置く。

### ほかの機能の基準

次の機能の正しさの基準は、それぞれのADRで定める。候補を挙げる。

- COLRv1とSVGのグリフの描画：HarfBuzzの `libharfbuzz-raster` の出力。
- SVGとPDFへの書き出し：HarfBuzzの `libharfbuzz-vector` の出力。
- GPUでの描画：HarfBuzzの `libharfbuzz-gpu` の符号化の出力と、`sofel-raster` の出力との画素の比較。
- WOFFとWOFF2の展開：FreeTypeの展開の結果（`ft-dump` で表を書き出す）。
- 表の検証器：FreeTypeのotvalidとgxvalidの判定。
- DEFLATE、gzip、LZW、bzip2、Brotli、PNGの展開：それぞれの仕様の参照実装が配布するテストのデータ（ライセンスを確かめてから使う。[ADR 0002](0002-license-and-reuse.md)）。

### CI

- `sofel-conformance` を作るプルリクエストで、テストのデータの取得、`ft-dump` のビルド、適合テストを行うCIのジョブを足す。取得したデータはコミットをキーにキャッシュする。

## 実装しないこと

- HarfBuzzのAPIのテスト（`test/api/`）。HarfBuzzのC言語のAPIの挙動を検査するもので、sofelはそのAPIを持たない（[ADR 0001](0001-project-goal-and-scope.md)）。
- ファジング。表の読み取りの層を作るADRで、別に判断する。
- HarfBuzzとFreeTypeを、テストのたびにその場で動かして比べること。シェーピングは `*.tests` の期待値で足り、FreeTypeは `ft-dump` の出力で比べる。
- 同梱しないフォント以外の大きなフォントの集まり（Google Fontsなど）による検査。必要になったら、`testdata.lock` に加えるADRを作成する。

## テスト設計

この ADR は検査の仕組みを定める。仕組み自体を、次のテストで確かめる。

- `*.tests` の行の解析：フォントのパス、オプション、Unicodeの列、期待値に分けられること。`@` の行と空行とコメントを扱えること。HarfBuzzの `in-house` の全ファイルを解析して、解析の失敗が0であること。
- 期待する失敗の一覧の判定：一覧に無いテストの失敗、一覧にあるテストの成功、の2つがともにテストの失敗になること。小さな偽のテストの集まりで確かめる。
- `scripts/fetch-testdata.sh`：手元で実行し、`target/testdata/harfbuzz` のHEADが `testdata.lock` のコミットと一致することを確かめる。2回目の実行で取得しなおさないことを確かめる。
- `ft-dump`：1つのフォントの1グリフについて、出力をFreeTypeの `ftdump` やCの小さな確認のコードと照らして、一度手で確かめる。

## トレードオフ

- 適合テストは外部のリポジトリの取得とCのビルドを要するため、`cargo test` だけでは走らない。単体テストと適合テストの2段になる。
- サブセット化はバイト単位の一致を求めるため、表の並べ方やオフセットの詰め方までHarfBuzzに合わせる必要がある。より小さい、または速い書き出し方があっても選べない。HarfBuzzの結果と一致させることで、`hb-subset` から置きかえやすくなる利点を取る。
- 輪郭の完全一致を求めるため、FreeTypeと異なる、より速い演算（浮動小数点など）を選べない箇所が出る。速さと一致がぶつかった場合は、ADRで判断する。
- 期待する失敗の一覧は、実装が進むたびに書きかえが要る。代わりに、進み具合が一覧の行数で見え、退行を確実に止められる。
- `@font-funcs=` を無視するため、HarfBuzzのフォントの関数（`ot` と `ft`）の差を前提にしたテストが失敗しうる。その場合は一覧に載せ、理由をコメントで書く。

## 参考文献

- HarfBuzz `test/shape/README.md`、`test/shape/data/in-house/tests/*.tests`、`test/subset/run-tests.py`、`test/subset/data/tests/*.tests`（15.1.0）
- fontations `fauntlet/` https://github.com/googlefonts/fontations
- HarfRust `HARFBUZZ.md`（0.15.0）
