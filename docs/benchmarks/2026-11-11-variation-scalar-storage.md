# 可変フォントの位置が保持する係数の記憶量

[sofel ADR 0030](../adr/0030-variation-evaluation-corrections.md) の3フォントについて、`Location::new` に `wght=500` を渡した直後の係数の記憶量を測った。macOS arm64 のコミット `63b7e0f` で、各ストアの `regions.capacity()` に係数の大きさ（4バイト）を掛け、HarfBuzz用とFreeType用に分けて合計した。gvarの共有タプルについても `gvar_hb` と `gvar_ft` の容量を同じ方法で合計した。

| フォント | ストアのHarfBuzz用係数 (B) | ストアのFreeType用係数 (B) | gvarの両方式の係数 (B) | 係数合計 (B) |
|---|---:|---:|---:|---:|
| RobotoFlex-Variable | 608 | 608 | 672 | 1,888 |
| SourceSerifVariable-Roman | 24 | 24 | 16 | 64 |
| AdobeVFPrototype | 80 | 80 | 0 | 160 |

このほかに位置本体が624バイトあり、設計座標と正規化座標は別途ヒープを使う。表の値はアロケータの管理領域や一時的な作業領域を含まない。
