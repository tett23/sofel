# sofel

HarfBuzz（シェーピング、サブセット化）とFreeType（グリフの読み込み、ヒンティング、ラスタライズ）の役割を1つにまとめた、Rustのフォントエンジン。
フォントの表の読み取りと、その結果から作るキャッシュをシェーピングとラスタライズで共有し、両者より速いことを目指す（[ADR 0001](docs/adr/0001-project-goal-and-scope.md)）。
WOFFとWOFF2、Type1やBDFなどの古い形式、AAT、絵文字（COLR、埋め込みビットマップ、SVGのグリフ）、古いフォントのためのヒンティングとラスタライズ、表の検証、SVGとPDFとGPUへの出力までを範囲とする。

- 高速性を最優先にする。HarfBuzzとFreeTypeを相手に、ネイティブとwasmの両方で計測する（[ADR 0006](docs/adr/0006-performance-measurement.md)）。
- wasm（`wasm32-unknown-unknown`）へのビルドを前提とし、ライブラリは `no_std` で書く（[ADR 0004](docs/adr/0004-wasm-and-no-std.md)）。
- 外部のクレートに依存せず、すべて自分で書く。正しさは、HarfBuzzとFreeTypeの出力と突き合わせて確かめる（[ADR 0005](docs/adr/0005-conformance-tests.md)）。

`wght=500` での位置の生成時間は[可変フォントの計測記録](docs/benchmarks/2026-11-11-variable-location-creation.md)にある。

対応範囲を広げる段階と今後の実装予定は[ロードマップ](docs/roadmap.md)に記す。現在の実装を支えるクレート、適合テスト、性能計測の完了条件は [ADR 0045](docs/adr/0045-foundation-completion-criteria.md) を参照。

## 状態

OpenType タグから得るスクリプト値は、[ADR 0050](docs/adr/0050-preserve-opentype-script-values.md) に従い、HarfBuzz が返す4バイトをそのまま保持する。

シェーピングの作業バッファは、[ADR 0051](docs/adr/0051-shaping-buffer-item-size.md) に従い、グリフ情報を1要素28バイト、位置を1要素20バイトで保持する。

2段階目（glyfの輪郭とアンチエイリアスのラスタライズ）まで実装し、6段階目のCFFとCFF2の輪郭、CIDの問い合わせ、PostScriptのフォントの情報（[ADR 0021](docs/adr/0021-cff-outlines.md)）と、CIDとグリフ名を速く引くcharsetの索引（[ADR 0025](docs/adr/0025-cff-charset-index.md)）を実装している。CFF2の輪郭は可変フォントの指定した位置でも取り出せる。

VARC の輪郭は、部品の表示条件、軸座標、変形を評価して指定位置で描く（[ADR 0067](docs/adr/0067-varc-components.md)）。HarfBuzz との比較には、整数の輪郭 API を保ったまま浮動小数点の描画命令を返す経路を使う。

サブセット化（[ADR 0022](docs/adr/0022-subsetting.md)）は、sfntの組み立てと直列化、オフセットのあふれを解く再配置、glyf系の表、cmap、name、OS/2、post、hdmx、CFF（名前のキーとCIDのキー）、VORG、GDEF、GSUB、GPOS、BASEとレイアウトの閉包、COLR（版0、変化量の表を持たない版1、版1の塗りが残らず版0にできるもの、残す版1の塗りとクリップがすべて静的なもの、軸を固定せず変化量ストアを直接またはマップを介して参照するもの、全軸を既定位置に固定したもの）、CPAL、CBLC、CBDT、sbixとCOLRの閉包、MATHとその閉包、軸を指定しないgvarとGVARの書き出し、glyfとCFF2の可変フォントのすべての軸の既定の値への固定（fvar、avar、STATの読み取りと、CFF2をCFFに変換する `DOWNGRADE_CFF2` を含む）、同じフォントを繰り返しサブセット化するための前処理（`Prepared`）、CFFのcharstringの取り出し、グリフ名での指定（`--glyphs`）と、これらのC ABI（`sofel_subset` など）までを実装し、HarfBuzzのサブセット化のテストのうちこれらの表だけを持つ組でバイト単位で一致する。Golemの使い方（グリフIDと `RETAIN_GIDS`）は、HarfBuzzのテストのフォントのうちこれらの表だけを持つものすべてで `hb-subset` と同じバイト列を出す。軸を指定しない HVAR と VVAR は、複数のItemVariationDataから使う行と領域だけを残し、暗黙・明示の送り幅とサイドベアリングのマップを書ける。明示マップの末尾で繰り返す行番号は1項目にまとめる。軸を指定しないGDEF 1.3のItemVariationStoreは、GSUBのFeatureVariationsが軸範囲の条件だけを持つ場合も含めて、残すキャレット、値の記録、アンカー、MATHの定数・斜体補正・上アクセント・カーニング・組み立てが参照する番号と領域を詰めて書く。参照が0件ならストアを除く。MATHだけをコピーして参照番号を詰め直せない場合とFeatureVariationsを持つGSUBをそのままコピーする場合のGDEFの変化量の表と、軸を固定しないCFF2の変化量の表は、10段階目で書く。
[ADR 0061](docs/adr/0061-subset-axis-instancing.md) の部分実装では、fvarとavar版1以外の変化量表、レイアウト・色の表を持たないフォントについて、固定した軸を除き、残る軸の範囲と名前つきインスタンスを更新して書く。STATがある場合は、指定した軸範囲の外の軸値を除いて残す。avar版1がある場合は、既定位置に固定した軸の写像を除き、残る軸の写像を保つ。対象外の表を持つフォントの部分軸固定は、次の VARC の場合を除き引き続き `Unsupported` とする。

VARC のサブセット化は、残す部品の閉包と参照の再採番、条件と変化量ストアの再配置を行う（[ADR 0067](docs/adr/0067-varc-components.md)）。VARC が参照しない1軸を既定値に固定する場合は、gvar、GDEF、GSUB などの対象表が存在しないか捨てられ、avar が版2でなければ部分軸固定できる。これらの対象表と avar が最初からない場合は、同じ軸を既定値以外に固定するか範囲を指定することもできる。VARC をそのまま写す指定は、グリフ ID を保持し、軸を変えない場合だけ受け付ける。

