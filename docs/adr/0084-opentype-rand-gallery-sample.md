# ADR 0084: OpenType の `rand` を足した派生フォントを描画見本に加える

Status: proposed

## 文脈

OpenType の機能 `rand`（Randomize）は、文字を異体字の一つにランダムに置き換える。
HarfBuzz 15.1.0 は `rand` を既定で有効にし（`src/hb-ot-shape.cc` の `map->enable_feature (HB_TAG ('r','a','n','d'), F_RANDOM, HB_OT_MAP_MAX_VALUE)`）、バッファの乱数の状態（初め1）で異体字を選ぶ。
sofel もバッファに同じ状態を持つ（`crates/sofel-shape/src/buffer.rs` の `random_state`）。

[ADR 0069](0069-font-render-gallery.md) の描画見本には、`rand` を持つフォントが無い。
調べた範囲では、`rand` を持つ OFL の実在のフォントは見つからなかった。
この ADR は、OFL の手書き風のフォント Caveat に、すでにある異体字を使う `rand` を足した派生フォントを、描画見本のビルドで作り、見本に加える。

## 調査した事項

### `rand` を持つフォントの探索

次の配布元の ttf と otf の GSUB の FeatureList を読み、`rand` を探した（2026-11-18）。

| 配布元 | 走査した本数 | `rand` を持つもの |
| --- | ---: | ---: |
| google/fonts の `ofl/`、`apache/`、`ufl/`（コミット `7af0d90b`） | 3914 | 0 |
| GitHub の theleagueof、adobe-fonts、silnrsi、Collletttivo、velvetyne の各 org | 1615 | 0 |
| GitHub の googlefonts org（778 リポジトリのうち 245） | 4599 | 0 |
| GitHub の arrowtype、raphlinus の各 org | 18227 | HarfBuzz のテストのフォントの写しだけ |

- `rand` を持つのは、HarfBuzz のテストのフォント `test/shape/data/in-house/fonts/5bb74492f5e0ffa1fbb72e4c881be035120b6513.ttf`（1352 バイト、T、U、V の3文字だけ、HarfBuzz の Old MIT）と、機能の検査用の GPL と Bitstream Vera のフォント（`twardoch/test-fonts`）だけだった。
- Velvetyne の自前のサイト、Open Font Library、Use & Modify、Typotheque と、googlefonts org の残りは走査していない（未確認）。

### Caveat

次の事実は、`https://raw.githubusercontent.com/google/fonts/5571d84c0d8c70ec1af4f64072d8c5cf1e4e9643/ofl/caveat/Caveat%5Bwght%5D.ttf` を取得して確かめた。

| 事項 | 値 |
| --- | --- |
| ファイル | `Caveat[wght].ttf`、403648 バイト、SHA-256 `0bdb6b660482d31531b3945849fba5916b3ef8695da7024a9e6b9ee3c4157988` |
| 形式 | glyf の可変フォント（wght 400〜700）。静的なファイルは google/fonts に無い |
| GSUB | 版 1.0、8212 バイト。ScriptList は DFLT と latn（言語 `CAT `）、FeatureList は13、LookupList は30。FeatureVariations と拡張の lookup（型7）は無い |
| 異体字 | a〜z と A〜Z の52文字が、それぞれ `x.ss01` と `x.ss02` を持つ。`ss01` は lookup 28、`ss02` は lookup 29（どちらも型1） |
| 既定の LangSys | DFLT と latn の既定の LangSys は、`aalt, calt, case, dlig, frac, liga, ordn, salt, ss01, ss02, subs, sups` を持つ |
| `calt` | lookup 3（型6）が、文脈に応じて lookup 4〜15（型1）で `.ss01` と `.ss02` に切り替える |

- a、e、o、t、r の `.ss01` と `.ss02` は、基本形とも、互いとも外形が違う。
- ライセンスは、同じコミットの `ofl/caveat/OFL.txt` である。1行目は `Copyright 2014 The Caveat Project Authors (https://github.com/googlefonts/caveat)` で、予約フォント名の宣言（`with Reserved Font Name`）は無い。

## 実装すること

### 元のフォントを固定する

- [gallery-fonts.lock](../../gallery-fonts.lock) に、上の固定した URL の `Caveat[wght].ttf` と `OFL.txt` を、SHA-256 とバイト数とともに足す。台帳（`conformance/gallery-fonts.tsv`）にも足す。

### 派生フォントを作る

