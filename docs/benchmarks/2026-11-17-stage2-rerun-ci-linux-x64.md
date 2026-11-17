# stage2-rerun の計測（CI、linux-x64、2026-11-17）

[ADR 0080](../adr/0080-ci-benchmark-records.md) のワークフローで取った記録である。
時間は1回あたりの中央値（±中央絶対偏差）、単位はマイクロ秒で、比が1未満なら sofel が速い。

## 環境

| 項目 | 値 |
| --- | --- |
| ランナーのイメージと版 | ubuntu26 20261111.196.1 |
| OS の名前と版 | Ubuntu 26.04.1 LTS |
| CPU の名前 | AMD EPYC 9V74 80-Core Processor |
| 論理 CPU の数 | 4 |
| メモリ | 16366636 kB |
| 計測の直前の負荷平均 |  11:10:21 up 4 min,  0 users,  load average: 3.90, 1.72, 0.68 |
| Rust の版 | rustc 1.99.1 (58cf8f9c3 2026-10-13) |
| C と C++ のコンパイラの版 | cc: cc (Ubuntu 15.2.0-16ubuntu1) 15.2.0; c++: c++ (Ubuntu 15.2.0-16ubuntu1) 15.2.0 |
| wasmtime の版 | wasmtime 50.0.2 (f9a93b049 2026-11-16) |
| wasi-sdk の版 | wasi-sdk-34 |
| FreeType と HarfBuzz の版 | harfbuzz 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372, freetype 37515cf56ba3349656049edbed89ffc6f6466f2d |
| sofel のコミット | 72512b48adbac8367592d5cddab095d63f89e706 |
| 実行の URL | https://github.com/tett23/sofel/actions/runs/44070967915 |

