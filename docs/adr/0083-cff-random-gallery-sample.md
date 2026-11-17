# ADR 0083: CFF の `random` を使うグリフを描画見本に加える

Status: accepted

## 文脈

[ADR 0082](0082-cff-random-interval-check.md) は、CFF の charstring の演算子 `random`（12 23）の決まりを明文にした。
`random` の状態は、グリフを描くたびに Private DICT の `initialRandomSeed` から始まり、その値と進め方は FreeType の `cf2_escRANDOM` と `cff_random` と同じである。
そのため、sofel では、同じグリフは何度描いても同じ輪郭になる。

[ADR 0069](0069-font-render-gallery.md) の描画見本には、`random` を使うグリフが無い。
`random` を使う実在のフォントは、知られていない。
Golem の調べ（Golem root ADR 0390）は、既存のフォントの1グリフの charstring を `random` を使うプログラムに書き換えた試験のフォントを作った。
この ADR は、同じ作り方の派生フォントを描画見本のビルドで作り、見本に加える。

## 調査した事項

次の事実は、`target/testdata/extra-fonts/fonts/stix-math.otf`（[fonts.lock](../../fonts.lock) の `stix-math`、SHA-256 `56cc42241cccc86d6d570134aa918fe8ac714f4b340d9916282fcc53c592d546`）を fontTools で読んで確かめた。

| 事項 | 値 |
| --- | --- |
| CFF の種類 | 名前で引く CFF（CID ではない） |
| グリフの数 | 5543 |
| Private DICT の `initialRandomSeed` | 0（sofel と FreeType は 987654321 として扱う） |
| `R`（U+0052）のグリフ | グリフ名 `R`、グリフ ID 20、charstring 139 バイト |
| name の ID 1、4 | `STIX Two Math` |
| name の ID 6 | `STIXTwoMath` |

ライセンスは、`target/testdata/extra-fonts/licenses/stix-license.txt` の1行目で確かめた。

- SIL Open Font License 1.1 である。
- 予約フォント名（Reserved Font Name）は「TM Math」である。
- 「STIX Fonts」は IEEE の商標である。

Golem の試験のフォントは、`target/handoff/golem-cff-random/random-audit/src/patch.py` で作られた。
このスクリプトは、1グリフの charstring を、同じ長さのまま次のプログラムに書き換え、余りのバイトを `endchar`（14）で埋める。

```
random 1000 mul random 1000 mul rmoveto 500 0 rlineto 0 500 rlineto endchar
```

このプログラムは26バイトで、`R` の charstring（139バイト）に収まる。

## 実装すること

### 派生フォントを作る

- `tools/font-gallery` に、固定した STIX Two Math から派生フォントを作る関数を足す。
- 派生フォントは、`R` のグリフの charstring を、上のプログラムと `endchar` の埋め草で、同じ長さのまま書き換える。CharStrings の INDEX のオフセットは変えない。
- name 表の ID 1、3、4、6（ID 16 と 17 があればそれも）を、「TM Math」と「STIX」を含まない名前（ファミリー名 `Sofel Random Sample`、PostScript 名 `SofelRandomSample-Regular`）に置き換える。ID 0（著作権）、13（ライセンス）、14（ライセンスの URL）は変えない。
- 表のチェックサムと `head` の `checkSumAdjustment` は、`sofel-write` の `build_sfnt` で作り直す。
- 派生フォントは `target/derived-fonts/sofel-random-sample.otf` に書き、隣に元のライセンス（`stix-license.txt`）を写す。描画見本の出力先（`target/font-gallery/`）には置かない。

### 描画見本に加える

- 描画見本に、識別子 `derived-cff-random` のカードを足す。
- 単字の欄は `U+0052` を、文字列の欄は `RRRR` を、48 ppem で描く。
- 文字列の4つの `R` は、どれも同じ輪郭になる。これが `random` の決まり（グリフごとに種から始める）を目で見られる形にする。

### ライセンス（改変したとき）

派生フォントは、OFL 1.1 の「改変版（Modified Version）」に当たる。
OFL の条件と、この ADR での扱いは次のとおりである。

| OFL 1.1 の条件 | この ADR での扱い |
| --- | --- |
| 改変版は、予約フォント名を、利用者に見える名前に使わない（条件3） | 名前を `Sofel Random Sample` に変え、「TM Math」を使わない。商標の「STIX」も使わない |
| 改変版も、OFL のもとでだけ配る（条件5） | 派生フォントは配らない。手元と CI の `target/` の中で作るだけで、リポジトリにも artifact にも入れない |
| 元の著作権表示とライセンスを添える（条件2） | name の ID 0、13、14 を残し、派生フォントの隣にライセンスの全文を写す |
| フォント単体を売らない（条件1） | 売らない |
| 作者の名前を、改変版の宣伝に使わない（条件4） | 使わない |