`sofel-read` は、sfntの目録、TTC/OTC、dfont（[ADR 0058](docs/adr/0058-dfont-face-selection.md)）、head、maxp/MAXP、hhea/HHEA、hmtx/HMTX、cmap（フォーマット0〜15）、loca、glyf（[ADR 0007](docs/adr/0007-sfnt-read-foundation.md)、[0011](docs/adr/0011-glyf-outlines.md)）と、OS/2、name、post、vhea/VHEA、vmtx/VMTX、VORG（[ADR 0016](docs/adr/0016-font-and-vertical-metrics.md)）、BASE（[ADR 0017](docs/adr/0017-base-baselines.md)）、CFFとCFF2（[ADR 0021](docs/adr/0021-cff-outlines.md)）、GDEF、GSUB、GPOS（[ADR 0020](docs/adr/0020-shaping-foundation.md)）、MATH、fvar、avar、STAT（[ADR 0022](docs/adr/0022-subsetting.md)）を読み、各表の検証器を持つ。VARC は版1のヘッダ、部品記録、条件、変化量ストアを検査しながら読む（[ADR 0067](docs/adr/0067-varc-components.md)）。

大文字の HHEA と VHEA は、32ビットのメトリクス件数を読む（[ADR 0054](docs/adr/0054-wide-metrics-header-count.md)）。
可変フォントの位置でFreeType互換の生の塗り、停止点、クリップ矩形を読むには `FtPaintGraph` を使う。FreeType互換のCOLRの生の塗りを複数グリフで調べるときは、`RawPaintGraph::new` の結果を再利用できる。
COLRv1の描画は、Rustの `paint`・`paint_recording` とCの `sofel_paint_glyph` に可変フォントの位置を渡して記録できる。
MVARのメトリクス差分は、Rustの `Instance::metrics_variation` とCの `sofel_instance_metrics_variation` で丸める前の値を取得できる。
さらに、[ADR 0027](docs/adr/0027-variable-fonts.md) の可変フォントの表（ItemVariationStore、DeltaSetIndexMap、HVAR、VVAR、MVAR、gvar、GVAR、avar版2、FeatureVariations）を読み、検証する。`sofel-subset` はFeatureVariationsをこのビューから読む。glyfのFreeType互換の送り幅は、位置設定後の初回 `FT_Get_Advances` の値に合わせる（[ADR 0048](docs/adr/0048-freetype-advance-query-state.md)）。
FeatureVariations版1.1以降のlookup条件も読み、軸の位置に合うlookupをシェーピング計画に加える（[ADR 0046](docs/adr/0046-harfbuzz-lookup-variations.md)）。
範囲と実装の順序は [ADR 0001](docs/adr/0001-project-goal-and-scope.md)、クレートの構成は [ADR 0003](docs/adr/0003-crate-layers.md) を参照。
仕様は [docs/specifications.md](docs/specifications.md) にまとめる。フォントの用語と sofel の型名は[用語集](docs/glossary.md)で引ける。
シェーピング計画の生成時間は、[Coverageの走査を打ち切る計測](docs/benchmarks/2026-11-11-shape-coverage-digest.md) に記録している。
可変フォントの座標と問い合わせは [ADR 0027](docs/adr/0027-variable-fonts.md)、変化量の評価は [ADR 0029](docs/adr/0029-variation-deltas.md)、評価の修正は [ADR 0030](docs/adr/0030-variation-evaluation-corrections.md) で採択している。C API は位置ごとの ItemVariationStore の行の差分も返す。
`sofel-font::var` には、設計座標と正規化座標からの位置の構築、座標の正規化、ItemVariationStoreの差分の評価の核、軸と名前つきインスタンスの問い合わせ、FreeTypeのMMVarの読み取り、位置ごとのPostScript名を実装した。名前つきインスタンスに専用のPostScript名がない場合は、FreeTypeと同じ規則で名前を組み立てる。
FreeTypeが合成する既定の名前つきスタイルは、名前が欠ける場合も件数と既定番号を再現する（[ADR 0036](docs/adr/0036-freetype-missing-default-named-style.md)）。
C ABIには位置の作成、座標の取得、Variationの文字列変換、軸と名前つきインスタンスの問い合わせ、位置ごとのPostScript名を追加した。
さらに、CFF2の `blend` は指定した位置の係数で評価し、輪郭とグリフの箱に反映する。Private DICTも位置ごとに読みなおす。C ABIの `sofel_cff2_blend_vector` は、既定成分と0の列を含む係数列を返す。
さらに、メトリクス、基準線、輪郭、シェーピング、塗りの既存関数には位置の引数を追加した。
C API の `sofel_scale_create_with_ppem` は、シェーピング用スケールに横と縦の ppem を指定する（[ADR 0052](docs/adr/0052-c-shaping-ppem.md)）。従来の `sofel_scale_create` と NULL のスケールでは両軸とも 0 になる。
レイアウト問い合わせ（[ADR 0062](docs/adr/0062-layout-query-device-adjustments.md)）は、Rust の `Instance::layout_queries` に位置、スケール、横と縦の ppem を渡す。GPOS、GDEF の合字キャレット、BASE、MATH は同じ Device 評価を使い、MATH の定数、グリフの値、カーニング、異体グリフと組み立て部品を取得する。C ABI の `sofel_layout_ligature_carets` と `sofel_math_*` は列の開始位置、容量、格納件数、総数を分けて返す。`HbScale::with_ptem` と `sofel_scale_create_with_ptem` は、スタイルの光学サイズを問い合わせるためのポイントサイズを渡す。
可変フォントの位置を指定すると、HVARとVVARの送り幅、gvarの幻の点から求める送り幅とグリフの箱、VORGとVVARの縦原点、MVARのメトリクス、BASEのVariationIndexを評価する。FreeTypeの方針ではgvarの輪郭とCFF2の輪郭を位置に応じて描き、HarfBuzzの方針ではFeatureVariationsとGPOSのVariationIndexをシェーピングに適用する。COLRv1の塗りとクリップの箱にも位置を反映する。
`sofel-shape`、`sofel-render`、`sofel-paint` は `--variations` と `--named-instance` で位置を受け付ける。
非既定位置のシェーピングでは、水平送り幅を1回の呼び出し内で128枠のキャッシュに保存する。既定位置ではキャッシュを作らない。Rustの呼び出し側も、特定の `Instance` に結びつく `AdvanceCache` を保持できる（[ADR 0049](docs/adr/0049-hvar-advance-cache.md)）。計測結果は [HVAR送り幅キャッシュ](docs/benchmarks/2026-11-11-hvar-advance-cache.md) に記録する。
ライセンスと取得元を固定した追加フォントの検証は [ADR 0031](docs/adr/0031-diverse-font-corpus.md) に従って実装した。
文書だけの変更でCIを省く方針は [ADR 0032](docs/adr/0032-documentation-only-ci.md) と [ADR 0034](docs/adr/0034-ci-document-input-classification.md) に従って実装している。
FreeTypeとHarfBuzzとの性能の比較は [docs/benchmarks/](docs/benchmarks/) に記録する（最新は [追加フォント](docs/benchmarks/2026-11-11-extra-fonts.md)、[2段階目](docs/benchmarks/2026-11-10-stage2.md)、[CFFとCFF2](docs/benchmarks/2026-11-10-cff.md)、[charsetの索引](docs/benchmarks/2026-11-10-cff-charset-index.md)、[COLRv0の画像](docs/benchmarks/2026-11-10-colrv0-raster.md)、[シェーピング](docs/benchmarks/2026-11-10-shaping.md)、[サブセット化](docs/benchmarks/2026-11-10-subset.md)、[前処理つきのサブセット化](docs/benchmarks/2026-11-10-subset-prepared.md)）。
ライブラリの使い方は [docs/api.md](docs/api.md) にまとめる（各項目の説明はrustdoc）。

