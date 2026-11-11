# ADR 0046: FeatureVariations v1.1のlookup条件を計画に含める

ステータス: 採択

## 文脈

[ADR 0027](0027-variable-fonts.md) は、GSUBとGPOSのFeatureVariationsで最初に一致する記録を選び、2つの記録番号を`ShapePlan::key`に含める。HarfBuzz 15.1.0のFeatureVariations v1.1は、通常のFeatureVariationRecordの後にLookupVariationRecordの配列を追加する。同じFeatureVariationRecordを選ぶ位置でも、lookup条件の真偽によって有効なlookupが異なる。

HarfBuzz 15.1.0の`hb_ot_shape_plan_key_t`は、2つの記録番号に加え、GSUBとGPOSそれぞれの真になった条件のオフセットを昇順で保持する。`test/api/test-shape-plan.c`は、同じフォントのTEST軸0、0.3、0.6、0.8で計画を区別し、0.3と0.4では同じ計画を使うことを検査する。sofelの現行のキーは記録番号だけなので、この差を表せない。

現行の可変フォントの参照コーパスでは、`in-house/variations.tests:7–12`の6件と、同じフォントの`--shape-random`の1件がこの拡張に依存する。`in-house/variations.tests:4`と`in-house/vertical.tests:22`の差は本ADRの対象ではない。

## 実装すること

1. `sofel-read::feature_variations`は、FeatureVariationsの版が1.1以上のときだけ、通常の記録の直後にあるLookupVariationRecordの配列を読む。版1.0では後続のバイトを拡張として扱わない。LookupVariationRecordの`featureIndex`、FeatureLookupsTableへのオフセット、FeatureLookupsTableの`flags`、LookupConditionRecordの条件とLookupIndexListを、境界を検査するビューで公開する。検証器にもこれらの配列、オフセット、条件の形式を追加する。GSUB・GPOSのLookupListの件数が分かる呼び出し元では、lookup番号の範囲も診断する。
2. 条件の評価には[ADR 0027](0027-variable-fonts.md)の形式1〜5とGDEFのItemVariationStoreを使う。nullの条件は常に真とする。同じFeatureLookupsTableと同じ条件オフセットの評価を再利用し、真になった相異なる条件のFeatureVariations先頭からのオフセットを昇順に並べる。HarfBuzzと同じく、条件の真偽が異なれば有効なlookup集合が同じでも別のキーとしてよい。
3. `ShapePlan::key()`と`ShapePlan::variations_index_for(instance)`の結果に、GSUBとGPOSの条件状態を加える。[ADR 0027](0027-variable-fonts.md)の「鍵が同じなら計画を使いまわせる」という条件は、この拡張を含む鍵に適用する。形を整えるため、新しい専用のキー構造体を使ってよい。
4. 機能の索引に一致する最初のLookupVariationRecordがあれば、そのFeatureLookupsTableを使う。`flags`のbit 0が立つ場合は[ADR 0027](0027-variable-fonts.md)で選んだFeature（代替後）のlookupを加え、立たなければ加えない。条件が真のLookupConditionRecordが示すlookupを加える。重複を除いたlookup番号の昇順を計画に渡し、範囲外のlookup番号を使わない。該当するLookupVariationRecordがなければ、[ADR 0027](0027-variable-fonts.md)で選んだ代替Featureのlookupを使う。
5. HarfBuzz 15.1.0の`test/api/test-shape-plan.c`と`test/api/test-ot-layout.c`の該当テスト、および`in-house/variations.tests:7–12`と同じフォントの`--shape-random`を参照テストにする。実際に一致した行だけ、期待する失敗の一覧から削除する。既存の`fuzz/fuzz_targets/feature_variations.rs`にも、v1.1の条件とオフセットを読む経路を足す。
6. [docs/specifications.md](../specifications.md)と[README.md](../../README.md)を実装時の振る舞いに合わせて更新する。

## 実装しないこと

HarfBuzzのこの拡張を使わないFeatureVariations v1.0の選択規則は変えない。gvarの幻の点、HVAR/VVAR、CFF2の箱、軸の正規化は本ADRでは変更しない。HarfBuzz以外のエンジンの独自拡張は扱わない。

## データ構造とアルゴリズム

FeatureVariations v1.1の末尾の配列は、`u32`の件数と6バイトのLookupVariationRecordからなる。各記録は`u16 featureIndex`と、FeatureVariations先頭からの`u32 featureLookupsOffset`を持つ。FeatureLookupsTableは16.16の版、`u16 flags`、`u32`の件数、8バイトのLookupConditionRecordを持つ。各条件記録はFeatureLookupsTable先頭からの`u32 conditionOffset`と`u32 lookupIndicesOffset`を持ち、LookupIndexListは`u16`の件数と`u16`のlookup番号の列を持つ。

