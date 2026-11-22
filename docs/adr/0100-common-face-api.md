# ADR 0100: 旧形式を含む共通フェイス API とメトリクス単位

Status: accepted-pending

## 文脈

[root ADR 0065](0065-legacy-faces-and-webfonts.md) は、旧形式を共通のフェイス番号、文字対応、メトリクス、輪郭またはビットマップの操作へ接続すると決めた。
この ADR は、共通操作を受け持つ型、メトリクスの単位、C ABI の関数の形を補足する。
既存の sfnt 用の `Font` と `sofel_font_t` は保ち、共通操作には別の `Face` と `sofel_face_t` を使う。

[root ADR 0015](0015-c-abi.md) は、Rust の公開機能を C からも使えることと、新しい関数の名前と形を ADR に書くことを求める。
共通フェイスの Rust API と C ABI は、同じ実装 PR で追加する。

## 調査した事項

`crates/sofel-font/src/lib.rs` の `Font` は sfnt の情報を持ち、`Font::new` は `head` のない入力を開けない。
素の CFF と CEF、Type1、CID、Type42、PFR、旧ビットマップ形式には別のフェイス型がある。
`crates/sofel-outline/src/lib.rs` の輪郭取得には `Font::sfnt()` を直接使う箇所がある。

既存の送り幅の単位は形式によって違う。
sfnt と Type42 はフォント単位、BDF、PCF、FNT、FON の `Glyph::advance` は整数ピクセル、PFR の埋め込み画像の送り幅は 26.6 ピクセルである。
Type1、CID、素の CFF、PFR の輪郭を描く入口は、各形式の UPM と ppem からピクセル座標へ変換する。
これらは `crates/sofel-font/src/var.rs`、`type42.rs`、`bare_cff.rs`、`pfr.rs`、`bitmap_face.rs`、`crates/sofel-bitmapfont/src/lib.rs`、`crates/sofel-bitmap/src/pfr.rs`、`crates/sofel-glyph-render/src/lib.rs` で確認した。

`crates/sofel-type1/src/face.rs` の `attach_afm` と `attach_pfm` は、付加メトリクスを置き換える。
`crates/sofel-shape/src/font.rs` の `SubFont::from_type1` は、Type1 の文字対応、送り幅、付加メトリクスのカーニングをシェーピングに渡す。

## 実装すること

### 共通操作と層

`sofel-font` に、既存の形式別フェイスを借用する `Face` を置く。
共通操作は、グリフ数、Unicode 文字対応、送り幅、グリフ対のカーニング、固定ストライクの列挙と選択とする。
フェイスを開く入口は、root [ADR 0065](0065-legacy-faces-and-webfonts.md) の形式判定と展開を共有し、フェイス件数と番号を検査する。
外側の gzip、LZW、bzip2、WOFF、WOFF2 も、この入口で既存のデコーダを使って扱う。
Rust では `FaceData` が入力、展開結果、Type1 の復号結果を所有し、`FaceData::open` がそれらを借用する `OpenedFace` を返す。
`OpenedFace` は形式別の解析結果を所有し、`OpenedFace::as_face` が共通問い合わせ用の `Face` を返す。
入力データ、解析結果、借用ビューを分け、Rust の所有型に自分のフィールドへの参照を格納しない。
C ABI のハンドルはこの所有と借用の寿命を内部で管理し、解析結果を先に破棄する。

輪郭と画像の共通操作は `sofel-glyph-render`、シェーピングの共通操作は `sofel-shape` に置く。
`sofel-font` から輪郭、描画、シェーピングのクレートへ逆向きの依存を追加しない。
CLI の旧形式を扱う入口も共通操作を使う。
CLI の既定の画像優先順位と `--no-bitmap` の扱いを保ち、既存の選択で使うストライクを共通入口へ明示する。

### サイズと単位

共通操作のサイズは、正方形ピクセルの `ppem` と、任意の固定ストライク番号で指定する。
`ppem` は 1 以上の `u16` とする。
ストライクの横と縦の 26.6 ppem は、それぞれ `(値 + 32) / 64` で整数へ丸めて要求 ppem と比較する。
固定ストライクを指定した場合は、丸めた横と縦の両方が要求 ppem と一致しなければ失敗する。
指定しない場合、ビットマップ専用フェイスではこの条件を満たす最初のストライクを選び、一致するものがなければ失敗する。
画像を拡大縮小せず、選んだストライクそのものを FreeType の `FT_Select_Size` と比較する。
輪郭を持つフェイスでストライクを指定しない場合は輪郭を使う。
存在しないストライクを黙って別のサイズに置き換えない。

共通操作の送り幅、カーニング、輪郭座標、シェーピングの位置は、符号付き整数の 26.6 ピクセルで返す。
輪郭は FreeType の `FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP` に相当する条件で読み、既存の形式別の座標変換と丸めを使う。
フォント単位の送り幅とカーニングは、`FT_DivFix(ppem * 64, UPM)` で得る 16.16 スケールに `FT_MulFix` を適用する整数演算と丸めで換算する。
カーニングの参照条件は `FT_KERNING_UNFITTED` とし、ピクセル格子への丸めを追加しない。
固定画像の整数ピクセル値は 64 倍し、既に 26.6 ピクセルの値はそのまま使う。
UPM がゼロの場合と、スケールまたは変換結果が `i32` の範囲を超えた場合は失敗する。
既存の形式別 API が返す単位は変えない。

