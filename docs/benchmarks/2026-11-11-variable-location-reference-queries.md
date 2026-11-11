# 可変位置の作成と参照ライブラリの初回問い合わせ

[sofel ADR 0029](../adr/0029-variation-deltas.md) の3フォントを使い、同じフェイスの `wght` を499と500の間で交互に変えた。1回の計測単位は1回の位置変更と、それに続く問い合わせである。[同一座標の再設定の計測](2026-11-11-variable-location-creation.md)と入力を変えたのは、FreeTypeが設定済みの座標を検出して処理を省くためである。

sofelは位置と領域の係数を作り、Aの横送りを取得する。FreeTypeは座標設定後にAの輪郭を読み、横送りを取得する。HarfBuzzは座標設定後にx-height、ラテン文字のローマン基準線、Aの箱を問い合わせ、AVをシェーピングしてAの横送りを取得する。各フェイスとHarfBuzzのバッファは使い回した。表には問い合わせそのものの実行時間も含む。参照ライブラリの内部で作られた係数の個数は計測していない。

macOS arm64のreleaseビルドで `sofel-bench --filter 'location with reference queries'` を実行した。wasm32-wasip1ではWasmtime 50.0.1を使った。値は標準ハーネスの中央値と中央絶対偏差である。

## Native

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location with reference queries wght=499/500 | RobotoFlex-Variable | 6.1 ±0.0 | 1.1 ±0.0 | 5.43 | 1.9 ±0.0 | 3.15 |
| location with reference queries wght=499/500 | SourceSerifVariable-Roman | 0.7 ±0.1 | 0.5 ±0.0 | 1.52 | 0.9 ±0.0 | 0.79 |
| location with reference queries wght=499/500 | AdobeVFPrototype | 14.5 ±0.1 | 4.8 ±0.1 | 3.00 | 4.2 ±0.0 | 3.50 |

## wasm32-wasip1

| 項目 | フォント | sofel (µs) | FreeType (µs) | sofel ÷ FreeType | HarfBuzz (µs) | sofel ÷ HarfBuzz |
|---|---|---:|---:|---:|---:|---:|
| location with reference queries wght=499/500 | RobotoFlex-Variable | 8.8 ±0.0 | 1.8 ±0.0 | 4.86 | 2.4 ±0.0 | 3.67 |
| location with reference queries wght=499/500 | SourceSerifVariable-Roman | 0.9 ±0.0 | 0.8 ±0.0 | 1.12 | 1.2 ±0.0 | 0.70 |
| location with reference queries wght=499/500 | AdobeVFPrototype | 18.3 ±0.1 | 6.8 ±0.0 | 2.67 | 5.9 ±0.0 | 3.12 |