## ネイティブ

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location wght=500 | RobotoFlex-Variable | 14.2 ±0.0 | 0.0 ±0.0 | 998.00 | 0.4 ±0.0 | 38.74 |
| box wght=500 | RobotoFlex-Variable | 5196.3 ±7.2 | — | — | 1428.3 ±10.9 | 3.64 |
| location with reference queries wght=499/500 | RobotoFlex-Variable | 14.6 ±0.0 | 1.4 ±0.0 | 10.69 | 2.4 ±0.0 | 6.08 |
| advance wght=500 | RobotoFlex-Variable | 108.0 ±0.3 | 45.6 ±0.1 | 2.37 | 63.8 ±0.1 | 1.69 |
| outline unscaled wght=500 | RobotoFlex-Variable | 4620.0 ±10.9 | 1074.4 ±7.2 | 4.30 | 870.7 ±7.8 | 5.31 |
| outline 48px wght=500 | RobotoFlex-Variable | 4703.5 ±5.5 | 1169.2 ±6.4 | 4.02 | 868.6 ±1.9 | 5.41 |
| raster 24px wght=500 | RobotoFlex-Variable | 12086.2 ±19.3 | 7534.8 ±7.7 | 1.60 | — | — |
| location wght=500 | SourceSerifVariable-Roman | 1.1 ±0.0 | 0.0 ±0.0 | 137.03 | 0.1 ±0.0 | 18.15 |
| box wght=500 | SourceSerifVariable-Roman | 3257.9 ±5.4 | — | — | 819.3 ±2.0 | 3.98 |
| location with reference queries wght=499/500 | SourceSerifVariable-Roman | 1.0 ±0.0 | 0.6 ±0.0 | 1.72 | 1.2 ±0.0 | 0.85 |
| advance wght=500 | SourceSerifVariable-Roman | 103.4 ±0.1 | 33.3 ±0.0 | 3.11 | 45.9 ±0.1 | 2.25 |
| outline unscaled wght=500 | SourceSerifVariable-Roman | 3318.9 ±9.1 | 1541.1 ±3.2 | 2.15 | 949.6 ±1.8 | 3.50 |
| outline 48px wght=500 | SourceSerifVariable-Roman | 3489.7 ±7.8 | 1736.1 ±2.7 | 2.01 | 956.0 ±2.0 | 3.65 |
| raster 24px wght=500 | SourceSerifVariable-Roman | 6631.4 ±10.2 | 5080.1 ±4.9 | 1.31 | — | — |
| location wght=500 | AdobeVFPrototype | 14.7 ±0.0 | 0.0 ±0.0 | 1688.30 | 0.1 ±0.0 | 174.13 |
| box wght=500 | AdobeVFPrototype | 1183.0 ±2.6 | — | — | 1195.5 ±2.3 | 0.99 |
| location with reference queries wght=499/500 | AdobeVFPrototype | 14.8 ±0.0 | 5.2 ±0.0 | 2.86 | 3.6 ±0.0 | 4.09 |
| advance wght=500 | AdobeVFPrototype | 21.7 ±0.0 | 7.5 ±0.0 | 2.91 | 6.0 ±0.0 | 3.61 |
| outline unscaled wght=500 | AdobeVFPrototype | 4380.4 ±7.7 | 1334.0 ±1.3 | 3.28 | 1177.4 ±1.3 | 3.72 |
| outline 48px wght=500 | AdobeVFPrototype | 4396.9 ±6.9 | 1371.4 ±1.7 | 3.21 | 1172.9 ±1.0 | 3.75 |
| raster 24px wght=500 | AdobeVFPrototype | 5109.9 ±6.2 | 2130.9 ±1.9 | 2.40 | — | — |
| Unicode properties (1682970 characters) | perf/texts | 12583.6 ±6.8 | — | — | 16071.4 ±8.1 | 0.78 |
| cmap | extra/arabic | 0.1 ±0.0 | 0.1 ±0.0 | 1.15 | 0.1 ±0.0 | 1.00 |
| advance | extra/arabic | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/arabic | 56.3 ±0.1 | 45.5 ±0.1 | 1.24 | — | — |
| cmap | extra/devanagari | 0.1 ±0.0 | 0.1 ±0.0 | 1.18 | 0.1 ±0.0 | 1.00 |
| advance | extra/devanagari | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/devanagari | 64.5 ±0.1 | 50.7 ±0.1 | 1.27 | — | — |
| cmap | extra/khmer | 0.1 ±0.0 | 0.1 ±0.0 | 1.15 | 0.1 ±0.0 | 1.11 |
| advance | extra/khmer | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/khmer | 60.0 ±0.2 | 45.8 ±0.1 | 1.31 | — | — |
| cmap | extra/myanmar | 0.1 ±0.0 | 0.1 ±0.0 | 1.09 | 0.1 ±0.0 | 1.10 |
| advance | extra/myanmar | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/myanmar | 69.7 ±0.1 | 52.7 ±0.1 | 1.32 | — | — |
| cmap | extra/balinese | 0.1 ±0.0 | 0.1 ±0.0 | 1.14 | 0.1 ±0.0 | 1.07 |
| advance | extra/balinese | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/balinese | 91.2 ±0.1 | 68.4 ±0.1 | 1.33 | — | — |
| cmap | extra/hebrew | 0.1 ±0.0 | 0.1 ±0.0 | 1.15 | 0.1 ±0.0 | 1.08 |
| advance | extra/hebrew | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/hebrew | 39.3 ±0.1 | 34.7 ±0.1 | 1.13 | — | — |
| cmap | extra/source-serif | 0.1 ±0.0 | 0.1 ±0.0 | 1.14 | 0.1 ±0.0 | 1.02 |
| advance | extra/source-serif | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/source-serif | 2768.6 ±7.1 | 789.4 ±2.8 | 3.51 | — | — |
| cmap | extra/stix-math | 0.2 ±0.0 | 0.1 ±0.0 | 1.08 | 0.2 ±0.0 | 0.94 |
| advance | extra/stix-math | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/stix-math | 233.9 ±0.2 | 325.1 ±0.2 | 0.72 | — | — |
| MATH constants | extra/stix-math | 0.1 ±0.0 | — | — | 0.3 ±0.0 | 0.39 |
| cmap | extra/source-han | 0.2 ±0.0 | 0.2 ±0.0 | 1.13 | 0.2 ±0.0 | 1.15 |
| advance | extra/source-han | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/source-han | 257.9 ±0.4 | 192.6 ±0.2 | 1.34 | — | — |
| vertical advance | extra/source-han | 0.8 ±0.0 | — | — | 0.9 ±0.0 | 0.86 |
| cmap | extra/amstelvar | 0.1 ±0.0 | 0.1 ±0.0 | 1.06 | 0.1 ±0.0 | 0.89 |
| advance | extra/amstelvar | 0.1 ±0.0 | 1.8 ±0.0 | 0.05 | 0.8 ±0.0 | 0.11 |
| outline unscaled sampled | extra/amstelvar | 74.3 ±0.1 | 62.5 ±0.1 | 1.19 | — | — |
| cmap | extra/color-cff | 0.1 ±0.0 | 0.1 ±0.0 | 1.37 | 0.1 ±0.0 | 1.06 |
| advance | extra/color-cff | 0.0 ±0.0 | 0.4 ±0.0 | 0.06 | 0.2 ±0.0 | 0.14 |
| outline unscaled sampled | extra/color-cff | 14.5 ±0.0 | 12.2 ±0.0 | 1.19 | — | — |
| cmap | extra/color-sbix | 0.1 ±0.0 | 0.1 ±0.0 | 1.37 | 0.1 ±0.0 | 1.06 |
| advance | extra/color-sbix | 0.0 ±0.0 | 0.4 ±0.0 | 0.06 | 0.2 ±0.0 | 0.13 |
| outline unscaled sampled | extra/color-sbix | 1.4 ±0.0 | 1.9 ±0.0 | 0.73 | — | — |
| cmap | extra/color-svg | 0.1 ±0.0 | 0.1 ±0.0 | 1.37 | 0.1 ±0.0 | 1.05 |
| advance | extra/color-svg | 0.0 ±0.0 | 0.4 ±0.0 | 0.06 | 0.2 ±0.0 | 0.13 |
| outline unscaled sampled | extra/color-svg | 1.4 ±0.0 | 2.0 ±0.0 | 0.70 | — | — |
| outline unscaled | SourceSansPro-Regular | 4165.9 ±4.7 | 4257.4 ±2.8 | 0.98 | 1607.3 ±1.0 | 2.59 |
| outline 48px | SourceSansPro-Regular | 4275.4 ±7.5 | 4464.4 ±5.0 | 0.96 | 1613.7 ±5.7 | 2.65 |
| raster 12px | SourceSansPro-Regular | 6379.4 ±6.2 | 6467.2 ±6.1 | 0.99 | — | — |
| raster 24px | SourceSansPro-Regular | 7652.6 ±11.6 | 7547.8 ±13.7 | 1.01 | — | — |
| raster 48px | SourceSansPro-Regular | 10444.0 ±15.4 | 9921.1 ±13.0 | 1.05 | — | — |
| raster 96px | SourceSansPro-Regular | 16675.0 ±23.9 | 13967.7 ±17.2 | 1.19 | — | — |
| open (FreeType) | SourceSansPro-Regular | 8.7 ±0.0 | 22.2 ±0.0 | 0.39 | — | — |
| glyph extents | SourceSansPro-Regular | 1638.5 ±1.0 | — | — | 1672.1 ±4.3 | 0.98 |
| open (HarfBuzz) | SourceSansPro-Regular | 1.3 ±0.0 | — | — | 2.1 ±0.0 | 0.63 |
| glyph names | SourceSansPro-Regular | 31.5 ±0.1 | — | — | 153.4 ±0.1 | 0.21 |
| glyph from name | SourceSansPro-Regular | 837.6 ±0.8 | — | — | 320.8 ±1.2 | 2.61 |
| outline unscaled | NotoSerifMyanmar-Regular | 1915.3 ±1.7 | 1832.1 ±1.7 | 1.05 | 867.0 ±2.1 | 2.21 |
| outline 48px | NotoSerifMyanmar-Regular | 1974.9 ±5.0 | 1952.7 ±2.6 | 1.01 | 882.7 ±1.1 | 2.24 |
| raster 12px | NotoSerifMyanmar-Regular | 3425.4 ±9.7 | 3446.3 ±7.1 | 0.99 | — | — |
| raster 24px | NotoSerifMyanmar-Regular | 4316.4 ±6.0 | 4354.5 ±8.3 | 0.99 | — | — |
| raster 48px | NotoSerifMyanmar-Regular | 6457.3 ±9.4 | 6381.5 ±8.0 | 1.01 | — | — |
| raster 96px | NotoSerifMyanmar-Regular | 11396.2 ±13.1 | 9959.7 ±12.4 | 1.14 | — | — |
| open (FreeType) | NotoSerifMyanmar-Regular | 4.9 ±0.0 | 11.6 ±0.0 | 0.42 | — | — |
| glyph extents | NotoSerifMyanmar-Regular | 880.4 ±1.9 | — | — | 953.4 ±2.7 | 0.92 |
| open (HarfBuzz) | NotoSerifMyanmar-Regular | 0.9 ±0.0 | — | — | 1.9 ±0.0 | 0.51 |
| glyph names | NotoSerifMyanmar-Regular | 13.0 ±0.1 | — | — | 11.0 ±0.0 | 1.18 |
| glyph from name | NotoSerifMyanmar-Regular | 136.7 ±0.1 | — | — | 32.9 ±0.0 | 4.15 |
| outline unscaled | AdobeVFPrototype | 4428.6 ±9.4 | 1334.8 ±1.5 | 3.32 | 1130.6 ±1.4 | 3.92 |
| outline 48px | AdobeVFPrototype | 4429.3 ±5.3 | 1378.3 ±1.2 | 3.21 | 1123.9 ±2.3 | 3.94 |
| raster 12px | AdobeVFPrototype | 4868.9 ±13.9 | 1812.4 ±5.3 | 2.69 | — | — |
| raster 24px | AdobeVFPrototype | 5116.6 ±10.2 | 2115.4 ±3.2 | 2.42 | — | — |
| raster 48px | AdobeVFPrototype | 5700.7 ±7.6 | 2692.0 ±3.9 | 2.12 | — | — |
| raster 96px | AdobeVFPrototype | 6887.0 ±8.3 | 3662.4 ±5.9 | 1.88 | — | — |
| open (FreeType) | AdobeVFPrototype | 6.5 ±0.0 | 11.2 ±0.0 | 0.58 | — | — |
| glyph extents | AdobeVFPrototype | 1094.8 ±1.4 | — | — | 1206.4 ±7.2 | 0.91 |
| open (HarfBuzz) | AdobeVFPrototype | 2.1 ±0.0 | — | — | 2.4 ±0.0 | 0.89 |
| outline unscaled | SourceHanSans-Regular_subset | 48231.4 ±35.1 | 56345.4 ±65.1 | 0.86 | 20861.7 ±50.8 | 2.31 |
| outline 48px | SourceHanSans-Regular_subset | 49751.5 ±58.0 | 58557.3 ±59.4 | 0.85 | 20437.4 ±28.8 | 2.43 |
| raster 12px | SourceHanSans-Regular_subset | 76493.5 ±71.6 | 89120.2 ±173.5 | 0.86 | — | — |
| raster 24px | SourceHanSans-Regular_subset | 96865.4 ±60.9 | 111385.4 ±115.8 | 0.87 | — | — |
| raster 48px | SourceHanSans-Regular_subset | 138936.3 ±141.9 | 150451.1 ±134.2 | 0.92 | — | — |
| raster 96px | SourceHanSans-Regular_subset | 233886.0 ±505.9 | 222001.7 ±118.2 | 1.05 | — | — |
| cid | SourceHanSans-Regular_subset | 194.0 ±0.2 | 347.6 ±0.6 | 0.56 | — | — |
| cid (no index) | SourceHanSans-Regular_subset | 116672.4 ±269.7 | 347.9 ±0.3 | 335.40 | — | — |
| open (FreeType) | SourceHanSans-Regular_subset | 58.6 ±0.1 | 76.7 ±0.2 | 0.76 | — | — |
| glyph extents | SourceHanSans-Regular_subset | 23522.2 ±71.8 | — | — | 21711.3 ±49.4 | 1.08 |
| open (HarfBuzz) | SourceHanSans-Regular_subset | 6.7 ±0.0 | — | — | 6.2 ±0.0 | 1.08 |
| glyph color format | extra/color-cff | 2.0 ±0.0 | — | — | 0.3 ±0.0 | 6.19 |
| COLRv1 paint | extra/color-cff | 4.5 ±0.0 | 0.8 ±0.0 | 5.35 | 1.8 ±0.0 | 2.45 |
| COLRv1 recording | extra/color-cff | 4.8 ±0.0 | — | — | — | — |
| color glyph extents | extra/color-cff | 11.0 ±0.0 | — | — | 3.1 ±0.0 | 3.56 |
| glyph color format | extra/color-sbix | 1.8 ±0.0 | — | — | 0.3 ±0.0 | 7.22 |
| bitmap data 16 ppem | extra/color-sbix | 4.3 ±0.0 | — | — | 2.7 ±0.0 | 1.59 |
| bitmap data 109 ppem | extra/color-sbix | 4.3 ±0.0 | — | — | 2.7 ±0.0 | 1.59 |
| bitmap data 136 ppem | extra/color-sbix | 4.5 ±0.0 | — | — | 3.0 ±0.0 | 1.48 |
| bitmap data 0 ppem | extra/color-sbix | 4.5 ±0.0 | — | — | 2.9 ±0.0 | 1.52 |
| color glyph extents | extra/color-sbix | 2.4 ±0.0 | — | — | 1.5 ±0.0 | 1.59 |
| glyph color format | extra/color-svg | 1.2 ±0.0 | — | — | 0.3 ±0.0 | 4.56 |
| SVG documents | extra/color-svg | 0.8 ±0.0 | — | — | 0.4 ±0.0 | 2.26 |
| color glyph extents | extra/color-svg | 1.5 ±0.0 | — | — | 0.7 ±0.0 | 2.09 |
| glyph color format | TwemojiMozilla.subset | 0.4 ±0.0 | — | — | 0.1 ±0.0 | 6.54 |
| COLRv0 layers | TwemojiMozilla.subset | 0.2 ±0.0 | 0.1 ±0.0 | 4.30 | 0.1 ±0.0 | 2.11 |
| COLRv0 raster 12px | TwemojiMozilla.subset | 28.0 ±0.0 | 16.9 ±0.0 | 1.66 | — | — |
| COLRv0 raster 24px | TwemojiMozilla.subset | 42.4 ±0.1 | 26.7 ±0.0 | 1.59 | — | — |
| COLRv0 raster 48px | TwemojiMozilla.subset | 91.5 ±0.1 | 56.9 ±0.1 | 1.61 | — | — |
| COLRv0 paint | TwemojiMozilla.subset | 0.6 ±0.0 | — | — | 0.2 ±0.0 | 2.83 |
| color glyph extents | TwemojiMozilla.subset | 5.8 ±0.0 | — | — | 2.8 ±0.0 | 2.04 |
| glyph color format | test_glyphs-glyf_colr_1 | 14.3 ±0.0 | — | — | 3.8 ±0.0 | 3.75 |
| COLRv0 layers | test_glyphs-glyf_colr_1 | 7.1 ±0.0 | 1.2 ±0.0 | 5.78 | 2.9 ±0.0 | 2.44 |
| COLRv0 raster 12px | test_glyphs-glyf_colr_1 | 125.7 ±0.1 | 102.2 ±0.1 | 1.23 | — | — |
| COLRv0 raster 24px | test_glyphs-glyf_colr_1 | 185.5 ±0.2 | 172.7 ±0.2 | 1.07 | — | — |
| COLRv0 raster 48px | test_glyphs-glyf_colr_1 | 347.5 ±0.3 | 296.6 ±0.3 | 1.17 | — | — |
| COLRv1 paint | test_glyphs-glyf_colr_1 | 58.2 ±0.1 | 14.4 ±0.0 | 4.05 | 27.2 ±0.0 | 2.14 |
| COLRv1 recording | test_glyphs-glyf_colr_1 | 61.8 ±0.1 | — | — | — | — |
| color glyph extents | test_glyphs-glyf_colr_1 | 61.5 ±0.1 | — | — | 29.9 ±0.0 | 2.06 |
| glyph color format | noto_handwriting-glyf_colr_1 | 1.5 ±0.0 | — | — | 0.2 ±0.0 | 5.91 |
| COLRv1 paint | noto_handwriting-glyf_colr_1 | 6.7 ±0.0 | 2.0 ±0.0 | 3.39 | 2.9 ±0.0 | 2.33 |
| COLRv1 recording | noto_handwriting-glyf_colr_1 | 7.2 ±0.0 | — | — | — | — |
| color glyph extents | noto_handwriting-glyf_colr_1 | 7.8 ±0.0 | — | — | 1.3 ±0.0 | 6.05 |
| glyph color format | NotoColorEmoji.subset | 1.7 ±0.0 | — | — | 0.1 ±0.0 | 30.07 |
| bitmap data 16 ppem | NotoColorEmoji.subset | 2.0 ±0.0 | — | — | 0.6 ±0.0 | 3.04 |
| bitmap data 109 ppem | NotoColorEmoji.subset | 1.9 ±0.0 | — | — | 0.6 ±0.0 | 3.02 |
| bitmap data 136 ppem | NotoColorEmoji.subset | 2.0 ±0.0 | — | — | 0.7 ±0.0 | 2.79 |
| bitmap data 0 ppem | NotoColorEmoji.subset | 1.9 ±0.0 | — | — | 0.7 ±0.0 | 2.88 |
| color glyph extents | NotoColorEmoji.subset | 0.8 ±0.0 | — | — | 0.5 ±0.0 | 1.70 |
| glyph color format | noto_handwriting-sbix | 0.9 ±0.0 | — | — | 0.1 ±0.0 | 7.25 |
| bitmap data 16 ppem | noto_handwriting-sbix | 2.3 ±0.0 | — | — | 1.4 ±0.0 | 1.67 |
| bitmap data 109 ppem | noto_handwriting-sbix | 2.3 ±0.0 | — | — | 1.4 ±0.0 | 1.67 |
| bitmap data 136 ppem | noto_handwriting-sbix | 2.4 ±0.0 | — | — | 1.5 ±0.0 | 1.54 |
| bitmap data 0 ppem | noto_handwriting-sbix | 2.4 ±0.0 | — | — | 1.5 ±0.0 | 1.57 |
| color glyph extents | noto_handwriting-sbix | 1.2 ±0.0 | — | — | 0.7 ±0.0 | 1.62 |
| glyph color format | chromacheck-svg | 0.1 ±0.0 | — | — | 0.0 ±0.0 | 3.97 |
| SVG documents | chromacheck-svg | 0.1 ±0.0 | — | — | 0.0 ±0.0 | 2.13 |
| color glyph extents | chromacheck-svg | 0.2 ±0.0 | — | — | 0.1 ±0.0 | 2.31 |
| bitmap pixels 16 ppem | EBDT (built, 200 glyphs) | 71.0 ±0.8 | 49.9 ±0.1 | 1.42 | — | — |
| cmap | Roboto-Regular | 3593.5 ±5.7 | 3132.1 ±7.0 | 1.15 | 4249.5 ±3.7 | 0.85 |
| advance | Roboto-Regular | 2.8 ±0.0 | 46.8 ±0.1 | 0.06 | 21.3 ±0.0 | 0.13 |
| v advance | Roboto-Regular | 96.4 ±0.2 | — | — | 156.8 ±0.1 | 0.61 |
| v origin | Roboto-Regular | 463.4 ±0.5 | — | — | 301.9 ±0.4 | 1.53 |
| outline unscaled | Roboto-Regular | 1547.4 ±3.1 | 1500.9 ±4.6 | 1.03 | 1275.3 ±2.2 | 1.21 |
| outline 48px | Roboto-Regular | 1659.0 ±3.0 | 1724.5 ±2.0 | 0.96 | 1274.0 ±1.3 | 1.30 |
| raster 12px | Roboto-Regular | 5007.2 ±5.5 | 5124.2 ±6.5 | 0.98 | — | — |
| raster 24px | Roboto-Regular | 7567.5 ±12.1 | 7487.7 ±9.0 | 1.01 | — | — |
| raster 48px | Roboto-Regular | 12085.6 ±17.5 | 11583.5 ±17.7 | 1.04 | — | — |
| raster 96px | Roboto-Regular | 23791.4 ±13.3 | 18684.4 ±20.0 | 1.27 | — | — |
| cmap | Comfortaa-Regular-new | 3815.7 ±10.7 | 3417.1 ±7.0 | 1.12 | 3950.7 ±9.7 | 0.97 |
| advance | Comfortaa-Regular-new | 0.7 ±0.0 | 12.2 ±0.0 | 0.06 | 5.6 ±0.0 | 0.13 |
| v advance | Comfortaa-Regular-new | 27.1 ±0.0 | — | — | 33.3 ±0.0 | 0.81 |
| v origin | Comfortaa-Regular-new | 128.2 ±0.2 | — | — | 73.5 ±0.1 | 1.74 |
| outline unscaled | Comfortaa-Regular-new | 569.9 ±1.1 | 456.5 ±1.3 | 1.25 | 390.0 ±0.9 | 1.46 |
| outline 48px | Comfortaa-Regular-new | 623.9 ±1.3 | 558.1 ±1.3 | 1.12 | 394.6 ±1.3 | 1.58 |
| raster 12px | Comfortaa-Regular-new | 1767.2 ±3.3 | 1665.0 ±1.8 | 1.06 | — | — |
| raster 24px | Comfortaa-Regular-new | 2536.3 ±2.4 | 2439.4 ±3.5 | 1.04 | — | — |
| raster 48px | Comfortaa-Regular-new | 4162.7 ±4.0 | 3880.8 ±5.1 | 1.07 | — | — |
| raster 96px | Comfortaa-Regular-new | 8044.3 ±7.6 | 6531.5 ±9.1 | 1.23 | — | — |
| cmap | NotoNastaliqUrdu-Regular | 3784.8 ±7.0 | 3332.2 ±7.2 | 1.14 | 3719.9 ±20.0 | 1.02 |
| advance | NotoNastaliqUrdu-Regular | 0.9 ±0.0 | 15.8 ±0.0 | 0.06 | 7.2 ±0.0 | 0.13 |
| v advance | NotoNastaliqUrdu-Regular | 32.5 ±0.0 | — | — | 52.9 ±0.1 | 0.61 |
| v origin | NotoNastaliqUrdu-Regular | 155.7 ±0.1 | — | — | 101.0 ±0.1 | 1.54 |
| outline unscaled | NotoNastaliqUrdu-Regular | 759.6 ±1.1 | 652.9 ±1.3 | 1.16 | 581.0 ±0.6 | 1.31 |
| outline 48px | NotoNastaliqUrdu-Regular | 822.8 ±1.0 | 771.9 ±1.9 | 1.07 | 584.2 ±1.3 | 1.41 |
| raster 12px | NotoNastaliqUrdu-Regular | 2436.8 ±2.3 | 2369.8 ±1.9 | 1.03 | — | — |
| raster 24px | NotoNastaliqUrdu-Regular | 3302.7 ±3.7 | 3360.5 ±2.3 | 0.98 | — | — |
| raster 48px | NotoNastaliqUrdu-Regular | 5272.0 ±7.3 | 5247.1 ±4.8 | 1.00 | — | — |
| raster 96px | NotoNastaliqUrdu-Regular | 9914.9 ±6.7 | 8665.2 ±13.7 | 1.14 | — | — |
| cmap | RobotoFlex-Variable | 3820.1 ±9.8 | 3391.0 ±8.2 | 1.13 | 3977.1 ±12.1 | 0.96 |
| advance | RobotoFlex-Variable | 0.8 ±0.0 | 13.4 ±0.1 | 0.06 | 6.0 ±0.0 | 0.13 |
| v advance | RobotoFlex-Variable | 29.2 ±0.0 | — | — | 35.9 ±0.0 | 0.81 |
| v origin | RobotoFlex-Variable | 137.5 ±0.1 | — | — | 78.9 ±0.1 | 1.74 |
| outline unscaled | RobotoFlex-Variable | 408.1 ±0.9 | 346.9 ±3.0 | 1.18 | 295.8 ±1.3 | 1.38 |
| outline 48px | RobotoFlex-Variable | 448.2 ±1.3 | 428.0 ±1.6 | 1.05 | 300.1 ±0.7 | 1.49 |
| raster 12px | RobotoFlex-Variable | 3759.7 ±1.7 | 3596.1 ±5.3 | 1.05 | — | — |
| raster 24px | RobotoFlex-Variable | 7338.5 ±5.3 | 6416.4 ±5.6 | 1.14 | — | — |
| raster 48px | RobotoFlex-Variable | 17607.0 ±23.7 | 13044.3 ±14.4 | 1.35 | — | — |
| raster 96px | RobotoFlex-Variable | 51343.2 ±64.8 | 31787.1 ±48.6 | 1.62 | — | — |
| cmap | SourceSerifVariable-Roman | 3846.8 ±5.7 | 3432.9 ±4.5 | 1.12 | 4507.2 ±8.0 | 0.85 |
| advance | SourceSerifVariable-Roman | 1.2 ±0.0 | 19.9 ±0.0 | 0.06 | 9.1 ±0.0 | 0.13 |
| v advance | SourceSerifVariable-Roman | 41.1 ±0.1 | — | — | 70.3 ±0.2 | 0.58 |
| v origin | SourceSerifVariable-Roman | 197.8 ±0.1 | — | — | 135.0 ±0.4 | 1.47 |
| outline unscaled | SourceSerifVariable-Roman | 963.7 ±1.6 | 799.6 ±1.6 | 1.21 | 681.3 ±1.9 | 1.41 |
| outline 48px | SourceSerifVariable-Roman | 1032.7 ±2.1 | 951.4 ±1.7 | 1.09 | 676.4 ±1.5 | 1.53 |
| raster 12px | SourceSerifVariable-Roman | 2854.1 ±4.8 | 2985.7 ±37.6 | 0.96 | — | — |
| raster 24px | SourceSerifVariable-Roman | 4008.1 ±8.1 | 4161.2 ±5.4 | 0.96 | — | — |
| raster 48px | SourceSerifVariable-Roman | 6471.3 ±8.5 | 6653.4 ±10.3 | 0.97 | — | — |
| raster 96px | SourceSerifVariable-Roman | 11783.4 ±10.9 | 11057.6 ±9.6 | 1.07 | — | — |
| HVAR item delta cached | RobotoFlex-Variable.ttf | 104.4 ±0.1 | — | — | — | — |
| HVAR item delta copied font | RobotoFlex-Variable.ttf | 635.3 ±0.6 | — | — | — | — |
| HVAR item delta different font | RobotoFlex-Variable.ttf | 635.2 ±1.4 | — | — | — | — |
| HVAR item delta cached | SourceSerifVariable-Roman.ttf | 83.4 ±0.1 | — | — | — | — |
| HVAR item delta copied font | SourceSerifVariable-Roman.ttf | 134.6 ±0.1 | — | — | — | — |
| HVAR item delta different font | SourceSerifVariable-Roman.ttf | 134.4 ±0.2 | — | — | — | — |
| MVAR item delta static default | SourceSerifVariable-Roman.ttf | 0.1 ±0.0 | — | — | — | — |
| HVAR item delta cached | AdobeVFPrototype.otf | 18.1 ±0.0 | — | — | — | — |
| HVAR item delta copied font | AdobeVFPrototype.otf | 31.3 ±0.1 | — | — | — | — |
| HVAR item delta different font | AdobeVFPrototype.otf | 31.3 ±0.1 | — | — | — | — |
| MVAR item delta static default | AdobeVFPrototype.otf | 0.2 ±0.0 | — | — | — | — |
| shaping en-thelittleprince.txt | Roboto-Regular | 34128.8 ±33.5 | — | — | 5790.0 ±5.6 | 5.89 |
| first shaping en-thelittleprince.txt | Roboto-Regular | 72.8 ±0.1 | — | — | 62.6 ±0.1 | 1.16 |
| shaping en-words.txt | Roboto-Regular | 41946.9 ±36.5 | — | — | 8148.4 ±11.8 | 5.15 |
| first shaping en-words.txt | Roboto-Regular | 48.6 ±0.1 | — | — | 57.4 ±0.1 | 0.85 |
| shaping react-dom.txt | SourceSerifVariable-Roman | 248383.3 ±319.1 | — | — | 79545.0 ±76.9 | 3.12 |
| first shaping react-dom.txt | SourceSerifVariable-Roman | 22.5 ±0.0 | — | — | 15.4 ±0.0 | 1.46 |
| shaping hi-words.txt | NotoSansDevanagari-Regular | 50737.8 ±43.5 | — | — | 25414.2 ±50.8 | 2.00 |
| first shaping hi-words.txt | NotoSansDevanagari-Regular | 57.6 ±0.1 | — | — | 40.5 ±0.1 | 1.42 |
| shaping fa-thelittleprince.txt | Amiri-Regular | 82621.1 ±195.8 | — | — | 43915.9 ±69.7 | 1.88 |
| first shaping fa-thelittleprince.txt | Amiri-Regular | 139.3 ±0.3 | — | — | 43.2 ±0.0 | 3.22 |
| shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 289398.4 ±429.9 | — | — | 65743.8 ±210.3 | 4.40 |
| first shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 271.6 ±0.3 | — | — | 1389.0 ±1.5 | 0.20 |
| shaping fa-words.txt | NotoNastaliqUrdu-Regular | 314676.7 ±345.4 | — | — | 81016.5 ±97.1 | 3.88 |
| first shaping fa-words.txt | NotoNastaliqUrdu-Regular | 238.7 ±0.4 | — | — | 1379.3 ±2.3 | 0.17 |
| shaping fa-thelittleprince.txt | Gulzar-Regular | 5647528.6 ±19553.4 | — | — | 932587.0 ±1137.3 | 6.06 |
| first shaping fa-thelittleprince.txt | Gulzar-Regular | 2109.9 ±6.5 | — | — | 1623.9 ±1.5 | 1.30 |
| shaping fa-words.txt | Gulzar-Regular | 4089826.5 ±8262.6 | — | — | 691938.7 ±402.2 | 5.91 |
| first shaping fa-words.txt | Gulzar-Regular | 1308.2 ±1.9 | — | — | 1387.6 ±3.4 | 0.94 |
| shaping duployan.txt | NotoSansDuployan-Regular | 9107577.2 ±10916.6 | — | — | 1662390.3 ±6985.1 | 5.48 |
| first shaping duployan.txt | NotoSansDuployan-Regular | 32099.2 ±88.5 | — | — | 6338.4 ±18.1 | 5.06 |
| shaping ja-runs.txt | Mplus1p-Regular | 26945.0 ±18.7 | — | — | 15342.0 ±12.5 | 1.76 |
| first shaping ja-runs.txt | Mplus1p-Regular | 15.6 ±0.0 | — | — | 17.2 ±0.0 | 0.91 |
| shaping ja-runs.txt TTB | Mplus1p-Regular | 34915.7 ±44.4 | — | — | 22087.8 ±32.8 | 1.58 |
| first shaping ja-runs.txt TTB | Mplus1p-Regular | 15.4 ±0.0 | — | — | 8.2 ±0.0 | 1.86 |
| shaping role-default-und | extra/arabic | 2.1 ±0.0 | — | — | 1.3 ±0.0 | 1.58 |
| first shaping role-default-und | extra/arabic | 32.8 ±0.0 | — | — | 17.4 ±0.0 | 1.88 |
| shaping role-default-und | extra/devanagari | 2.7 ±0.0 | — | — | 1.6 ±0.0 | 1.63 |
| first shaping role-default-und | extra/devanagari | 120.9 ±0.2 | — | — | 852.3 ±1.1 | 0.14 |
| shaping role-default-und | extra/khmer | 1.3 ±0.0 | — | — | 0.9 ±0.0 | 1.51 |
| first shaping role-default-und | extra/khmer | 45.9 ±0.1 | — | — | 22.0 ±0.0 | 2.09 |
| shaping role-default-und | extra/myanmar | 3.6 ±0.0 | — | — | 1.5 ±0.0 | 2.41 |
| first shaping role-default-und | extra/myanmar | 130.9 ±0.2 | — | — | 48.2 ±0.1 | 2.71 |
| shaping role-default-und | extra/balinese | 13.0 ±0.0 | — | — | 5.6 ±0.0 | 2.31 |
| first shaping role-default-und | extra/balinese | 181.3 ±0.2 | — | — | 45.5 ±0.1 | 3.99 |
| shaping role-default-und | extra/hebrew | 1.9 ±0.0 | — | — | 1.2 ±0.0 | 1.66 |
| first shaping role-default-und | extra/hebrew | 13.5 ±0.0 | — | — | 9.3 ±0.0 | 1.46 |
| shaping role-default-und | extra/source-serif | 1.6 ±0.0 | — | — | 0.6 ±0.0 | 2.51 |
| first shaping role-default-und | extra/source-serif | 38.8 ±0.0 | — | — | 20.9 ±0.0 | 1.86 |
| shaping role-default-und TTB | extra/source-han | 0.5 ±0.0 | — | — | 0.3 ±0.0 | 1.48 |
| first shaping role-default-und TTB | extra/source-han | 109.6 ±0.2 | — | — | 26.2 ±0.1 | 4.18 |
| shaping role-default-und | extra/amstelvar | 0.7 ±0.0 | — | — | 0.4 ±0.0 | 1.64 |
| first shaping role-default-und | extra/amstelvar | 4.0 ±0.0 | — | — | 10.9 ±0.0 | 0.36 |
| shaping react-dom.txt wght=500 | SourceSerifVariable-Roman | 316656.2 ±251.5 | — | — | 85743.0 ±110.7 | 3.69 |
| subset (42 glyphs, retain-gids) | Roboto-Regular | 203.7 ±1.2 | — | — | 122.5 ±0.4 | 1.66 |
| subset (2322 characters) | Roboto-Regular | 277.0 ±1.7 | — | — | 129.9 ±0.6 | 2.13 |
| subset prepared (42 glyphs, retain-gids) | Roboto-Regular | 136.3 ±0.4 | — | — | 81.2 ±0.1 | 1.68 |
| subset prepared (2322 characters) | Roboto-Regular | 220.7 ±1.7 | — | — | 93.1 ±0.2 | 2.37 |
| preprocess | Roboto-Regular | 195.2 ±1.9 | — | — | 756.6 ±1.6 | 0.26 |
| subset (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 59774.8 ±583.7 | — | — | 13684.5 ±95.3 | 4.37 |
| subset (2322 characters) | SourceHanSans-Regular_subset | 39738.3 ±475.0 | — | — | 13317.9 ±181.5 | 2.98 |
| subset prepared (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 40617.1 ±348.2 | — | — | 5274.9 ±17.7 | 7.70 |
| subset prepared (2322 characters) | SourceHanSans-Regular_subset | 20283.0 ±94.5 | — | — | 5058.5 ±20.0 | 4.01 |
| preprocess | SourceHanSans-Regular_subset | 62261.6 ±597.2 | — | — | 48616.5 ±418.5 | 1.28 |
| subset (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 87.5 ±0.2 | — | — | 135.4 ±0.5 | 0.65 |
| subset (2322 characters) | NotoNastaliqUrdu-Bold | 157.9 ±0.4 | — | — | 143.6 ±0.4 | 1.10 |
| subset prepared (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 70.0 ±0.3 | — | — | 59.6 ±0.1 | 1.18 |
| subset prepared (2322 characters) | NotoNastaliqUrdu-Bold | 143.7 ±0.2 | — | — | 70.5 ±0.1 | 2.04 |
| preprocess | NotoNastaliqUrdu-Bold | 54.3 ±0.1 | — | — | 13777.8 ±33.6 | 0.00 |
| subset unicode range 10 | Roboto-Regular.ttf | 52.0 ±0.1 | — | — | 34.0 ±0.1 | 1.53 |
| subset unicode range 10, retain-gids | Roboto-Regular.ttf | 51.8 ±0.2 | — | — | 33.9 ±0.1 | 1.53 |
| subset unicode range 64 | Roboto-Regular.ttf | 141.2 ±0.8 | — | — | 87.0 ±0.3 | 1.62 |
| subset unicode range 64, retain-gids | Roboto-Regular.ttf | 148.3 ±0.6 | — | — | 91.0 ±0.2 | 1.63 |
| subset unicode range 512 | Roboto-Regular.ttf | 859.7 ±2.1 | — | — | 420.2 ±2.8 | 2.05 |
| subset unicode range 512, retain-gids | Roboto-Regular.ttf | 882.4 ±3.3 | — | — | 425.5 ±2.3 | 2.07 |
| subset unicode range 1000 | Roboto-Regular.ttf | 1231.7 ±1.9 | — | — | 619.5 ±1.7 | 1.99 |
| subset unicode range 1000, retain-gids | Roboto-Regular.ttf | 1250.1 ±2.0 | — | — | 622.2 ±1.7 | 2.01 |
| subset unicode range 10 | Amiri-Regular.ttf | 118.8 ±0.6 | — | — | 64.4 ±0.2 | 1.84 |
| subset unicode range 10, retain-gids | Amiri-Regular.ttf | 118.3 ±0.5 | — | — | 65.0 ±0.2 | 1.82 |
| subset unicode range 64 | Amiri-Regular.ttf | 251.2 ±1.0 | — | — | 130.4 ±0.3 | 1.93 |
| subset unicode range 64, retain-gids | Amiri-Regular.ttf | 340.3 ±1.3 | — | — | 179.0 ±0.4 | 1.90 |
| subset unicode range 512 | Amiri-Regular.ttf | 4106.5 ±10.8 | — | — | 2653.1 ±5.5 | 1.55 |
| subset unicode range 512, retain-gids | Amiri-Regular.ttf | 4233.3 ±9.5 | — | — | 2688.7 ±5.6 | 1.57 |
| subset unicode range 4096 | Amiri-Regular.ttf | 6752.9 ±9.3 | — | — | 4230.0 ±6.2 | 1.60 |
| subset unicode range 4096, retain-gids | Amiri-Regular.ttf | 6769.3 ±18.0 | — | — | 4194.5 ±5.0 | 1.61 |
| subset unicode range 10 | NotoNastaliqUrdu-Regular.ttf | 56.8 ±0.1 | — | — | 51.9 ±0.1 | 1.09 |
| subset unicode range 10, retain-gids | NotoNastaliqUrdu-Regular.ttf | 70.0 ±0.1 | — | — | 59.5 ±0.1 | 1.18 |
| subset unicode range 64 | NotoNastaliqUrdu-Regular.ttf | 15604.3 ±56.6 | — | — | 2203.6 ±5.6 | 7.08 |
| subset unicode range 64, retain-gids | NotoNastaliqUrdu-Regular.ttf | 15623.1 ±30.1 | — | — | 2206.0 ±3.1 | 7.08 |
| subset unicode range 512 | NotoNastaliqUrdu-Regular.ttf | 66816.1 ±791.8 | — | — | 29433.7 ±382.8 | 2.27 |
| subset unicode range 512, retain-gids | NotoNastaliqUrdu-Regular.ttf | 66626.8 ±398.0 | — | — | 28760.3 ±129.7 | 2.32 |
| subset unicode range 1400 | NotoNastaliqUrdu-Regular.ttf | 66757.1 ±740.7 | — | — | 28684.5 ±102.2 | 2.33 |
| subset unicode range 1400, retain-gids | NotoNastaliqUrdu-Regular.ttf | 66466.6 ±290.1 | — | — | 28667.9 ±86.2 | 2.32 |
| subset unicode range 10 | NotoSansDevanagari-Regular.ttf | 60.0 ±0.3 | — | — | 55.0 ±0.1 | 1.09 |
| subset unicode range 10, retain-gids | NotoSansDevanagari-Regular.ttf | 70.8 ±0.1 | — | — | 60.3 ±0.1 | 1.18 |
| subset unicode range 64 | NotoSansDevanagari-Regular.ttf | 164.3 ±0.3 | — | — | 120.9 ±0.2 | 1.36 |
| subset unicode range 64, retain-gids | NotoSansDevanagari-Regular.ttf | 174.6 ±0.4 | — | — | 126.5 ±0.3 | 1.38 |
| subset unicode range 512 | NotoSansDevanagari-Regular.ttf | 3539.9 ±5.9 | — | — | 1682.3 ±5.5 | 2.10 |
| subset unicode range 512, retain-gids | NotoSansDevanagari-Regular.ttf | 3537.3 ±5.2 | — | — | 1674.4 ±2.3 | 2.11 |
| subset unicode range 1000 | NotoSansDevanagari-Regular.ttf | 3542.0 ±7.7 | — | — | 1681.0 ±2.9 | 2.11 |
| subset unicode range 1000, retain-gids | NotoSansDevanagari-Regular.ttf | 3538.7 ±5.3 | — | — | 1673.8 ±5.1 | 2.11 |
| subset unicode range 10 | Mplus1p-Regular.ttf | 47.3 ±0.1 | — | — | 35.3 ±0.1 | 1.34 |
| subset unicode range 10, retain-gids | Mplus1p-Regular.ttf | 47.3 ±0.2 | — | — | 35.3 ±0.1 | 1.34 |
| subset unicode range 64 | Mplus1p-Regular.ttf | 137.3 ±0.2 | — | — | 83.9 ±0.1 | 1.64 |
| subset unicode range 64, retain-gids | Mplus1p-Regular.ttf | 137.3 ±0.3 | — | — | 84.0 ±0.1 | 1.64 |
| subset unicode range 512 | Mplus1p-Regular.ttf | 509.2 ±2.8 | — | — | 254.2 ±0.7 | 2.00 |
| subset unicode range 512, retain-gids | Mplus1p-Regular.ttf | 669.9 ±6.9 | — | — | 344.8 ±0.8 | 1.94 |
| subset unicode range 4096 | Mplus1p-Regular.ttf | 3813.9 ±7.5 | — | — | 1747.5 ±1.7 | 2.18 |
| subset unicode range 4096, retain-gids | Mplus1p-Regular.ttf | 3913.8 ±11.5 | — | — | 1793.3 ±3.4 | 2.18 |
| subset unicode range 10000 | Mplus1p-Regular.ttf | 8280.7 ±21.9 | — | — | 3778.2 ±8.3 | 2.19 |
| subset unicode range 10000, retain-gids | Mplus1p-Regular.ttf | 8214.4 ±13.4 | — | — | 3739.3 ±9.9 | 2.20 |
| subset unicode range 10 | SourceHanSans-Regular_subset.otf | 146.0 ±0.5 | — | — | 53.9 ±0.1 | 2.71 |
| subset unicode range 10, retain-gids | SourceHanSans-Regular_subset.otf | 28927.2 ±42.0 | — | — | 263.8 ±0.7 | 109.65 |
| subset unicode range 64 | SourceHanSans-Regular_subset.otf | 285.6 ±0.9 | — | — | 101.6 ±0.3 | 2.81 |
| subset unicode range 64, retain-gids | SourceHanSans-Regular_subset.otf | 29079.1 ±46.1 | — | — | 310.7 ±0.5 | 93.60 |
| subset unicode range 512 | SourceHanSans-Regular_subset.otf | 1522.3 ±5.9 | — | — | 686.3 ±1.6 | 2.22 |
| subset unicode range 512, retain-gids | SourceHanSans-Regular_subset.otf | 30223.0 ±53.0 | — | — | 891.1 ±4.1 | 33.92 |
| subset unicode range 4096 | SourceHanSans-Regular_subset.otf | 35432.0 ±182.5 | — | — | 8451.8 ±26.6 | 4.19 |
| subset unicode range 4096, retain-gids | SourceHanSans-Regular_subset.otf | 48466.1 ±217.0 | — | — | 8552.9 ±93.1 | 5.67 |
| subset unicode range 10000 | SourceHanSans-Regular_subset.otf | 51037.0 ±365.1 | — | — | 10616.7 ±61.1 | 4.81 |
| subset unicode range 10000, retain-gids | SourceHanSans-Regular_subset.otf | 54521.4 ±630.9 | — | — | 10623.5 ±53.9 | 5.13 |
| subset unicode range 10 | SourceSansPro-Regular.otf | 98.5 ±0.2 | — | — | 55.1 ±0.1 | 1.79 |
| subset unicode range 10, retain-gids | SourceSansPro-Regular.otf | 709.8 ±1.3 | — | — | 123.0 ±0.3 | 5.77 |
| subset unicode range 64 | SourceSansPro-Regular.otf | 231.7 ±1.3 | — | — | 125.3 ±0.4 | 1.85 |
| subset unicode range 64, retain-gids | SourceSansPro-Regular.otf | 981.2 ±1.2 | — | — | 199.1 ±0.5 | 4.93 |
| subset unicode range 512 | SourceSansPro-Regular.otf | 1419.5 ±1.5 | — | — | 637.0 ±2.3 | 2.23 |
| subset unicode range 512, retain-gids | SourceSansPro-Regular.otf | 1959.3 ±2.5 | — | — | 706.7 ±2.0 | 2.77 |
| subset unicode range 2000 | SourceSansPro-Regular.otf | 3050.4 ±3.8 | — | — | 1316.8 ±2.6 | 2.32 |
| subset unicode range 2000, retain-gids | SourceSansPro-Regular.otf | 3293.7 ±3.0 | — | — | 1339.4 ±3.1 | 2.46 |

## wasm

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location wght=500 | RobotoFlex-Variable | 25.3 ±0.0 | 0.0 ±0.0 | 699.21 | 0.3 ±0.0 | 80.84 |
| box wght=500 | RobotoFlex-Variable | 5675.0 ±9.1 | — | — | 2489.1 ±7.1 | 2.28 |
| location with reference queries wght=499/500 | RobotoFlex-Variable | 25.7 ±0.1 | 2.5 ±0.0 | 10.29 | 3.5 ±0.0 | 7.33 |
| advance wght=500 | RobotoFlex-Variable | 183.3 ±0.4 | 128.3 ±0.5 | 1.43 | 105.3 ±0.2 | 1.74 |
| outline unscaled wght=500 | RobotoFlex-Variable | 6516.3 ±10.7 | 1965.2 ±6.4 | 3.32 | 1838.7 ±7.5 | 3.54 |
| outline 48px wght=500 | RobotoFlex-Variable | 6598.1 ±11.0 | 2116.1 ±5.8 | 3.12 | 1828.2 ±4.8 | 3.61 |
| raster 24px wght=500 | RobotoFlex-Variable | 17018.7 ±26.7 | 9929.4 ±22.7 | 1.71 | — | — |
| location wght=500 | SourceSerifVariable-Roman | 1.6 ±0.0 | 0.0 ±0.0 | 93.21 | 0.1 ±0.0 | 16.89 |
| box wght=500 | SourceSerifVariable-Roman | 4917.3 ±11.5 | — | — | 1545.8 ±2.8 | 3.18 |
| location with reference queries wght=499/500 | SourceSerifVariable-Roman | 1.8 ±0.0 | 1.2 ±0.0 | 1.52 | 2.0 ±0.0 | 0.90 |
| advance wght=500 | SourceSerifVariable-Roman | 144.7 ±0.1 | 78.2 ±0.2 | 1.85 | 82.2 ±0.1 | 1.76 |
| outline unscaled wght=500 | SourceSerifVariable-Roman | 5786.4 ±11.7 | 2821.9 ±15.5 | 2.05 | 1985.0 ±2.7 | 2.91 |
| outline 48px wght=500 | SourceSerifVariable-Roman | 5993.9 ±10.1 | 3091.0 ±4.6 | 1.94 | 1987.7 ±4.8 | 3.02 |
| raster 24px wght=500 | SourceSerifVariable-Roman | 10531.4 ±31.3 | 7058.8 ±29.4 | 1.49 | — | — |
| location wght=500 | AdobeVFPrototype | 24.3 ±0.0 | 0.0 ±0.0 | 1015.79 | 0.1 ±0.0 | 209.99 |
| box wght=500 | AdobeVFPrototype | 1946.8 ±3.9 | — | — | 2432.1 ±18.3 | 0.80 |
| location with reference queries wght=499/500 | AdobeVFPrototype | 24.5 ±0.0 | 8.4 ±0.0 | 2.90 | 7.7 ±0.0 | 3.20 |
| advance wght=500 | AdobeVFPrototype | 30.9 ±0.1 | 16.9 ±0.0 | 1.83 | 11.5 ±0.0 | 2.69 |
| outline unscaled wght=500 | AdobeVFPrototype | 6653.2 ±16.8 | 2235.2 ±3.2 | 2.98 | 2330.4 ±51.9 | 2.85 |
| outline 48px wght=500 | AdobeVFPrototype | 6667.4 ±8.2 | 2287.0 ±3.8 | 2.92 | 2320.7 ±4.5 | 2.87 |
| raster 24px wght=500 | AdobeVFPrototype | 7683.9 ±15.3 | 3144.5 ±4.7 | 2.44 | — | — |
| Unicode properties (1682970 characters) | perf/texts | 14230.1 ±21.9 | — | — | 35972.4 ±88.8 | 0.40 |
| cmap | extra/arabic | 0.3 ±0.0 | 0.3 ±0.0 | 1.04 | 0.4 ±0.0 | 0.85 |
| advance | extra/arabic | 0.4 ±0.0 | 4.3 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/arabic | 88.5 ±0.2 | 80.2 ±0.1 | 1.10 | — | — |
| cmap | extra/devanagari | 0.3 ±0.0 | 0.3 ±0.0 | 1.02 | 0.3 ±0.0 | 0.83 |
| advance | extra/devanagari | 0.4 ±0.0 | 4.3 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/devanagari | 102.0 ±0.2 | 92.0 ±0.1 | 1.11 | — | — |
| cmap | extra/khmer | 0.3 ±0.0 | 0.3 ±0.0 | 1.02 | 0.3 ±0.0 | 1.00 |
| advance | extra/khmer | 0.4 ±0.0 | 4.3 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/khmer | 94.3 ±0.2 | 82.3 ±0.1 | 1.15 | — | — |
| cmap | extra/myanmar | 0.3 ±0.0 | 0.3 ±0.0 | 1.03 | 0.3 ±0.0 | 1.01 |
| advance | extra/myanmar | 0.4 ±0.0 | 4.3 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/myanmar | 109.8 ±0.4 | 93.7 ±0.2 | 1.17 | — | — |
| cmap | extra/balinese | 0.3 ±0.0 | 0.3 ±0.0 | 1.01 | 0.3 ±0.0 | 0.97 |
| advance | extra/balinese | 0.4 ±0.0 | 4.3 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/balinese | 144.3 ±0.2 | 122.6 ±0.2 | 1.18 | — | — |
| cmap | extra/hebrew | 0.3 ±0.0 | 0.3 ±0.0 | 1.04 | 0.3 ±0.0 | 0.98 |
| advance | extra/hebrew | 0.4 ±0.0 | 4.3 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/hebrew | 60.2 ±0.1 | 62.2 ±0.1 | 0.97 | — | — |
| cmap | extra/source-serif | 0.3 ±0.0 | 0.3 ±0.0 | 1.04 | 0.4 ±0.0 | 0.87 |
| advance | extra/source-serif | 0.4 ±0.0 | 4.2 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/source-serif | 4245.7 ±3.4 | 1477.8 ±2.3 | 2.87 | — | — |
| cmap | extra/stix-math | 0.3 ±0.0 | 0.3 ±0.0 | 1.04 | 0.4 ±0.0 | 0.84 |
| advance | extra/stix-math | 0.4 ±0.0 | 4.2 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/stix-math | 403.1 ±0.5 | 585.2 ±1.1 | 0.69 | — | — |
| MATH constants | extra/stix-math | 0.3 ±0.0 | — | — | 1.0 ±0.0 | 0.33 |
| cmap | extra/source-han | 0.4 ±0.0 | 0.3 ±0.0 | 1.11 | 0.4 ±0.0 | 1.00 |
| advance | extra/source-han | 0.4 ±0.0 | 4.2 ±0.0 | 0.08 | 2.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/source-han | 418.4 ±0.5 | 356.1 ±0.6 | 1.17 | — | — |
| vertical advance | extra/source-han | 1.2 ±0.0 | — | — | 2.9 ±0.0 | 0.41 |
| cmap | extra/amstelvar | 0.3 ±0.0 | 0.3 ±0.0 | 0.98 | 0.3 ±0.0 | 0.88 |
| advance | extra/amstelvar | 0.4 ±0.0 | 4.3 ±0.0 | 0.08 | 2.6 ±0.0 | 0.13 |
| outline unscaled sampled | extra/amstelvar | 114.3 ±0.1 | 118.5 ±0.1 | 0.96 | — | — |
| cmap | extra/color-cff | 0.2 ±0.0 | 0.2 ±0.0 | 0.99 | 0.3 ±0.0 | 0.82 |
| advance | extra/color-cff | 0.1 ±0.0 | 1.0 ±0.0 | 0.08 | 0.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/color-cff | 20.7 ±0.0 | 15.9 ±0.0 | 1.31 | — | — |
| cmap | extra/color-sbix | 0.2 ±0.0 | 0.2 ±0.0 | 0.98 | 0.3 ±0.0 | 0.82 |
| advance | extra/color-sbix | 0.1 ±0.0 | 1.0 ±0.0 | 0.08 | 0.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/color-sbix | 2.5 ±0.0 | 3.0 ±0.0 | 0.85 | — | — |
| cmap | extra/color-svg | 0.2 ±0.0 | 0.2 ±0.0 | 0.99 | 0.3 ±0.0 | 0.82 |
| advance | extra/color-svg | 0.1 ±0.0 | 1.0 ±0.0 | 0.08 | 0.6 ±0.0 | 0.14 |
| outline unscaled sampled | extra/color-svg | 2.5 ±0.0 | 3.1 ±0.0 | 0.83 | — | — |
| outline unscaled | SourceSansPro-Regular | 6788.2 ±6.2 | 6984.9 ±29.7 | 0.97 | 3175.9 ±8.5 | 2.14 |
| outline 48px | SourceSansPro-Regular | 6882.2 ±13.4 | 7195.4 ±13.6 | 0.96 | 3205.4 ±9.7 | 2.15 |
| raster 12px | SourceSansPro-Regular | 10157.6 ±10.0 | 9538.0 ±14.2 | 1.06 | — | — |
| raster 24px | SourceSansPro-Regular | 11878.3 ±12.5 | 10752.8 ±13.2 | 1.10 | — | — |
| raster 48px | SourceSansPro-Regular | 16039.6 ±9.9 | 13354.8 ±22.9 | 1.20 | — | — |
| raster 96px | SourceSansPro-Regular | 25668.3 ±18.2 | 18812.9 ±19.1 | 1.36 | — | — |
| open (FreeType) | SourceSansPro-Regular | 12.1 ±0.0 | 36.5 ±0.0 | 0.33 | — | — |
| glyph extents | SourceSansPro-Regular | 2532.7 ±4.1 | — | — | 2650.3 ±5.2 | 0.96 |
| open (HarfBuzz) | SourceSansPro-Regular | 2.5 ±0.0 | — | — | 3.5 ±0.0 | 0.72 |
| glyph names | SourceSansPro-Regular | 55.7 ±0.1 | — | — | 214.7 ±0.4 | 0.26 |
| glyph from name | SourceSansPro-Regular | 1333.4 ±2.4 | — | — | 460.6 ±0.7 | 2.89 |
| outline unscaled | NotoSerifMyanmar-Regular | 3076.7 ±3.0 | 3286.9 ±9.1 | 0.94 | 1870.3 ±3.2 | 1.65 |
| outline 48px | NotoSerifMyanmar-Regular | 3165.1 ±5.1 | 3455.1 ±8.1 | 0.92 | 1872.1 ±6.7 | 1.69 |
| raster 12px | NotoSerifMyanmar-Regular | 5356.5 ±3.3 | 5135.8 ±8.2 | 1.04 | — | — |
| raster 24px | NotoSerifMyanmar-Regular | 6692.6 ±3.2 | 6195.2 ±12.4 | 1.08 | — | — |
| raster 48px | NotoSerifMyanmar-Regular | 9936.0 ±5.0 | 8584.4 ±11.2 | 1.16 | — | — |
| raster 96px | NotoSerifMyanmar-Regular | 17584.4 ±10.8 | 13284.1 ±12.8 | 1.32 | — | — |
| open (FreeType) | NotoSerifMyanmar-Regular | 6.9 ±0.0 | 17.8 ±0.0 | 0.39 | — | — |
| glyph extents | NotoSerifMyanmar-Regular | 1436.5 ±1.6 | — | — | 1533.4 ±3.5 | 0.94 |
| open (HarfBuzz) | NotoSerifMyanmar-Regular | 1.7 ±0.0 | — | — | 3.2 ±0.0 | 0.52 |
| glyph names | NotoSerifMyanmar-Regular | 22.5 ±0.0 | — | — | 32.7 ±0.1 | 0.69 |
| glyph from name | NotoSerifMyanmar-Regular | 218.1 ±0.4 | — | — | 59.3 ±0.1 | 3.68 |
| outline unscaled | AdobeVFPrototype | 6697.7 ±5.0 | 2216.6 ±2.7 | 3.02 | 2341.3 ±11.3 | 2.86 |
| outline 48px | AdobeVFPrototype | 6731.7 ±7.7 | 2263.9 ±5.1 | 2.97 | 2215.7 ±10.0 | 3.04 |
| raster 12px | AdobeVFPrototype | 7335.4 ±9.1 | 2775.3 ±4.0 | 2.64 | — | — |
| raster 24px | AdobeVFPrototype | 7698.8 ±8.3 | 3137.5 ±3.0 | 2.45 | — | — |
| raster 48px | AdobeVFPrototype | 8513.1 ±9.0 | 3725.5 ±3.3 | 2.29 | — | — |
| raster 96px | AdobeVFPrototype | 10295.2 ±8.2 | 4953.1 ±5.5 | 2.08 | — | — |
| open (FreeType) | AdobeVFPrototype | 11.0 ±0.0 | 17.0 ±0.0 | 0.65 | — | — |
| glyph extents | AdobeVFPrototype | 1687.3 ±5.7 | — | — | 2362.7 ±31.2 | 0.71 |
| open (HarfBuzz) | AdobeVFPrototype | 3.6 ±0.1 | — | — | 4.9 ±0.0 | 0.74 |
| outline unscaled | SourceHanSans-Regular_subset | 74813.2 ±72.7 | 90278.2 ±111.7 | 0.83 | 41522.8 ±65.9 | 1.80 |
| outline 48px | SourceHanSans-Regular_subset | 76149.2 ±50.4 | 93270.7 ±92.6 | 0.82 | 41798.7 ±100.1 | 1.82 |
| raster 12px | SourceHanSans-Regular_subset | 116062.9 ±95.3 | 129315.5 ±159.1 | 0.90 | — | — |
| raster 24px | SourceHanSans-Regular_subset | 145386.3 ±140.1 | 153700.7 ±275.9 | 0.95 | — | — |
| raster 48px | SourceHanSans-Regular_subset | 205387.6 ±143.6 | 199679.1 ±531.8 | 1.03 | — | — |
| raster 96px | SourceHanSans-Regular_subset | 350197.8 ±463.9 | 291871.1 ±425.0 | 1.20 | — | — |
| cid | SourceHanSans-Regular_subset | 270.2 ±0.3 | 358.5 ±1.5 | 0.75 | — | — |
| cid (no index) | SourceHanSans-Regular_subset | 250532.7 ±678.0 | 357.8 ±1.6 | 700.17 | — | — |
| open (FreeType) | SourceHanSans-Regular_subset | 81.5 ±0.1 | 124.4 ±0.5 | 0.65 | — | — |
| glyph extents | SourceHanSans-Regular_subset | 34897.4 ±60.7 | — | — | 35056.2 ±56.8 | 1.00 |
| open (HarfBuzz) | SourceHanSans-Regular_subset | 12.0 ±0.0 | — | — | 9.7 ±0.0 | 1.23 |
| glyph color format | extra/color-cff | 4.3 ±0.0 | — | — | 0.6 ±0.0 | 7.06 |
| COLRv1 paint | extra/color-cff | 9.1 ±0.0 | 2.0 ±0.0 | 4.60 | 4.5 ±0.0 | 2.00 |
| COLRv1 recording | extra/color-cff | 9.3 ±0.0 | — | — | — | — |
| color glyph extents | extra/color-cff | 13.4 ±0.0 | — | — | 5.7 ±0.0 | 2.37 |
| glyph color format | extra/color-sbix | 3.3 ±0.0 | — | — | 0.5 ±0.0 | 7.06 |
| bitmap data 16 ppem | extra/color-sbix | 6.1 ±0.0 | — | — | 5.4 ±0.0 | 1.14 |
| bitmap data 109 ppem | extra/color-sbix | 6.1 ±0.0 | — | — | 5.4 ±0.0 | 1.14 |
| bitmap data 136 ppem | extra/color-sbix | 6.6 ±0.0 | — | — | 5.8 ±0.0 | 1.13 |
| bitmap data 0 ppem | extra/color-sbix | 6.6 ±0.0 | — | — | 5.7 ±0.0 | 1.16 |
| color glyph extents | extra/color-sbix | 3.6 ±0.0 | — | — | 2.6 ±0.0 | 1.35 |
| glyph color format | extra/color-svg | 2.4 ±0.0 | — | — | 0.5 ±0.0 | 5.04 |
| SVG documents | extra/color-svg | 1.2 ±0.0 | — | — | 0.7 ±0.0 | 1.64 |
| color glyph extents | extra/color-svg | 2.7 ±0.0 | — | — | 1.4 ±0.0 | 1.93 |
| glyph color format | TwemojiMozilla.subset | 0.9 ±0.0 | — | — | 0.1 ±0.0 | 7.01 |
| COLRv0 layers | TwemojiMozilla.subset | 0.3 ±0.0 | 0.2 ±0.0 | 1.72 | 0.2 ±0.0 | 1.45 |
| COLRv0 raster 12px | TwemojiMozilla.subset | 42.0 ±0.0 | 27.6 ±0.0 | 1.52 | — | — |
| COLRv0 raster 24px | TwemojiMozilla.subset | 62.8 ±0.0 | 43.5 ±0.1 | 1.44 | — | — |
| COLRv0 raster 48px | TwemojiMozilla.subset | 133.5 ±0.1 | 97.2 ±0.1 | 1.37 | — | — |
| COLRv0 paint | TwemojiMozilla.subset | 1.1 ±0.0 | — | — | 0.6 ±0.0 | 1.92 |
| color glyph extents | TwemojiMozilla.subset | 7.2 ±0.0 | — | — | 5.9 ±0.0 | 1.22 |
| glyph color format | test_glyphs-glyf_colr_1 | 33.0 ±0.1 | — | — | 6.6 ±0.0 | 5.01 |
| COLRv0 layers | test_glyphs-glyf_colr_1 | 11.9 ±0.0 | 4.0 ±0.0 | 2.97 | 7.2 ±0.0 | 1.66 |
| COLRv0 raster 12px | test_glyphs-glyf_colr_1 | 213.6 ±0.3 | 160.9 ±0.2 | 1.33 | — | — |
| COLRv0 raster 24px | test_glyphs-glyf_colr_1 | 311.8 ±0.4 | 231.8 ±0.2 | 1.35 | — | — |
| COLRv0 raster 48px | test_glyphs-glyf_colr_1 | 587.2 ±0.4 | 425.5 ±0.9 | 1.38 | — | — |
| COLRv1 paint | test_glyphs-glyf_colr_1 | 116.4 ±0.2 | 32.2 ±0.0 | 3.62 | 67.8 ±0.1 | 1.72 |
| COLRv1 recording | test_glyphs-glyf_colr_1 | 119.2 ±0.2 | — | — | — | — |
| color glyph extents | test_glyphs-glyf_colr_1 | 94.0 ±0.1 | — | — | 63.8 ±0.0 | 1.47 |
| glyph color format | noto_handwriting-glyf_colr_1 | 3.4 ±0.0 | — | — | 0.5 ±0.0 | 7.49 |
| COLRv1 paint | noto_handwriting-glyf_colr_1 | 12.4 ±0.0 | 4.9 ±0.0 | 2.52 | 6.7 ±0.0 | 1.84 |
| COLRv1 recording | noto_handwriting-glyf_colr_1 | 12.7 ±0.0 | — | — | — | — |
| color glyph extents | noto_handwriting-glyf_colr_1 | 8.9 ±0.0 | — | — | 2.7 ±0.0 | 3.28 |
| glyph color format | NotoColorEmoji.subset | 2.6 ±0.0 | — | — | 0.1 ±0.0 | 25.20 |
| bitmap data 16 ppem | NotoColorEmoji.subset | 2.5 ±0.0 | — | — | 1.3 ±0.0 | 2.02 |
| bitmap data 109 ppem | NotoColorEmoji.subset | 2.5 ±0.0 | — | — | 1.3 ±0.0 | 2.00 |
| bitmap data 136 ppem | NotoColorEmoji.subset | 2.6 ±0.0 | — | — | 1.4 ±0.0 | 1.92 |
| bitmap data 0 ppem | NotoColorEmoji.subset | 2.7 ±0.0 | — | — | 1.3 ±0.0 | 2.00 |
| color glyph extents | NotoColorEmoji.subset | 1.1 ±0.0 | — | — | 0.8 ±0.0 | 1.37 |
| glyph color format | noto_handwriting-sbix | 1.6 ±0.0 | — | — | 0.2 ±0.0 | 7.17 |
| bitmap data 16 ppem | noto_handwriting-sbix | 3.1 ±0.0 | — | — | 2.7 ±0.0 | 1.14 |
| bitmap data 109 ppem | noto_handwriting-sbix | 3.1 ±0.0 | — | — | 2.7 ±0.0 | 1.14 |
| bitmap data 136 ppem | noto_handwriting-sbix | 3.4 ±0.0 | — | — | 2.9 ±0.0 | 1.15 |
| bitmap data 0 ppem | noto_handwriting-sbix | 3.4 ±0.0 | — | — | 2.9 ±0.0 | 1.17 |
| color glyph extents | noto_handwriting-sbix | 1.6 ±0.0 | — | — | 1.3 ±0.0 | 1.27 |
| glyph color format | chromacheck-svg | 0.2 ±0.0 | — | — | 0.1 ±0.0 | 4.62 |
| SVG documents | chromacheck-svg | 0.1 ±0.0 | — | — | 0.1 ±0.0 | 1.77 |
| color glyph extents | chromacheck-svg | 0.3 ±0.0 | — | — | 0.2 ±0.0 | 1.77 |
| bitmap pixels 16 ppem | EBDT (built, 200 glyphs) | 123.3 ±0.2 | 72.7 ±0.6 | 1.70 | — | — |
| cmap | Roboto-Regular | 7059.8 ±14.3 | 6884.4 ±7.6 | 1.03 | 9342.5 ±17.0 | 0.76 |
| advance | Roboto-Regular | 9.2 ±0.0 | 116.3 ±0.1 | 0.08 | 68.6 ±0.1 | 0.13 |
| v advance | Roboto-Regular | 150.3 ±0.3 | — | — | 238.7 ±0.6 | 0.63 |
| v origin | Roboto-Regular | 497.4 ±0.7 | — | — | 538.5 ±0.9 | 0.92 |
| outline unscaled | Roboto-Regular | 2297.3 ±3.6 | 2316.3 ±2.4 | 0.99 | 2322.8 ±4.3 | 0.99 |
| outline 48px | Roboto-Regular | 2345.6 ±2.5 | 2659.2 ±5.3 | 0.88 | 2318.4 ±2.9 | 1.01 |
| raster 12px | Roboto-Regular | 7403.7 ±9.6 | 7069.5 ±9.5 | 1.05 | — | — |
| raster 24px | Roboto-Regular | 10912.3 ±8.6 | 9820.2 ±9.0 | 1.11 | — | — |
| raster 48px | Roboto-Regular | 17844.7 ±16.7 | 14486.5 ±33.5 | 1.23 | — | — |
| raster 96px | Roboto-Regular | 35624.3 ±49.7 | 24360.9 ±59.7 | 1.46 | — | — |
| cmap | Comfortaa-Regular-new | 6708.5 ±8.1 | 7120.3 ±16.0 | 0.94 | 7831.8 ±12.9 | 0.86 |
| advance | Comfortaa-Regular-new | 2.4 ±0.0 | 29.8 ±0.1 | 0.08 | 17.9 ±0.0 | 0.13 |
| v advance | Comfortaa-Regular-new | 49.5 ±0.2 | — | — | 49.4 ±0.1 | 1.00 |
| v origin | Comfortaa-Regular-new | 146.2 ±0.2 | — | — | 128.0 ±0.3 | 1.14 |
| outline unscaled | Comfortaa-Regular-new | 864.7 ±1.9 | 803.4 ±3.4 | 1.08 | 853.7 ±1.6 | 1.01 |
| outline 48px | Comfortaa-Regular-new | 927.4 ±1.2 | 947.0 ±1.3 | 0.98 | 852.1 ±1.4 | 1.09 |
| raster 12px | Comfortaa-Regular-new | 2648.1 ±4.0 | 2351.0 ±4.6 | 1.13 | — | — |
| raster 24px | Comfortaa-Regular-new | 3769.8 ±5.6 | 3270.0 ±5.0 | 1.15 | — | — |
| raster 48px | Comfortaa-Regular-new | 6171.3 ±10.0 | 4956.0 ±8.5 | 1.25 | — | — |
| raster 96px | Comfortaa-Regular-new | 12127.7 ±11.7 | 8560.0 ±12.0 | 1.42 | — | — |
| cmap | NotoNastaliqUrdu-Regular | 6619.4 ±7.4 | 7055.5 ±28.0 | 0.94 | 7520.4 ±10.5 | 0.88 |
| advance | NotoNastaliqUrdu-Regular | 3.1 ±0.0 | 39.2 ±0.1 | 0.08 | 23.3 ±0.1 | 0.13 |
| v advance | NotoNastaliqUrdu-Regular | 50.5 ±0.1 | — | — | 80.5 ±0.2 | 0.63 |
| v origin | NotoNastaliqUrdu-Regular | 164.5 ±0.3 | — | — | 182.4 ±0.3 | 0.90 |
| outline unscaled | NotoNastaliqUrdu-Regular | 1109.3 ±1.0 | 1018.7 ±1.3 | 1.09 | 1183.0 ±1.6 | 0.94 |
| outline 48px | NotoNastaliqUrdu-Regular | 1185.5 ±2.2 | 1195.7 ±1.7 | 0.99 | 1181.4 ±1.3 | 1.00 |
| raster 12px | NotoNastaliqUrdu-Regular | 3460.2 ±3.5 | 3215.7 ±4.5 | 1.08 | — | — |
| raster 24px | NotoNastaliqUrdu-Regular | 4756.4 ±3.4 | 4339.5 ±3.6 | 1.10 | — | — |
| raster 48px | NotoNastaliqUrdu-Regular | 7600.3 ±10.8 | 6616.6 ±5.4 | 1.15 | — | — |
| raster 96px | NotoNastaliqUrdu-Regular | 14619.5 ±11.7 | 11373.2 ±10.0 | 1.29 | — | — |
| cmap | RobotoFlex-Variable | 6752.9 ±11.4 | 7136.4 ±17.9 | 0.95 | 7880.8 ±10.9 | 0.86 |
| advance | RobotoFlex-Variable | 2.6 ±0.0 | 32.1 ±0.1 | 0.08 | 19.3 ±0.0 | 0.13 |
| v advance | RobotoFlex-Variable | 53.4 ±0.3 | — | — | 53.1 ±0.1 | 1.00 |
| v origin | RobotoFlex-Variable | 157.5 ±0.5 | — | — | 138.1 ±0.3 | 1.14 |
| outline unscaled | RobotoFlex-Variable | 616.1 ±1.0 | 638.0 ±2.7 | 0.97 | 635.3 ±1.2 | 0.97 |
| outline 48px | RobotoFlex-Variable | 658.8 ±2.0 | 744.7 ±2.9 | 0.88 | 631.8 ±0.9 | 1.04 |
| raster 12px | RobotoFlex-Variable | 5332.1 ±7.6 | 4672.0 ±5.8 | 1.14 | — | — |
| raster 24px | RobotoFlex-Variable | 10540.1 ±6.9 | 8124.6 ±9.1 | 1.30 | — | — |
| raster 48px | RobotoFlex-Variable | 26250.3 ±20.1 | 16190.4 ±20.6 | 1.62 | — | — |
| raster 96px | RobotoFlex-Variable | 81204.7 ±131.7 | 38955.0 ±48.8 | 2.08 | — | — |
| cmap | SourceSerifVariable-Roman | 7603.9 ±14.4 | 7294.5 ±10.4 | 1.04 | 9866.4 ±18.1 | 0.77 |
| advance | SourceSerifVariable-Roman | 3.9 ±0.0 | 49.6 ±0.2 | 0.08 | 29.3 ±0.1 | 0.13 |
| v advance | SourceSerifVariable-Roman | 63.9 ±0.1 | — | — | 104.3 ±0.1 | 0.61 |
| v origin | SourceSerifVariable-Roman | 211.8 ±0.4 | — | — | 230.7 ±0.2 | 0.92 |
| outline unscaled | SourceSerifVariable-Roman | 1399.1 ±1.5 | 1382.6 ±2.0 | 1.01 | 1457.5 ±2.1 | 0.96 |
| outline 48px | SourceSerifVariable-Roman | 1509.0 ±2.9 | 1610.8 ±1.8 | 0.94 | 1457.4 ±1.4 | 1.04 |
| raster 12px | SourceSerifVariable-Roman | 4254.5 ±5.5 | 3990.6 ±3.3 | 1.07 | — | — |
| raster 24px | SourceSerifVariable-Roman | 5846.6 ±8.0 | 5425.1 ±10.3 | 1.08 | — | — |
| raster 48px | SourceSerifVariable-Roman | 9400.6 ±8.0 | 8228.2 ±13.8 | 1.14 | — | — |
| raster 96px | SourceSerifVariable-Roman | 17611.0 ±14.0 | 13869.8 ±21.7 | 1.27 | — | — |
| HVAR item delta cached | RobotoFlex-Variable.ttf | 186.4 ±0.2 | — | — | — | — |
| HVAR item delta copied font | RobotoFlex-Variable.ttf | 1023.3 ±1.4 | — | — | — | — |
| HVAR item delta different font | RobotoFlex-Variable.ttf | 1017.3 ±1.7 | — | — | — | — |
| HVAR item delta cached | SourceSerifVariable-Roman.ttf | 127.4 ±0.2 | — | — | — | — |
| HVAR item delta copied font | SourceSerifVariable-Roman.ttf | 196.8 ±0.7 | — | — | — | — |
| HVAR item delta different font | SourceSerifVariable-Roman.ttf | 194.7 ±0.8 | — | — | — | — |
| MVAR item delta static default | SourceSerifVariable-Roman.ttf | 0.1 ±0.0 | — | — | — | — |
| HVAR item delta cached | AdobeVFPrototype.otf | 27.5 ±0.0 | — | — | — | — |
| HVAR item delta copied font | AdobeVFPrototype.otf | 52.9 ±0.2 | — | — | — | — |
| HVAR item delta different font | AdobeVFPrototype.otf | 52.5 ±0.3 | — | — | — | — |
| MVAR item delta static default | AdobeVFPrototype.otf | 0.2 ±0.0 | — | — | — | — |
| shaping en-thelittleprince.txt | Roboto-Regular | 57434.0 ±43.5 | — | — | 8745.7 ±8.4 | 6.57 |
| first shaping en-thelittleprince.txt | Roboto-Regular | 109.8 ±0.2 | — | — | 82.3 ±0.2 | 1.33 |
| shaping en-words.txt | Roboto-Regular | 74455.9 ±50.6 | — | — | 15125.0 ±28.0 | 4.92 |
| first shaping en-words.txt | Roboto-Regular | 68.1 ±0.1 | — | — | 74.8 ±0.1 | 0.91 |
| shaping react-dom.txt | SourceSerifVariable-Roman | 437679.5 ±566.9 | — | — | 118728.4 ±61.7 | 3.69 |
| first shaping react-dom.txt | SourceSerifVariable-Roman | 30.7 ±0.1 | — | — | 22.4 ±0.0 | 1.37 |
| shaping hi-words.txt | NotoSansDevanagari-Regular | 78069.7 ±71.0 | — | — | 37566.0 ±32.3 | 2.08 |
| first shaping hi-words.txt | NotoSansDevanagari-Regular | 84.6 ±0.1 | — | — | 61.9 ±0.1 | 1.37 |
| shaping fa-thelittleprince.txt | Amiri-Regular | 127714.8 ±91.6 | — | — | 63283.0 ±68.3 | 2.02 |
| first shaping fa-thelittleprince.txt | Amiri-Regular | 170.8 ±0.3 | — | — | 69.2 ±0.1 | 2.47 |
| shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 376522.9 ±225.2 | — | — | 97938.8 ±128.2 | 3.84 |
| first shaping fa-thelittleprince.txt | NotoNastaliqUrdu-Regular | 334.8 ±1.2 | — | — | 2545.5 ±4.1 | 0.13 |
| shaping fa-words.txt | NotoNastaliqUrdu-Regular | 413220.4 ±512.8 | — | — | 120229.0 ±144.5 | 3.44 |
| first shaping fa-words.txt | NotoNastaliqUrdu-Regular | 288.9 ±0.5 | — | — | 2528.4 ±5.1 | 0.11 |
| shaping fa-thelittleprince.txt | Gulzar-Regular | 7667477.2 ±11404.9 | — | — | 1640005.1 ±2440.9 | 4.68 |
| first shaping fa-thelittleprince.txt | Gulzar-Regular | 2952.1 ±5.2 | — | — | 2567.7 ±4.9 | 1.15 |
| shaping fa-words.txt | Gulzar-Regular | 5276638.8 ±8432.8 | — | — | 1169071.2 ±1354.2 | 4.51 |
| first shaping fa-words.txt | Gulzar-Regular | 1850.3 ±2.9 | — | — | 2229.5 ±3.5 | 0.83 |
| shaping duployan.txt | NotoSansDuployan-Regular | 17949995.2 ±24320.8 | — | — | 2587884.4 ±3111.7 | 6.94 |
| first shaping duployan.txt | NotoSansDuployan-Regular | 61224.3 ±56.2 | — | — | 9769.4 ±23.7 | 6.27 |
| shaping ja-runs.txt | Mplus1p-Regular | 44965.3 ±33.5 | — | — | 28357.7 ±35.4 | 1.59 |
| first shaping ja-runs.txt | Mplus1p-Regular | 27.0 ±0.0 | — | — | 24.0 ±0.0 | 1.12 |
| shaping ja-runs.txt TTB | Mplus1p-Regular | 61682.7 ±143.3 | — | — | 40005.0 ±53.4 | 1.54 |
| first shaping ja-runs.txt TTB | Mplus1p-Regular | 26.8 ±0.0 | — | — | 14.7 ±0.0 | 1.83 |
| shaping role-default-und | extra/arabic | 4.1 ±0.0 | — | — | 2.4 ±0.0 | 1.69 |
| first shaping role-default-und | extra/arabic | 46.5 ±0.1 | — | — | 29.1 ±0.0 | 1.60 |
| shaping role-default-und | extra/devanagari | 4.8 ±0.0 | — | — | 2.8 ±0.0 | 1.68 |
| first shaping role-default-und | extra/devanagari | 160.9 ±0.6 | — | — | 1567.5 ±1.6 | 0.10 |
| shaping role-default-und | extra/khmer | 2.4 ±0.0 | — | — | 1.7 ±0.0 | 1.48 |
| first shaping role-default-und | extra/khmer | 61.6 ±0.1 | — | — | 34.3 ±0.0 | 1.79 |
| shaping role-default-und | extra/myanmar | 5.1 ±0.0 | — | — | 2.7 ±0.0 | 1.91 |
| first shaping role-default-und | extra/myanmar | 160.6 ±0.1 | — | — | 81.6 ±0.1 | 1.97 |
| shaping role-default-und | extra/balinese | 20.5 ±0.0 | — | — | 10.1 ±0.0 | 2.04 |
| first shaping role-default-und | extra/balinese | 213.9 ±0.4 | — | — | 72.8 ±0.1 | 2.94 |
| shaping role-default-und | extra/hebrew | 3.5 ±0.0 | — | — | 2.1 ±0.0 | 1.67 |
| first shaping role-default-und | extra/hebrew | 20.3 ±0.0 | — | — | 15.1 ±0.0 | 1.35 |
| shaping role-default-und | extra/source-serif | 3.2 ±0.0 | — | — | 1.2 ±0.0 | 2.71 |
| first shaping role-default-und | extra/source-serif | 54.1 ±0.1 | — | — | 30.2 ±0.0 | 1.79 |
| shaping role-default-und TTB | extra/source-han | 1.0 ±0.0 | — | — | 0.7 ±0.0 | 1.47 |
| first shaping role-default-und TTB | extra/source-han | 123.5 ±0.1 | — | — | 45.3 ±0.1 | 2.72 |
| shaping role-default-und | extra/amstelvar | 1.4 ±0.0 | — | — | 0.8 ±0.0 | 1.71 |
| first shaping role-default-und | extra/amstelvar | 6.3 ±0.0 | — | — | 15.2 ±0.0 | 0.41 |
| shaping react-dom.txt wght=500 | SourceSerifVariable-Roman | 538554.9 ±295.8 | — | — | 132759.3 ±49.0 | 4.06 |
| subset (42 glyphs, retain-gids) | Roboto-Regular | 321.8 ±2.7 | — | — | 170.7 ±0.6 | 1.88 |
| subset (2322 characters) | Roboto-Regular | 433.2 ±2.3 | — | — | 186.3 ±0.3 | 2.32 |
| subset prepared (42 glyphs, retain-gids) | Roboto-Regular | 213.9 ±0.9 | — | — | 113.4 ±0.7 | 1.89 |
| subset prepared (2322 characters) | Roboto-Regular | 326.6 ±1.6 | — | — | 136.5 ±0.5 | 2.39 |
| preprocess | Roboto-Regular | 254.9 ±3.3 | — | — | 989.2 ±2.9 | 0.26 |
| subset (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 86645.0 ±138.7 | — | — | 21689.8 ±381.7 | 3.99 |
| subset (2322 characters) | SourceHanSans-Regular_subset | 55318.2 ±111.3 | — | — | 21151.2 ±223.9 | 2.62 |
| subset prepared (1918 glyphs, retain-gids) | SourceHanSans-Regular_subset | 63055.2 ±205.6 | — | — | 7400.2 ±261.8 | 8.52 |
| subset prepared (2322 characters) | SourceHanSans-Regular_subset | 31557.2 ±293.2 | — | — | 6475.9 ±36.4 | 4.87 |
| preprocess | SourceHanSans-Regular_subset | 84330.6 ±622.3 | — | — | 80688.0 ±364.8 | 1.05 |
| subset (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 142.2 ±0.5 | — | — | 216.1 ±0.4 | 0.66 |
| subset (2322 characters) | NotoNastaliqUrdu-Bold | 227.5 ±0.6 | — | — | 228.1 ±0.9 | 1.00 |
| subset prepared (5 glyphs, retain-gids) | NotoNastaliqUrdu-Bold | 111.4 ±0.1 | — | — | 79.3 ±0.1 | 1.40 |
| subset prepared (2322 characters) | NotoNastaliqUrdu-Bold | 205.6 ±0.3 | — | — | 94.2 ±0.4 | 2.18 |
| preprocess | NotoNastaliqUrdu-Bold | 74.5 ±0.1 | — | — | 19602.9 ±58.6 | 0.00 |
| subset unicode range 10 | Roboto-Regular.ttf | 81.6 ±0.1 | — | — | 49.1 ±0.1 | 1.66 |
| subset unicode range 10, retain-gids | Roboto-Regular.ttf | 81.7 ±0.1 | — | — | 49.0 ±0.0 | 1.67 |
| subset unicode range 64 | Roboto-Regular.ttf | 217.9 ±0.5 | — | — | 120.6 ±0.4 | 1.81 |
| subset unicode range 64, retain-gids | Roboto-Regular.ttf | 230.5 ±1.4 | — | — | 125.5 ±0.6 | 1.84 |
| subset unicode range 512 | Roboto-Regular.ttf | 1195.7 ±5.0 | — | — | 552.6 ±2.5 | 2.16 |
| subset unicode range 512, retain-gids | Roboto-Regular.ttf | 1223.3 ±4.7 | — | — | 559.8 ±3.7 | 2.19 |
| subset unicode range 1000 | Roboto-Regular.ttf | 1739.8 ±4.9 | — | — | 815.1 ±2.9 | 2.13 |
| subset unicode range 1000, retain-gids | Roboto-Regular.ttf | 1763.1 ±4.4 | — | — | 821.8 ±3.4 | 2.15 |
| subset unicode range 10 | Amiri-Regular.ttf | 190.3 ±0.5 | — | — | 91.2 ±0.4 | 2.09 |
| subset unicode range 10, retain-gids | Amiri-Regular.ttf | 192.3 ±0.9 | — | — | 90.2 ±0.4 | 2.13 |
| subset unicode range 64 | Amiri-Regular.ttf | 402.0 ±2.6 | — | — | 181.7 ±1.1 | 2.21 |
| subset unicode range 64, retain-gids | Amiri-Regular.ttf | 539.7 ±2.3 | — | — | 236.4 ±0.5 | 2.28 |
| subset unicode range 512 | Amiri-Regular.ttf | 5837.0 ±10.6 | — | — | 3499.4 ±4.7 | 1.67 |
| subset unicode range 512, retain-gids | Amiri-Regular.ttf | 6008.1 ±7.2 | — | — | 3521.1 ±11.6 | 1.71 |
| subset unicode range 4096 | Amiri-Regular.ttf | 9600.7 ±24.1 | — | — | 5571.9 ±12.3 | 1.72 |
| subset unicode range 4096, retain-gids | Amiri-Regular.ttf | 9600.4 ±12.0 | — | — | 5579.8 ±7.0 | 1.72 |
| subset unicode range 10 | NotoNastaliqUrdu-Regular.ttf | 94.5 ±0.2 | — | — | 66.0 ±0.2 | 1.43 |
| subset unicode range 10, retain-gids | NotoNastaliqUrdu-Regular.ttf | 115.7 ±0.3 | — | — | 75.2 ±0.4 | 1.54 |
| subset unicode range 64 | NotoNastaliqUrdu-Regular.ttf | 30971.8 ±90.5 | — | — | 3382.3 ±15.0 | 9.16 |
| subset unicode range 64, retain-gids | NotoNastaliqUrdu-Regular.ttf | 30964.4 ±48.3 | — | — | 3393.6 ±11.2 | 9.12 |
| subset unicode range 512 | NotoNastaliqUrdu-Regular.ttf | 109967.3 ±208.0 | — | — | 42516.9 ±119.1 | 2.59 |
| subset unicode range 512, retain-gids | NotoNastaliqUrdu-Regular.ttf | 109826.9 ±164.4 | — | — | 42489.5 ±111.5 | 2.58 |
| subset unicode range 1400 | NotoNastaliqUrdu-Regular.ttf | 109594.6 ±211.4 | — | — | 42542.2 ±117.3 | 2.58 |
| subset unicode range 1400, retain-gids | NotoNastaliqUrdu-Regular.ttf | 109686.8 ±345.3 | — | — | 42442.8 ±125.8 | 2.58 |
| subset unicode range 10 | NotoSansDevanagari-Regular.ttf | 93.4 ±0.2 | — | — | 67.1 ±0.3 | 1.39 |
| subset unicode range 10, retain-gids | NotoSansDevanagari-Regular.ttf | 108.6 ±0.1 | — | — | 73.7 ±0.2 | 1.47 |
| subset unicode range 64 | NotoSansDevanagari-Regular.ttf | 244.7 ±1.9 | — | — | 154.6 ±0.4 | 1.58 |
| subset unicode range 64, retain-gids | NotoSansDevanagari-Regular.ttf | 257.8 ±0.6 | — | — | 160.9 ±0.5 | 1.60 |
| subset unicode range 512 | NotoSansDevanagari-Regular.ttf | 4891.4 ±8.5 | — | — | 2269.5 ±3.3 | 2.16 |
| subset unicode range 512, retain-gids | NotoSansDevanagari-Regular.ttf | 4903.8 ±6.5 | — | — | 2271.1 ±4.4 | 2.16 |
| subset unicode range 1000 | NotoSansDevanagari-Regular.ttf | 4893.1 ±6.8 | — | — | 2274.3 ±5.4 | 2.15 |
| subset unicode range 1000, retain-gids | NotoSansDevanagari-Regular.ttf | 4890.6 ±10.3 | — | — | 2268.3 ±4.0 | 2.16 |
| subset unicode range 10 | Mplus1p-Regular.ttf | 73.7 ±0.1 | — | — | 47.2 ±0.1 | 1.56 |
| subset unicode range 10, retain-gids | Mplus1p-Regular.ttf | 74.1 ±0.2 | — | — | 47.1 ±0.1 | 1.57 |
| subset unicode range 64 | Mplus1p-Regular.ttf | 207.8 ±0.6 | — | — | 113.4 ±0.2 | 1.83 |
| subset unicode range 64, retain-gids | Mplus1p-Regular.ttf | 206.8 ±0.7 | — | — | 113.0 ±0.3 | 1.83 |
| subset unicode range 512 | Mplus1p-Regular.ttf | 734.3 ±3.3 | — | — | 345.6 ±0.6 | 2.12 |
| subset unicode range 512, retain-gids | Mplus1p-Regular.ttf | 959.3 ±2.5 | — | — | 450.5 ±1.2 | 2.13 |
| subset unicode range 4096 | Mplus1p-Regular.ttf | 5241.4 ±10.9 | — | — | 2322.9 ±6.6 | 2.26 |
| subset unicode range 4096, retain-gids | Mplus1p-Regular.ttf | 5402.9 ±14.0 | — | — | 2378.1 ±10.4 | 2.27 |
| subset unicode range 10000 | Mplus1p-Regular.ttf | 11473.2 ±59.1 | — | — | 4892.5 ±24.2 | 2.35 |
| subset unicode range 10000, retain-gids | Mplus1p-Regular.ttf | 11349.1 ±15.3 | — | — | 4864.9 ±36.2 | 2.33 |
| subset unicode range 10 | SourceHanSans-Regular_subset.otf | 223.5 ±0.6 | — | — | 74.0 ±0.3 | 3.02 |
| subset unicode range 10, retain-gids | SourceHanSans-Regular_subset.otf | 46119.9 ±95.9 | — | — | 316.8 ±0.5 | 145.57 |
| subset unicode range 64 | SourceHanSans-Regular_subset.otf | 451.5 ±1.5 | — | — | 140.4 ±0.6 | 3.22 |
| subset unicode range 64, retain-gids | SourceHanSans-Regular_subset.otf | 46326.3 ±53.8 | — | — | 382.9 ±0.9 | 121.00 |
| subset unicode range 512 | SourceHanSans-Regular_subset.otf | 2254.6 ±5.7 | — | — | 981.6 ±4.0 | 2.30 |
| subset unicode range 512, retain-gids | SourceHanSans-Regular_subset.otf | 48181.5 ±127.2 | — | — | 1216.4 ±5.4 | 39.61 |
| subset unicode range 4096 | SourceHanSans-Regular_subset.otf | 54009.8 ±438.1 | — | — | 11636.5 ±237.2 | 4.64 |
| subset unicode range 4096, retain-gids | SourceHanSans-Regular_subset.otf | 73047.6 ±406.1 | — | — | 11237.6 ±82.9 | 6.50 |
| subset unicode range 10000 | SourceHanSans-Regular_subset.otf | 75284.9 ±653.8 | — | — | 14567.2 ±200.7 | 5.17 |
| subset unicode range 10000, retain-gids | SourceHanSans-Regular_subset.otf | 78704.3 ±768.1 | — | — | 14297.0 ±335.8 | 5.50 |
| subset unicode range 10 | SourceSansPro-Regular.otf | 159.9 ±0.2 | — | — | 81.3 ±0.2 | 1.97 |
| subset unicode range 10, retain-gids | SourceSansPro-Regular.otf | 1019.9 ±4.0 | — | — | 181.8 ±0.2 | 5.61 |
| subset unicode range 64 | SourceSansPro-Regular.otf | 377.7 ±2.4 | — | — | 193.0 ±0.7 | 1.96 |
| subset unicode range 64, retain-gids | SourceSansPro-Regular.otf | 1388.5 ±4.8 | — | — | 303.1 ±1.3 | 4.58 |
| subset unicode range 512 | SourceSansPro-Regular.otf | 1951.7 ±2.6 | — | — | 849.9 ±2.3 | 2.30 |
| subset unicode range 512, retain-gids | SourceSansPro-Regular.otf | 2695.7 ±5.7 | — | — | 949.0 ±3.8 | 2.84 |
| subset unicode range 2000 | SourceSansPro-Regular.otf | 4176.8 ±9.0 | — | — | 1783.0 ±3.7 | 2.34 |
| subset unicode range 2000, retain-gids | SourceSansPro-Regular.otf | 4535.2 ±8.0 | — | — | 1822.9 ±4.3 | 2.49 |
