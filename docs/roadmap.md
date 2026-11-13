# ロードマップ（放置していること）

この文書は、2026-11-12 時点で sofel に残っている作業と、利用者の判断を待っている事項を一か所にまとめたものである。利用者と、Claude Code や Codex のような道具が、次に何をするかを選ぶために使う。仕様ではないので、決まったことは [docs/specifications.md](specifications.md) と各 ADR に書く。この文書は、作業が済むたびに該当する項を消す。

各項は、何が残っているか、どこに由来するか、何があれば進められるかの順に書く。節と項は、ほかの項が依存するものを先に置いた。確認できなかった事項には「未確認」と書いた。

## HarfBuzz・FreeType 互換に必要な機能

ここでいう互換は、[ADR 0001](adr/0001-project-goal-and-scope.md) が対象にした機能で、同じ入力と設定から参照実装と同じ結果を得ることを指す。次の表には実装済みの機能も含む。C の呼び出し口は [ADR 0015](adr/0015-c-abi.md) の sofel 独自 ABI を使う。

| 照合先 | 機能 | 照合する結果 |
| --- | --- | --- |
| HarfBuzz | OpenType シェーピング。Unicode 正規化、文字体系別の処理、GSUB・GPOS、縦書き、フォールバック、表のサニタイズ | グリフ ID、クラスタ、送り幅、オフセット、グリフフラグ |
| HarfBuzz | AAT シェーピング。morx・mort の置換、kerx・kern のカーニング、trak の字間調整 | 指定した機能と文字サイズでのグリフ列と位置 |
| HarfBuzz | レイアウトの問い合わせ。BASE の基準線、GDEF の合字キャレット、MATH、スタイル値、ppem に応じた Device 補正 | 指定した文字体系、軸座標、サイズでの値 |
| HarfBuzz | サブセット化と可変フォントのインスタンス化。軸の固定と範囲制限 | 出力フォントの表、グリフの対応、残した文字のシェーピング結果 |
| FreeType | sfnt とコレクション、dfont、WOFF・WOFF2、PostScript 系、PFR、BDF・PCF・FNT・FON、圧縮フォントの読み取り | フェイスの選択、文字とグリフの対応、メトリクス、輪郭またはビットマップ |
| FreeType | glyf・CFF・CFF2・VARC、Type1・CID・PFR の輪郭と、TrueType v35・v40、PostScript 系、自動ヒンタによるヒンティング | 指定したサイズとロードフラグでの輪郭点、送り幅、境界 |
| FreeType | グレースケール・モノクロ・LCD・SDF のラスタライズ、埋め込みビットマップ | ビットマップの寸法、原点、画素値 |
| 両方 | 可変フォントの軸と名前つきインスタンス、メトリクス・輪郭・配置・色への変化量の適用、合成太字・斜体 | 同じ軸座標と変形条件で各参照実装が返す値 |
| 両方 | COLRv0・COLRv1、CPAL、CBDT・sbix、OT-SVG のカラーグリフ | パレット、描画命令、復号した画像、合成後の画素 |

すべての機能の実装が終わってから高速化に着手する。実装中は [ADR 0005](adr/0005-conformance-tests.md) に従って出力を照合し、[ADR 0006](adr/0006-performance-measurement.md) に従って性能を記録する。下記の性能差の原因調査と最適化は実装完了後に行う。

### Golem が使う OpenType 機能タグの列挙

Golem はフォントを開くときに GSUB と GPOS の機能タグをすべて読み、指定した機能を持つフォントがない場合に診断を出す。[ADR 0020](adr/0020-shaping-foundation.md) の FeatureList は Rust からタグを読める。Golem の Haskell バインディングからも使えるよう、サニタイズ後の各表のタグを列挙する C API を [ADR 0075](adr/0075-layout-feature-tags.md) で採択した。表がない場合、複数のタグがある場合、同じタグが繰り返される場合の件数と順序を HarfBuzz の `hb_ot_layout_table_get_feature_tags` と照合する。

### FreeType のグリフの縁取り