| クレート | 役割 |
|---|---|
| [`sofel-geom`](crates/sofel-geom) | FreeTypeと同じ結果を返す固定小数点の演算と、輪郭を受け渡す `Pen`、HarfBuzzと同じ丸め（`sofel_geom::float`）。カラーグリフの塗りの命令を受け渡す `Painter` と塗りの値、変換、どの環境でも同じ結果の三角関数、塗りの補助の関数（[ADR 0023](docs/adr/0023-color-glyphs.md)） |
| [`sofel-unicode`](crates/sofel-unicode) | Unicode 18.0.0の文字の性質（General_Category、Script、正規分解と合成、Vertical_Orientationなど。[ADR 0019](docs/adr/0019-unicode-character-data.md)） |
| [`sofel-read`](crates/sofel-read) | sfntの表の読み取りと検証。コピーも確保もせず、どの入力でもpanicしない |
| [`sofel-font`](crates/sofel-font) | フォントを一度だけ読み、グリフの数、文字からグリフへの対応、送り幅、フォント全体と縦書きのメトリクス、BASEの基準線とその代わりの値、名前とグリフ名（postとCFFのcharset）をHarfBuzzの方針で返し、HarfBuzzと同じ拡大縮小（`HbScale`）をする。HarfBuzzのCFFとCFF2のアクセラレータ（`HbCff1`、`HbCff2`）を持つ。PostScript名と `FT_Face` の値はFreeTypeの方針。TrueType の制御値は指定した軸位置で取得できる |
| [`sofel-outline`](crates/sofel-outline) | glyfとGLYFの輪郭を、ヒンティングなしでFreeTypeと同じ座標で `Pen` に渡す。HarfBuzzと同じグリフの箱（glyf、CFF2、CFF。`HbExtents`、[ADR 0021](docs/adr/0021-cff-outlines.md)）を求める |
| [`sofel-shape`](crates/sofel-shape) | HarfBuzz 15.1.0のOTシェーパーと同じシェーピング（バッファ、区間の性質、計画、正規化、`GSUB` と `GPOS`、フォールバックの位置。[ADR 0020](docs/adr/0020-shaping-foundation.md)）、機能の文字列、直列化、言語と文字体系のタグ。文字体系ごとのシェーパー（hebrew、hangul、thai、arabic、indic、khmer、myanmar、USE。[ADR 0024](docs/adr/0024-complex-shapers.md)）もすべて実装した |
| [`sofel-inflate`](crates/sofel-inflate) | 出力上限を指定した DEFLATE・zlib・gzip の展開 |
| [`sofel-png`](crates/sofel-png) | PNG の各色型と Adam7 をアルファ乗算済み BGRA に復号する（[ADR 0066](docs/adr/0066-color-glyph-pixels.md)） |
| [`sofel-bitmap`](crates/sofel-bitmap) | 埋め込みビットマップ（CBLC/CBDT、EBLC/EBDT、`bloc`/`bdat`、sbix）のストライク、その選び方（HarfBuzzとFreeTypeの両方）、グリフの画像のデータとメトリクス、EBDTの画素のFreeTypeと同じ書き込み、CBDT と sbix の PNG の復号（[ADR 0023](docs/adr/0023-color-glyphs.md)） |
| [`sofel-color`](crates/sofel-color) | カラーグリフ：CPALのパレット、COLR（版0と版1）、SVG、CBDT、sbixをHarfBuzzと同じ命令で `Painter` に渡す。gzip 圧縮された SVG は展開して渡す。命令の記録（`Recording`）、カラーグリフを含むグリフの箱、フォールバックのための問い合わせ（[ADR 0023](docs/adr/0023-color-glyphs.md)） |
| [`sofel-write`](crates/sofel-write) | sfntの組み立てと、HarfBuzzと同じ詰め方のオブジェクトの直列化（[ADR 0022](docs/adr/0022-subsetting.md)） |
| [`sofel-subset`](crates/sofel-subset) | HarfBuzzの `hb-subset` とバイト単位で同じ出力のサブセット化（[ADR 0022](docs/adr/0022-subsetting.md)。書ける表はglyf系、cmap、name、OS/2、post、hdmx、CFF、VORG、GDEF、GSUB、GPOS、BASE、COLR、CPAL、CBLC、CBDT、sbix、MATH、STAT、軸をすべて固定したCFF2など。軸はすべて既定の値に固定できる。前処理（`Prepared`）で同じフォントの計画を速く作れる） |
| [`sofel-vector`](crates/sofel-vector) | 配置した単色の輪郭を SVG または PDF のページに書く。SVG には変換と矩形クリップを指定して PNG も配置できる（[ADR 0068](docs/adr/0068-vector-and-gpu-output.md)） |
| [`sofel-raster`](crates/sofel-raster) | FreeTypeのsmoothのラスタライザと同じ計算で、8bitのアンチエイリアスの画像を作る。COLRv0の層をFreeTypeと同じ整数の計算でBGRAの画像に重ねる（`blend_layer`）。水平・垂直 LCD は Harmony と5タップフィルタに対応する |
| [`sofel-woff`](crates/sofel-woff) | 単一フェイスの sfnt を WOFF・WOFF2 に変換するライブラリとコマンド |
| [`sofel-cli`](crates/sofel-cli) | 小さな道具（`sofel-validate`、`sofel-render`、`sofel-paint`、`sofel-shape`、`sofel-subset`） |
| [`sofel-capi`](crates/sofel-capi) | C ABI（ヘッダは [`sofel.h`](crates/sofel-capi/include/sofel.h)、[ADR 0015](docs/adr/0015-c-abi.md)） |
| [`sofel-conformance`](crates/sofel-conformance) | FreeTypeとHarfBuzzとの突き合わせのテスト（`cargo test` では走らない） |