- OFL は、フォントで作った文書（描画見本の PNG と HTML）には及ばない（OFL の条件5の後半）。描画見本の出力は、これまでどおり artifact にしてよい。
- [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) に、STIX Two Math を描画見本で改変して使うことと、その名前と出どころを書き足す。
- 派生フォントを配る必要が出たときは、新しい ADR で、OFL の改変版として配る形（名前、ライセンスの同梱、FONTLOG）を決める。

### 文書

- [docs/specifications.md](../specifications.md) と README の描画見本の節に、このカードと派生フォントの作り方を書く。

## 実装しないこと

- 派生フォントの配布と、リポジトリへのコミット（[ADR 0002](0002-license-and-reuse.md) のテストのデータの決まり）。
- `random` の区間による確かめ（[ADR 0082](0082-cff-random-interval-check.md) の範囲）。
- FreeType との描画の比較。FreeType の `random` の種はプロセスの状態に依る（[ADR 0082](0082-cff-random-interval-check.md) の調査）ので、比べられない。
- CID で引く CFF と CFF2 の見本。CFF2 は `random` を持たない。

## 不変条件

- `I1`: 派生フォントと元のフォントは、`CFF ` の表の `R` の charstring の範囲と、`name` の表と、表のチェックサムと `head` の `checkSumAdjustment` のほかで、バイトが等しい。
- `I2`: `R` の charstring は、上のプログラムの26バイトに、元の長さまで `endchar` を続けたものである。
- `I3`: sofel で派生フォントの `R` を、別々に2回、間に別のグリフを挟んで描くと、輪郭の命令の列は等しい。
- `I4`: その輪郭の始点は、種 987654321 から FreeType の `cff_random` の式で求めた2つの値を `1000 mul` したものに一致する。テストは、この式を自前で書いた参照で値を求める。
- `I5`: 派生フォントの name の ID 1、3、4、6、16、17 は「TM Math」と「STIX」を含まず、ID 0、13、14 は元と等しい。
- `I6`: 元のフォントの SHA-256 が `fonts.lock` と違うとき、または `R` の charstring が26バイトより短いとき、派生フォントを書かずにエラーにする。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 固定した STIX Two Math | 派生フォントができ、差は charstring と name とチェックサムだけ | I1, I2, I5 |
| `R` を2回、間に `S` を描いて描く | 2回の命令の列が等しい | I3 |
| 種 987654321 | 始点が参照の式の値と等しい | I4 |
| SHA-256 の違うフォント | エラー、出力なし | I6 |
| charstring が25バイトのグリフ | エラー、出力なし | I6 |
| charstring がちょうど26バイトのグリフ | 埋め草なしで書き換わる | I2 |

## 受けいれ条件

- `A1`: `sh scripts/render-font-gallery.sh` で、`derived-cff-random` のカードが一覧に入り、文字列の欄に同じ形の `R` が4つ並ぶ。
- `A2`: CI の描画見本の artifact（`font-gallery-<sha>`）に、`.otf` と `.ttf` のファイルが無い。

## テスト設計

- 実装のまえに、`tools/font-gallery/tests/` に `I1`〜`I6` のテストを書いて失敗させる。テストの名前は `adr0083_i1_...` の形とする。
- `I1` は、元と派生のフォントの表の目録を読み、`CFF ` の表は `R` の charstring の範囲を除いて、ほかの表はそのまま、バイトを比べる。
- `I4` は、テストの中に `cff_random`（`r ^= r << 13; r ^= r >> 17; r ^= r << 5`）と `(r & 0xFFFF) + 1` の16.16の値を書き、その値に1000を掛けた始点と、sofel の輪郭の最初の `MoveTo` を比べる。
- `I6` は、1バイトを変えた元のフォントと、charstring を短く切ったフォントで確かめる。
- `A1` と `A2` は、描画見本のテストと、CI の artifact のファイルの一覧で確かめる。`A2` は、描画見本のジョブに、出力先に `.otf` と `.ttf` が無いことを確かめる一行を足す。

## トレードオフ

- 実在のフォントではなく派生フォントなので、`random` の実際の使われ方の見本にはならない。代わりに、決まりの効き目（同じ形になること）を目で確かめられる。
- 描画見本のビルドに、派生フォントを作る手順が増える。手順は数ミリ秒で、取得したフォントから毎回作り直すので、キャッシュは要らない。
- 元のフォントが新しい版に変わると、`R` のグリフ ID と charstring の長さが変わりうる。そのときは、固定を変える変更で、この ADR の調べた事項を新しい ADR で取り直す。

## 参考文献

- [ADR 0002](0002-license-and-reuse.md)、[ADR 0069](0069-font-render-gallery.md)、[ADR 0073](0073-rust-font-gallery.md)、[ADR 0082](0082-cff-random-interval-check.md)
- SIL Open Font License 1.1、https://openfontlicense.org/open-font-license-official-text/
- Adobe Technical Note #5177, "The Type 2 Charstring Format"（`random`）
- Golem root ADR 0390 の調査の写し（`target/handoff/golem-cff-random/`）
