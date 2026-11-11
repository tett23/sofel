# 可変フォントの位置を作る時間

[sofel ADR 0027](../adr/0027-variable-fonts.md) の3フォントで、同じ読み込み済みフェイスに `wght=500` を繰り返し指定した。計測の前に、sofelのHarfBuzz用2.14座標とFreeType用16.16座標を、それぞれ参照ライブラリが返した全軸の値と照合した。

sofelは毎回 `Location::new` で新しい位置と係数キャッシュを構築して解放する。FreeTypeは用意した座標を同じフェイスに `FT_Set_Var_Design_Coordinates` で指定し、HarfBuzzも同じフォントに `hb_font_set_variations` で指定する。両ライブラリの時間には、すでに同じ座標が設定されている場合の処理が含まれる。0.0 µsは小数1桁への丸めである。

macOS arm64上でreleaseビルドの `sofel-bench --filter 'location wght=500'` を実行した。wasm32-wasip1はWasmtime 50.0.1で実行した。表は標準ハーネスが出力した中央値と中央絶対偏差である。

## Native

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location wght=500 | RobotoFlex-Variable | 10.1 ±0.8 | 0.0 ±0.0 | 504.66 | 0.3 ±0.0 | 34.19 |
| location wght=500 | SourceSerifVariable-Roman | 0.9 ±0.0 | 0.0 ±0.0 | 82.10 | 0.1 ±0.0 | 7.41 |
| location wght=500 | AdobeVFPrototype | 22.5 ±0.1 | 0.0 ±0.0 | 1728.29 | 0.1 ±0.0 | 182.00 |

## wasm32-wasip1

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location wght=500 | RobotoFlex-Variable | 12.5 ±0.1 | 0.0 ±0.0 | 296.82 | 0.3 ±0.0 | 37.97 |
| location wght=500 | SourceSerifVariable-Roman | 1.2 ±0.0 | 0.0 ±0.0 | 56.06 | 0.1 ±0.0 | 9.48 |
| location wght=500 | AdobeVFPrototype | 29.0 ±1.7 | 0.0 ±0.0 | 981.64 | 0.1 ±0.0 | 217.46 |
