# 可変フォントの全グリフの水平アドバンスを取得する時間

[sofel ADR 0027](../adr/0027-variable-fonts.md) に従い、位置を wght=500 に設定して、各フォントの全グリフの水平アドバンスを取得した。計測前に3フォント計2691グリフでFreeTypeおよびHarfBuzzの結果と照合した。

macOS arm64でreleaseビルドの `sofel-bench --filter "advance wght=500"` を実行した。wasm32-wasip1はWasmtime 50.0.1で実行した。表は1フォントの全グリフを処理する時間の中央値と中央絶対偏差を示す。

## Native

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| advance wght=500 | RobotoFlex-Variable | 153.7 ±2.9 | 96.6 ±17.0 | 1.59 | 121.3 ±3.8 | 1.27 |
| advance wght=500 | SourceSerifVariable-Roman | 114.6 ±6.9 | 46.3 ±0.2 | 2.48 | 89.1 ±2.6 | 1.29 |
| advance wght=500 | AdobeVFPrototype | 30.1 ±0.2 | 11.4 ±0.1 | 2.64 | 15.9 ±0.3 | 1.89 |

## wasm32-wasip1

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| advance wght=500 | RobotoFlex-Variable | 306.2 ±6.7 | 209.3 ±13.7 | 1.46 | 269.4 ±52.6 | 1.14 |
| advance wght=500 | SourceSerifVariable-Roman | 236.7 ±5.5 | 163.7 ±0.7 | 1.45 | 170.5 ±1.7 | 1.39 |
| advance wght=500 | AdobeVFPrototype | 59.3 ±3.7 | 39.7 ±2.3 | 1.49 | 26.3 ±0.4 | 2.25 |