カーニングは各形式のペア情報を問い合わせる。
PFR のペア情報の読み取りと検索、および sfnt の旧 `kern` の直接ペア問い合わせを共通操作の実装に含める。
既存の問い合わせがないことを理由に一律ゼロを返さない。
登録のない有効なグリフ対はゼロ、範囲外のグリフはエラーとする。
sfnt では `TT_CONFIG_OPTION_GPOS_KERNING` が無効の固定版 FreeType の `FT_Get_Kerning` に相当するペアの値を返し、GPOS のシェーピング結果をペア問い合わせへ混ぜない。

### 所有権と付加メトリクス

C のフェイスハンドルは、入力バイト列、展開結果、必要な形式別フェイスを所有する。
作成に渡した入力は、作成関数から戻った後に解放してよい。
フェイスハンドルは読み取り専用で共有でき、作業領域と画像の出力は呼び出し側が持つ。

AFM または PFM を付加するときは、新しいフェイスハンドルを返す。
元のハンドルを変更せず、新しいハンドルの付加メトリクスを指定した内容で置き換える。
Type1 以外への付加、破損した入力、未定義の形式番号は失敗とする。
新しいハンドルは元のハンドルと付加データの呼び出し側バッファを解放した後も使える。

### C ABI

`sofel_face_t` は不透明な型とする。
固定ストライクの指定なしは `UINT32_MAX`、付加メトリクスの形式は `SOFEL_METRICS_AFM = 0` と `SOFEL_METRICS_PFM = 1` とする。
以下の関数を `sofel-capi` と `sofel.h` に追加する。

```c
typedef struct sofel_face sofel_face_t;
sofel_face_t *sofel_face_create(const uint8_t *data, size_t len,
                               uint32_t index, int32_t *error);
void sofel_face_destroy(sofel_face_t *face);
int32_t sofel_face_count(const uint8_t *data, size_t len, uint32_t *count);
int32_t sofel_face_num_glyphs(const sofel_face_t *face, uint32_t *count);
int32_t sofel_face_nominal_glyph(const sofel_face_t *face, uint32_t codepoint,
                                uint32_t *glyph);
int32_t sofel_face_strike_count(const sofel_face_t *face, uint32_t *count);
int32_t sofel_face_strike(const sofel_face_t *face, uint32_t index,
                         uint32_t *x_ppem_26_6, uint32_t *y_ppem_26_6);
int32_t sofel_face_h_advance(const sofel_face_t *face, uint32_t glyph,
                            uint16_t ppem, uint32_t strike, int32_t *advance_26_6);
int32_t sofel_face_kerning(const sofel_face_t *face, uint32_t left, uint32_t right,
                          uint16_t ppem, uint32_t strike,
                          int32_t *x_26_6, int32_t *y_26_6);
sofel_face_t *sofel_face_with_metrics(const sofel_face_t *face,
                                     const uint8_t *data, size_t len,
                                     uint32_t format, int32_t *error);
int32_t sofel_face_outline_draw(const sofel_face_t *face, uint32_t glyph,
                                uint16_t ppem, sofel_outline_scratch_t *scratch,
                                sofel_move_to_func_t move_to,
                                sofel_line_to_func_t line_to,
                                sofel_quad_to_func_t quad_to,
                                sofel_cubic_to_func_t cubic_to,
                                sofel_close_func_t close, void *user,
                                int32_t *cbox, uint32_t *overlap);
int32_t sofel_face_render_glyph(const sofel_face_t *face, uint32_t glyph,
                               uint16_t ppem, uint32_t strike,
                               sofel_outline_scratch_t *scratch,
                               sofel_rasterizer_t *rasterizer,
                               uint8_t *pixels, size_t capacity,
                               size_t *len, int32_t *box);
int32_t sofel_face_shape(const sofel_face_t *face, uint16_t ppem, uint32_t strike,
                         const sofel_segment_t *segment,
                         const sofel_feature_t *features, size_t n_features,
                         sofel_buffer_t *buffer);
```

戻り値、NULL、呼び出し側配列、画像バッファと箱の配置は、root [ADR 0015](0015-c-abi.md) と既存の対応する関数に従う。
Unicode 対応のない文字は、既存の `sofel_font_nominal_glyph` と同じエラーを返す。
ビットマップ専用フェイスへの輪郭要求は、対応しない操作として失敗する。
画像の出力形式は既存の `sofel_render_glyph` と同じとする。