シェーピングのフォント関数を差し替えるときは、`sofel-shape::SubFont` で親からの継承と問い合わせ別の上書きを指定する。子の送り、原点、グリフの箱、輪郭は親子のスケールを換算する。合成太字と斜体はシェーピングと輪郭に反映する。`sofel-shape` コマンドでは `--font-slant`、`--font-bold`、`--font-grade` を指定できる（[ADR 0063](docs/adr/0063-font-functions-and-synthesis.md)）。

FreeTypeの合成操作には、`sofel-outline::Scratch` のスロット用の斜体と太字、`sofel-bitmap` の画素形式変換と太字を使う。ビットマップの太字では `embolden_bitmap_slot` が画像とスロットのメトリクス、配置を合わせて更新する。HarfBuzzに合わせた輪郭の命令は、glyfを `Scratch::hb_decompose`、CFFとCFF2を `HbExtents::cff_glyph_path` から得る。画像を含むカラーグリフの合成斜体は、`sofel-color::paint_recording_at_with_slant` が描画命令を変換で包む（[ADR 0063](docs/adr/0063-font-functions-and-synthesis.md)）。

`sofel-validate` は CFF2 の FDSelect フォーマット4を互換性の警告として報告する。ほかにエラーがなければ検証は成功する（[ADR 0056](docs/adr/0056-cff2-fdselect-format4-warning.md)）。

### `sofel` で文字列を描く

リポジトリのルートで次のコマンドを実行すると、`sofel` をインストールできます。

```sh
cargo install --path crates/sofel-cli --bin sofel
sofel help
```

手元のアウトラインフォントを `font.ttf` に指定すると、一行の文字列を白地に黒の PGM 画像に書き出せます。
同じ見本を繰り返し使うときは UTF-8 のテキストファイルで渡せます。

```sh
sofel render --ppem 48 --text "office & affix" font.ttf > sample.pgm
sofel render --ppem 48 --text-file sample.txt font.ttf > sample.pgm
```

合字の効果は `--features=-liga` を付けた画像と比較できます。
グリフ番号と配置は `sofel shape`、カラーグリフの描画命令は `sofel paint` で調べられます。
各操作の引数と使用例は `sofel help render` のように操作名を指定して表示します。

### `sofel-render`

可変フォントの輪郭には `--variations=wght=500` や `--named-instance=0` で位置を指定できる。既定以外の位置では埋め込みビットマップを使わず、輪郭を描く。

1グリフをヒンティングなしで描き、PGM（P5、白地に黒）を標準出力に書く（[ADR 0012](docs/adr/0012-antialiased-rasterizer.md)）。FreeTypeの `FT_Render_Glyph` と同じ画像になる。そのppemの埋め込みビットマップがあれば、FreeTypeと同じく `--no-bitmap` が無いかぎりそれを書く（32bitはPAM。[ADR 0023](docs/adr/0023-color-glyphs.md)）。`--color` では、COLRv0、輪郭でクリップした COLRv1 の線形・放射グラデーションと、矩形、円、パスを単色や線形・放射グラデーションで塗る OT-SVG のグリフをPAMで書く（[ADR 0072](docs/adr/0072-color-gallery-acceptance.md)）。COLRv0の色はFreeTypeの `FT_LOAD_COLOR` と同じで、`--palette N` でパレットを選ぶ。OT-SVG では同じ文書内の図形を `use` で再利用し、`clipPath` で塗る範囲を指定できる（[ADR 0074](docs/adr/0074-svg-feature-conformance.md)）。

要求した ppem に一致するストライクがなくても、`--color` は sbix と CBDT の PNG を双線形補間して描く。`--no-bitmap` はこれらの画像を除く。

CLI のレイヤー選択と描画制御は独自実装で、画素合成には `sofel-raster` を使う（[ADR 0055](docs/adr/0055-rewrite-cli-color-rendering.md)）。

```sh
cargo run --release --bin sofel-render -- --ppem 48 --char U+0041 font.ttf > a.pgm
cargo run --release --bin sofel-render -- --ppem 48 --color --char U+1F600 emoji.ttf > a.pam
```

### `sofel-shape`

HarfBuzz 15.1.0の `hb-shape` と同じオプションと出力で文字列を形にする（[ADR 0020](docs/adr/0020-shaping-foundation.md)）。文字列は行ごとに形にし、1行に1行を書く。

```sh
cargo run --release --bin sofel-shape -- font.ttf 'Hello'
cargo run --release --bin sofel-shape -- --unicodes U+0041,U+0056 --ned --show-flags font.ttf
cargo run --release --bin sofel-shape -- --direction=ttb --features=vrt2 --output-format=json --text-file=ja.txt font.otf
cargo run --release --bin sofel-shape -- --variations=wght=700 --text=Hello variable.ttf
```

AAT の `morx` と旧 `mort` による非文脈置換、並べ替え、マークしたグリフと現在のグリフの文脈置換、挿入、合字に対応する。`feat` を持つフォントでは、指定した OpenType 機能を AAT のセレクターへ変換し、クラスタの範囲ごとに置換を選ぶ。`ltag` を持つフォントでは、指定言語に対応する `morx` の置換を選ぶ。AAT 置換を選ぶ文字体系では、専用の OT シェーパーを HarfBuzz の `dumber` に切り替える。`kerx` の形式 6 によるクラスカーニングと、旧 `kern` の形式 0〜3 に対応する。旧 `kern` では、グリフ対、状態機械、クラスによる位置調整にも対応する。
`--font-ptem` でポイントサイズを指定すると、`STAT` と `trak` を持つフォントではそのサイズに応じて字間を調整する。
`--variations` は軸のタグと設計座標を、`--named-instance` は0から数える名前つきインスタンスを指定する（[ADR 0027](docs/adr/0027-variable-fonts.md)）。言語は `--language`、無ければ `--default-language` で、どちらも無ければ「言語なし」とする（`hb-shape` は `$LANG` を読む）。終了コードは、成功が0、フォントか文字列のファイルを読めなければ1、使い方の誤りと解釈しないオプション（`--font-bold` など）が2、`--verify` の検査の失敗が3。オプションの一覧は `sofel-shape --help` で示す。
### `sofel-paint`

