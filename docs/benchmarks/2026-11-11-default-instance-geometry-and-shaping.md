# 既定位置の箱、輪郭、シェーピングの比較

[sofel ADR 0029](../adr/0029-variation-deltas.md) の既定位置の計測として、位置APIの導入前（`d445749`）と可変フォント対応の統合後（`4e3e23a`）を比較した。macOS arm64で各コミットからreleaseビルドした標準ハーネスを使い、各フィルタに `glyph extents`、`outline unscaled`、`shaping react-dom.txt` を指定した。表はsofelの中央値と中央絶対偏差（µs）。送り幅は[別の計測](2026-11-11-default-instance-advances.md)に記録した。

| 項目 | フォント | 導入前 | 統合後 |
|---|---|---:|---:|
| glyph extents | AdobeVFPrototype | 1047.6 ±6.7 | 1011.8 ±0.7 |
| outline unscaled | AdobeVFPrototype | 3765.7 ±27.4 | 3510.3 ±27.4 |
| outline unscaled | RobotoFlex-Variable | 328.3 ±1.8 | 327.2 ±1.5 |
| outline unscaled | SourceSerifVariable-Roman | 736.7 ±6.6 | 747.0 ±0.6 |
| shaping react-dom.txt | SourceSerifVariable-Roman | 202432.6 ±865.6 | 203690.1 ±1144.3 |
