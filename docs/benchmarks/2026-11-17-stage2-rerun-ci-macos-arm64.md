# stage2-rerun の計測（CI、macos-arm64、2026-11-17）

[ADR 0080](../adr/0080-ci-benchmark-records.md) のワークフローで取った記録である。
時間は1回あたりの中央値（±中央絶対偏差）、単位はマイクロ秒で、比が1未満なら sofel が速い。

## 環境

| 項目 | 値 |
| --- | --- |
| ランナーのイメージと版 | macos26 20261102.0473.1 |
| OS の名前と版 | macOS 26.7.2 |
| CPU の名前 | Apple M1 (Virtual) |
| 論理 CPU の数 | 3 |
| メモリ | 7516192768 bytes |
| 計測の直前の負荷平均 | 12:10  up 7 mins, 1 user, load averages: 41.56 35.98 21.34 |
| Rust の版 | rustc 1.99.1 (58cf8f9c3 2026-10-13) |
| C と C++ のコンパイラの版 | cc: Apple clang version 21.0.0 (clang-2100.1.1.101); c++: Apple clang version 21.0.0 (clang-2100.1.1.101) |
| wasmtime の版 | wasmtime 50.0.2 (f9a93b049 2026-11-16) |
| wasi-sdk の版 | wasi-sdk-34 |
| FreeType と HarfBuzz の版 | harfbuzz 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372, freetype 37515cf56ba3349656049edbed89ffc6f6466f2d |
| sofel のコミット | 72512b48adbac8367592d5cddab095d63f89e706 |
| 実行の URL | https://github.com/tett23/sofel/actions/runs/44070967915 |