- `tools/font-gallery` に、Caveat から派生フォントを作る関数を足す。
- GSUB には次を足す。既存の Script、LangSys、Feature、Lookup の番号と中身は変えない。
  - LookupList の末尾（番号 30）に、型3（Alternate Substitution）の lookup を1つ足す。a〜z と A〜Z の各文字 `x` の AlternateSet を `[x, x.ss01, x.ss02]` とする。
  - FeatureList の末尾（番号 13）に、`rand` の FeatureRecord を足し、その lookup だけを参照させる。
  - DFLT と latn の既定の LangSys の FeatureIndex に、13 を足す。`CAT ` の LangSys は変えない。
- name 表の ID 1、3、4、6（ID 16 と 17 があればそれも）を、ファミリー名 `Sofel Rand Sample`、PostScript 名 `SofelRandSample-Regular` に置き換える。ID 0、13、14 は変えない。
- 表のチェックサムと `head` の `checkSumAdjustment` は、`sofel-write` の `build_sfnt` で作り直す。
- 派生フォントは `target/derived-fonts/sofel-rand-sample.ttf` に書き、隣に `OFL.txt` を写す。描画見本の出力先（`target/font-gallery/`）には置かない。

### データ構造（GSUB の組み立て）

- 新しい GSUB は、新しいヘッダ、新しい ScriptList、FeatureList、LookupList、変えた LangSys、足した lookup とその subtable を前に置き、元の GSUB のバイト列をそのまま後ろに続ける。
- 新しい一覧の項目のうち、変えないもの（既存の Feature と Lookup、`CAT ` の LangSys）は、後ろに続く元のバイト列の中の、元の表を指す。元の表の中のオフセットは、それぞれの表の先頭からの相対なので、そのまま使える。
- オフセットは16ビットなので、新しい一覧から元の表への距離が65535を超えるときは、派生フォントを書かずにエラーにする。Caveat の GSUB は8212バイトなので、超えない。
- 組み立てには `sofel-write` の `Serializer` を使う。

### 描画見本に加える

- 描画見本に、識別子 `derived-rand` のカードを足す。
- 単字の欄は `U+0061`（a）を、文字列の欄は `aaaa eeee oooo tttt` を、既定の機能（`rand` を含む）でシェーピングして48 ppem で描く。
- 可変フォントの軸は既定の位置（wght 400）とする。
- `calt` も異体字に切り替えるので、文字列の欄の字形は `rand` と `calt` の両方で決まる。`rand` だけの効き目は、不変条件 `I3` のテストで確かめる。

### ライセンス（改変したとき）

派生フォントは、OFL 1.1 の「改変版（Modified Version）」に当たる。
OFL の条件と、この ADR での扱いは次のとおりである。

| OFL 1.1 の条件 | この ADR での扱い |
| --- | --- |
| 改変版は、予約フォント名を、利用者に見える名前に使わない（条件3） | Caveat に予約フォント名は無い。改変版と元を取り違えないよう、名前を `Sofel Rand Sample` に変える |
| 改変版も、OFL のもとでだけ配る（条件5） | 派生フォントは配らない。手元と CI の `target/` の中で作るだけで、リポジトリにも artifact にも入れない |
| 元の著作権表示とライセンスを添える（条件2） | name の ID 0、13、14 を残し、派生フォントの隣に `OFL.txt` を写す |
| フォント単体を売らない（条件1） | 売らない |
| 作者の名前を、改変版の宣伝に使わない（条件4） | 使わない |

- OFL は、フォントで作った文書（描画見本の PNG と HTML）には及ばない（OFL の条件5の後半）。描画見本の出力は、これまでどおり artifact にしてよい。
- [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) に、Caveat を描画見本のために取得して改変すること、その出どころ、著作権表示、OFL の全文を足す。
- 派生フォントを配る必要が出たときは、新しい ADR で、OFL の改変版として配る形（名前、ライセンスの同梱、FONTLOG）を決める。

### 文書

- [docs/specifications.md](../specifications.md) と README の描画見本の節に、このカードと派生フォントの作り方を書く。

## 実装しないこと

- 派生フォントの配布と、リポジトリへのコミット（[ADR 0002](0002-license-and-reuse.md) のテストのデータの決まり）。
- `rand` の新しい異体字のグリフを描くこと。異体字は Caveat にすでにあるものだけを使う。
- 可変フォントの軸の見本（wght を変えた見本）。
- 一般の GSUB の編集の API。この ADR の組み立ては、描画見本の道具の中だけで使う。

## 不変条件