計画のキーは、GSUBとGPOSそれぞれで、通常のFeatureVariationRecordの選択番号と、真になった相異なる条件のオフセット列を持つ。条件のオフセットはFeatureVariations先頭を基準とする。条件の評価は位置ごとに一度行い、計画の組み立てではオフセット列を二分探索して各条件の真偽を得る。この手順はHarfBuzz 15.1.0の`collect_lookup_conditions`と`get_lookup_variations`に合わせる。

## 不変条件

1. **ADR0046-I1**: 版1.0では末尾のバイトにかかわらずlookup条件状態は空で、版1.1以上では読み取れる記録だけから状態を作る。境界を越える配列またはオフセットを読んでもpanicしない。
2. **ADR0046-I2**: 同じフォント、区間の性質、機能、FeatureVariationRecordの番号について、条件状態が等しければ計画のlookup集合も等しい。条件状態が異なれば、計画のキーも異なる。
3. **ADR0046-I3**: 条件状態の各オフセットは相異なり、厳密な昇順である。null条件は状態に追加せず、lookupには適用する。
4. **ADR0046-I4**: 各機能のlookup集合は、最初に一致するLookupVariationRecordの選択済みFeatureのlookup（flag bit 0が立つときだけ）と、真の条件が示すlookupの和集合である。番号は昇順で重複せず、LookupListの件数未満である。

| 境界入力 | I1 | I2・I3 | I4 |
| --- | --- | --- | --- |
| 版1.0、拡張のような後続バイト | 状態は空 | 既定のキー | 既定のFeature |
| 版1.1、記録0件 | 読める | 状態は空 | 既定のFeature |
| 条件1件、真／偽 | 読める | 異なるキー | 真の側だけlookupを追加 |
| 同じ条件オフセットを2記録が参照 | 読める | オフセットは1件 | lookupは和集合 |
| null条件、flag bit 0が0／1 | 読める | 状態は空 | 条件のlookupは常に有効、選択済みFeatureのlookupはflagに従う |
| lookup番号が最後の有効値／1つ超過 | 読める | 状態は同じ | 超過番号を除く |
| 配列がちょうど収まる／1バイト足りない | 後者は誤り | panicしない | 不正な記録を適用しない |

## テスト設計

実装の前に、次のテストを追加して失敗を確認する。ビューと検証器は、版1.0と1.1、件数0と1、末尾がちょうど収まる場合と1バイト足りない場合、相対オフセット0と範囲外、条件の形式1〜5、lookup番号の境界を手で組んだバイト列で調べる。テスト名にADR0046-I1を入れる。

計画は、同じ条件を2つの記録で共有する場合、null条件、flag bit 0の両方、重複したlookup、同じ機能の2つのLookupVariationRecordを手で組んで調べる。TEST軸の0、0.3、0.4、0.6、0.8では、HarfBuzzのテストと同じキーの等値関係を確かめる。テスト名にADR0046-I2、ADR0046-I3、ADR0046-I4を入れる。

参照コーパスとファズでは、`test/api/test-shape-plan.c`、`test/api/test-ot-layout.c`、`in-house/variations.tests:7–12`を通す。ファズの入力には、件数とオフセットの境界、条件の深さの上限、条件が同じ表を再訪する場合を含める。

## トレードオフ

位置ごとのキーに条件のオフセット列を2つ持つため、記録番号だけのキーよりメモリと比較の時間が増える。HarfBuzzと同じく、結果のlookup集合が同じでも条件状態が違えば別の計画を作りうる。条件を1回だけ評価してキーに再利用すると、計画を作る過程で同じ条件を繰り返し評価せずに済む。

## 参考文献

- HarfBuzz 15.1.0 `src/hb-ot-layout-common.hh`：`LookupConditionRecord`、`FeatureLookupsTable`、`LookupVariationRecord`、`FeatureVariations`。https://github.com/harfbuzz/harfbuzz/blob/15.1.0/src/hb-ot-layout-common.hh
- HarfBuzz 15.1.0 `src/hb-ot-shape.hh`：`hb_ot_shape_plan_key_t`。https://github.com/harfbuzz/harfbuzz/blob/15.1.0/src/hb-ot-shape.hh
- HarfBuzz 15.1.0 `src/hb-ot-map.cc`：`hb_ot_map_builder_t::add_lookups`。https://github.com/harfbuzz/harfbuzz/blob/15.1.0/src/hb-ot-map.cc
- HarfBuzz 15.1.0 `test/api/test-shape-plan.c`。https://github.com/harfbuzz/harfbuzz/blob/15.1.0/test/api/test-shape-plan.c
- HarfBuzz 15.1.0 `test/api/test-ot-layout.c`。https://github.com/harfbuzz/harfbuzz/blob/15.1.0/test/api/test-ot-layout.c
- OpenType FeatureVariationsの標準部分。https://learn.microsoft.com/en-us/typography/opentype/spec/chapter2#featurevariations-table
