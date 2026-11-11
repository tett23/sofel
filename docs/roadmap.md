# ロードマップ（放置していること）

この文書は、2026-11-11 時点で sofel に残っている作業と、利用者の判断を待っている事項を一か所にまとめたものである。利用者と、Claude Code や Codex のような道具が、次に何をするかを選ぶために使う。仕様ではないので、決まったことは [docs/specifications.md](specifications.md) と各 ADR に書く。この文書は、作業が済むたびに該当する項を消す。

各項は、何が残っているか、どこに由来するか、何があれば進められるかの順に書く。節と項は、ほかの項が依存するものを先に置いた。確認できなかった事項には「未確認」と書いた。

[ADR 0027](adr/0027-variable-fonts.md)（可変フォント）、[ADR 0029](adr/0029-variation-deltas.md)（変化量の扱い）は、利用者のレビューを経て採択（未実装）になった。[ADR 0028](adr/0028-ptem-is-not-opsz.md) の `--font-ptem` の分類訂正は実装済みである。[ADR 0029](adr/0029-variation-deltas.md) の調査の記録は、手元の `target/handoff/variation-deltas-research.md` にある（コミットしていない）。
[ADR 0027](adr/0027-variable-fonts.md) の作業1の表の読み取りと検証器は実装済みである。
作業2のうち、座標の正規化と [ADR 0029](adr/0029-variation-deltas.md) と [ADR 0030](adr/0030-variation-evaluation-corrections.md) のItemVariationStoreの評価の核も実装した。
C ABIには位置の作成、座標の取得、Variationの文字列変換を実装した。
軸と名前つきインスタンスの問い合わせ、FreeTypeが名前を持つときの合成既定インスタンス、位置ごとのPostScript名と対応するC ABIは実装した。
HVARとVVARの送り幅、VORGとVVARの縦原点、MVARのフォント全体のメトリクス、FreeTypeの縦送り幅には座標を適用した。
BASEのVariationIndexによる基準線とレイアウトのフォント全体の値の変化量を実装した。
次はgvarの幻の点による送り幅と箱、輪郭、シェーピング、塗りの変化量とCLIから位置を指定する機能を実装する。
FreeTypeが必要なnameレコードを持たないときの末尾スタイルの扱いは未決定である。
FeatureVariationsの条件評価の単独ファジングは、評価を実装する作業5に残す。

## 未実装の ADR

### 変化量の評価の修正案

[ADR 0029](adr/0029-variation-deltas.md) の引き継ぎ調査による修正案を、[ADR 0030](adr/0030-variation-evaluation-corrections.md) にまとめた。
FreeType の丸めの順序、係数のキャッシュの同一性、空の座標と明示した0座標、CFF2 の列数とエラー、gvar の適用手順を扱う。
ステータスは採択（未実装）である。
[ADR 0027](adr/0027-variable-fonts.md) と [ADR 0029](adr/0029-variation-deltas.md) の本文は変更していない。


### [ADR 0022](adr/0022-subsetting.md): サブセット化

[ADR 0022](adr/0022-subsetting.md) は採択（未実装）で、段階ごとに実装している。PR #42 までで、レイアウトの段階などが main に入った。10段階目（gvar、GVAR、HVAR、VVAR、ピン留めなしの CFF2、GDEF、BASE、COLR の項目の変化量の記憶）は、可変フォントの [ADR 0027](adr/0027-variable-fonts.md) の読み取り部分を使って実装する。VARC は13段階目まで待つ。[ADR 0022](adr/0022-subsetting.md) のステータスを採択にする条件は、[ADR 0027](adr/0027-variable-fonts.md) の採択と、10段階目の実装である。

### シェーピング、フォントの検証と描画、照合の記録

各項目の実装状況はリンク先の ADR に記す。