可変フォントの色の描画には、たとえば `--variations=wght=500` を指定する。`--named-instance=0` は先頭の名前つきインスタンスを選ぶ。FreeTypeの輪郭とラスタライズでは、FreeTypeが末尾に補った既定インスタンスの番号も指定できる。

カラーグリフの塗りの命令を、HarfBuzzの `test/api/test-paint.c` と同じ字下げのテキストで標準出力に書く（[ADR 0023](docs/adr/0023-color-glyphs.md)）。数はCの `%.3g`、`--exact` では `f32` のビットの16進。カラーでないグリフは終了コード1。

```sh
cargo run --release --bin sofel-paint -- --palette 0 --foreground 000000ff --char U+1F600 emoji.ttf
```

### `sofel-subset`

HarfBuzzの `hb-subset` と同じオプションでサブセット化する（[ADR 0022](docs/adr/0022-subsetting.md)）。出力は `hb-subset` 15.1.0とバイト単位で一致させる。まだ書けない表や、変化量の表を持つCOLRを含むフォント（版1の塗りが残らず版0にできる場合と、残す版1の塗りとクリップがすべて静的な場合、軸を固定せず変化量ストアを直接またはマップを介して参照する場合、全軸を既定位置に固定する場合を除く）は、その表を `--drop-tables` で捨てないかぎり誤りとなる（可変フォントの表は、すべての軸を既定の値に固定すれば捨てられる）。`--variations` は、すべての軸を既定の値に固定する指定（`*=drop`、各軸の `drop` か既定の値）と、上記の条件を満たす部分軸固定を受け付ける。`--preprocess` はフォントを前処理してから計画を作る（出力は同じ）。`--glyphs` はグリフ名（postとCFFの名前、`gidDDD`、`uniXXXX`）で指定し、`--gids` などとはコマンドラインの順に働く。
全軸を既定値に固定すると、BASE のベースライン座標を保ち、変化量の表とその参照を除く。
軸を固定しない場合も、BASE の座標に VariationIndex の参照がなければ、使われない変化量の表を除く。
適合テストで出力が違えば、表ごとに最初の差の位置を示す。片方だけにある表は、どちらに欠けているかも示す。

軸を固定しない CFF2 は `--desubroutinize` を指定するとサブセット化できる。指定しない場合は、残すグリフがサブルーチンを呼ばなければ未使用のサブルーチンを除き、呼ぶ場合は即値の番号で呼ぶ global/local のサブルーチンを必要な分だけ残して番号を詰める。`blend`、`vsindex` と VariationStore を残す。ヒントを保持する場合は `hintmask` と `cntrmask` のデータバイトを解釈器に従って読み飛ばす。`--no-hinting` では、ヒントが描画命令より前にあり、同じ charstring 内で先行する call がなく、引数が同じ charstring 内の連続した数値だけで、使用するサブルーチンがヒント除去後も空にならず、`blend`、`vsindex`、`dotsection` を含まない場合に限ってヒントを除く。サブルーチン先頭の stem に対しては、呼び出し直前に積んだ連続する数値の引数も除く。命令が空か `return` だけのサブルーチンは、その呼び出しとともに除く。同じサブルーチンのマスク長が呼び出しごとに異なる場合は、まだ対応していない。

```sh
sofel-subset --font-file=Roboto.ttf --unicodes=41-5A -o out.ttf
```

フォントを読むコマンド（`sofel-shape`、`sofel-render`、`sofel-paint`、`sofel-subset`、`sofel-validate`）は WOFF 1.0、gzip および UNIX compress（LZW）で包んだ入力を署名から識別する。[ADR 0065](docs/adr/0065-legacy-faces-and-webfonts.md)

### `sofel-woff`

単一フェイスの TrueType、CFF、CFF2 を Web 配布用に変換する。入力と出力にはファイル名か `-`（標準入力・標準出力）を指定する。[ADR 0070](docs/adr/0070-webfont-encoding.md)

```sh
sofel-woff --format woff font.ttf -o font.woff
sofel-subset --font-file=font.ttf --unicodes=41-5A -o - | sofel-woff --format woff2 - -o subset.woff2
```

### `sofel-validate`

sfnt、TTC/OTC、dfontの各表を検証する（[ADR 0008](docs/adr/0008-validate-command-and-freetype-comparison.md)、[ADR 0058](docs/adr/0058-dfont-face-selection.md)）。誤りを1件1行で出し、`--json` ではJSON Linesで出す。

```sh
cargo run --release --bin sofel-validate -- font.ttf
cargo run --release --bin sofel-validate -- --json font.ttf
```

GSUB、GPOS、GDEF に破損があると、その表をシェーピングで無視する場合は警告を出す。元の表を検査して、フェイス、表タグ、バイト位置ごとに破損を数える。参照先を検査できなかった場合は、確認できた件数を下限として示し、未検査の理由を添える（[ADR 0059](docs/adr/0059-layout-sanitizer-diagnostics.md)）。

終了コードは、`Error` が無ければ0、あれば1、使い方の誤りか読めないファイルがあれば2。出力の形とJSONのキーは `sofel-validate --help` で示す。

## 開発

CI は、用途を確認して `ci/documentation-only.rules` の `document` に登録した文書だけの変更では、ビルドとテストを省く（[ADR 0032](docs/adr/0032-documentation-only-ci.md)、[ADR 0034](docs/adr/0034-ci-document-input-classification.md)）。
現在は `docs/` 以下とリポジトリ直下の README、CONTRIBUTING の名前を登録し、テスト入力の `docs/ci-fixtures/input.txt` は `input` に登録している。
入力に一致するパス、未知のパス、規則自体の変更があれば通常の CI を実行する。
新たに文書をビルドやテストの入力にする際は、同じ変更で `input` 規則を追加する。

