# 可変フォントの全グリフの箱、輪郭、ラスタライズ

[sofel ADR 0027](../adr/0027-variable-fonts.md) の3フォントで、位置を `wght=500` に設定してから全グリフを処理した。macOS arm64のreleaseビルドとWasmtime 50.0.1のwasm32-wasip1ビルドで `sofel-bench --filter wght=500` を実行した。表は1フォントの全グリフを処理する時間の中央値と中央絶対偏差である。

箱はHarfBuzzと4辺の値を足した結果を、輪郭はFreeTypeとcloseを除いたコマンド数を、ラスタライズはFreeTypeと画素濃度の合計を計測前に照合した。HarfBuzzの輪郭はコマンドへの分解方法が異なり、時間のみを比較した。別の適合テストで、3フォントの既定位置、全名前付きインスタンス、各軸の両端、乱数8位置で非拡大・16px・48pxの全輪郭をFreeTypeと座標単位で照合した。既定位置、全名前付きインスタンス、最初の乱数2位置では12px・24px・48pxの全画素も照合した。合計254,187輪郭と119,541ビットマップが一致した。

## Native

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| box wght=500 | RobotoFlex-Variable | 28427.1 ±174.6 | — | — | 1349.0 ±3.1 | 21.07 |
| outline unscaled wght=500 | RobotoFlex-Variable | 24398.8 ±111.5 | 918.1 ±4.7 | 26.57 | 789.6 ±8.5 | 30.90 |
| outline 48px wght=500 | RobotoFlex-Variable | 24384.3 ±162.9 | 984.1 ±2.7 | 24.78 | 799.3 ±9.2 | 30.51 |
| raster 24px wght=500 | RobotoFlex-Variable | 30473.9 ±113.8 | 7452.0 ±65.2 | 4.09 | — | — |
| box wght=500 | SourceSerifVariable-Roman | 2676.0 ±6.0 | — | — | 827.5 ±3.5 | 3.23 |
| outline unscaled wght=500 | SourceSerifVariable-Roman | 2270.7 ±8.5 | 1345.6 ±2.0 | 1.69 | 930.5 ±2.0 | 2.44 |
| outline 48px wght=500 | SourceSerifVariable-Roman | 2416.7 ±15.6 | 1519.7 ±21.6 | 1.59 | 932.8 ±2.1 | 2.59 |
| raster 24px wght=500 | SourceSerifVariable-Roman | 4809.1 ±23.5 | 4197.5 ±22.1 | 1.15 | — | — |
| box wght=500 | AdobeVFPrototype | 1150.3 ±6.7 | — | — | 1311.5 ±9.7 | 0.88 |
| outline unscaled wght=500 | AdobeVFPrototype | 3513.5 ±26.4 | 1248.3 ±11.3 | 2.81 | 1289.3 ±6.2 | 2.73 |
| outline 48px wght=500 | AdobeVFPrototype | 3513.2 ±9.5 | 1277.8 ±6.6 | 2.75 | 1283.5 ±1.4 | 2.74 |
| raster 24px wght=500 | AdobeVFPrototype | 4051.8 ±15.1 | 1874.2 ±6.9 | 2.16 | — | — |

## wasm32-wasip1

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| box wght=500 | RobotoFlex-Variable | 38732.8 ±138.3 | — | — | 2046.7 ±8.4 | 18.92 |
| outline unscaled wght=500 | RobotoFlex-Variable | 33565.2 ±172.6 | 1515.6 ±2.6 | 22.15 | 1514.5 ±8.0 | 22.16 |
| outline 48px wght=500 | RobotoFlex-Variable | 33503.8 ±70.9 | 1612.9 ±1.7 | 20.77 | 1533.3 ±22.4 | 21.85 |
| raster 24px wght=500 | RobotoFlex-Variable | 42270.4 ±355.0 | 9345.1 ±9.2 | 4.52 | — | — |
| box wght=500 | SourceSerifVariable-Roman | 3358.1 ±14.6 | — | — | 1150.6 ±2.6 | 2.92 |
| outline unscaled wght=500 | SourceSerifVariable-Roman | 3661.0 ±30.1 | 2065.8 ±10.3 | 1.77 | 1608.3 ±14.1 | 2.28 |
| outline 48px wght=500 | SourceSerifVariable-Roman | 3827.6 ±26.7 | 2288.6 ±15.5 | 1.67 | 1611.4 ±11.5 | 2.38 |
| raster 24px wght=500 | SourceSerifVariable-Roman | 7395.4 ±41.1 | 5931.4 ±56.2 | 1.25 | — | — |
| box wght=500 | AdobeVFPrototype | 1470.0 ±11.6 | — | — | 1873.0 ±9.1 | 0.78 |
| outline unscaled wght=500 | AdobeVFPrototype | 5171.3 ±42.8 | 1810.1 ±8.6 | 2.86 | 1846.7 ±11.4 | 2.80 |
| outline 48px wght=500 | AdobeVFPrototype | 5305.2 ±64.1 | 1842.5 ±11.9 | 2.88 | 1882.3 ±26.5 | 2.82 |
| raster 24px wght=500 | AdobeVFPrototype | 5983.5 ±44.0 | 2599.3 ±36.8 | 2.30 | — | — |
