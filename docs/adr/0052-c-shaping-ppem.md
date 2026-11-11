# ADR 0052: C API でシェーピング時の横と縦の ppem を指定する

ステータス: 採択（未実装）

## 文脈

ppem はフォントの 1 em に相当するピクセル数である。
[ADR 0020](0020-shaping-foundation.md) のシェーピングでは、横と縦の倍率に加えて、それぞれの軸の ppem を使う。
特定のピクセルサイズで GPOS の Device 表による位置補正を使いたい C の呼び出し側にも、この値を渡す手段が要る。

## 調査した事項

C API のスケールを表す不透明型 `sofel_scale_t` の実装は、倍率を表す `HbScale` を保持する。
`sofel_shape` と `sofel_shape_fallback` は、この倍率と横・縦ともにゼロの ppem で Rust の `Scale` を作る。
Rust の `Scale::new` は各軸の ppem を `u32` で受け取り、Device 表の補正量は軸ごとに求める。

## 実装すること

次の関数を公開ヘッダーに追加する。

```c
sofel_scale_t *sofel_scale_create_with_ppem(const sofel_font_t *font,
    int32_t x_scale, int32_t y_scale, uint32_t x_ppem, uint32_t y_ppem,
    int32_t *error);
```

新しい構築子は既存の `sofel_scale_create` と同じ倍率の解釈、エラーの返し方、解放手順を用いる。
スケールの内部に横と縦の ppem を保持し、両方のシェーピング関数が Rust の `Scale` を作るときに渡す。
既存の構築子とシェーピング関数のシグネチャを維持する。
実装時に `unmapped.txt` の `sofel_shape::Scale::new` を公開ヘッダーの Rust API 対応欄に移し、公開ヘッダーの説明、仕様書と README を更新する。

## 実装しないこと

ラスタライズやメトリクスを求める API の振る舞いは対象外とする。

## 不変条件

1. 任意の有効なフォントと倍率について、既存の構築子で作ったスケールによるシェーピング結果は、新しい構築子で同じ倍率と ppem の組 `(0, 0)` を指定した結果と一致する。
2. 任意の横・縦の ppem について、C の各シェーピング関数は、同じ入力、倍率、ppem を渡した対応する Rust の関数と同じグリフ列と位置を返す。
3. スケールが `NULL` のとき、C の各シェーピング関数の結果は、Rust で同じフォントの `Scale::unscaled` を使った結果と一致する。

| スケールまたは ppem の指定 | Rust に渡す横・縦の ppem |
| --- | --- |
| `NULL` または既存の構築子 | (0, 0) |
| 新しい構築子で (0, 0) | (0, 0) |
| (1, 0) または (0, 1) | 指定した軸だけ 1 |
| (12, 16) | (12, 16) |
| (UINT32_MAX, 1) | (UINT32_MAX, 1) |

## テスト設計

C の公開ヘッダーを使うテストに `inv1_legacy_scale`、`inv2_axis_ppem`、`inv3_null_scale` を追加し、表の各ケースを通常とフォールバックの両方で確かめる。
通常のシェーピングには、横と縦で異なる ppem を指定すると両軸で補正が現れる Device 表を使う。
片軸だけをゼロにした場合も Rust の結果と照合し、軸の取り違えとゼロへの固定を検出する。
C API クレートの Rust 単体テスト `inv2_axis_ppem_values` では、最大値を含む組について、C のスケールから作った Rust の `Scale` の各値も直接照合し、切り詰めを検出する。

## トレードオフ

構築子を一つ追加すると、ppem を指定する呼び出し側は既存のシェーピング関数へ同じ型のスケールを渡せる。
横と縦の ppem を別の値に変えるときは、新しいスケールを作る。

## 参考文献

- [C の公開ヘッダー](../../crates/sofel-capi/include/sofel.h)
- [スケールの構築と解放](../../crates/sofel-capi/src/lib.rs)
- [C からのシェーピング](../../crates/sofel-capi/src/shape.rs)
- [Rust のシェーピング用スケール](../../crates/sofel-shape/src/scale.rs)