文書だけの変更でも、文書の検査と ADR、監査、計測記録の保護は実行する。
通常のCIでは、LinuxとmacOSで生成器の検査と適合テストを並列に実行する（[ADR 0039](docs/adr/0039-generator-ci-parallelism.md)）。
`ci-result` は、判定と保護、および全ジョブの結果を確認する。

クローン後、gitのhookを有効化する。コミット済みのADRは [adr-guard](https://github.com/tett23/adr-guard) で保護しており、手元に adr-guard が要る。

```sh
git config core.hooksPath .githooks
```

設計判断は `docs/adr/` にADRとして記録する。

ツールチェーンは `rust-toolchain.toml` で固定している。テストは次のとおり走らせる。

```sh
cargo test
cargo clippy --workspace --all-targets -- -D warnings
sh scripts/check-deps.sh   # クレートの依存が ADR 0003 の表に沿い、sofel-capi 以外が unsafe を禁じているか（jq が要る）
sh scripts/run-tests.sh    # シェルのスクリプトのテスト
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p sofel-geom -p sofel-unicode -p sofel-read -p sofel-font -p sofel-write -p sofel-subset -p sofel-outline -p sofel-bitmap -p sofel-color -p sofel-raster -p sofel-cli -p sofel-capi
```

C API でテキスト形式や JSON のグリフ列を読み込んだバッファは、パースエラーの後も、クリアすれば元の設定で再利用できる（[ADR 0053](docs/adr/0053-preserve-buffer-settings-after-glyph-deserialization.md)）。

C ABI（[ADR 0015](docs/adr/0015-c-abi.md)）のテストには、Cのコンパイラと `nm` と jq が要る。Cのテストの期待値 `crates/sofel-capi/tests/c/fixture.h`、`fixture_shape.h`、`fixture_subset.h` はRustのAPIから作り、`cargo test` が最新であることを確かめる（`SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test fixture` で書きなおす）。シェーピングとGolemのサブセット化のCのテスト（[ADR 0020](docs/adr/0020-shaping-foundation.md)、[ADR 0022](docs/adr/0022-subsetting.md)）はテストのデータのフォントを読み（場所は `SOFEL_TESTDATA`、既定は `target/testdata`）、データが無ければ飛ばす。`SOFEL_REQUIRE_TESTDATA=1` で飛ばさずに失敗させる。

```sh
sh scripts/test-capi.sh        # ネイティブで走らせ、公開のシンボルとヘッダの一致も確かめる
sh scripts/test-capi.sh tsan   # ThreadSanitizer
sh scripts/test-capi.sh asan   # AddressSanitizer
sh scripts/test-capi.sh wasm   # wasi-sdk で wasm32-wasip1 にビルドし、wasmtime で走らせる
sh scripts/check-capi.sh       # ライブラリの公開の関数がすべてC ABIにあるか、除外の理由があるか
```

ライブラリのクレートは公開の項目すべてにrustdocを書く（`#![warn(missing_docs)]`）。使い方の手引きは [docs/api.md](docs/api.md)。

FreeTypeとの突き合わせ（[ADR 0005](docs/adr/0005-conformance-tests.md)、[0008](docs/adr/0008-validate-command-and-freetype-comparison.md)）には、固定したコミットのHarfBuzzとFreeTypeの取得と、`ft-dump` のビルド（Cのコンパイラが要る）が要る。Vertical_Orientationの照合には、`scripts/fetch-ucd.sh` で取得したUCDのファイルが要る。HarfBuzzとのメトリクスとUnicodeの文字の性質の突き合わせ（[ADR 0016](docs/adr/0016-font-and-vertical-metrics.md)、[ADR 0019](docs/adr/0019-unicode-character-data.md)）、丸めの突き合わせ（[ADR 0026](docs/adr/0026-harfbuzz-rounding.md)）には、HarfBuzzの `src/harfbuzz.cc` を取りこんだ `hb-dump` のビルド（C++のコンパイラが要る）が要る。期待する失敗は `conformance/expected-failures-freetype.txt`、`conformance/expected-failures-otvalid.txt`（[ADR 0020](docs/adr/0020-shaping-foundation.md) のotvalidとの照合）、`conformance/expected-failures-harfbuzz.txt` と `conformance/expected-failures-serialize.txt`（[ADR 0020](docs/adr/0020-shaping-foundation.md) の直列化の照合）に理由を添えて書く。
シェーピングの適合テスト（HarfBuzzの `test/shape` のすべての行と、`hb-dump --shape`、`--shape-random`、`--layout-map` との突き合わせ。[ADR 0020](docs/adr/0020-shaping-foundation.md)）の期待する失敗は、`conformance/expected-failures.txt`、`expected-failures-shape-pairs.txt`、`expected-failures-shape-random.txt`、`expected-failures-layout-map.txt` に、分類（`shaper:<名前>`、`variations`、`aat`、`synthetic`、`cff`、`color`、`out-of-scope:<名前>`）を1つずつ添えて書く。テストは分類がフォントと計画に合うことも確かめる。`--font-ptem` 単独の `trak` は `aat` とし、`variations` に含めない（[ADR 0028](docs/adr/0028-ptem-is-not-opsz.md)）。`SOFEL_SHAPE_VERBOSE=1` で違いを、`SOFEL_SHAPE_SUGGEST=1` で一覧に無い失敗の分類の候補を出す。
シェーピングの適合テストでは、dfont と破損したレイアウト表の固定ケースが HarfBuzz と一致する（[ADR 0058](docs/adr/0058-dfont-face-selection.md)、[ADR 0059](docs/adr/0059-layout-sanitizer-diagnostics.md)）。残る期待失敗は[ロードマップ](docs/roadmap.md)に記す。

追加フォントの適合テスト（[ADR 0031](docs/adr/0031-diverse-font-corpus.md)）は、OFL 1.1またはApache-2.0の13ファイルを固定し、TTCを含む57フェイスを調べる。バイナリはリポジトリに含めず、`fonts.lock` のSHA-256とバイト数で取得する。各フォントの取得元、著作権表示とライセンス全文は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) に記録する。`conformance/fonts.tsv` はファイル、ライセンス、検証する機能を結び、`conformance/font-cases.tsv` は可変フォントの各軸の最小・最大を含むシェーピングの入力と座標を固定する。機能がまだ実装されていないケースの理由は `conformance/expected-failures-extra-fonts.txt` にケースIDごとに記す。非既定座標のメトリクスと輪郭は、Arabic、Source Serif、Amstelvarの代表10ケースで実際の位置を渡し、固定版HarfBuzzとFreeTypeにそれぞれ照合する。`conformance/expected-failures-extra-variation.txt` は、この照合の期待失敗を記す。既定座標の一致と区別する。