縁取りした文字を描く用途では、FreeType のグリフストローカが元の輪郭から線幅、端点、接合部の指定に応じて内側と外側の輪郭を作る。閉じたグリフ輪郭を対象に、この操作を [ADR 0076](adr/0076-glyph-outline-stroking.md) で採択した。`FT_Glyph_Stroke` と `FT_Glyph_StrokeBorder` を照合先とする。[FreeType の Glyph Stroker](https://freetype.org/freetype2/docs/reference/ft2-glyph_stroker.html)

## 未実装の ADR

### [ADR 0022](adr/0022-subsetting.md): サブセット化

[ADR 0022](adr/0022-subsetting.md) は採択（未実装）である。可変フォントの読み取りは [ADR 0027](adr/0027-variable-fonts.md) で実装した。サブセット化では、名前レコードの上書きと VARC の部品参照の再採番まで実装した。残る作業には、10段階目の CFF2 サブルーチンのヒント除去と、[ADR 0061](adr/0061-subset-axis-instancing.md) の軸の固定・範囲変更がある。表ごとの対応範囲は[仕様書](specifications.md)に記す。

### シェーピング、フォントの検証と描画、照合の記録

cmap のフォーマット2と8は、通常のグリフ検索では読み、シェーピングでは HarfBuzz に合わせて対応なしとする処理を実装済みである。[仕様書](specifications.md) に両方の検索規則を記載している。

[ADR 0057](adr/0057-record-unexcluded-phantom-point-check.md)：[ADR 0018](adr/0018-phantom-point-shift.md) は採択を維持する。ネイティブと wasm の両方で、グリフを除外せずに照合した。[4サイズの照合結果](benchmarks/2026-11-11-unexcluded-phantom-point-check.md)を記録した。

## HarfBuzz のサニタイザの移植

[ADR 0044](adr/0044-shaping-conformance-exceptions.md) で認めた dfont の差は、[ADR 0058](adr/0058-dfont-face-selection.md) に従って解消した。
固定した `in-house/collections.tests:1` は HarfBuzz と一致した。シェーピングの全5850件では5632件が通り、残る218件は期待する失敗である。dfont を含めた FreeType の654ファイルでは9113項目が一致し、生成した2フェイスの dfont でも cmap とメトリクスの4項目が一致した。

- [ADR 0059](adr/0059-layout-sanitizer-diagnostics.md) に従い、GSUB、GPOS、GDEF のサニタイズを HarfBuzz 15.1.0 に合わせた。シェーピング、レイアウト選択、検証器で表の採否を共有する。`in-house/harfbust.tests:4`、レイアウト選択の3件、無作為な文字列のシェーピングの3件を照合し、7件すべてで HarfBuzz と一致し、`sanitize` の期待失敗を削除した。

`sofel-validate` は、表を破棄しても元のデータの検証を続ける。1回の検証で確認できたすべての破損箇所について、フェイス、表タグ、バイト位置、理由をテキストと JSON に出し、同じ箇所を重複させずに数える。未検査範囲があれば、件数が下限であることと、その範囲と理由を示す。シェーピングが表を無視することは、破損箇所数に含めず、破棄した表ごとに警告する。

## v1 リリース時の性能比較

v1 リリース時に、[ADR 0006](adr/0006-performance-measurement.md) に従って sofel と HarfBuzz・FreeType の対応する処理を同じ条件でベンチマークし、比較結果を `docs/benchmarks/` に記録する。

## 性能の目標に届いていないもの

[ADR 0006](adr/0006-performance-measurement.md) の目標は、すべての項目で HarfBuzz と FreeType より遅くならないことである。以下の数値は、`docs/benchmarks/` の記録と、集めた報告による。報告のうち、記録の中で確かめていない数値（Roboto 5.5倍など）は未確認である。

### シェーピング（長い文章）

文章全体のシェーピングが、HarfBuzz の 1.4〜5.5 倍の時間がかかる。Roboto が 5.5 倍と 5.1 倍、SourceSerif の react-dom が 3.1 倍、Mplus1p の日本語が横書き 1.6 倍と縦書き 1.4 倍である。最初のシェーピングは速く、0.35〜0.81 倍である。原因は、ClassDef と Coverage の cache を [ADR 0020](adr/0020-shaping-foundation.md) が測るまで後回しにしたことだと推測している。これは測っておらず、推測である。記録は [docs/benchmarks/2026-11-10-shaping.md](benchmarks/2026-11-10-shaping.md) にある。進めるには、プロファイルで原因を確かめ、cache を足すかどうかを決める ADR を書く。

### サブセット化

サブセット化が、前処理なしで HarfBuzz の 0.7〜4.3 倍かかる。前処理つきの SourceHanSans の CID（グリフ番号の保持）が 7.2 倍で最も遅い。Golem の日本語の経路がこれである。記録は [docs/benchmarks/2026-11-10-subset.md](benchmarks/2026-11-10-subset.md) と [docs/benchmarks/2026-11-10-subset-prepared.md](benchmarks/2026-11-10-subset-prepared.md) である。

### COLRv0 のカラーのラスタライズ

TwemojiMozilla.subset が、FreeType の 1.16〜1.45 倍かかる。wasm では 1.2〜1.5 倍である。`render_color` が、レイヤーとパレットの配列を割り当て、グリフごとに COLR を開き直していることが原因と見ている。記録は [docs/benchmarks/2026-11-10-colrv0-raster.md](benchmarks/2026-11-10-colrv0-raster.md) である。割り当てと開き直しを避ける変更で解消できる見込みである。

### Unicode の性質

Unicode の性質の引きが HarfBuzz の 1.24 倍である。2段または3段の表という制限のため、一般カテゴリと文字体系の表が HarfBuzz の深い表より大きくなる。表の構造を変えるかどうかを決める必要がある。

### RobotoFlex（重なりのある輪郭）のラスタライズ

[ADR 0014](adr/0014-raster-sweep-touched-cells.md) の実装は [PR #13](https://github.com/tett23/sofel/pull/13) でマージ済みである。ADR の本文にある計測では、修正後に FreeType の 0.92 倍と 0.93 倍になった。ただし `docs/benchmarks/` に [ADR 0014](adr/0014-raster-sweep-touched-cells.md) のベンチマークの記録がなく、これを足すのが残っている。報告では、記録を足す作業が別のブランチ（`bench-record-adr-0014`）で進んでいるとのことだが、このブランチはこの時点でマージされていない（未確認）。進めるには、負荷のないマシンで計測して記録を足す。

### 負荷の高いマシンで取った記録

負荷平均が最大 280 のマシンで取った記録がある。負荷のないマシンで取り直すべきものは、[ADR 0013](adr/0013-benchmark-harness.md) の2段階目の記録（[docs/benchmarks/2026-11-10-stage2.md](benchmarks/2026-11-10-stage2.md)）、[ADR 0019](adr/0019-unicode-character-data.md)、[ADR 0021](adr/0021-cff-outlines.md)（[docs/benchmarks/2026-11-10-cff.md](benchmarks/2026-11-10-cff.md)）、[ADR 0023](adr/0023-color-glyphs.md)、[ADR 0025](adr/0025-cff-charset-index.md)（[docs/benchmarks/2026-11-10-cff-charset-index.md](benchmarks/2026-11-10-cff-charset-index.md)）、[ADR 0020](adr/0020-shaping-foundation.md)、[ADR 0022](adr/0022-subsetting.md) である。どの記録が高負荷で取られたかを、記録の本文では確かめていない（未確認）。

## 後続の実装と設計

次の項は、各 ADR の「後回しにしたもの」に挙げられたものと、そこから派生するものである。互換機能の設計は次の ADR を採択した。

- [ADR 0060: AAT の置換と位置調整をシェーピングに組み込む](adr/0060-aat-shaping.md)
- [ADR 0061: 軸を固定または制限して可変フォントを書き出す](adr/0061-subset-axis-instancing.md)
- [ADR 0062: レイアウトの問い合わせでサイズと軸座標を引き継ぐ](adr/0062-layout-query-device-adjustments.md)
- [ADR 0063: サブフォントの関数と合成太字、合成斜体を適用する](adr/0063-font-functions-and-synthesis.md)
- [ADR 0064: ヒンティングと描画モードを独立に指定する](adr/0064-freetype-hinting-and-render-modes.md)
- [ADR 0065: 旧形式と Web フォントを既存のフェイスとして開く](adr/0065-legacy-faces-and-webfonts.md)
- [ADR 0066: カラーグリフの画像を復号し、塗りを画素に合成する](adr/0066-color-glyph-pixels.md)
- [ADR 0067: VARC の条件付き部品を指定座標で展開する](adr/0067-varc-components.md)
- [ADR 0068: 配置済みグリフ列を SVG、PDF、GPU に出力する](adr/0068-vector-and-gpu-output.md)

先に、可変フォントの [ADR 0027](adr/0027-variable-fonts.md) に依存する項を並べる。

- 任意の位置への可変フォントのインスタンス化と、軸の範囲を絞る部分的なインスタンス化。[ADR 0022](adr/0022-subsetting.md) の「後回しにしたもの」にある。[ADR 0027](adr/0027-variable-fonts.md) の実装後に [ADR 0061](adr/0061-subset-axis-instancing.md) に従って進める。解消すると、サブセット化のテストの期待する失敗のうち、インスタンスの91件と IUP の23件が消える。
- ppem を使った Device 表の補正（ヒンティングの Device）。GPOS、GDEF のキャレット、BASE、MATH が対象である。[ADR 0017](adr/0017-base-baselines.md) による。変化量の部分は [ADR 0027](adr/0027-variable-fonts.md) に入る。
- GDEF のリガチャのキャレットの API。[ADR 0027](adr/0027-variable-fonts.md) の一覧にある。
- スタイルの値 `hb_style_get_value`。[ADR 0027](adr/0027-variable-fonts.md) の一覧にある。
- MATH の問い合わせ（`hb_ot_math_*`）と、JSTF の扱いの決定。[ADR 0017](adr/0017-base-baselines.md) による。MATH のビューは [ADR 0022](adr/0022-subsetting.md) で入っている。
- AAT のシェーピング（10段階目）。morx、mort、kerx、ankr、trak、feat、ltag、dumber シェーパー、旧 `kern` 表、`--font-ptem` による trak。[ADR 0020](adr/0020-shaping-foundation.md) による。[ADR 0060](adr/0060-aat-shaping.md) で実装した。固定版 HarfBuzz とのシェーピング比較では `aat` 分類の期待失敗は0件である。
- ヒンティング（7段階目）。TrueType バイトコード（v35、v40）、cvar、幻の点の丸め（[ADR 0018](adr/0018-phantom-point-shift.md)）、CFF のヒンターとステムの太らせ（[ADR 0021](adr/0021-cff-outlines.md)）、オートヒンター、mono と LCD と SDF のラスタライズ。
- 合成の太字と斜体。HarfBuzz の `hb_font_set_synthetic_*`、FreeType の `FT_GlyphSlot_Embolden`、`FT_GlyphSlot_Oblique`、`FT_Bitmap_Embolden`。[ADR 0016](adr/0016-font-and-vertical-metrics.md)、[ADR 0020](adr/0020-shaping-foundation.md)、[ADR 0023](adr/0023-color-glyphs.md) による。適合テストの `synthetic` 分類が消える。
- カラーの残り（[ADR 0023](adr/0023-color-glyphs.md)）。`sofel-inflate` と `sofel-png` による復号、`sofel-raster` での RGBA の合成（グラデーションと28の合成モード）、`sofel-svg` での OT-SVG の描画、ベクトルと GPU の出力（12段階目）。
- sfnt 以外の形式（8段階目）。素の CFF と CEF（[ADR 0021](adr/0021-cff-outlines.md)）、Type1、CID、Type42、PFR、BDF、PCF、FNT、gzip、LZW、bzip2。dfont は [ADR 0058](adr/0058-dfont-face-selection.md) に従って実装した。9段階目に WOFF と WOFF2 と Brotli がある。
- C ABI。HarfBuzz と FreeType の互換層（`hb_*` と `FT_*` の shim）と、バインディングジェネレータ向けのヘッダの規約。[ADR 0015](adr/0015-c-abi.md) による。
- フォントの関数の差し替え（`hb_font_funcs_t` とサブフォント）。[ADR 0016](adr/0016-font-and-vertical-metrics.md) による。
- VARC（13段階目）。

## 監査とテストの改善

利用者から、すべての実装が終わった後に行うよう頼まれている作業がある。Fable を使って、ADR と実装とテストの食い違いを調べ、訂正する ADR を書く。セキュリティの監査も行う。その後、足りないテストを足し、弱いテストを直す。

あらかじめ分かっている工程の欠けは、次のとおりである。[ADR 0016](adr/0016-font-and-vertical-metrics.md) のプルリクエスト2〜4と、[ADR 0022](adr/0022-subsetting.md) のいくつかの段階では、テストを実装の後に書いた。これは、先にテストを書いて失敗させるという方針に反している。fuzz のターゲットは、手元では動かしていない（cargo-fuzz が入っていない）。CI は fuzz を動かしている。監査では、この2点を確かめる。

## 環境と運用

- ディスクが逼迫している。2026-11-10 の時点で 98% が埋まり、空きは約 18 GiB だった。`target/wt` の worktree とビルドの出力が容量を使う。使い終えた worktree を消す。
- 共有のツール `target/ft-dump`、`target/hb-dump`、`target/bench-c` は、ツールを変えると古くなる。`tools/*/build.sh` と `bench/build-c.sh` で作りなおす。
- Golem への移行。切り替えの前後で Golem の出力を比べる。Golem は HarfBuzz 14.6.0 と FreeType 2.14.3 を使い、sofel は 15.1.0 と 2.15.0 を目標にしている。メトリクスは HarfBuzz に従うので、Golem での FreeType 由来の値は変わる可能性がある。GHC のランタイムのロケールの動作は、確かめていない（言語の推定に影響する）。
- PDF の移行では、[ADR 0022](adr/0022-subsetting.md) のサブセットを Golem の PDF ライターに渡す。[ADR 0068](adr/0068-vector-and-gpu-output.md) の PDF 出力は文字を輪郭で描く。Golem の横書きと縦書きの原稿で、改行・改ページ、グリフの位置、フォントの埋め込み、ToUnicode と ActualText によるテキスト抽出を切り替え前後で比べる。