## ネイティブ

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location wght=500 | RobotoFlex-Variable | 25.8 ±5.0 | 0.0 ±0.0 | 1792.58 | 0.2 ±0.0 | 115.96 |
| box wght=500 | RobotoFlex-Variable | 6767.6 ±979.7 | — | — | 3064.1 ±723.3 | 2.21 |
| location with reference queries wght=499/500 | RobotoFlex-Variable | 22.4 ±4.7 | 2.1 ±0.4 | 10.66 | 3.6 ±0.9 | 6.24 |
| advance wght=500 | RobotoFlex-Variable | 101.0 ±6.5 | 69.3 ±15.3 | 1.46 | 90.7 ±10.2 | 1.11 |
| outline unscaled wght=500 | RobotoFlex-Variable | 3808.2 ±345.2 | 1133.5 ±77.3 | 3.36 | 990.9 ±93.0 | 3.84 |
| outline 48px wght=500 | RobotoFlex-Variable | 4443.9 ±271.6 | 1408.1 ±138.2 | 3.16 | 1117.1 ±92.8 | 3.98 |
| raster 24px wght=500 | RobotoFlex-Variable | 13065.6 ±1098.4 | 9827.4 ±637.7 | 1.33 | — | — |
| location wght=500 | SourceSerifVariable-Roman | 1.2 ±0.2 | 0.0 ±0.0 | 140.34 | 0.1 ±0.0 | 14.43 |
| box wght=500 | SourceSerifVariable-Roman | 4282.7 ±634.6 | — | — | 1351.0 ±129.2 | 3.17 |
| location with reference queries wght=499/500 | SourceSerifVariable-Roman | 1.2 ±0.2 | 0.8 ±0.2 | 1.54 | 1.4 ±0.2 | 0.88 |
| advance wght=500 | SourceSerifVariable-Roman | 80.8 ±9.5 | 38.2 ±4.6 | 2.12 | 63.5 ±6.6 | 1.27 |
| outline unscaled wght=500 | SourceSerifVariable-Roman | 3774.0 ±789.3 | 2121.2 ±317.2 | 1.78 | 1572.6 ±265.1 | 2.40 |
| outline 48px wght=500 | SourceSerifVariable-Roman | 4004.7 ±542.7 | 2524.4 ±452.8 | 1.59 | 1558.2 ±240.5 | 2.57 |
| raster 24px wght=500 | SourceSerifVariable-Roman | 7594.6 ±1342.0 | 6918.8 ±874.1 | 1.10 | — | — |
| location wght=500 | AdobeVFPrototype | 19.4 ±3.3 | 0.0 ±0.0 | 1808.65 | 0.1 ±0.0 | 184.69 |
| box wght=500 | AdobeVFPrototype | 1920.1 ±309.9 | — | — | 2255.3 ±232.3 | 0.85 |
| location with reference queries wght=499/500 | AdobeVFPrototype | 26.0 ±2.3 | 8.5 ±2.0 | 3.05 | 7.4 ±0.8 | 3.49 |
| advance wght=500 | AdobeVFPrototype | 23.8 ±3.8 | 11.0 ±2.4 | 2.17 | 11.7 ±1.0 | 2.02 |
| outline unscaled wght=500 | AdobeVFPrototype | 5888.4 ±1266.2 | 2637.8 ±483.1 | 2.23 | 1981.6 ±301.9 | 2.97 |
| outline 48px wght=500 | AdobeVFPrototype | 6187.7 ±724.6 | 2287.2 ±317.0 | 2.71 | 2250.4 ±480.8 | 2.75 |
| raster 24px wght=500 | AdobeVFPrototype | 6878.3 ±990.7 | 3566.7 ±1016.9 | 1.93 | — | — |
| Unicode properties (1682970 characters) | perf/texts | 21199.8 ±3091.0 | — | — | 21305.1 ±4098.2 | 1.00 |
| cmap | extra/arabic | 0.2 ±0.0 | 0.2 ±0.0 | 1.05 | 0.2 ±0.0 | 0.91 |
| advance | extra/arabic | 0.2 ±0.0 | 2.5 ±0.3 | 0.07 | 1.5 ±0.3 | 0.11 |
| outline unscaled sampled | extra/arabic | 106.4 ±31.3 | 64.6 ±10.3 | 1.65 | — | — |
| cmap | extra/devanagari | 0.2 ±0.0 | 0.2 ±0.0 | 1.03 | 0.2 ±0.1 | 0.84 |
| advance | extra/devanagari | 0.2 ±0.0 | 2.8 ±0.7 | 0.06 | 1.9 ±0.3 | 0.09 |
| outline unscaled sampled | extra/devanagari | 101.8 ±17.8 | 88.5 ±26.8 | 1.15 | — | — |
| cmap | extra/khmer | 0.2 ±0.0 | 0.2 ±0.1 | 0.89 | 0.2 ±0.0 | 1.09 |
| advance | extra/khmer | 0.2 ±0.0 | 2.8 ±0.5 | 0.06 | 1.7 ±0.2 | 0.09 |
| outline unscaled sampled | extra/khmer | 109.1 ±27.4 | 93.3 ±12.7 | 1.17 | — | — |
| cmap | extra/myanmar | 0.2 ±0.0 | 0.2 ±0.0 | 1.02 | 0.2 ±0.0 | 0.92 |
| advance | extra/myanmar | 0.2 ±0.0 | 3.0 ±0.6 | 0.05 | 1.5 ±0.2 | 0.11 |
| outline unscaled sampled | extra/myanmar | 99.8 ±17.4 | 85.4 ±8.7 | 1.17 | — | — |
| cmap | extra/balinese | 0.2 ±0.0 | 0.2 ±0.0 | 1.13 | 0.2 ±0.0 | 1.12 |
| advance | extra/balinese | 0.2 ±0.0 | 2.5 ±0.5 | 0.06 | 1.4 ±0.1 | 0.11 |
| outline unscaled sampled | extra/balinese | 120.9 ±22.5 | 102.6 ±21.1 | 1.18 | — | — |
| cmap | extra/hebrew | 0.2 ±0.0 | 0.1 ±0.0 | 1.34 | 0.2 ±0.0 | 0.91 |
| advance | extra/hebrew | 0.2 ±0.0 | 2.8 ±0.7 | 0.06 | 1.5 ±0.2 | 0.10 |
| outline unscaled sampled | extra/hebrew | 50.3 ±7.1 | 41.3 ±6.3 | 1.22 | — | — |
| cmap | extra/source-serif | 0.2 ±0.0 | 0.2 ±0.0 | 0.99 | 0.2 ±0.0 | 1.09 |
| advance | extra/source-serif | 0.1 ±0.0 | 2.1 ±0.5 | 0.07 | 1.7 ±0.2 | 0.08 |
| outline unscaled sampled | extra/source-serif | 2496.8 ±72.3 | 974.4 ±75.4 | 2.56 | — | — |
| cmap | extra/stix-math | 0.2 ±0.0 | 0.1 ±0.0 | 1.17 | 0.2 ±0.0 | 1.15 |
| advance | extra/stix-math | 0.1 ±0.0 | 1.8 ±0.2 | 0.08 | 1.3 ±0.1 | 0.11 |
| outline unscaled sampled | extra/stix-math | 351.5 ±32.5 | 387.9 ±27.6 | 0.91 | — | — |
| MATH constants | extra/stix-math | 0.1 ±0.0 | — | — | 0.6 ±0.0 | 0.16 |
| cmap | extra/source-han | 0.2 ±0.0 | 0.2 ±0.0 | 0.87 | 0.2 ±0.0 | 0.94 |
| advance | extra/source-han | 0.1 ±0.0 | 1.8 ±0.1 | 0.06 | 1.3 ±0.1 | 0.08 |
| outline unscaled sampled | extra/source-han | 697.4 ±161.8 | 244.9 ±18.6 | 2.85 | — | — |
| vertical advance | extra/source-han | 0.5 ±0.0 | — | — | 1.3 ±0.1 | 0.42 |
| cmap | extra/amstelvar | 0.1 ±0.0 | 0.1 ±0.0 | 0.86 | 0.2 ±0.0 | 0.74 |
| advance | extra/amstelvar | 0.1 ±0.0 | 1.6 ±0.1 | 0.09 | 1.3 ±0.1 | 0.11 |
| outline unscaled sampled | extra/amstelvar | 68.3 ±3.3 | 73.6 ±9.1 | 0.93 | — | — |
| cmap | extra/color-cff | 0.1 ±0.0 | 0.1 ±0.0 | 1.29 | 0.1 ±0.0 | 0.91 |
| advance | extra/color-cff | 0.0 ±0.0 | 0.4 ±0.0 | 0.06 | 0.3 ±0.0 | 0.09 |
| outline unscaled sampled | extra/color-cff | 18.3 ±0.7 | 18.3 ±1.1 | 1.00 | — | — |
| cmap | extra/color-sbix | 0.1 ±0.0 | 0.1 ±0.0 | 1.37 | 0.1 ±0.0 | 0.97 |
| advance | extra/color-sbix | 0.0 ±0.0 | 0.5 ±0.1 | 0.05 | 0.3 ±0.0 | 0.10 |
| outline unscaled sampled | extra/color-sbix | 2.0 ±0.4 | 1.8 ±0.5 | 1.07 | — | — |
| cmap | extra/color-svg | 0.1 ±0.0 | 0.1 ±0.0 | 1.45 | 0.1 ±0.0 | 1.07 |
| advance | extra/color-svg | 0.0 ±0.0 | 0.5 ±0.1 | 0.05 | 0.3 ±0.0 | 0.09 |
| outline unscaled sampled | extra/color-svg | 1.5 ±0.0 | 1.5 ±0.1 | 0.99 | — | — |
| outline unscaled | SourceSansPro-Regular | 5425.9 ±372.2 | 5869.9 ±1125.8 | 0.92 | 2304.7 ±300.1 | 2.35 |
| outline 48px | SourceSansPro-Regular | 5394.0 ±303.4 | 5378.4 ±419.2 | 1.00 | 1830.1 ±44.7 | 2.95 |
| raster 12px | SourceSansPro-Regular | 8619.2 ±1002.7 | 6672.0 ±620.3 | 1.29 | — | — |
| raster 24px | SourceSansPro-Regular | 11291.7 ±2084.6 | 11042.5 ±1413.5 | 1.02 | — | — |
| raster 48px | SourceSansPro-Regular | 13321.0 ±2427.3 | 13063.7 ±2188.1 | 1.02 | — | — |
| raster 96px | SourceSansPro-Regular | 19918.0 ±3676.9 | 20319.0 ±3734.3 | 0.98 | — | — |
| open (FreeType) | SourceSansPro-Regular | 9.5 ±1.0 | 27.9 ±3.3 | 0.34 | — | — |
| glyph extents | SourceSansPro-Regular | 2267.3 ±222.8 | — | — | 2799.2 ±347.5 | 0.81 |
| open (HarfBuzz) | SourceSansPro-Regular | 1.6 ±0.2 | — | — | 2.7 ±0.3 | 0.61 |
| glyph names | SourceSansPro-Regular | 35.0 ±4.6 | — | — | 353.4 ±70.4 | 0.10 |
| glyph from name | SourceSansPro-Regular | 1364.6 ±164.1 | — | — | 582.1 ±97.7 | 2.34 |
| outline unscaled | NotoSerifMyanmar-Regular | 2870.2 ±400.5 | 2323.1 ±410.0 | 1.24 | 1305.8 ±198.6 | 2.20 |
| outline 48px | NotoSerifMyanmar-Regular | 3149.6 ±508.1 | 2041.5 ±157.5 | 1.54 | 1089.0 ±107.5 | 2.89 |
| raster 12px | NotoSerifMyanmar-Regular | 4022.5 ±197.3 | 3570.0 ±360.4 | 1.13 | — | — |
| raster 24px | NotoSerifMyanmar-Regular | 5989.7 ±979.0 | 4685.0 ±462.9 | 1.28 | — | — |
| raster 48px | NotoSerifMyanmar-Regular | 9386.8 ±1736.8 | 8430.3 ±1244.4 | 1.11 | — | — |
| raster 96px | NotoSerifMyanmar-Regular | 15092.4 ±2438.0 | 12440.9 ±1536.9 | 1.21 | — | — |
| open (FreeType) | NotoSerifMyanmar-Regular | 7.0 ±1.6 | 15.3 ±2.3 | 0.46 | — | — |
| glyph extents | NotoSerifMyanmar-Regular | 1089.1 ±121.1 | — | — | 1351.0 ±206.6 | 0.81 |
| open (HarfBuzz) | NotoSerifMyanmar-Regular | 1.1 ±0.2 | — | — | 2.5 ±0.4 | 0.42 |
| glyph names | NotoSerifMyanmar-Regular | 16.1 ±2.8 | — | — | 27.7 ±2.8 | 0.58 |
| glyph from name | NotoSerifMyanmar-Regular | 214.1 ±11.0 | — | — | 84.0 ±8.3 | 2.55 |
| outline unscaled | AdobeVFPrototype | 5161.5 ±793.1 | 1639.2 ±162.1 | 3.15 | 1739.8 ±231.2 | 2.97 |
| outline 48px | AdobeVFPrototype | 5099.3 ±765.0 | 1697.0 ±249.2 | 3.00 | 1828.0 ±224.5 | 2.79 |
| raster 12px | AdobeVFPrototype | 5439.4 ±483.8 | 2987.2 ±355.0 | 1.82 | — | — |
| raster 24px | AdobeVFPrototype | 5899.5 ±698.8 | 2546.6 ±350.8 | 2.32 | — | — |
| raster 48px | AdobeVFPrototype | 8612.6 ±1336.6 | 4226.9 ±713.6 | 2.04 | — | — |
| raster 96px | AdobeVFPrototype | 8031.4 ±1170.8 | 5620.6 ±470.5 | 1.43 | — | — |
| open (FreeType) | AdobeVFPrototype | 8.2 ±1.1 | 12.4 ±1.5 | 0.66 | — | — |
| glyph extents | AdobeVFPrototype | 1312.5 ±144.2 | — | — | 1665.2 ±136.6 | 0.79 |
| open (HarfBuzz) | AdobeVFPrototype | 1.7 ±0.0 | — | — | 2.9 ±0.2 | 0.59 |
| outline unscaled | SourceHanSans-Regular_subset | 68555.1 ±2169.3 | 62758.6 ±6106.0 | 1.09 | 26329.2 ±2163.8 | 2.60 |
| outline 48px | SourceHanSans-Regular_subset | 66960.5 ±4507.5 | 56973.0 ±1776.7 | 1.18 | 31504.5 ±5937.7 | 2.13 |
| raster 12px | SourceHanSans-Regular_subset | 88206.6 ±2111.1 | 89484.7 ±3219.9 | 0.99 | — | — |
| raster 24px | SourceHanSans-Regular_subset | 112983.7 ±5426.6 | 107701.6 ±2335.2 | 1.05 | — | — |
| raster 48px | SourceHanSans-Regular_subset | 186582.0 ±14508.7 | 202328.5 ±13080.2 | 0.92 | — | — |
| raster 96px | SourceHanSans-Regular_subset | 314317.4 ±25138.6 | 304976.0 ±13105.5 | 1.03 | — | — |
| cid | SourceHanSans-Regular_subset | 166.4 ±14.3 | 314.4 ±40.1 | 0.53 | — | — |
| cid (no index) | SourceHanSans-Regular_subset | 226444.9 ±12610.1 | 284.3 ±11.1 | 796.63 | — | — |
| open (FreeType) | SourceHanSans-Regular_subset | 69.1 ±3.0 | 111.9 ±10.9 | 0.62 | — | — |
| glyph extents | SourceHanSans-Regular_subset | 24644.8 ±471.7 | — | — | 23318.8 ±1384.1 | 1.06 |
| open (HarfBuzz) | SourceHanSans-Regular_subset | 8.2 ±0.1 | — | — | 7.8 ±0.6 | 1.05 |
| glyph color format | extra/color-cff | 1.5 ±0.0 | — | — | 0.4 ±0.0 | 3.91 |
| COLRv1 paint | extra/color-cff | 3.8 ±0.2 | 0.8 ±0.0 | 4.64 | 2.0 ±0.2 | 1.89 |
| COLRv1 recording | extra/color-cff | 4.3 ±0.3 | — | — | — | — |
| color glyph extents | extra/color-cff | 13.0 ±0.7 | — | — | 3.2 ±0.2 | 4.07 |
| glyph color format | extra/color-sbix | 1.1 ±0.0 | — | — | 0.2 ±0.0 | 5.40 |
| bitmap data 16 ppem | extra/color-sbix | 5.0 ±1.0 | — | — | 5.0 ±0.9 | 1.00 |
| bitmap data 109 ppem | extra/color-sbix | 3.7 ±0.4 | — | — | 5.7 ±1.0 | 0.64 |
| bitmap data 136 ppem | extra/color-sbix | 3.8 ±0.4 | — | — | 5.0 ±1.1 | 0.76 |
| bitmap data 0 ppem | extra/color-sbix | 3.9 ±0.8 | — | — | 4.2 ±0.7 | 0.93 |
| color glyph extents | extra/color-sbix | 2.1 ±0.3 | — | — | 2.2 ±0.3 | 0.96 |
| glyph color format | extra/color-svg | 1.1 ±0.1 | — | — | 0.3 ±0.0 | 4.36 |
| SVG documents | extra/color-svg | 0.5 ±0.1 | — | — | 0.6 ±0.1 | 0.87 |
| color glyph extents | extra/color-svg | 1.9 ±0.2 | — | — | 1.2 ±0.2 | 1.59 |
| glyph color format | TwemojiMozilla.subset | 0.4 ±0.0 | — | — | 0.1 ±0.0 | 5.02 |
| COLRv0 layers | TwemojiMozilla.subset | 0.1 ±0.0 | 0.1 ±0.0 | 2.69 | 0.1 ±0.0 | 1.35 |
| COLRv0 raster 12px | TwemojiMozilla.subset | 21.6 ±2.2 | 15.6 ±2.7 | 1.39 | — | — |
| COLRv0 raster 24px | TwemojiMozilla.subset | 52.0 ±11.5 | 32.9 ±5.8 | 1.58 | — | — |
| COLRv0 raster 48px | TwemojiMozilla.subset | 68.9 ±15.3 | 60.0 ±15.2 | 1.15 | — | — |
| COLRv0 paint | TwemojiMozilla.subset | 0.7 ±0.1 | — | — | 0.3 ±0.0 | 2.79 |
| color glyph extents | TwemojiMozilla.subset | 9.7 ±1.4 | — | — | 4.2 ±0.6 | 2.30 |
| glyph color format | test_glyphs-glyf_colr_1 | 12.2 ±1.2 | — | — | 6.0 ±1.2 | 2.02 |
| COLRv0 layers | test_glyphs-glyf_colr_1 | 6.0 ±1.8 | 1.0 ±0.2 | 6.07 | 4.3 ±1.0 | 1.38 |
| COLRv0 raster 12px | test_glyphs-glyf_colr_1 | 122.7 ±25.4 | 135.3 ±18.6 | 0.91 | — | — |
| COLRv0 raster 24px | test_glyphs-glyf_colr_1 | 193.6 ±32.7 | 234.3 ±39.2 | 0.83 | — | — |
| COLRv0 raster 48px | test_glyphs-glyf_colr_1 | 305.9 ±24.8 | 499.6 ±52.3 | 0.61 | — | — |
| COLRv1 paint | test_glyphs-glyf_colr_1 | 66.5 ±4.6 | 17.9 ±2.1 | 3.71 | 34.3 ±5.1 | 1.94 |
| COLRv1 recording | test_glyphs-glyf_colr_1 | 65.6 ±6.6 | — | — | — | — |
| color glyph extents | test_glyphs-glyf_colr_1 | 71.1 ±8.0 | — | — | 34.6 ±3.0 | 2.05 |
| glyph color format | noto_handwriting-glyf_colr_1 | 1.2 ±0.1 | — | — | 0.3 ±0.0 | 4.08 |
| COLRv1 paint | noto_handwriting-glyf_colr_1 | 5.8 ±0.2 | 2.4 ±0.3 | 2.47 | 3.5 ±0.4 | 1.66 |
| COLRv1 recording | noto_handwriting-glyf_colr_1 | 7.1 ±0.9 | — | — | — | — |
| color glyph extents | noto_handwriting-glyf_colr_1 | 9.6 ±0.7 | — | — | 1.3 ±0.1 | 7.41 |
| glyph color format | NotoColorEmoji.subset | 1.4 ±0.2 | — | — | 0.0 ±0.0 | 37.06 |
| bitmap data 16 ppem | NotoColorEmoji.subset | 2.3 ±0.5 | — | — | 0.9 ±0.1 | 2.71 |
| bitmap data 109 ppem | NotoColorEmoji.subset | 1.6 ±0.3 | — | — | 1.1 ±0.2 | 1.41 |
| bitmap data 136 ppem | NotoColorEmoji.subset | 1.3 ±0.1 | — | — | 1.0 ±0.1 | 1.28 |
| bitmap data 0 ppem | NotoColorEmoji.subset | 1.1 ±0.0 | — | — | 0.8 ±0.0 | 1.43 |
| color glyph extents | NotoColorEmoji.subset | 0.7 ±0.1 | — | — | 0.5 ±0.0 | 1.40 |
| glyph color format | noto_handwriting-sbix | 0.7 ±0.1 | — | — | 0.1 ±0.0 | 4.93 |
| bitmap data 16 ppem | noto_handwriting-sbix | 1.4 ±0.0 | — | — | 1.6 ±0.0 | 0.91 |
| bitmap data 109 ppem | noto_handwriting-sbix | 1.5 ±0.0 | — | — | 1.6 ±0.0 | 0.93 |
| bitmap data 136 ppem | noto_handwriting-sbix | 1.5 ±0.0 | — | — | 1.9 ±0.2 | 0.79 |
| bitmap data 0 ppem | noto_handwriting-sbix | 1.8 ±0.1 | — | — | 1.8 ±0.1 | 0.99 |
| color glyph extents | noto_handwriting-sbix | 0.8 ±0.0 | — | — | 0.9 ±0.0 | 0.98 |
| glyph color format | chromacheck-svg | 0.1 ±0.0 | — | — | 0.0 ±0.0 | 5.74 |
| SVG documents | chromacheck-svg | 0.1 ±0.0 | — | — | 0.1 ±0.0 | 0.97 |
| color glyph extents | chromacheck-svg | 0.3 ±0.0 | — | — | 0.1 ±0.0 | 2.80 |
| bitmap pixels 16 ppem | EBDT (built, 200 glyphs) | 84.8 ±3.8 | 81.3 ±16.1 | 1.04 | — | — |
| cmap | Roboto-Regular | 3669.0 ±317.0 | 3638.2 ±339.8 | 1.01 | 4759.4 ±145.7 | 0.77 |
| advance | Roboto-Regular | 3.0 ±0.3 | 40.7 ±5.4 | 0.07 | 41.2 ±3.3 | 0.07 |
| v advance | Roboto-Regular | 99.5 ±7.7 | — | — | 161.4 ±20.7 | 0.62 |
| v origin | Roboto-Regular | 541.2 ±95.8 | — | — | 374.9 ±34.0 | 1.44 |
| outline unscaled | Roboto-Regular | 1323.9 ±40.7 | 1320.1 ±60.2 | 1.00 | 1275.8 ±72.1 | 1.04 |
| outline 48px | Roboto-Regular | 1561.9 ±28.0 | 1910.4 ±65.4 | 0.82 | 1441.2 ±38.0 | 1.08 |
| raster 12px | Roboto-Regular | 4903.1 ±100.3 | 5030.9 ±103.8 | 0.97 | — | — |
| raster 24px | Roboto-Regular | 7660.4 ±171.7 | 8088.6 ±184.1 | 0.95 | — | — |
| raster 48px | Roboto-Regular | 11616.8 ±260.9 | 11833.4 ±710.5 | 0.98 | — | — |
| raster 96px | Roboto-Regular | 23687.5 ±2024.2 | 23938.4 ±1128.5 | 0.99 | — | — |
| cmap | Comfortaa-Regular-new | 3320.5 ±162.1 | 3366.5 ±148.4 | 0.99 | 4309.9 ±160.9 | 0.77 |
| advance | Comfortaa-Regular-new | 0.8 ±0.1 | 13.1 ±1.6 | 0.06 | 9.2 ±1.3 | 0.08 |
| v advance | Comfortaa-Regular-new | 23.4 ±1.2 | — | — | 27.3 ±3.5 | 0.86 |
| v origin | Comfortaa-Regular-new | 141.8 ±20.6 | — | — | 93.0 ±9.9 | 1.52 |
| outline unscaled | Comfortaa-Regular-new | 697.8 ±127.2 | 562.6 ±58.4 | 1.24 | 613.6 ±73.0 | 1.14 |
| outline 48px | Comfortaa-Regular-new | 728.4 ±81.5 | 984.2 ±188.7 | 0.74 | 718.0 ±74.5 | 1.01 |
| raster 12px | Comfortaa-Regular-new | 2212.0 ±294.7 | 2069.8 ±135.9 | 1.07 | — | — |
| raster 24px | Comfortaa-Regular-new | 2678.8 ±294.3 | 3544.7 ±514.6 | 0.76 | — | — |
| raster 48px | Comfortaa-Regular-new | 5179.0 ±625.7 | 4751.0 ±248.9 | 1.09 | — | — |
| raster 96px | Comfortaa-Regular-new | 8520.4 ±571.7 | 9411.4 ±593.5 | 0.91 | — | — |
| cmap | NotoNastaliqUrdu-Regular | 4623.3 ±601.6 | 3505.3 ±330.8 | 1.32 | 4739.5 ±461.1 | 0.98 |
| advance | NotoNastaliqUrdu-Regular | 1.0 ±0.1 | 16.2 ±2.4 | 0.06 | 12.6 ±1.5 | 0.08 |
| v advance | NotoNastaliqUrdu-Regular | 37.0 ±7.2 | — | — | 59.4 ±7.0 | 0.62 |
| v origin | NotoNastaliqUrdu-Regular | 214.8 ±32.9 | — | — | 184.3 ±37.1 | 1.17 |
| outline unscaled | NotoNastaliqUrdu-Regular | 1286.7 ±128.5 | 994.7 ±158.9 | 1.29 | 1128.1 ±106.3 | 1.14 |
| outline 48px | NotoNastaliqUrdu-Regular | 1295.7 ±158.1 | 1217.9 ±214.9 | 1.06 | 959.1 ±105.9 | 1.35 |
| raster 12px | NotoNastaliqUrdu-Regular | 4189.5 ±567.2 | 3423.5 ±347.5 | 1.22 | — | — |
| raster 24px | NotoNastaliqUrdu-Regular | 5473.5 ±754.9 | 5080.3 ±1035.6 | 1.08 | — | — |
| raster 48px | NotoNastaliqUrdu-Regular | 8977.2 ±1444.6 | 7695.1 ±1222.7 | 1.17 | — | — |
| raster 96px | NotoNastaliqUrdu-Regular | 13300.0 ±1401.4 | 12224.4 ±1496.0 | 1.09 | — | — |
| cmap | RobotoFlex-Variable | 5073.5 ±528.9 | 4368.5 ±637.1 | 1.16 | 5721.1 ±930.2 | 0.89 |
| advance | RobotoFlex-Variable | 0.9 ±0.1 | 14.2 ±2.1 | 0.07 | 10.3 ±0.8 | 0.09 |
| v advance | RobotoFlex-Variable | 31.5 ±4.0 | — | — | 34.6 ±5.6 | 0.91 |
| v origin | RobotoFlex-Variable | 142.1 ±14.5 | — | — | 97.8 ±12.0 | 1.45 |
| outline unscaled | RobotoFlex-Variable | 522.7 ±105.0 | 405.3 ±22.8 | 1.29 | 562.4 ±89.8 | 0.93 |
| outline 48px | RobotoFlex-Variable | 467.4 ±56.6 | 523.4 ±76.4 | 0.89 | 477.9 ±109.2 | 0.98 |
| raster 12px | RobotoFlex-Variable | 3597.1 ±109.3 | 3673.7 ±147.7 | 0.98 | — | — |
| raster 24px | RobotoFlex-Variable | 7929.5 ±603.3 | 8191.7 ±1239.9 | 0.97 | — | — |
| raster 48px | RobotoFlex-Variable | 17817.3 ±501.4 | 21114.1 ±1237.2 | 0.84 | — | — |
| raster 96px | RobotoFlex-Variable | 55336.5 ±3040.5 | 64815.0 ±4032.7 | 0.85 | — | — |
| cmap | SourceSerifVariable-Roman | 3653.5 ±167.3 | 3608.5 ±256.5 | 1.01 | 5214.6 ±416.4 | 0.70 |
| advance | SourceSerifVariable-Roman | 1.1 ±0.0 | 16.5 ±0.5 | 0.07 | 13.1 ±0.9 | 0.08 |
| v advance | SourceSerifVariable-Roman | 31.4 ±0.5 | — | — | 57.9 ±2.9 | 0.54 |
| v origin | SourceSerifVariable-Roman | 189.9 ±4.9 | — | — | 138.4 ±4.3 | 1.37 |
| outline unscaled | SourceSerifVariable-Roman | 908.6 ±21.8 | 831.6 ±22.1 | 1.09 | 823.9 ±33.6 | 1.10 |
| outline 48px | SourceSerifVariable-Roman | 904.8 ±29.8 | 1008.3 ±42.5 | 0.90 | 806.3 ±43.1 | 1.12 |
| raster 12px | SourceSerifVariable-Roman | 2640.0 ±112.5 | 3711.1 ±693.8 | 0.71 | — | — |
| raster 24px | SourceSerifVariable-Roman | 3480.8 ±218.5 | 4465.2 ±452.1 | 0.78 | — | — |
| raster 48px | SourceSerifVariable-Roman | 5625.8 ±279.1 | 6562.7 ±901.8 | 0.86 | — | — |
| raster 96px | SourceSerifVariable-Roman | 10291.0 ±403.1 | 11675.8 ±688.3 | 0.88 | — | — |
| HVAR item delta cached | RobotoFlex-Variable.ttf | 83.7 ±6.0 | — | — | — | — |
| HVAR item delta copied font | RobotoFlex-Variable.ttf | 565.2 ±38.1 | — | — | — | — |
| HVAR item delta different font | RobotoFlex-Variable.ttf | 586.9 ±26.5 | — | — | — | — |
| HVAR item delta cached | SourceSerifVariable-Roman.ttf | 43.9 ±2.3 | — | — | — | — |
| HVAR item delta copied font | SourceSerifVariable-Roman.ttf | 90.4 ±3.0 | — | — | — | — |
| HVAR item delta different font | SourceSerifVariable-Roman.ttf | 85.3 ±2.8 | — | — | — | — |
| MVAR item delta static default | SourceSerifVariable-Roman.ttf | 0.0 ±0.0 | — | — | — | — |
| HVAR item delta cached | AdobeVFPrototype.otf | 9.6 ±0.3 | — | — | — | — |
| HVAR item delta copied font | AdobeVFPrototype.otf | 20.3 ±0.4 | — | — | — | — |
| HVAR item delta different font | AdobeVFPrototype.otf | 22.6 ±1.0 | — | — | — | — |
| MVAR item delta static default | AdobeVFPrototype.otf | 0.1 ±0.0 | — | — | — | — |
| shaping en-thelittleprince.txt | Roboto-Regular | 34635.3 ±3932.8 | — | — | 5755.7 ±124.5 | 6.02 |
| first shaping en-thelittleprince.txt | Roboto-Regular | 55.7 ±0.9 | — | — | 64.3 ±5.8 | 0.87 |
| shaping en-words.txt | Roboto-Regular | 37010.9 ±984.5 | — | — | 9371.8 ±492.3 | 3.95 |
| first shaping en-words.txt | Roboto-Regular | 30.9 ±0.3 | — | — | 50.4 ±1.6 | 0.61 |
| shaping react-dom.txt | SourceSerifVariable-Roman | 240831.3 ±12769.4 | — | — | 77193.6 ±3898.4 | 3.12 |
| first shaping react-dom.txt | SourceSerifVariable-Roman | 15.9 ±0.8 | — | — | 16.1 ±1.2 | 0.98 |
| shaping hi-words.txt | NotoSansDevanagari-Regular | 42794.0 ±1529.4 | — | — | 23374.9 ±2212.3 | 1.83 |
| first shaping hi-words.txt | NotoSansDevanagari-Regular | 40.4 ±0.5 | — | — | 43.2 ±0.4 | 0.93 |
| shaping fa-thelittleprince.txt | Amiri-Regular | 76134.7 ±3128.7 | — | — | 44051.0 ±1451.0 | 1.73 |
| first shaping fa-thelittleprince.txt | Amiri-Regular | 102.6 ±1.9 | — | — | 40.4 ±0.4 | 2.54 |
| shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 259278.0 ±12238.3 | — | — | 69243.5 ±1354.3 | 3.74 |
| first shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 172.7 ±1.4 | — | — | 1896.3 ±26.1 | 0.09 |
| shaping fa-words.txt | NotoNastaliqUrdu-Regular | 285074.0 ±15351.7 | — | — | 75176.7 ±2196.5 | 3.79 |
| first shaping fa-words.txt | NotoNastaliqUrdu-Regular | 151.7 ±3.3 | — | — | 1823.6 ±29.1 | 0.08 |
| shaping fa-thelittleprince.txt | Gulzar-Regular | 5078881.6 ±275168.2 | — | — | 935525.5 ±33155.6 | 5.43 |
| first shaping fa-thelittleprince.txt | Gulzar-Regular | 1653.4 ±123.9 | — | — | 1520.7 ±66.7 | 1.09 |
| shaping fa-words.txt | Gulzar-Regular | 3572057.3 ±177131.7 | — | — | 685688.0 ±45317.5 | 5.21 |
| first shaping fa-words.txt | Gulzar-Regular | 934.7 ±6.6 | — | — | 1379.4 ±12.8 | 0.68 |
| shaping duployan.txt | NotoSansDuployan-Regular | 8102065.5 ±188678.2 | — | — | 1924552.9 ±50069.0 | 4.21 |
| first shaping duployan.txt | NotoSansDuployan-Regular | 25536.0 ±639.5 | — | — | 6699.6 ±485.0 | 3.81 |
| shaping ja-runs.txt | Mplus1p-Regular | 22477.9 ±754.7 | — | — | 13971.1 ±34.0 | 1.61 |
| first shaping ja-runs.txt | Mplus1p-Regular | 10.9 ±0.2 | — | — | 12.9 ±0.0 | 0.84 |
| shaping ja-runs.txt TTB | Mplus1p-Regular | 28231.4 ±354.3 | — | — | 19064.8 ±316.2 | 1.48 |
| first shaping ja-runs.txt TTB | Mplus1p-Regular | 11.2 ±0.2 | — | — | 8.4 ±0.2 | 1.33 |
| shaping role-default-und | extra/arabic | 1.5 ±0.0 | — | — | 1.0 ±0.0 | 1.48 |
| first shaping role-default-und | extra/arabic | 24.7 ±1.2 | — | — | 17.0 ±0.4 | 1.46 |
| shaping role-default-und | extra/devanagari | 2.2 ±0.1 | — | — | 1.2 ±0.0 | 1.90 |
| first shaping role-default-und | extra/devanagari | 86.6 ±2.1 | — | — | 931.2 ±60.9 | 0.09 |
| shaping role-default-und | extra/khmer | 1.0 ±0.0 | — | — | 0.7 ±0.0 | 1.49 |
| first shaping role-default-und | extra/khmer | 31.7 ±0.8 | — | — | 20.9 ±1.0 | 1.51 |
| shaping role-default-und | extra/myanmar | 2.6 ±0.0 | — | — | 1.1 ±0.0 | 2.42 |
| first shaping role-default-und | extra/myanmar | 82.7 ±1.1 | — | — | 44.2 ±0.1 | 1.87 |
| shaping role-default-und | extra/balinese | 9.7 ±0.5 | — | — | 5.1 ±0.1 | 1.90 |
| first shaping role-default-und | extra/balinese | 116.6 ±3.7 | — | — | 40.1 ±1.6 | 2.91 |
| shaping role-default-und | extra/hebrew | 1.5 ±0.0 | — | — | 0.8 ±0.0 | 1.90 |
| first shaping role-default-und | extra/hebrew | 9.6 ±0.5 | — | — | 9.4 ±0.4 | 1.03 |
| shaping role-default-und | extra/source-serif | 1.5 ±0.1 | — | — | 0.5 ±0.0 | 3.03 |
| first shaping role-default-und | extra/source-serif | 28.2 ±0.6 | — | — | 22.6 ±0.4 | 1.25 |
| shaping role-default-und TTB | extra/source-han | 0.3 ±0.0 | — | — | 0.3 ±0.0 | 1.30 |
| first shaping role-default-und TTB | extra/source-han | 71.8 ±1.6 | — | — | 22.2 ±0.1 | 3.23 |
| shaping role-default-und | extra/amstelvar | 0.6 ±0.0 | — | — | 0.4 ±0.0 | 1.56 |
| first shaping role-default-und | extra/amstelvar | 3.2 ±0.1 | — | — | 10.2 ±0.1 | 0.32 |
| shaping react-dom.txt wght=500 | SourceSerifVariable-Roman | 268143.3 ±11026.2 | — | — | 85732.7 ±4499.4 | 3.13 |
| subset (42 glyphs, retain-gids) | Roboto-Regular | 264.4 ±8.3 | — | — | 112.1 ±0.5 | 2.36 |
| subset (2322 characters) | Roboto-Regular | 379.6 ±7.8 | — | — | 122.0 ±3.5 | 3.11 |
| subset prepared (42 glyphs, retain-gids) | Roboto-Regular | 144.7 ±4.6 | — | — | 82.2 ±1.0 | 1.76 |
| subset prepared (2322 characters) | Roboto-Regular | 256.4 ±6.7 | — | — | 97.3 ±2.3 | 2.63 |
| preprocess | Roboto-Regular | 234.2 ±23.0 | — | — | 658.5 ±26.7 | 0.36 |
| subset (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 59667.2 ±1925.2 | — | — | 13558.0 ±492.8 | 4.40 |
| subset (2322 characters) | SourceHanSans-Regular_subset | 35146.4 ±94.9 | — | — | 12429.7 ±208.9 | 2.83 |
| subset prepared (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 47796.8 ±894.2 | — | — | 5248.8 ±22.3 | 9.11 |
| subset prepared (2322 characters) | SourceHanSans-Regular_subset | 23962.9 ±1414.1 | — | — | 6482.6 ±195.9 | 3.70 |
| preprocess | SourceHanSans-Regular_subset | 52358.0 ±5177.4 | — | — | 48884.5 ±4308.5 | 1.07 |
| subset (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 113.1 ±5.7 | — | — | 160.6 ±10.3 | 0.70 |
| subset (2322 characters) | NotoNastaliqUrdu-Bold | 207.7 ±7.1 | — | — | 156.2 ±7.6 | 1.33 |
| subset prepared (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 84.0 ±3.2 | — | — | 56.5 ±0.8 | 1.49 |
| subset prepared (2322 characters) | NotoNastaliqUrdu-Bold | 164.4 ±1.4 | — | — | 67.1 ±1.1 | 2.45 |
| preprocess | NotoNastaliqUrdu-Bold | 60.5 ±3.5 | — | — | 12555.4 ±77.8 | 0.00 |
| subset unicode range 10 | Roboto-Regular.ttf | 50.8 ±0.3 | — | — | 33.8 ±0.8 | 1.50 |
| subset unicode range 10, retain-gids | Roboto-Regular.ttf | 62.0 ±3.4 | — | — | 41.7 ±2.0 | 1.49 |
| subset unicode range 64 | Roboto-Regular.ttf | 172.3 ±8.9 | — | — | 105.1 ±4.2 | 1.64 |
| subset unicode range 64, retain-gids | Roboto-Regular.ttf | 169.1 ±8.2 | — | — | 97.6 ±5.2 | 1.73 |
| subset unicode range 512 | Roboto-Regular.ttf | 867.4 ±36.6 | — | — | 441.3 ±5.0 | 1.97 |
| subset unicode range 512, retain-gids | Roboto-Regular.ttf | 841.4 ±3.5 | — | — | 399.5 ±6.2 | 2.11 |
| subset unicode range 1000 | Roboto-Regular.ttf | 1244.3 ±33.3 | — | — | 546.2 ±3.1 | 2.28 |
| subset unicode range 1000, retain-gids | Roboto-Regular.ttf | 1276.9 ±32.2 | — | — | 569.0 ±16.1 | 2.24 |
| subset unicode range 10 | Amiri-Regular.ttf | 130.1 ±6.6 | — | — | 66.3 ±1.8 | 1.96 |
| subset unicode range 10, retain-gids | Amiri-Regular.ttf | 120.6 ±0.6 | — | — | 58.4 ±0.2 | 2.07 |
| subset unicode range 64 | Amiri-Regular.ttf | 265.1 ±5.0 | — | — | 124.8 ±3.4 | 2.12 |
| subset unicode range 64, retain-gids | Amiri-Regular.ttf | 371.8 ±12.3 | — | — | 178.7 ±3.0 | 2.08 |
| subset unicode range 512 | Amiri-Regular.ttf | 3870.0 ±59.2 | — | — | 3165.1 ±122.2 | 1.22 |
| subset unicode range 512, retain-gids | Amiri-Regular.ttf | 4099.8 ±84.6 | — | — | 2770.1 ±20.1 | 1.48 |
| subset unicode range 4096 | Amiri-Regular.ttf | 6414.2 ±29.7 | — | — | 4917.1 ±99.1 | 1.30 |
| subset unicode range 4096, retain-gids | Amiri-Regular.ttf | 6444.8 ±42.2 | — | — | 4323.4 ±17.4 | 1.49 |
| subset unicode range 10 | NotoNastaliqUrdu-Regular.ttf | 59.5 ±0.8 | — | — | 47.1 ±0.3 | 1.26 |
| subset unicode range 10, retain-gids | NotoNastaliqUrdu-Regular.ttf | 75.1 ±0.4 | — | — | 56.4 ±0.3 | 1.33 |
| subset unicode range 64 | NotoNastaliqUrdu-Regular.ttf | 17273.1 ±59.0 | — | — | 1928.4 ±7.5 | 8.96 |
| subset unicode range 64, retain-gids | NotoNastaliqUrdu-Regular.ttf | 17556.5 ±249.7 | — | — | 1935.8 ±3.0 | 9.07 |
| subset unicode range 512 | NotoNastaliqUrdu-Regular.ttf | 70901.5 ±4200.8 | — | — | 34073.6 ±1195.6 | 2.08 |
| subset unicode range 512, retain-gids | NotoNastaliqUrdu-Regular.ttf | 67389.0 ±1021.1 | — | — | 32286.6 ±1240.4 | 2.09 |
| subset unicode range 1400 | NotoNastaliqUrdu-Regular.ttf | 67013.2 ±611.4 | — | — | 33605.8 ±969.5 | 1.99 |
| subset unicode range 1400, retain-gids | NotoNastaliqUrdu-Regular.ttf | 73828.9 ±3815.4 | — | — | 29913.3 ±321.5 | 2.47 |
| subset unicode range 10 | NotoSansDevanagari-Regular.ttf | 69.7 ±2.2 | — | — | 53.3 ±0.7 | 1.31 |
| subset unicode range 10, retain-gids | NotoSansDevanagari-Regular.ttf | 78.3 ±1.9 | — | — | 57.7 ±1.3 | 1.36 |
| subset unicode range 64 | NotoSansDevanagari-Regular.ttf | 172.4 ±1.2 | — | — | 114.2 ±7.1 | 1.51 |
| subset unicode range 64, retain-gids | NotoSansDevanagari-Regular.ttf | 188.5 ±2.9 | — | — | 117.6 ±3.0 | 1.60 |
| subset unicode range 512 | NotoSansDevanagari-Regular.ttf | 3473.6 ±58.0 | — | — | 1556.3 ±26.8 | 2.23 |
| subset unicode range 512, retain-gids | NotoSansDevanagari-Regular.ttf | 3522.7 ±51.9 | — | — | 1547.3 ±24.5 | 2.28 |
| subset unicode range 1000 | NotoSansDevanagari-Regular.ttf | 3536.6 ±95.3 | — | — | 1550.7 ±17.5 | 2.28 |
| subset unicode range 1000, retain-gids | NotoSansDevanagari-Regular.ttf | 3332.4 ±17.5 | — | — | 1476.8 ±6.2 | 2.26 |
| subset unicode range 10 | Mplus1p-Regular.ttf | 48.9 ±0.3 | — | — | 32.4 ±0.6 | 1.51 |
| subset unicode range 10, retain-gids | Mplus1p-Regular.ttf | 57.2 ±2.2 | — | — | 37.9 ±4.3 | 1.51 |
| subset unicode range 64 | Mplus1p-Regular.ttf | 165.3 ±4.2 | — | — | 83.0 ±9.1 | 1.99 |
| subset unicode range 64, retain-gids | Mplus1p-Regular.ttf | 145.7 ±5.2 | — | — | 82.1 ±4.6 | 1.78 |
| subset unicode range 512 | Mplus1p-Regular.ttf | 576.9 ±16.1 | — | — | 227.0 ±1.4 | 2.54 |
| subset unicode range 512, retain-gids | Mplus1p-Regular.ttf | 751.2 ±36.6 | — | — | 369.5 ±8.1 | 2.03 |
| subset unicode range 4096 | Mplus1p-Regular.ttf | 4187.9 ±82.9 | — | — | 1791.5 ±83.1 | 2.34 |
| subset unicode range 4096, retain-gids | Mplus1p-Regular.ttf | 4189.5 ±27.6 | — | — | 1849.4 ±27.1 | 2.27 |
| subset unicode range 10000 | Mplus1p-Regular.ttf | 9743.5 ±372.6 | — | — | 4322.8 ±196.4 | 2.25 |
| subset unicode range 10000, retain-gids | Mplus1p-Regular.ttf | 9660.0 ±345.5 | — | — | 4089.7 ±135.7 | 2.36 |
| subset unicode range 10 | SourceHanSans-Regular_subset.otf | 160.6 ±11.5 | — | — | 49.0 ±0.9 | 3.28 |
| subset unicode range 10, retain-gids | SourceHanSans-Regular_subset.otf | 33403.3 ±1333.8 | — | — | 308.6 ±15.8 | 108.23 |
| subset unicode range 64 | SourceHanSans-Regular_subset.otf | 281.4 ±3.4 | — | — | 99.8 ±2.1 | 2.82 |
| subset unicode range 64, retain-gids | SourceHanSans-Regular_subset.otf | 36480.9 ±629.6 | — | — | 307.8 ±0.8 | 118.52 |
| subset unicode range 512 | SourceHanSans-Regular_subset.otf | 1509.8 ±86.4 | — | — | 720.1 ±2.4 | 2.10 |
| subset unicode range 512, retain-gids | SourceHanSans-Regular_subset.otf | 31930.6 ±660.1 | — | — | 926.6 ±1.7 | 34.46 |
| subset unicode range 4096 | SourceHanSans-Regular_subset.otf | 36578.7 ±1920.7 | — | — | 9652.3 ±195.3 | 3.79 |
| subset unicode range 4096, retain-gids | SourceHanSans-Regular_subset.otf | 50790.5 ±2901.3 | — | — | 9473.7 ±376.5 | 5.36 |
| subset unicode range 10000 | SourceHanSans-Regular_subset.otf | 53496.0 ±540.1 | — | — | 13176.9 ±620.9 | 4.06 |
| subset unicode range 10000, retain-gids | SourceHanSans-Regular_subset.otf | 55152.2 ±2520.8 | — | — | 11588.9 ±73.3 | 4.76 |
| subset unicode range 10 | SourceSansPro-Regular.otf | 92.3 ±0.5 | — | — | 48.7 ±0.4 | 1.89 |
| subset unicode range 10, retain-gids | SourceSansPro-Regular.otf | 742.3 ±15.5 | — | — | 116.4 ±0.8 | 6.38 |
| subset unicode range 64 | SourceSansPro-Regular.otf | 247.5 ±2.6 | — | — | 133.4 ±3.5 | 1.86 |
| subset unicode range 64, retain-gids | SourceSansPro-Regular.otf | 1035.6 ±16.0 | — | — | 205.1 ±1.9 | 5.05 |
| subset unicode range 512 | SourceSansPro-Regular.otf | 1309.7 ±4.1 | — | — | 586.1 ±13.2 | 2.23 |
| subset unicode range 512, retain-gids | SourceSansPro-Regular.otf | 1840.3 ±2.5 | — | — | 715.3 ±24.3 | 2.57 |
| subset unicode range 2000 | SourceSansPro-Regular.otf | 2843.3 ±5.7 | — | — | 1291.3 ±51.1 | 2.20 |
| subset unicode range 2000, retain-gids | SourceSansPro-Regular.otf | 3465.0 ±97.3 | — | — | 1334.5 ±28.1 | 2.60 |

## wasm

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location wght=500 | RobotoFlex-Variable | 23.3 ±1.1 | 0.0 ±0.0 | 698.98 | 0.2 ±0.0 | 97.24 |
| box wght=500 | RobotoFlex-Variable | 4098.2 ±5.1 | — | — | 2027.8 ±11.8 | 2.02 |
| location with reference queries wght=499/500 | RobotoFlex-Variable | 19.9 ±0.0 | 1.8 ±0.0 | 11.31 | 2.4 ±0.0 | 8.44 |
| advance wght=500 | RobotoFlex-Variable | 152.1 ±1.7 | 93.5 ±0.1 | 1.63 | 101.4 ±0.2 | 1.50 |
| outline unscaled wght=500 | RobotoFlex-Variable | 5271.3 ±23.9 | 1567.5 ±1.8 | 3.36 | 1560.0 ±13.7 | 3.38 |
| outline 48px wght=500 | RobotoFlex-Variable | 4732.4 ±12.7 | 1604.7 ±1.5 | 2.95 | 1503.0 ±11.2 | 3.15 |
| raster 24px wght=500 | RobotoFlex-Variable | 12388.2 ±10.4 | 9292.0 ±11.1 | 1.33 | — | — |
| location wght=500 | SourceSerifVariable-Roman | 1.1 ±0.0 | 0.0 ±0.0 | 75.13 | 0.1 ±0.0 | 14.11 |
| box wght=500 | SourceSerifVariable-Roman | 3389.3 ±3.4 | — | — | 1130.9 ±0.9 | 3.00 |
| location with reference queries wght=499/500 | SourceSerifVariable-Roman | 1.1 ±0.0 | 0.8 ±0.0 | 1.51 | 1.2 ±0.0 | 0.92 |
| advance wght=500 | SourceSerifVariable-Roman | 96.5 ±0.2 | 77.8 ±0.2 | 1.24 | 80.1 ±0.1 | 1.20 |
| outline unscaled wght=500 | SourceSerifVariable-Roman | 3772.8 ±4.3 | 2044.5 ±3.5 | 1.85 | 1582.6 ±15.0 | 2.38 |
| outline 48px wght=500 | SourceSerifVariable-Roman | 3951.0 ±5.6 | 2627.8 ±72.2 | 1.50 | 1582.0 ±14.6 | 2.50 |
| raster 24px wght=500 | SourceSerifVariable-Roman | 7401.6 ±26.4 | 5787.8 ±11.2 | 1.28 | — | — |
| location wght=500 | AdobeVFPrototype | 18.1 ±0.0 | 0.0 ±0.0 | 925.84 | 0.1 ±0.0 | 214.25 |
| box wght=500 | AdobeVFPrototype | 1508.0 ±6.1 | — | — | 1828.8 ±3.1 | 0.82 |
| location with reference queries wght=499/500 | AdobeVFPrototype | 18.2 ±0.0 | 6.8 ±0.0 | 2.69 | 5.8 ±0.0 | 3.13 |
| advance wght=500 | AdobeVFPrototype | 22.4 ±0.0 | 17.9 ±0.1 | 1.25 | 12.4 ±0.0 | 1.81 |
| outline unscaled wght=500 | AdobeVFPrototype | 5110.0 ±9.1 | 1812.2 ±2.8 | 2.82 | 1825.1 ±2.9 | 2.80 |
| outline 48px wght=500 | AdobeVFPrototype | 5119.4 ±4.6 | 1802.4 ±9.3 | 2.84 | 1830.4 ±3.7 | 2.80 |
| raster 24px wght=500 | AdobeVFPrototype | 6967.2 ±196.3 | 2506.2 ±5.4 | 2.78 | — | — |
| Unicode properties (1682970 characters) | perf/texts | 14782.2 ±23.7 | — | — | 27591.7 ±36.1 | 0.54 |
| cmap | extra/arabic | 0.2 ±0.0 | 0.3 ±0.0 | 0.70 | 0.4 ±0.0 | 0.55 |
| advance | extra/arabic | 0.2 ±0.0 | 4.6 ±0.0 | 0.04 | 2.6 ±0.0 | 0.07 |
| outline unscaled sampled | extra/arabic | 75.6 ±1.4 | 70.1 ±1.3 | 1.08 | — | — |
| cmap | extra/devanagari | 0.2 ±0.0 | 0.3 ±0.0 | 0.71 | 0.4 ±0.0 | 0.53 |
| advance | extra/devanagari | 0.2 ±0.0 | 5.1 ±0.1 | 0.04 | 2.9 ±0.1 | 0.07 |
| outline unscaled sampled | extra/devanagari | 93.5 ±3.2 | 98.3 ±3.6 | 0.95 | — | — |
| cmap | extra/khmer | 0.2 ±0.0 | 0.3 ±0.0 | 0.71 | 0.3 ±0.0 | 0.63 |
| advance | extra/khmer | 0.2 ±0.0 | 5.3 ±0.4 | 0.04 | 3.0 ±0.1 | 0.07 |
| outline unscaled sampled | extra/khmer | 94.9 ±6.9 | 80.8 ±4.4 | 1.17 | — | — |
| cmap | extra/myanmar | 0.2 ±0.0 | 0.3 ±0.0 | 0.67 | 0.3 ±0.0 | 0.72 |
| advance | extra/myanmar | 0.2 ±0.0 | 4.9 ±0.1 | 0.04 | 2.7 ±0.0 | 0.07 |
| outline unscaled sampled | extra/myanmar | 94.8 ±1.9 | 86.5 ±1.4 | 1.10 | — | — |
| cmap | extra/balinese | 0.2 ±0.0 | 0.2 ±0.0 | 0.70 | 0.2 ±0.0 | 0.71 |
| advance | extra/balinese | 0.2 ±0.0 | 4.6 ±0.0 | 0.04 | 2.6 ±0.0 | 0.07 |
| outline unscaled sampled | extra/balinese | 137.7 ±2.2 | 109.8 ±6.0 | 1.25 | — | — |
| cmap | extra/hebrew | 0.2 ±0.0 | 0.3 ±0.0 | 0.69 | 0.2 ±0.0 | 0.71 |
| advance | extra/hebrew | 0.2 ±0.0 | 4.6 ±0.0 | 0.04 | 2.6 ±0.0 | 0.07 |
| outline unscaled sampled | extra/hebrew | 49.2 ±1.7 | 52.3 ±1.0 | 0.94 | — | — |
| cmap | extra/source-serif | 0.2 ±0.0 | 0.3 ±0.0 | 0.71 | 0.4 ±0.0 | 0.52 |
| advance | extra/source-serif | 0.2 ±0.0 | 4.8 ±0.1 | 0.04 | 3.1 ±0.1 | 0.06 |
| outline unscaled sampled | extra/source-serif | 3294.9 ±3.5 | 1075.7 ±1.3 | 3.06 | — | — |
| cmap | extra/stix-math | 0.2 ±0.0 | 0.3 ±0.0 | 0.70 | 0.4 ±0.0 | 0.54 |
| advance | extra/stix-math | 0.2 ±0.0 | 5.1 ±0.4 | 0.04 | 2.9 ±0.1 | 0.06 |
| outline unscaled sampled | extra/stix-math | 332.9 ±1.6 | 420.3 ±0.5 | 0.79 | — | — |
| MATH constants | extra/stix-math | 0.2 ±0.0 | — | — | 0.8 ±0.0 | 0.31 |
| cmap | extra/source-han | 0.3 ±0.0 | 0.4 ±0.0 | 0.73 | 0.4 ±0.0 | 0.66 |
| advance | extra/source-han | 0.2 ±0.0 | 4.6 ±0.0 | 0.04 | 2.6 ±0.0 | 0.07 |
| outline unscaled sampled | extra/source-han | 340.3 ±1.6 | 256.9 ±0.5 | 1.32 | — | — |
| vertical advance | extra/source-han | 0.9 ±0.0 | — | — | 2.7 ±0.0 | 0.33 |
| cmap | extra/amstelvar | 0.2 ±0.0 | 0.3 ±0.0 | 0.64 | 0.3 ±0.0 | 0.59 |
| advance | extra/amstelvar | 0.2 ±0.0 | 4.6 ±0.0 | 0.04 | 2.6 ±0.0 | 0.08 |
| outline unscaled sampled | extra/amstelvar | 87.6 ±0.1 | 104.4 ±1.1 | 0.84 | — | — |
| cmap | extra/color-cff | 0.1 ±0.0 | 0.2 ±0.0 | 0.76 | 0.2 ±0.0 | 0.66 |
| advance | extra/color-cff | 0.0 ±0.0 | 1.1 ±0.0 | 0.04 | 0.6 ±0.0 | 0.07 |
| outline unscaled sampled | extra/color-cff | 15.5 ±0.3 | 13.3 ±0.2 | 1.17 | — | — |
| cmap | extra/color-sbix | 0.1 ±0.0 | 0.2 ±0.0 | 0.76 | 0.2 ±0.0 | 0.67 |
| advance | extra/color-sbix | 0.0 ±0.0 | 1.0 ±0.0 | 0.04 | 0.6 ±0.0 | 0.07 |
| outline unscaled sampled | extra/color-sbix | 1.8 ±0.0 | 2.5 ±0.0 | 0.73 | — | — |
| cmap | extra/color-svg | 0.2 ±0.0 | 0.2 ±0.0 | 0.85 | 0.2 ±0.0 | 0.75 |
| advance | extra/color-svg | 0.0 ±0.0 | 1.0 ±0.0 | 0.04 | 0.6 ±0.0 | 0.07 |
| outline unscaled sampled | extra/color-svg | 1.8 ±0.0 | 2.6 ±0.0 | 0.71 | — | — |
| outline unscaled | SourceSansPro-Regular | 5506.9 ±7.8 | 4961.3 ±5.7 | 1.11 | 2707.4 ±5.5 | 2.03 |
| outline 48px | SourceSansPro-Regular | 5559.9 ±7.5 | 5116.3 ±5.6 | 1.09 | 2705.9 ±2.7 | 2.05 |
| raster 12px | SourceSansPro-Regular | 7870.9 ±7.6 | 7096.2 ±8.7 | 1.11 | — | — |
| raster 24px | SourceSansPro-Regular | 9280.3 ±6.3 | 8148.3 ±11.6 | 1.14 | — | — |
| raster 48px | SourceSansPro-Regular | 12643.9 ±20.4 | 10464.6 ±17.2 | 1.21 | — | — |
| raster 96px | SourceSansPro-Regular | 20393.1 ±73.3 | 15544.1 ±50.7 | 1.31 | — | — |
| open (FreeType) | SourceSansPro-Regular | 10.0 ±0.1 | 35.3 ±0.6 | 0.28 | — | — |
| glyph extents | SourceSansPro-Regular | 2053.7 ±3.9 | — | — | 2346.1 ±2.4 | 0.88 |
| open (HarfBuzz) | SourceSansPro-Regular | 1.9 ±0.0 | — | — | 2.6 ±0.0 | 0.74 |
| glyph names | SourceSansPro-Regular | 48.1 ±0.1 | — | — | 210.0 ±0.3 | 0.23 |
| glyph from name | SourceSansPro-Regular | 1346.2 ±1.5 | — | — | 441.3 ±0.5 | 3.05 |
| outline unscaled | NotoSerifMyanmar-Regular | 2597.2 ±4.4 | 2352.9 ±1.7 | 1.10 | 1583.4 ±2.4 | 1.64 |
| outline 48px | NotoSerifMyanmar-Regular | 2641.4 ±3.2 | 2466.5 ±4.2 | 1.07 | 1582.7 ±1.9 | 1.67 |
| raster 12px | NotoSerifMyanmar-Regular | 4743.6 ±114.0 | 3992.4 ±14.6 | 1.19 | — | — |
| raster 24px | NotoSerifMyanmar-Regular | 5383.4 ±7.9 | 4991.7 ±8.0 | 1.08 | — | — |
| raster 48px | NotoSerifMyanmar-Regular | 7974.9 ±7.1 | 7307.9 ±54.4 | 1.09 | — | — |
| raster 96px | NotoSerifMyanmar-Regular | 15373.2 ±232.3 | 13345.8 ±258.6 | 1.15 | — | — |
| open (FreeType) | NotoSerifMyanmar-Regular | 5.0 ±0.0 | 13.9 ±0.0 | 0.36 | — | — |
| glyph extents | NotoSerifMyanmar-Regular | 1143.8 ±5.4 | — | — | 1409.9 ±3.5 | 0.81 |
| open (HarfBuzz) | NotoSerifMyanmar-Regular | 1.3 ±0.0 | — | — | 2.3 ±0.0 | 0.56 |
| glyph names | NotoSerifMyanmar-Regular | 23.1 ±0.1 | — | — | 36.3 ±0.0 | 0.64 |
| glyph from name | NotoSerifMyanmar-Regular | 268.5 ±0.9 | — | — | 78.4 ±0.1 | 3.43 |
| outline unscaled | AdobeVFPrototype | 5119.2 ±7.9 | 1759.6 ±2.6 | 2.91 | 1765.8 ±3.7 | 2.90 |
| outline 48px | AdobeVFPrototype | 5147.4 ±8.8 | 1789.0 ±2.4 | 2.88 | 1765.6 ±5.0 | 2.92 |
| raster 12px | AdobeVFPrototype | 5610.0 ±8.3 | 2208.8 ±4.0 | 2.54 | — | — |
| raster 24px | AdobeVFPrototype | 6447.5 ±26.6 | 2493.0 ±4.1 | 2.59 | — | — |
| raster 48px | AdobeVFPrototype | 6546.3 ±7.2 | 3376.0 ±30.8 | 1.94 | — | — |
| raster 96px | AdobeVFPrototype | 7949.3 ±9.1 | 4202.0 ±2.0 | 1.89 | — | — |
| open (FreeType) | AdobeVFPrototype | 8.2 ±0.0 | 11.6 ±0.2 | 0.71 | — | — |
| glyph extents | AdobeVFPrototype | 1473.3 ±57.2 | — | — | 1857.7 ±27.7 | 0.79 |
| open (HarfBuzz) | AdobeVFPrototype | 2.6 ±0.0 | — | — | 3.5 ±0.1 | 0.76 |
| outline unscaled | SourceHanSans-Regular_subset | 62671.7 ±502.3 | 65128.1 ±78.0 | 0.96 | 33826.8 ±65.9 | 1.85 |
| outline 48px | SourceHanSans-Regular_subset | 63418.5 ±409.5 | 67755.9 ±81.2 | 0.94 | 33842.7 ±78.0 | 1.87 |
| raster 12px | SourceHanSans-Regular_subset | 94837.7 ±299.1 | 100286.0 ±948.6 | 0.95 | — | — |
| raster 24px | SourceHanSans-Regular_subset | 117843.4 ±817.0 | 123677.0 ±451.0 | 0.95 | — | — |
| raster 48px | SourceHanSans-Regular_subset | 164913.0 ±292.2 | 167727.3 ±256.2 | 0.98 | — | — |
| raster 96px | SourceHanSans-Regular_subset | 339856.5 ±6052.0 | 314515.3 ±5849.4 | 1.08 | — | — |
| cid | SourceHanSans-Regular_subset | 267.3 ±12.4 | 314.2 ±12.7 | 0.85 | — | — |
| cid (no index) | SourceHanSans-Regular_subset | 333787.6 ±6874.5 | 329.5 ±9.1 | 1013.02 | — | — |
| open (FreeType) | SourceHanSans-Regular_subset | 77.6 ±0.9 | 142.7 ±2.5 | 0.54 | — | — |
| glyph extents | SourceHanSans-Regular_subset | 36345.0 ±1569.7 | — | — | 34840.0 ±861.5 | 1.04 |
| open (HarfBuzz) | SourceHanSans-Regular_subset | 14.3 ±1.6 | — | — | 12.2 ±1.8 | 1.18 |
| glyph color format | extra/color-cff | 4.2 ±0.0 | — | — | 0.7 ±0.0 | 6.38 |
| COLRv1 paint | extra/color-cff | 8.0 ±0.3 | 2.2 ±0.1 | 3.62 | 4.4 ±0.1 | 1.83 |
| COLRv1 recording | extra/color-cff | 8.8 ±0.2 | — | — | — | — |
| color glyph extents | extra/color-cff | 16.0 ±0.4 | — | — | 6.1 ±0.4 | 2.62 |
| glyph color format | extra/color-sbix | 2.8 ±0.0 | — | — | 0.7 ±0.0 | 4.17 |
| bitmap data 16 ppem | extra/color-sbix | 5.5 ±0.4 | — | — | 6.0 ±0.1 | 0.91 |
| bitmap data 109 ppem | extra/color-sbix | 5.6 ±0.3 | — | — | 6.1 ±0.1 | 0.91 |
| bitmap data 136 ppem | extra/color-sbix | 5.8 ±0.2 | — | — | 6.8 ±0.1 | 0.85 |
| bitmap data 0 ppem | extra/color-sbix | 6.0 ±0.2 | — | — | 6.4 ±0.3 | 0.93 |
| color glyph extents | extra/color-sbix | 3.1 ±0.1 | — | — | 3.3 ±0.0 | 0.94 |
| glyph color format | extra/color-svg | 2.1 ±0.0 | — | — | 0.6 ±0.0 | 3.22 |
| SVG documents | extra/color-svg | 1.2 ±0.0 | — | — | 0.8 ±0.0 | 1.47 |
| color glyph extents | extra/color-svg | 2.4 ±0.0 | — | — | 1.8 ±0.0 | 1.36 |
| glyph color format | TwemojiMozilla.subset | 0.9 ±0.0 | — | — | 0.1 ±0.0 | 6.61 |
| COLRv0 layers | TwemojiMozilla.subset | 0.4 ±0.0 | 0.1 ±0.0 | 3.05 | 0.2 ±0.0 | 1.80 |
| COLRv0 raster 12px | TwemojiMozilla.subset | 36.8 ±0.7 | 24.7 ±0.6 | 1.49 | — | — |
| COLRv0 raster 24px | TwemojiMozilla.subset | 51.6 ±0.9 | 39.2 ±1.0 | 1.32 | — | — |
| COLRv0 raster 48px | TwemojiMozilla.subset | 107.2 ±2.1 | 88.6 ±2.7 | 1.21 | — | — |
| COLRv0 paint | TwemojiMozilla.subset | 1.0 ±0.0 | — | — | 0.5 ±0.0 | 2.04 |
| color glyph extents | TwemojiMozilla.subset | 8.0 ±0.1 | — | — | 5.9 ±0.1 | 1.37 |
| glyph color format | test_glyphs-glyf_colr_1 | 33.8 ±0.3 | — | — | 8.4 ±0.3 | 4.01 |
| COLRv0 layers | test_glyphs-glyf_colr_1 | 15.8 ±0.3 | 2.9 ±0.1 | 5.47 | 6.0 ±0.1 | 2.65 |
| COLRv0 raster 12px | test_glyphs-glyf_colr_1 | 172.6 ±2.1 | 128.3 ±3.2 | 1.34 | — | — |
| COLRv0 raster 24px | test_glyphs-glyf_colr_1 | 286.5 ±5.7 | 216.2 ±4.5 | 1.33 | — | — |
| COLRv0 raster 48px | test_glyphs-glyf_colr_1 | 577.8 ±9.7 | 468.7 ±10.7 | 1.23 | — | — |
| COLRv1 paint | test_glyphs-glyf_colr_1 | 101.7 ±1.7 | 35.6 ±1.0 | 2.86 | 55.7 ±1.3 | 1.83 |
| COLRv1 recording | test_glyphs-glyf_colr_1 | 109.2 ±2.4 | — | — | — | — |
| color glyph extents | test_glyphs-glyf_colr_1 | 88.2 ±1.3 | — | — | 51.2 ±0.1 | 1.72 |
| glyph color format | noto_handwriting-glyf_colr_1 | 3.4 ±0.1 | — | — | 0.5 ±0.0 | 7.31 |
| COLRv1 paint | noto_handwriting-glyf_colr_1 | 10.9 ±0.8 | 5.4 ±0.1 | 2.03 | 5.7 ±0.5 | 1.90 |
| COLRv1 recording | noto_handwriting-glyf_colr_1 | 11.5 ±0.9 | — | — | — | — |
| color glyph extents | noto_handwriting-glyf_colr_1 | 12.9 ±2.0 | — | — | 3.4 ±0.4 | 3.84 |
| glyph color format | NotoColorEmoji.subset | 1.9 ±0.0 | — | — | 0.1 ±0.0 | 16.49 |
| bitmap data 16 ppem | NotoColorEmoji.subset | 2.0 ±0.0 | — | — | 1.5 ±0.1 | 1.33 |
| bitmap data 109 ppem | NotoColorEmoji.subset | 2.2 ±0.1 | — | — | 1.5 ±0.0 | 1.46 |
| bitmap data 136 ppem | NotoColorEmoji.subset | 2.3 ±0.1 | — | — | 1.8 ±0.0 | 1.31 |
| bitmap data 0 ppem | NotoColorEmoji.subset | 2.2 ±0.1 | — | — | 1.7 ±0.1 | 1.25 |
| color glyph extents | NotoColorEmoji.subset | 1.0 ±0.0 | — | — | 1.1 ±0.0 | 0.96 |
| glyph color format | noto_handwriting-sbix | 1.4 ±0.1 | — | — | 0.3 ±0.0 | 5.04 |
| bitmap data 16 ppem | noto_handwriting-sbix | 2.9 ±0.1 | — | — | 3.3 ±0.2 | 0.89 |
| bitmap data 109 ppem | noto_handwriting-sbix | 2.9 ±0.1 | — | — | 3.0 ±0.1 | 0.95 |
| bitmap data 136 ppem | noto_handwriting-sbix | 2.8 ±0.0 | — | — | 3.3 ±0.1 | 0.84 |
| bitmap data 0 ppem | noto_handwriting-sbix | 2.8 ±0.1 | — | — | 3.2 ±0.1 | 0.90 |
| color glyph extents | noto_handwriting-sbix | 1.4 ±0.0 | — | — | 1.4 ±0.0 | 0.95 |
| glyph color format | chromacheck-svg | 0.2 ±0.0 | — | — | 0.1 ±0.0 | 4.06 |
| SVG documents | chromacheck-svg | 0.1 ±0.0 | — | — | 0.1 ±0.0 | 1.62 |
| color glyph extents | chromacheck-svg | 0.3 ±0.0 | — | — | 0.2 ±0.0 | 1.49 |
| bitmap pixels 16 ppem | EBDT (built, 200 glyphs) | 111.2 ±5.2 | 75.2 ±4.1 | 1.48 | — | — |
| cmap | Roboto-Regular | 5594.4 ±127.5 | 7517.3 ±157.3 | 0.74 | 12308.8 ±202.4 | 0.45 |
| advance | Roboto-Regular | 5.4 ±0.1 | 142.6 ±4.8 | 0.04 | 81.5 ±2.4 | 0.07 |
| v advance | Roboto-Regular | 192.9 ±24.4 | — | — | 279.3 ±8.8 | 0.69 |
| v origin | Roboto-Regular | 506.2 ±46.6 | — | — | 568.5 ±24.2 | 0.89 |
| outline unscaled | Roboto-Regular | 2037.2 ±65.3 | 2275.0 ±80.9 | 0.90 | 2312.3 ±88.4 | 0.88 |
| outline 48px | Roboto-Regular | 2230.0 ±110.7 | 2917.0 ±311.6 | 0.76 | 2759.5 ±140.9 | 0.81 |
| raster 12px | Roboto-Regular | 6873.6 ±169.9 | 7496.0 ±217.8 | 0.92 | — | — |
| raster 24px | Roboto-Regular | 10545.0 ±229.5 | 10218.5 ±211.5 | 1.03 | — | — |
| raster 48px | Roboto-Regular | 17189.3 ±826.0 | 15668.0 ±292.8 | 1.10 | — | — |
| raster 96px | Roboto-Regular | 34521.8 ±508.8 | 29283.1 ±847.1 | 1.18 | — | — |
| cmap | Comfortaa-Regular-new | 6322.3 ±156.1 | 8833.8 ±213.4 | 0.72 | 9236.5 ±244.0 | 0.68 |
| advance | Comfortaa-Regular-new | 1.5 ±0.0 | 38.5 ±0.8 | 0.04 | 21.0 ±1.0 | 0.07 |
| v advance | Comfortaa-Regular-new | 41.2 ±0.6 | — | — | 59.8 ±0.8 | 0.69 |
| v origin | Comfortaa-Regular-new | 146.7 ±2.1 | — | — | 140.0 ±2.4 | 1.05 |
| outline unscaled | Comfortaa-Regular-new | 740.6 ±67.9 | 787.6 ±16.4 | 0.94 | 925.5 ±30.6 | 0.80 |
| outline 48px | Comfortaa-Regular-new | 711.0 ±5.9 | 877.6 ±16.5 | 0.81 | 826.9 ±12.2 | 0.86 |
| raster 12px | Comfortaa-Regular-new | 2449.5 ±37.3 | 2007.3 ±5.5 | 1.22 | — | — |
| raster 24px | Comfortaa-Regular-new | 2918.0 ±16.9 | 2820.8 ±6.3 | 1.03 | — | — |
| raster 48px | Comfortaa-Regular-new | 5500.6 ±145.3 | 4787.2 ±91.4 | 1.15 | — | — |
| raster 96px | Comfortaa-Regular-new | 10971.4 ±239.2 | 8977.9 ±273.5 | 1.22 | — | — |
| cmap | NotoNastaliqUrdu-Regular | 5071.6 ±16.3 | 8011.1 ±93.1 | 0.63 | 8005.1 ±726.7 | 0.63 |
| advance | NotoNastaliqUrdu-Regular | 1.6 ±0.0 | 41.9 ±1.5 | 0.04 | 22.6 ±0.0 | 0.07 |
| v advance | NotoNastaliqUrdu-Regular | 34.0 ±0.1 | — | — | 79.0 ±0.1 | 0.43 |
| v origin | NotoNastaliqUrdu-Regular | 135.8 ±0.2 | — | — | 161.2 ±0.5 | 0.84 |
| outline unscaled | NotoNastaliqUrdu-Regular | 955.6 ±20.0 | 846.8 ±1.0 | 1.13 | 983.9 ±2.8 | 0.97 |
| outline 48px | NotoNastaliqUrdu-Regular | 991.0 ±40.3 | 1002.6 ±22.6 | 0.99 | 1069.8 ±36.4 | 0.93 |
| raster 12px | NotoNastaliqUrdu-Regular | 2787.2 ±5.6 | 2832.0 ±2.8 | 0.98 | — | — |
| raster 24px | NotoNastaliqUrdu-Regular | 3851.8 ±9.7 | 4372.2 ±98.3 | 0.88 | — | — |
| raster 48px | NotoNastaliqUrdu-Regular | 6228.8 ±10.2 | 5983.9 ±40.8 | 1.04 | — | — |
| raster 96px | NotoNastaliqUrdu-Regular | 12174.6 ±24.6 | 10807.8 ±204.9 | 1.13 | — | — |
| cmap | RobotoFlex-Variable | 5206.2 ±11.1 | 7221.1 ±10.0 | 0.72 | 7681.1 ±12.9 | 0.68 |
| advance | RobotoFlex-Variable | 1.3 ±0.0 | 33.9 ±0.1 | 0.04 | 18.9 ±0.0 | 0.07 |
| v advance | RobotoFlex-Variable | 37.7 ±0.1 | — | — | 52.3 ±0.1 | 0.72 |
| v origin | RobotoFlex-Variable | 126.8 ±0.2 | — | — | 122.0 ±0.2 | 1.04 |
| outline unscaled | RobotoFlex-Variable | 498.3 ±1.4 | 538.9 ±2.5 | 0.92 | 621.4 ±11.2 | 0.80 |
| outline 48px | RobotoFlex-Variable | 521.1 ±1.3 | 615.7 ±0.9 | 0.85 | 613.1 ±4.9 | 0.85 |
| raster 12px | RobotoFlex-Variable | 4135.9 ±60.5 | 4106.7 ±10.2 | 1.01 | — | — |
| raster 24px | RobotoFlex-Variable | 7917.9 ±6.6 | 7773.3 ±24.2 | 1.02 | — | — |
| raster 48px | RobotoFlex-Variable | 22339.5 ±132.2 | 22403.3 ±301.1 | 1.00 | — | — |
| raster 96px | RobotoFlex-Variable | 56933.5 ±540.1 | 69344.0 ±833.2 | 0.82 | — | — |
| cmap | SourceSerifVariable-Roman | 6429.8 ±47.5 | 7727.5 ±8.9 | 0.83 | 10676.8 ±10.9 | 0.60 |
| advance | SourceSerifVariable-Roman | 2.0 ±0.0 | 51.2 ±0.0 | 0.04 | 28.8 ±0.2 | 0.07 |
| v advance | SourceSerifVariable-Roman | 42.8 ±0.1 | — | — | 107.4 ±1.9 | 0.40 |
| v origin | SourceSerifVariable-Roman | 172.0 ±0.3 | — | — | 212.7 ±0.4 | 0.81 |
| outline unscaled | SourceSerifVariable-Roman | 1105.3 ±2.5 | 1113.3 ±1.7 | 0.99 | 1234.2 ±1.2 | 0.90 |
| outline 48px | SourceSerifVariable-Roman | 1159.1 ±1.2 | 1287.7 ±1.5 | 0.90 | 1260.7 ±26.0 | 0.92 |
| raster 12px | SourceSerifVariable-Roman | 3266.2 ±4.7 | 3426.6 ±5.9 | 0.95 | — | — |
| raster 24px | SourceSerifVariable-Roman | 4472.6 ±4.8 | 5161.7 ±64.7 | 0.87 | — | — |
| raster 48px | SourceSerifVariable-Roman | 7590.0 ±309.8 | 7257.7 ±9.2 | 1.05 | — | — |
| raster 96px | SourceSerifVariable-Roman | 13762.2 ±18.9 | 13556.4 ±719.5 | 1.02 | — | — |
| HVAR item delta cached | RobotoFlex-Variable.ttf | 121.3 ±0.5 | — | — | — | — |
| HVAR item delta copied font | RobotoFlex-Variable.ttf | 789.1 ±0.9 | — | — | — | — |
| HVAR item delta different font | RobotoFlex-Variable.ttf | 801.8 ±2.1 | — | — | — | — |
| HVAR item delta cached | SourceSerifVariable-Roman.ttf | 67.1 ±0.1 | — | — | — | — |
| HVAR item delta copied font | SourceSerifVariable-Roman.ttf | 116.0 ±0.2 | — | — | — | — |
| HVAR item delta different font | SourceSerifVariable-Roman.ttf | 115.8 ±0.3 | — | — | — | — |
| MVAR item delta static default | SourceSerifVariable-Roman.ttf | 0.0 ±0.0 | — | — | — | — |
| HVAR item delta cached | AdobeVFPrototype.otf | 15.8 ±0.1 | — | — | — | — |
| HVAR item delta copied font | AdobeVFPrototype.otf | 29.1 ±0.1 | — | — | — | — |
| HVAR item delta different font | AdobeVFPrototype.otf | 29.0 ±0.1 | — | — | — | — |
| MVAR item delta static default | AdobeVFPrototype.otf | 0.1 ±0.0 | — | — | — | — |
| shaping en-thelittleprince.txt | Roboto-Regular | 42842.4 ±167.2 | — | — | 8065.9 ±18.4 | 5.31 |
| first shaping en-thelittleprince.txt | Roboto-Regular | 73.4 ±0.1 | — | — | 68.6 ±0.4 | 1.07 |
| shaping en-words.txt | Roboto-Regular | 53164.7 ±153.6 | — | — | 12082.6 ±134.5 | 4.40 |
| first shaping en-words.txt | Roboto-Regular | 41.7 ±0.1 | — | — | 61.6 ±0.3 | 0.68 |
| shaping react-dom.txt | SourceSerifVariable-Roman | 360461.8 ±957.0 | — | — | 102752.8 ±141.4 | 3.51 |
| first shaping react-dom.txt | SourceSerifVariable-Roman | 20.0 ±0.0 | — | — | 17.2 ±0.0 | 1.16 |
| shaping hi-words.txt | NotoSansDevanagari-Regular | 63605.9 ±497.8 | — | — | 29094.5 ±75.4 | 2.19 |
| first shaping hi-words.txt | NotoSansDevanagari-Regular | 62.6 ±0.1 | — | — | 45.9 ±0.9 | 1.36 |
| shaping fa-thelittleprince.txt | Amiri-Regular | 120001.9 ±615.4 | — | — | 58447.0 ±130.0 | 2.05 |
| first shaping fa-thelittleprince.txt | Amiri-Regular | 125.8 ±0.2 | — | — | 51.8 ±0.3 | 2.43 |
| shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 320443.8 ±1890.4 | — | — | 87450.5 ±117.6 | 3.66 |
| first shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 240.2 ±0.2 | — | — | 1771.5 ±3.4 | 0.14 |
| shaping fa-words.txt | NotoNastaliqUrdu-Regular | 343373.8 ±798.9 | — | — | 98907.7 ±690.9 | 3.47 |
| first shaping fa-words.txt | NotoNastaliqUrdu-Regular | 204.9 ±0.7 | — | — | 1760.5 ±1.4 | 0.12 |
| shaping fa-thelittleprince.txt | Gulzar-Regular | 6675081.9 ±34534.0 | — | — | 1362369.6 ±11483.5 | 4.90 |
| first shaping fa-thelittleprince.txt | Gulzar-Regular | 3388.9 ±685.5 | — | — | 1957.1 ±169.5 | 1.73 |
| shaping fa-words.txt | Gulzar-Regular | 4702721.6 ±105043.0 | — | — | 989165.0 ±11208.2 | 4.75 |
| first shaping fa-words.txt | Gulzar-Regular | 1235.2 ±3.4 | — | — | 1513.7 ±2.1 | 0.82 |
| shaping duployan.txt | NotoSansDuployan-Regular | 18311089.8 ±423042.2 | — | — | 2892189.7 ±147795.7 | 6.33 |
| first shaping duployan.txt | NotoSansDuployan-Regular | 62164.7 ±1755.1 | — | — | 10460.1 ±382.4 | 5.94 |
| shaping ja-runs.txt | Mplus1p-Regular | 40859.6 ±2361.9 | — | — | 29790.2 ±3016.5 | 1.37 |
| first shaping ja-runs.txt | Mplus1p-Regular | 21.3 ±0.8 | — | — | 23.4 ±1.1 | 0.91 |
| shaping ja-runs.txt TTB | Mplus1p-Regular | 58432.7 ±6389.4 | — | — | 37800.0 ±3570.3 | 1.55 |
| first shaping ja-runs.txt TTB | Mplus1p-Regular | 19.4 ±0.2 | — | — | 12.1 ±0.3 | 1.61 |
| shaping role-default-und | extra/arabic | 3.1 ±0.0 | — | — | 1.8 ±0.0 | 1.72 |
| first shaping role-default-und | extra/arabic | 38.6 ±0.3 | — | — | 25.2 ±0.4 | 1.53 |
| shaping role-default-und | extra/devanagari | 3.8 ±0.0 | — | — | 2.2 ±0.0 | 1.72 |
| first shaping role-default-und | extra/devanagari | 129.1 ±2.6 | — | — | 1235.9 ±6.2 | 0.10 |
| shaping role-default-und | extra/khmer | 1.8 ±0.0 | — | — | 1.3 ±0.0 | 1.43 |
| first shaping role-default-und | extra/khmer | 48.8 ±0.4 | — | — | 30.2 ±0.9 | 1.62 |
| shaping role-default-und | extra/myanmar | 4.0 ±0.0 | — | — | 1.9 ±0.0 | 2.09 |
| first shaping role-default-und | extra/myanmar | 131.2 ±1.0 | — | — | 65.6 ±0.3 | 2.00 |
| shaping role-default-und | extra/balinese | 19.7 ±0.2 | — | — | 8.8 ±0.1 | 2.24 |
| first shaping role-default-und | extra/balinese | 169.6 ±2.2 | — | — | 61.1 ±0.9 | 2.77 |
| shaping role-default-und | extra/hebrew | 2.7 ±0.0 | — | — | 1.5 ±0.0 | 1.78 |
| first shaping role-default-und | extra/hebrew | 15.7 ±0.3 | — | — | 13.0 ±0.3 | 1.21 |
| shaping role-default-und | extra/source-serif | 2.5 ±0.0 | — | — | 0.8 ±0.0 | 3.02 |
| first shaping role-default-und | extra/source-serif | 41.8 ±0.2 | — | — | 25.7 ±0.3 | 1.63 |
| shaping role-default-und TTB | extra/source-han | 0.7 ±0.1 | — | — | 0.5 ±0.0 | 1.50 |
| first shaping role-default-und TTB | extra/source-han | 102.5 ±11.2 | — | — | 42.5 ±4.7 | 2.41 |
| shaping role-default-und | extra/amstelvar | 0.9 ±0.0 | — | — | 0.6 ±0.0 | 1.55 |
| first shaping role-default-und | extra/amstelvar | 4.6 ±0.1 | — | — | 13.4 ±0.5 | 0.34 |
| shaping react-dom.txt wght=500 | SourceSerifVariable-Roman | 526331.5 ±18988.7 | — | — | 144107.0 ±9067.4 | 3.65 |
| subset (42 glyphs, retain-gids) | Roboto-Regular | 383.1 ±34.3 | — | — | 235.2 ±32.5 | 1.63 |
| subset (2322 characters) | Roboto-Regular | 480.4 ±30.1 | — | — | 194.7 ±15.3 | 2.47 |
| subset prepared (42 glyphs, retain-gids) | Roboto-Regular | 192.7 ±13.8 | — | — | 127.8 ±9.0 | 1.51 |
| subset prepared (2322 characters) | Roboto-Regular | 392.0 ±20.3 | — | — | 122.6 ±9.8 | 3.20 |
| preprocess | Roboto-Regular | 339.6 ±4.7 | — | — | 938.0 ±43.2 | 0.36 |
| subset (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 116471.0 ±21303.5 | — | — | 22541.9 ±1131.5 | 5.17 |
| subset (2322 characters) | SourceHanSans-Regular_subset | 60877.5 ±8738.4 | — | — | 25407.2 ±1494.4 | 2.40 |
| subset prepared (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 61085.9 ±4308.5 | — | — | 8109.3 ±881.4 | 7.53 |
| subset prepared (2322 characters) | SourceHanSans-Regular_subset | 29113.4 ±1935.2 | — | — | 8462.0 ±693.5 | 3.44 |
| preprocess | SourceHanSans-Regular_subset | 76735.8 ±2060.7 | — | — | 66591.8 ±1292.5 | 1.15 |
| subset (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 133.0 ±4.8 | — | — | 199.3 ±6.2 | 0.67 |
| subset (2322 characters) | NotoNastaliqUrdu-Bold | 303.3 ±6.0 | — | — | 236.1 ±9.1 | 1.28 |
| subset prepared (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 107.1 ±2.4 | — | — | 79.5 ±2.0 | 1.35 |
| subset prepared (2322 characters) | NotoNastaliqUrdu-Bold | 257.8 ±12.6 | — | — | 93.1 ±3.2 | 2.77 |
| preprocess | NotoNastaliqUrdu-Bold | 101.8 ±7.9 | — | — | 20913.7 ±1535.8 | 0.00 |
| subset unicode range 10 | Roboto-Regular.ttf | 80.6 ±2.3 | — | — | 47.5 ±1.7 | 1.70 |
| subset unicode range 10, retain-gids | Roboto-Regular.ttf | 78.2 ±1.7 | — | — | 47.5 ±2.0 | 1.65 |
| subset unicode range 64 | Roboto-Regular.ttf | 229.6 ±8.6 | — | — | 121.1 ±5.4 | 1.90 |
| subset unicode range 64, retain-gids | Roboto-Regular.ttf | 281.8 ±24.9 | — | — | 151.8 ±10.7 | 1.86 |
| subset unicode range 512 | Roboto-Regular.ttf | 1463.0 ±147.6 | — | — | 778.4 ±96.3 | 1.88 |
| subset unicode range 512, retain-gids | Roboto-Regular.ttf | 1337.1 ±55.1 | — | — | 666.9 ±27.2 | 2.00 |
| subset unicode range 1000 | Roboto-Regular.ttf | 2235.7 ±164.6 | — | — | 1090.6 ±95.9 | 2.05 |
| subset unicode range 1000, retain-gids | Roboto-Regular.ttf | 2094.1 ±292.0 | — | — | 1176.5 ±64.4 | 1.78 |
| subset unicode range 10 | Amiri-Regular.ttf | 192.9 ±12.0 | — | — | 116.4 ±9.2 | 1.66 |
| subset unicode range 10, retain-gids | Amiri-Regular.ttf | 187.0 ±5.1 | — | — | 106.2 ±4.1 | 1.76 |
| subset unicode range 64 | Amiri-Regular.ttf | 404.3 ±15.8 | — | — | 190.5 ±5.8 | 2.12 |
| subset unicode range 64, retain-gids | Amiri-Regular.ttf | 603.3 ±21.6 | — | — | 282.3 ±15.8 | 2.14 |
| subset unicode range 512 | Amiri-Regular.ttf | 6280.4 ±231.3 | — | — | 4393.4 ±307.1 | 1.43 |
| subset unicode range 512, retain-gids | Amiri-Regular.ttf | 6932.5 ±298.4 | — | — | 3886.5 ±86.9 | 1.78 |
| subset unicode range 4096 | Amiri-Regular.ttf | 9826.8 ±719.9 | — | — | 6401.6 ±653.4 | 1.54 |
| subset unicode range 4096, retain-gids | Amiri-Regular.ttf | 16129.9 ±2296.5 | — | — | 6762.6 ±1176.5 | 2.39 |
| subset unicode range 10 | NotoNastaliqUrdu-Regular.ttf | 142.4 ±30.8 | — | — | 93.8 ±20.7 | 1.52 |
| subset unicode range 10, retain-gids | NotoNastaliqUrdu-Regular.ttf | 135.7 ±15.8 | — | — | 87.9 ±11.9 | 1.54 |
| subset unicode range 64 | NotoNastaliqUrdu-Regular.ttf | 39465.0 ±3229.7 | — | — | 4300.6 ±601.8 | 9.18 |
| subset unicode range 64, retain-gids | NotoNastaliqUrdu-Regular.ttf | 37527.8 ±2920.5 | — | — | 4653.9 ±498.9 | 8.06 |
| subset unicode range 512 | NotoNastaliqUrdu-Regular.ttf | 116145.2 ±5392.3 | — | — | 53450.3 ±3814.6 | 2.17 |
| subset unicode range 512, retain-gids | NotoNastaliqUrdu-Regular.ttf | 96902.7 ±6158.0 | — | — | 42518.5 ±1199.7 | 2.28 |
| subset unicode range 1400 | NotoNastaliqUrdu-Regular.ttf | 105380.1 ±5253.4 | — | — | 40393.3 ±2773.9 | 2.61 |
| subset unicode range 1400, retain-gids | NotoNastaliqUrdu-Regular.ttf | 105834.8 ±2810.1 | — | — | 42535.8 ±1775.2 | 2.49 |
| subset unicode range 10 | NotoSansDevanagari-Regular.ttf | 94.5 ±1.3 | — | — | 67.1 ±1.5 | 1.41 |
| subset unicode range 10, retain-gids | NotoSansDevanagari-Regular.ttf | 108.4 ±1.7 | — | — | 75.4 ±1.3 | 1.44 |
| subset unicode range 64 | NotoSansDevanagari-Regular.ttf | 228.0 ±9.7 | — | — | 138.8 ±3.5 | 1.64 |
| subset unicode range 64, retain-gids | NotoSansDevanagari-Regular.ttf | 258.3 ±8.6 | — | — | 146.5 ±4.5 | 1.76 |
| subset unicode range 512 | NotoSansDevanagari-Regular.ttf | 4457.1 ±197.7 | — | — | 2270.5 ±113.5 | 1.96 |
| subset unicode range 512, retain-gids | NotoSansDevanagari-Regular.ttf | 4085.6 ±66.6 | — | — | 2073.6 ±46.9 | 1.97 |
| subset unicode range 1000 | NotoSansDevanagari-Regular.ttf | 4291.2 ±132.2 | — | — | 2118.7 ±27.0 | 2.03 |
| subset unicode range 1000, retain-gids | NotoSansDevanagari-Regular.ttf | 4028.0 ±66.0 | — | — | 2202.7 ±43.8 | 1.83 |
| subset unicode range 10 | Mplus1p-Regular.ttf | 65.5 ±0.7 | — | — | 39.9 ±0.9 | 1.64 |
| subset unicode range 10, retain-gids | Mplus1p-Regular.ttf | 64.4 ±2.5 | — | — | 45.3 ±1.3 | 1.42 |
| subset unicode range 64 | Mplus1p-Regular.ttf | 190.9 ±6.2 | — | — | 101.8 ±1.1 | 1.87 |
| subset unicode range 64, retain-gids | Mplus1p-Regular.ttf | 183.1 ±2.2 | — | — | 102.0 ±1.9 | 1.80 |
| subset unicode range 512 | Mplus1p-Regular.ttf | 717.8 ±29.7 | — | — | 315.9 ±7.9 | 2.27 |
| subset unicode range 512, retain-gids | Mplus1p-Regular.ttf | 989.2 ±16.4 | — | — | 444.3 ±15.3 | 2.23 |
| subset unicode range 4096 | Mplus1p-Regular.ttf | 6003.9 ±570.4 | — | — | 2523.6 ±72.1 | 2.38 |
| subset unicode range 4096, retain-gids | Mplus1p-Regular.ttf | 5637.6 ±538.1 | — | — | 2713.6 ±86.1 | 2.08 |
| subset unicode range 10000 | Mplus1p-Regular.ttf | 11527.2 ±165.3 | — | — | 4923.8 ±134.1 | 2.34 |
| subset unicode range 10000, retain-gids | Mplus1p-Regular.ttf | 11501.7 ±223.7 | — | — | 4946.0 ±158.8 | 2.33 |
| subset unicode range 10 | SourceHanSans-Regular_subset.otf | 206.3 ±10.1 | — | — | 66.0 ±3.8 | 3.12 |
| subset unicode range 10, retain-gids | SourceHanSans-Regular_subset.otf | 43719.8 ±1193.5 | — | — | 310.2 ±4.7 | 140.95 |
| subset unicode range 64 | SourceHanSans-Regular_subset.otf | 390.6 ±5.1 | — | — | 119.5 ±5.6 | 3.27 |
| subset unicode range 64, retain-gids | SourceHanSans-Regular_subset.otf | 43839.1 ±1015.4 | — | — | 385.0 ±15.1 | 113.86 |
| subset unicode range 512 | SourceHanSans-Regular_subset.otf | 2316.9 ±33.0 | — | — | 824.1 ±12.2 | 2.81 |
| subset unicode range 512, retain-gids | SourceHanSans-Regular_subset.otf | 43979.7 ±758.5 | — | — | 1068.4 ±19.5 | 41.16 |
| subset unicode range 4096 | SourceHanSans-Regular_subset.otf | 48238.6 ±844.0 | — | — | 11939.1 ±436.6 | 4.04 |
| subset unicode range 4096, retain-gids | SourceHanSans-Regular_subset.otf | 72363.5 ±6776.5 | — | — | 12078.3 ±450.6 | 5.99 |
| subset unicode range 10000 | SourceHanSans-Regular_subset.otf | 69452.4 ±1301.4 | — | — | 15653.0 ±516.5 | 4.44 |
| subset unicode range 10000, retain-gids | SourceHanSans-Regular_subset.otf | 72216.9 ±2495.1 | — | — | 16998.5 ±380.0 | 4.25 |
| subset unicode range 10 | SourceSansPro-Regular.otf | 137.9 ±1.2 | — | — | 65.4 ±0.7 | 2.11 |
| subset unicode range 10, retain-gids | SourceSansPro-Regular.otf | 1058.7 ±10.6 | — | — | 162.6 ±2.2 | 6.51 |
| subset unicode range 64 | SourceSansPro-Regular.otf | 395.9 ±16.5 | — | — | 172.4 ±5.5 | 2.30 |
| subset unicode range 64, retain-gids | SourceSansPro-Regular.otf | 1536.8 ±43.6 | — | — | 333.0 ±37.0 | 4.61 |
| subset unicode range 512 | SourceSansPro-Regular.otf | 1883.3 ±17.4 | — | — | 792.8 ±16.3 | 2.38 |
| subset unicode range 512, retain-gids | SourceSansPro-Regular.otf | 2705.6 ±26.4 | — | — | 873.0 ±12.4 | 3.10 |
| subset unicode range 2000 | SourceSansPro-Regular.otf | 4290.8 ±147.3 | — | — | 1682.5 ±17.8 | 2.55 |
| subset unicode range 2000, retain-gids | SourceSansPro-Regular.otf | 4829.6 ±101.0 | — | — | 1737.3 ±31.3 | 2.78 |