```sh
sh scripts/fetch-fonts.sh
SOFEL_FONTS_OFFLINE=1 sh scripts/fetch-fonts.sh  # キャッシュとライセンスを再検証する
cargo test --release -p sofel-conformance --test extra_registry --test extra_structure --test extra_cmap --test extra_metrics --test extra_outline --test extra_shape --test extra_color --test extra_variation
```

追加テストの固定した参照値は、固定版のFreeTypeとHarfBuzzから生成したテキストである。nativeでは `ft-dump` と `hb-dump` をビルドして参照値を再生成し、wasmでは同じテキストと照合する。フォントやライセンスが一つでも欠ければテストは失敗する。

サブセット化の適合テスト（HarfBuzzの `test/subset` の1123件。[ADR 0022](docs/adr/0022-subsetting.md)）の期待する失敗は、`conformance/expected-failures-subset.txt` に理由（`stage:<段階>`、`deferred:instance`、`deferred:iup`、HarfBuzzの `meson.build` が外していて期待するファイルの無い組の `upstream:disabled`）を添えて書く。各組は `run-tests.py` と同じく前処理の有無で2回走らせ、前処理を使う回の識別子の末尾には `+prepared` を付ける（期待する失敗の理由は前処理の無い回と同じ）。再配置は、固定の種から作ったオブジェクトのグラフ（任意のグラフと、分割や拡張への昇格が要るGSUBとGPOSの表）を `hb-dump --repack` と突き合わせ、出力のバイト列（長さとハッシュ）か失敗の別が一致することを確かめる（`harfbuzz_repack`）。期待する出力の無い組（全フォントでのGolemの使い方とその前処理つき、グリフ名での指定、`repack_tests`）は `hb-dump --subset` の出力と比べ（`harfbuzz_subset_hb`）、期待する失敗は `conformance/expected-failures-subset-hb.txt` に書く。`harfbuzz_subset` は `hb-dump` も `ft-dump` も要らず、`wasm32-wasip1` でも走る。

```sh
sh scripts/fetch-testdata.sh
sh scripts/fetch-ucd.sh
sh tools/ft-dump/build.sh
sh tools/hb-dump/build.sh
cargo test --release -p sofel-conformance
```

ラスタの適合テストはFreeTypeの出力をグリフごとに比較する。参照出力の全文や成功した全グリフの識別子は保持しない（[ADR 0037](docs/adr/0037-stream-raster-reference.md)）。
輪郭の適合テストもグリフごとの比較を共用し、命令、overlap、制御箱を照合する（[ADR 0040](docs/adr/0040-stream-outline-reference.md)）。

適合テストを最大2本の実行ファイルで並列に走らせるには、`sh scripts/test-conformance.sh 2` を使う（`jq` が必要。[ADR 0033](docs/adr/0033-ci-latency-and-ligature-iteration.md)）。
引数を1にすると直列で実行する。
計測用の HarfBuzz は、元の include の順序を保った最大4個の翻訳単位に分け、native と wasm のビルドを並行したままコンパイラのメモリ使用量を抑える（[ADR 0038](docs/adr/0038-partition-benchmark-harfbuzz-units.md)）。
CI ではこの runner を使い、参照ツールと計測用の C ライブラリは入力とツールチェーンが一致する場合にキャッシュから復元する。
直列化の適合テストは、入力を一行ずつ書き、HarfBuzz の参照出力も一行ずつ比較して、全文の重複保持を避ける（[ADR 0035](docs/adr/0035-stream-serialization-reference-output.md)）。
カラーグリフの適合テストはHarfBuzzの塗りをグリフごとに読み、各本文を比較する（[ADR 0043](docs/adr/0043-stream-harfbuzz-paint-records.md)）。
SVG の機能別の検査は `sh scripts/test-svg-conformance.sh` で実行する。ギャラリーを生成して入力と出力の全31ケースを表示し、失敗や未実行の観測が残っていれば終了コード 1 を返す（[ADR 0074](docs/adr/0074-svg-feature-conformance.md)）。
シェーピングの適合テストではフォントと文章の組ごとに一致行の数を集計し、不一致行の詳細をカテゴリ判定に使う（[ADR 0041](docs/adr/0041-compact-shape-comparison-results.md)）。
テストと結果の照合は、キャッシュがある場合も実行する。
CI の参照ツールのビルド、適合テストと計測用 C ビルドは、子プロセスを含む合計 RSS の最大観測値をログへ書く。

Unicodeの文字の表（`crates/sofel-unicode/src/tables.rs`、`crates/sofel-font/src/tables/arabic_pua.rs` と、シェーパーの分類の表 `crates/sofel-shape/src/tables/` の `indic.rs`、`use_.rs`、`arabic.rs`、`vowel_constraints.rs`）は生成してコミットする（[ADR 0019](docs/adr/0019-unicode-character-data.md)）。`scripts/fetch-ucd.sh` が `ucd.lock` のUCDのファイルを `target/ucd/` に取得してSHA-256を確かめ、`tools/gen-unicode/` が表を書く。アラビア文字の私用領域の表と、シェーパーの表のHarfBuzzの入力（Microsoftの `src/ms-use/` と状態機械の番号）は、`scripts/fetch-testdata.sh` で取得したHarfBuzzのファイルから読む。CIは作りなおした表とコミットしたものの一致を確かめる。

```sh
sh scripts/fetch-ucd.sh
sh scripts/fetch-testdata.sh
cargo run --release --manifest-path tools/gen-unicode/Cargo.toml
cargo test --manifest-path tools/gen-unicode/Cargo.toml
```

`sofel-shape` の言語のタグの表（`crates/sofel-shape/src/tag_table.rs`）は、`scripts/fetch-testdata.sh` で取得したHarfBuzzの `src/hb-ot-tag-table.hh` から `tools/gen-ot-tag/` が書き、コミットする（[ADR 0020](docs/adr/0020-shaping-foundation.md)）。CIは作りなおした表とコミットしたものの一致を確かめる。

