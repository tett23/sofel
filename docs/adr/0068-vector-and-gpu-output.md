# ADR 0068: 配置済みグリフ列を SVG、PDF、GPU に出力する

ステータス: 採択（未実装）

## 文脈

同じグリフ列を画面と紙に出すには、輪郭、色、クリップを各出力先に引き渡す必要がある。

## 調査した事項

[ADR 0001](0001-project-goal-and-scope.md) は SVG と PDF の出力、GPU 用データと4言語のシェーダーを範囲に含める。
固定した HarfBuzz は SVG と PDF の描画処理を src/hb-vector-paint-svg.cc と src/hb-vector-paint-pdf.cc に分けている。
test/api/test-gpu.cc は空の輪郭、接する輪郭、クリップの深さの境界を扱う。

## 実装すること

呼び出し側が配置済みのグリフ ID、原点、スケール、軸座標、色から輪郭と塗りを求める。
sofel-vector は Pen と Painter 経由で受け取った命令を SVG または PDF のページに書く。
[ADR 0003](0003-crate-layers.md) に従い、sofel-vector と sofel-gpu はフォントの型に依存しない。
PDF では文字を輪郭として書き、元フォントを埋め込まない。
PDF で直接表せない合成は固定した HarfBuzz の写像を使い、PLUS は Screen、XOR は Difference、SRC_IN は Normal として出力する。
sofel-gpu は輪郭と塗りを Slug 方式のデータに符号化し、呼び出し側にバッファの形式と対応する WGSL、GLSL、MSL、HLSL のシェーダーを提供する。
同じ入力の CPU 描画を照合に使えるよう、原点、変換、クリップ、色の指定を出力先で共有する。

## 実装しないこと

GPU デバイスの作成、ウィンドウ管理、PDF の文字検索は対象外とする。

## 不変条件

1. SVG と PDF の各ページは入力の寸法とグリフの配置を保ち、参照出力と同じ領域に同じ順で描く。
2. 同じ入力と符号化設定で GPU に渡す論理レコードの値、順序、件数が HarfBuzz GPU の出力と一致する。

| 境界の入力 | 期待する結果 |
| --- | --- |
| グリフ0件、1件、同位置に異なる色の2件 | ページの寸法を保ち条件1を満たす |
| 空輪郭、接する輪郭、クリップの深さが参照実装の上限と上限を1超えた入力 | 打ち切りを含め条件2を満たす |

## テスト設計

I1_vector_page は HarfBuzz の test/vector の入力で SVG の期待ファイルと照合し、パス、変換、塗り、画像、クリップと命令の順序を検査する。
PDF はページ寸法を取り出し、参照 PDF と同じレンダラー、解像度、背景色で画素を比較する。
半透明の図形を重ね、PLUS、XOR、SRC_IN の各合成結果も照合する。
I2_gpu_records は論理レコードをダンプし、違う場合は先頭のレコード番号を示す。
さらに4言語のシェーダーをそれぞれコンパイルし、描画テストでは同じ原点と座標で輪郭の内点、外点、重なりの色を CPU 描画と比較する。

## トレードオフ

輪郭を使う PDF は元フォントがない環境でも表示できる一方、文字検索や選択に必要なテキスト情報を持たない。
GPU のテストには符号化だけを調べる環境と、シェーダーを動かして画素を調べる環境が要る。

## 参考文献

- [HarfBuzz のベクトル描画テスト](https://github.com/harfbuzz/harfbuzz/tree/3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372/test/vector)
- [HarfBuzz の GPU テスト](https://github.com/harfbuzz/harfbuzz/blob/3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372/test/api/test-gpu.cc)