- `I1`: 派生フォントと元のフォントは、`GSUB` と `name` の表と、表のチェックサムと `head` の `checkSumAdjustment` のほかで、バイトが等しい。
- `I2`: 派生フォントの GSUB の Script、LangSys（DFLT と latn の既定の LangSys を除く）、Feature 0〜12、Lookup 0〜29 は、元の GSUB のものと内容が等しい。Feature 13 は `rand` で Lookup 30 だけを参照し、Lookup 30 は型3で、a〜z と A〜Z の各文字 `x` の AlternateSet が `[x, x.ss01, x.ss02]` である。DFLT と latn の既定の LangSys の FeatureIndex は、元の列の末尾に 13 を足したものである。
- `I3`: sofel で、派生フォントを `rand` を切って（`-rand`）シェーピングした結果（グリフ ID と位置）は、元のフォントを既定の機能でシェーピングした結果と等しい。
- `I4`: sofel で、派生フォントを既定の機能でシェーピングした結果は、固定した HarfBuzz（`hb-shape`）の結果と、グリフ ID と位置で等しい。
- `I5`: 派生フォントの name の ID 1、3、4、6、16、17 は `Sofel Rand Sample` の名前で、ID 0、13、14 は元と等しい。
- `I6`: 元のフォントの SHA-256 が台帳と違うとき、ある文字の `x.ss01` か `x.ss02` が無いとき、またはオフセットが65535を超えるとき、派生フォントを書かずにエラーにする。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 固定した Caveat | 派生フォントができ、差は GSUB と name とチェックサムだけ | I1, I2, I5 |
| `aaaa eeee oooo tttt`、`-rand` | 元のフォントと同じ結果 | I3 |
| `aaaa eeee oooo tttt`、既定の機能 | `hb-shape` と同じ結果 | I4 |
| 空の文字列 | 両方とも0グリフ | I3, I4 |
| `a` 一つ | `hb-shape` と同じ1グリフ | I4 |
| `CAT ` の言語でのシェーピング | `rand` は効かず、元のフォントと同じ結果 | I2, I3 |
| SHA-256 の違うフォント | エラー、出力なし | I6 |
| `z.ss02` を消したフォント | エラー、出力なし | I6 |

## 受けいれ条件

- `A1`: `sh scripts/render-font-gallery.sh` で、`derived-rand` のカードが一覧に入り、文字列の欄で、同じ文字の繰り返しに2種類以上の字形が出る。
- `A2`: CI の描画見本の artifact（`font-gallery-<sha>`）に、`.otf` と `.ttf` のファイルが無い。

## テスト設計

- 実装のまえに、`tools/font-gallery/tests/` に `I1`〜`I6` のテストを書いて失敗させる。テストの名前は `adr0084_i1_...` の形とする。
- `I2` は、派生フォントの GSUB を sofel の GSUB のビューで読み、Feature と Lookup を番号ごとに元のものと比べる。
- `I4` は、`sofel-conformance` の `hb-shape` の比較の仕組み（[ADR 0020](0020-shaping-foundation.md)）で、派生フォントと見本の文字列を比べる。
- `I6` は、1バイトを変えた元のフォントと、`z.ss02` のグリフ名を消したフォントで確かめる。オフセットのあふれは、元の GSUB の長さを65535に近づけた組み立てのフォントで確かめる。
- `A1` は描画見本のテストで、文字列の欄のグリフ ID の列に、同じ文字の違う異体字が2つ以上あることを確かめる。`A2` は、[ADR 0083](0083-cff-random-gallery-sample.md) と同じ CI の一行で確かめる。

## トレードオフ

- 実在のフォントの `rand` ではないので、デザイナーが意図した `rand` の見本にはならない。代わりに、HarfBuzz と sofel の `rand` の選び方が同じであることを、見本と `I4` で確かめられる。
- Caveat の `calt` も異体字に切り替えるので、文字列の欄だけでは `rand` の効き目と区別できない。`I3` が `rand` だけの差を確かめる。
- 元の GSUB を後ろにそのまま残すので、使われないヘッダと一覧のバイトが残る。GSUB を全部組み立て直すより単純で、元の lookup を壊さない。
- Caveat の版が変わると、異体字の名前や GSUB の番号が変わりうる。そのときは、固定を変える変更で、この ADR の調べた事項を新しい ADR で取り直す。

## 参考文献

- [ADR 0002](0002-license-and-reuse.md)、[ADR 0020](0020-shaping-foundation.md)、[ADR 0069](0069-font-render-gallery.md)、[ADR 0073](0073-rust-font-gallery.md)、[ADR 0083](0083-cff-random-gallery-sample.md)
- SIL Open Font License 1.1、https://openfontlicense.org/open-font-license-official-text/
- OpenType の機能 `rand`、https://learn.microsoft.com/typography/opentype/spec/features_pt#tag-rand
- HarfBuzz 15.1.0 `src/hb-ot-shape.cc`、`src/hb-ot-layout-gsub-table.hh`（AlternateSubst の乱数の選び方）
- Caveat、https://github.com/googlefonts/caveat