```sh
sh scripts/fetch-testdata.sh
cargo run --release --manifest-path tools/gen-ot-tag/Cargo.toml
cargo test --manifest-path tools/gen-ot-tag/Cargo.toml
```

`sofel-shape` の音節の状態機械の表（`crates/sofel-shape/src/tables/machine_*.rs`）は、HarfBuzzの `*-machine.rl` から写した文法（`tools/gen-machine/grammars/`）から `tools/gen-machine/` が書き、コミットする（[ADR 0024](docs/adr/0024-complex-shapers.md)）。CIは作りなおした表とコミットしたものの一致を確かめる。

```sh
cargo run --release --manifest-path tools/gen-machine/Cargo.toml
cargo test --manifest-path tools/gen-machine/Cargo.toml
```

wasmでは、`wasm32-unknown-unknown` へのビルドと、`wasm32-wasip1` での単体テストを確かめる（[ADR 0004](docs/adr/0004-wasm-and-no-std.md)）。単体テストには [wasmtime](https://wasmtime.dev/) が要る。

```sh
cargo build --release --target wasm32-unknown-unknown -p sofel-read -p sofel-geom -p sofel-unicode -p sofel-font -p sofel-write -p sofel-subset -p sofel-outline -p sofel-bitmap -p sofel-color -p sofel-raster -p sofel-shape -p sofel-capi
CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime cargo test --target wasm32-wasip1 -p sofel-read -p sofel-geom -p sofel-unicode -p sofel-font -p sofel-write -p sofel-subset -p sofel-outline -p sofel-bitmap -p sofel-color -p sofel-raster -p sofel-shape
# サブセット化の適合テストをwasmで（リポジトリと target/testdata が見えるように）
CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=$PWD::$PWD" cargo test --release --target wasm32-wasip1 -p sofel-conformance --test harfbuzz_subset
```

Roboto と Comfortaa のラスタライズについて、12、24、48、96 ppem で全グリフを FreeType と比較した[照合記録](docs/benchmarks/2026-11-11-unexcluded-phantom-point-check.md)がある。

性能の計測（[ADR 0013](docs/adr/0013-benchmark-harness.md)）は、ワークスペースの外の `bench/` で行う。テストのデータの取得のあと、FreeTypeとHarfBuzzを静的ライブラリにビルドする（C++のコンパイラが要る。wasmでは `bench/fetch-wasi-sdk.sh` がwasi-sdkを取得する）。`--once` は各項目を1回だけ走らせ、結果の照合だけを行う。シェーピングの項目（[ADR 0020](docs/adr/0020-shaping-foundation.md)）は、計測の前に全行の結果をHarfBuzzと比べる。サブセット化の項目（[ADR 0022](docs/adr/0022-subsetting.md)）は、計測の前に出力のバイト列のハッシュをHarfBuzzと比べる。記録は `docs/benchmarks/` に置く。

SourceSerifVariable-Romanの `wght=500` でのシェーピングは `--filter 'shaping react-dom.txt wght=500'` で選ぶ。全行の照合とnative・wasmの測定値は [可変位置のシェーピング計測](docs/benchmarks/2026-11-11-variable-shaping.md) に記録した。

可変フォントの変化量は `--filter 'item delta'` で計れる。HVAR の係数を再利用できる場合と計算し直す場合、および既定位置の MVAR を個別に表示する。

HarfBuzz と同じ文字数でサブセット化を計るには、計測の引数に `--filter 'subset unicode range'` を渡す。静的フォント7個について、既定の設定とグリフIDを保つ設定の両方を計る。

追加フォントは `--filter extra` で選べる。cmap・advanceに加え、輪郭、縦advance、MATH定数、カラー形式、シェーピングの代表ケースを比較する（[ADR 0031](docs/adr/0031-diverse-font-corpus.md)）。条件と計測値は [追加フォントの役割別計測](docs/benchmarks/2026-11-11-extra-font-roles.md) に記す。

```sh
sh bench/build-c.sh
cargo run --release --manifest-path bench/Cargo.toml            # Markdownの表を書く
sh bench/build-c.sh wasm32-wasip1
cargo build --release --manifest-path bench/Cargo.toml --target wasm32-wasip1
wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm
```

ファジングには、nightlyのツールチェーンと [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) が要る。対象は `cmap`、`metrics`、`font`、`outline`、`os2`、`name`、`post`、`vorg`、`base`、`gdef`、`gsub`、`gpos`、`cff`、`shape`（シェーピング）、`deserialize`（直列化の読み込み）。

```sh
cd fuzz
cargo +nightly fuzz run cmap -- -max_total_time=60
```

## License

MIT（[LICENSE](LICENSE)、[ADR 0002](docs/adr/0002-license-and-reuse.md)）。流用したコードの著作権表示は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) に載せる。

Unicode Character Databaseから生成した表を含む `sofel-unicode` と `sofel-shape` は、Unicode License v3にも従う（`MIT AND Unicode-3.0`）。

FreeTypeの計算を移植した部分を含むクレート（`sofel-geom` など）は、FreeType License（FTL）にも従う（`MIT AND FTL`）。FTLの求めにより、次の文を載せる。

> Portions of this software are copyright © 2026 The FreeType Project (https://freetype.org). All rights reserved.

### フォントごとの描画見本

リポジトリ直下で `sh scripts/render-font-gallery.sh` を実行し、`target/font-gallery/index.html` をブラウザーで開く。
Rust のツールチェーンと、適合テストと同じフォント取得環境を使う。生成器とそのテストは `tools/font-gallery` の Rust プログラムである。
各フェイスの単字と、その書体に合う文字列を一枚の PNG で見比べられる。
sbix の見本は 109 ppem のストライクを使い、グリフ 19 の単字と `simple_linear` の合字 26 をそれぞれ 128 × 128 画素で表示する。
CFF の COLRv1、OT-SVG、可変 COLRv1 の見本は 48 ppem で表示する。カラーの4書体は単字と文字列の両方を検査する。
CI では Actions の対象の実行からコミット SHA 付きのフォントギャラリーをダウンロードし、展開したディレクトリの `index.html` を開く。
保存期間は14日間である（[sofel ADR 0069](docs/adr/0069-font-render-gallery.md)）。