`sofel_face_shape` は、sfnt では通常のシェーピングと指定したフィーチャを使う。
レイアウト表を持たない形式は、既存の fallback シェーパと同じ文字順、方向、クラスタの扱いに従って文字対応と送り幅を使う。
`kern` の既定は有効とし、フィーチャ指定で無効にできる。
横組みで `kern` が有効なら形式別の横ペアカーニングを反映する。
縦組みのフォールバックは既存のシェーパの規則に従い、横ペアカーニングを適用しない。
セグメントとバッファの規則は既存のシェーピング入口に従い、位置の単位を 26.6 ピクセルに揃える。
AFM と PFM の付加はこの入口にも反映する。

## 実装しないこと

この ADR では、既存の `Font` と `sofel_font_t` を旧形式を所有する型へ変更しない。
既存の可変フォントの座標指定、ヒンティングのプロパティ、色グリフの API を置き換えない。
共通フェイスの輪郭とメトリクスは既定の変化位置を使い、追加の変化座標やヒンティング設定の指定はこの API に含めない。

## 不変条件

- I1: 同じ入力、フェイス番号、ppem、ストライクで、共通操作と対応する形式別操作は、指定した 26.6 ピクセルへの換算後に同じ結果を返す。
- I2: 同じ入力と読み込み条件で、送り幅、ペアカーニング、輪郭、固定画像は root [ADR 0065](0065-legacy-faces-and-webfonts.md) の FreeType 参照結果と一致する。
- I3: メトリクスを付加したハンドルは元のハンドルを変更せず、元のハンドルと呼び出し側の入力バッファを解放した後も同じ結果を返す。
- I4: 範囲外のフェイス、グリフ、ストライク、ゼロの ppem、ゼロの UPM、換算のオーバーフローは失敗し、別のフェイスやサイズの結果を返さない。
- I5: レイアウト表を持たない形式では、同じ付加メトリクスと `kern` 設定で、通常のシェーピング入口の送りに、共通ペア問い合わせと同じカーニングが反映される。
- I6: C ABI と Rust の共通操作は同じ結果と成否を返し、C の画像バッファ不足時は必要長を返してバッファを書き換えない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 各形式のフェイス0と最後のフェイス | 形式別操作と参照結果に一致 | I1, I2 |
| フェイス数と同じ番号、グリフ数と同じ番号 | 失敗 | I4 |
| 26.6 ppem が510のストライクに ppem 8 | ストライクの元の画像と送りを返す | I1, I2 |
| ppem 0、UPM 0、存在しない固定サイズ、範囲外ストライク | 失敗 | I4 |
| ppem 1、最大の u16、換算が i32 に収まる値とあふれる値 | 範囲内は一致、あふれは失敗 | I1, I4 |
| AFM/PFM の登録ペアと未登録ペア、kern の有効と無効 | 付加と設定に従って一致 | I2, I5 |
| 元のフェイスと入力を解放した付加済みフェイス | 結果を維持 | I3 |
| C の画像容量0、必要長ちょうど、1バイト不足 | 必要長を返し、容量不足では書き込まない | I6 |

## 受けいれ条件

- A1: root [ADR 0090](0090-legacy-format-conformance-fonts.md) の実フォントと不足形式の合成入力について、Rust、C、CLI の共通入口が不変条件を満たす。
- A2: 既存の sfnt 用 API とそのテストが成功し、C のヘッダ、シンボル、公開 Rust API の対応検査に不足がない。

## テスト設計

`adr0100_i1_i2_common_face_queries` を実装より先に書き、単位の異なる輪郭と固定画像、全フェイス番号、全収録グリフを比較する。
共通入口がない状態、単位を取り違えた実装、カーニングを一律ゼロにした実装が失敗することを確かめる。
FreeType 参照のフラグとサイズとストライクをテスト中に明記する。

`adr0100_i3_metrics_ownership` は、AFM と PFM の各入力、元のフェイス、付加後のフェイスを別々に解放し、残るハンドルから問い合わせる。
`adr0100_i4_bounds` は表の境界入力を使う。
`adr0100_i5_shape_metrics` は通常のシェーピング入口へ同じ文字列を渡し、付加前後と `kern` の有効無効を比較する。

C のテストは `adr0100_i6_c_api` で同じ入力を使い、バッファの前後と不足時の全バイトを番兵で確認する。
既存の native と wasm の C ABI 検査、形式別テスト、通常のシェーピングテストを実行する。
仕様書と README に共通操作と単位を書き、root [ADR 0065](0065-legacy-faces-and-webfonts.md) の完了条件を縮めずに残件をロードマップへ残す。

## トレードオフ

共通フェイスを別の型にすると、既存の sfnt 利用者は移行なしで使えるが、二つの入口を保守する必要がある。
26.6 ピクセルに揃えると、呼び出し側で形式ごとに単位を分岐せずに済むが、共通メトリクスの問い合わせにサイズ指定が必要になる。
付加メトリクスを新しいハンドルへ適用すると、読み取り専用の共有を保てるが、入力や解析結果の所有を引き継ぐ処理が要る。

## 参考文献

- [root ADR 0065](0065-legacy-faces-and-webfonts.md)
- [root ADR 0015](0015-c-abi.md)
- [root ADR 0090](0090-legacy-format-conformance-fonts.md)
