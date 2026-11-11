# HVAR と MVAR の変化量の計測

2026-11-11 に Apple M1 Max で sofel のコミット 23f0a51 のベンチマークを実行した。release ビルドを使い、native と Wasmtime 50.0.1 の順に `--filter 'item delta'` で計測した。他のビルドが動きうる開発機での測定である。

同じ内容を別の領域にコピーしたフォントでは、HVAR のキャッシュを使えず、領域ごとの係数を再計算する。コピー時の中央値をキャッシュ使用時の中央値で割ると、Roboto Flex では native で 6.94 倍、Wasmtime で 6.90 倍、Source Serif では 2.02 倍と 1.67 倍、Adobe VF Prototype では 2.13 倍と 1.86 倍だった。MVAR の 0.0 は小数第 1 位までの表示による。

## Native

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| HVAR item delta cached | RobotoFlex-Variable.ttf | 74.1 ±0.3 | — | — | — | — |
| HVAR item delta copied font | RobotoFlex-Variable.ttf | 514.5 ±2.4 | — | — | — | — |
| HVAR item delta different font | RobotoFlex-Variable.ttf | 517.3 ±3.3 | — | — | — | — |
| HVAR item delta cached | SourceSerifVariable-Roman.ttf | 39.6 ±0.2 | — | — | — | — |
| HVAR item delta copied font | SourceSerifVariable-Roman.ttf | 80.1 ±0.4 | — | — | — | — |
| HVAR item delta different font | SourceSerifVariable-Roman.ttf | 79.6 ±0.2 | — | — | — | — |
| MVAR item delta static default | SourceSerifVariable-Roman.ttf | 0.0 ±0.0 | — | — | — | — |
| HVAR item delta cached | AdobeVFPrototype.otf | 9.8 ±0.1 | — | — | — | — |
| HVAR item delta copied font | AdobeVFPrototype.otf | 20.9 ±0.2 | — | — | — | — |
| HVAR item delta different font | AdobeVFPrototype.otf | 20.7 ±0.2 | — | — | — | — |
| MVAR item delta static default | AdobeVFPrototype.otf | 0.1 ±0.0 | — | — | — | — |

## Wasmtime

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| HVAR item delta cached | RobotoFlex-Variable.ttf | 116.8 ±1.5 | — | — | — | — |
| HVAR item delta copied font | RobotoFlex-Variable.ttf | 806.3 ±2.5 | — | — | — | — |
| HVAR item delta different font | RobotoFlex-Variable.ttf | 818.1 ±2.9 | — | — | — | — |
| HVAR item delta cached | SourceSerifVariable-Roman.ttf | 71.3 ±1.0 | — | — | — | — |
| HVAR item delta copied font | SourceSerifVariable-Roman.ttf | 119.0 ±0.4 | — | — | — | — |
| HVAR item delta different font | SourceSerifVariable-Roman.ttf | 119.9 ±1.0 | — | — | — | — |
| MVAR item delta static default | SourceSerifVariable-Roman.ttf | 0.1 ±0.0 | — | — | — | — |
| HVAR item delta cached | AdobeVFPrototype.otf | 16.1 ±0.1 | — | — | — | — |
| HVAR item delta copied font | AdobeVFPrototype.otf | 29.9 ±0.2 | — | — | — | — |
| HVAR item delta different font | AdobeVFPrototype.otf | 29.7 ±0.1 | — | — | — | — |
| MVAR item delta static default | AdobeVFPrototype.otf | 0.2 ±0.0 | — | — | — | — |