- [ADR 0050](adr/0050-preserve-opentype-script-values.md)：OpenType タグからスクリプトを求める二つの API で、HarfBuzz が返す4バイトを保持する。
- [ADR 0052](adr/0052-c-shaping-ppem.md)：C API で横と縦の ppem を指定できるスケール構築子を追加する。既存の呼び出しでは両軸とも0を使う。
- [ADR 0053](adr/0053-preserve-buffer-settings-after-glyph-deserialization.md)：C API でグリフ列を読み込んだ後も、バッファをクリアして再利用するときは通常のクリアと同じ設定を引き継ぐ。
- [ADR 0055](adr/0055-rewrite-cli-color-rendering.md)：CLI の COLRv0 描画制御を独自実装に書き直し、sofel-cli の MIT 表記を維持する。画素の合成には sofel-raster の既存実装を使う。

cmap のフォーマット2と8は、通常のグリフ検索では読み、シェーピングでは HarfBuzz に合わせて対応なしとする処理を実装済みである。[仕様書](specifications.md) に両方の検索規則を記載している。

[ADR 0057](adr/0057-record-unexcluded-phantom-point-check.md)：[ADR 0018](adr/0018-phantom-point-shift.md) は採択を維持する。ネイティブと wasm の両方で、グリフを除外せずに照合した。[4サイズの照合結果](benchmarks/2026-11-11-unexcluded-phantom-point-check.md)を記録した。

## HarfBuzz のサニタイザの移植

[ADR 0044](adr/0044-shaping-conformance-exceptions.md) で認めた dfont の差は、[ADR 0058](adr/0058-dfont-face-selection.md) に従って解消した。残るサニタイザの差は次の ADR で扱う。
固定した `in-house/collections.tests:1` は HarfBuzz と一致した。シェーピングの全5850件では5631件が通り、残る219件は期待する失敗である。dfont を含めた FreeType の654ファイルでは9113項目が一致し、生成した2フェイスの dfont でも cmap とメトリクスの4項目が一致した。

- [ADR 0059](adr/0059-layout-sanitizer-diagnostics.md) に従い、GSUB、GPOS、GDEF のサニタイズを HarfBuzz 15.1.0 に合わせる。シェーピング、レイアウト選択、検証器で表の採否を共有する。`in-house/harfbust.tests:4`、レイアウト選択の3件、無作為な文字列のシェーピングの3件を照合し、一致した項目から `sanitize` の期待失敗を削除する。

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

## 実装のあとに起こす ADR

次の項は、各 ADR の「後回しにしたもの」に挙げられたものと、そこから派生するものである。それぞれ、先行する実装が終わってから ADR を書く。先に、可変フォントの [ADR 0027](adr/0027-variable-fonts.md) に依存する項を並べる。

- 任意の位置への可変フォントのインスタンス化と、軸の範囲を絞る部分的なインスタンス化。[ADR 0022](adr/0022-subsetting.md) の「後回しにしたもの」にある。[ADR 0027](adr/0027-variable-fonts.md) の採択と実装の後に書く。解消すると、サブセット化のテストの期待する失敗のうち、インスタンスの91件と IUP の23件が消える。
- ppem を使った Device 表の補正（ヒンティングの Device）。GPOS、GDEF のキャレット、BASE、MATH が対象である。[ADR 0017](adr/0017-base-baselines.md) による。変化量の部分は [ADR 0027](adr/0027-variable-fonts.md) に入る。
- GDEF のリガチャのキャレットの API。[ADR 0027](adr/0027-variable-fonts.md) の一覧にある。
- スタイルの値 `hb_style_get_value`。[ADR 0027](adr/0027-variable-fonts.md) の一覧にある。
- MATH の問い合わせ（`hb_ot_math_*`）と、JSTF の扱いの決定。[ADR 0017](adr/0017-base-baselines.md) による。MATH のビューは [ADR 0022](adr/0022-subsetting.md) で入っている。
- 送り幅の cache（HarfBuzz の `advance_cache` に当たるもの）。領域の係数は [ADR 0029](adr/0029-variation-deltas.md) が `Location` に持つ。送り幅の cache は、計測で遅い項目が出たときに ADR を書く。
- AAT のシェーピング（10段階目）。morx、mort、kerx、ankr、trak、feat、ltag、dumber シェーパー、旧 `kern` 表、`--font-ptem` による trak。[ADR 0020](adr/0020-shaping-foundation.md) による。適合テストの `aat` 分類の期待する失敗が、これで消える。
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
