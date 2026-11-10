# ADR 0013: 計測の道具（`bench/`）と2段階目の計測の項目

ステータス: 採択

## 文脈

[ADR 0006](0006-performance-measurement.md) は、性能の計測の方法を定めた。
比較の相手はHarfBuzz 15.1.0とFreeType 2.15.0、計測の枠組みは自分で書き、相手は手で書いた `extern "C"` で呼ぶ。ネイティブとwasm（wasmtime）の両方で計測し、結果を `docs/benchmarks/` に記録する。
[ADR 0007](0007-sfnt-read-foundation.md) は、計測の道具を2段階目のADRで作り、cmapとhmtxの引き方も計測の項目に加えるとした。

2段階目では、文字からグリフへの対応と送り幅（[ADR 0010](0010-font-foundation.md)）、glyfの輪郭（[ADR 0011](0011-glyf-outlines.md)）、アンチエイリアスのラスタライズ（[ADR 0012](0012-antialiased-rasterizer.md)）ができる。

HarfBuzzは、`src/harfbuzz.cc` の1つの翻訳単位でビルドできる（C++11、例外とRTTIなし）。
FreeTypeは、`ft-dump` と同じくCのコンパイラだけでビルドできる（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。
wasmへは、wasi-sdk（WASIのためのclangとlibc）でどちらもビルドできる。

HarfBuzzの計測（`perf/benchmark-font.cc`）は、`perf/fonts/` と `test/subset/data/fonts/` のフォントを使う。
glyfのフォントは、Roboto-Regular、Comfortaa-Regular-new、NotoNastaliqUrdu-Regular、RobotoFlex-Variable、SourceSerifVariable-Romanである。

## 実装すること

### 構成

- `bench/` に、ワークスペースの外のCargoのパッケージを置く。ライブラリのクレートの依存には入れない（[ADR 0006](0006-performance-measurement.md)）。外部のクレートには依存しない。
- `bench/build-c.sh [TARGET]` は、取得したFreeTypeとHarfBuzzのソース（[ADR 0008](0008-validate-command-and-freetype-comparison.md) の `testdata.lock`）から、静的ライブラリを `target/bench-c/<TARGET>/` に作る。
  - FreeTypeは `ft-dump` と同じファイルの一覧とモジュールの一覧を使う。
  - HarfBuzzは `src/harfbuzz.cc` を `-O2` でビルドする。外部のライブラリ（FreeType、ICU、GLibなど）との統合は無効にする。
  - `TARGET` が `wasm32-wasip1` のときは、wasi-sdkのclangを使う。
- `bench/wasi-sdk.lock` に、wasi-sdkの版（作成時点の最新の34）と、ホストごとのアーカイブのURLとSHA-256を書く。`bench/fetch-wasi-sdk.sh` は、それを `target/wasi-sdk/` に取得し、SHA-256を確かめる。
- `bench/build.rs` は、`target/bench-c/<TARGET>/` の静的ライブラリをリンクする指示を出す。無ければ、`build-c.sh` を実行するよう案内してビルドを失敗させる。
- FreeTypeとHarfBuzzの関数は、使うものだけを `bench/src/ffi.rs` に手で宣言する。

### 計測の枠組み

- 1つの項目は、準備（フォントを開くなど）と、計測する本体の関数からなる。
- 本体を、1回の標本が10ミリ秒以上になる回数まとめて呼ぶ。ウォームアップの標本を3つ捨てたあと、標本を30個取る。
- 標本から、1回あたりの時間の中央値と中央絶対偏差を求める。
- 本体の結果は `std::hint::black_box` に通し、最適化で消えないようにする。
- `--filter <文字列>` で項目を絞り、`--once` で各項目の本体を1回だけ呼ぶ（CIでの確かめのため）。
- 結果は、項目ごとに、sofelと相手の中央値、比（sofel ÷ 相手）、中央絶対偏差を、Markdownの表として標準出力に書く。

### 2段階目の項目

フォントは上の5つ（可変フォントは既定のインスタンス）、文章はHarfBuzzの `perf/texts/` の `en-paragraph.txt`、`fa-paragraph.txt`、`hi-words.txt`、`ja-runs.txt` とする。

| 項目 | sofel | FreeType | HarfBuzz |
|---|---|---|---|
| 文字からグリフ | 文章の全文字で `nominal_glyph` | `FT_Get_Char_Index` | `hb_font_get_nominal_glyph` |
| 送り幅 | 全グリフで `advance` | `FT_Get_Advance`（`FT_LOAD_NO_SCALE`） | `hb_font_get_glyph_h_advance`（拡大縮小なし） |
| 輪郭（拡大縮小なし、48画素） | 全グリフで `draw`（何もしない `Pen`） | `FT_Load_Glyph`（`NO_HINTING`、`NO_BITMAP`）と `FT_Outline_Decompose` | `hb_font_draw_glyph`（何もしない描画の関数） |
| ラスタライズ（12、24、48、96画素） | 全グリフで `draw` と `render` | `FT_Load_Glyph` と `FT_Render_Glyph`（`NORMAL`） | なし |

- 相手の関数の呼び出しの費用も相手の時間に含める。sofelの側も、公開のAPIだけを呼ぶ。
- 比べる相手と同じ結果を出していることは、計測の前に、各項目で1回ずつ結果（グリフID、送り幅、命令の数、画素の合計）を比べて確かめる。違えば、その項目を失敗として報告する。

### wasm

- `cargo build --release --target wasm32-wasip1` で計測の道具をwasmにし、`wasmtime` で走らせる。フォントと文章は、wasmtimeの `--dir` で渡す。
- 同じ項目を計測し、同じ形の表を書く。

### 記録とCI

- このADRの実装のプルリクエストで、ネイティブとwasmの最初の記録を `docs/benchmarks/<YYYY-MM-DD>-stage2.md` に置く。記録には、機械、OS、ツールチェーンとwasmtimeとwasi-sdkの版、sofelのコミット、表を含める（[ADR 0006](0006-performance-measurement.md)）。
- CIでは、Linuxのランナーで、ネイティブとwasmの両方を `--once` でビルドして走らせ、落ちないことと、結果の照合が通ることだけを確かめる。時間は記録しない。

## 実装しないこと

- CIでの時間の計測と、退行による失敗（[ADR 0006](0006-performance-measurement.md)）。
- シェーピング、カラーグリフ、WOFF2、サブセット化などの項目。それぞれの段階のADRで加える。
- ブラウザでの計測。
- 計測の結果のグラフ。必要になったら、記録から作る道具を足す。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- 統計：既知の数列（奇数個、偶数個、同じ値の並び、外れ値を含む列）について、中央値と中央絶対偏差が正しいことを、`bench/` の単体テストで確かめる。
- 回数の決め方：本体の時間を偽の時計で与え、1回の標本が10ミリ秒以上になる回数を選ぶことを確かめる。
- 表の書き出し：決まった結果から、期待するMarkdownの表が出ることを確かめる。
- 結果の照合：sofelと相手の結果がわざと違う偽の項目で、失敗として報告されることを確かめる。

自動テストにしにくいものは、次のとおり自動で確かめる。

- `build-c.sh` と `fetch-wasi-sdk.sh`：CIのLinuxのランナーで、ネイティブとwasmの両方のビルドが通ることで確かめる。`fetch-wasi-sdk.sh` のSHA-256の照合は、わざと違う値を書いた一時の `wasi-sdk.lock` で失敗することを、`scripts/tests/` のテストで確かめる（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。
- FFIの宣言：CIで `--once` を走らせ、結果の照合が通ることで確かめる。

## トレードオフ

- C++のHarfBuzzのビルドに、C++のコンパイラ（ネイティブ）とwasi-sdk（wasm）が要る。計測の道具だけの要件で、ライブラリには及ばない。
- 標本の数と長さを固定するため、揺れの大きい機械では中央絶対偏差が大きくなる。記録に中央絶対偏差を残し、比を読むときの目安とする。
- ラスタライズはHarfBuzzと比べない。HarfBuzzの `libharfbuzz-raster` は輪郭の扱いがFreeTypeと異なり、2段階目の基準（FreeTypeとの完全一致、[ADR 0012](0012-antialiased-rasterizer.md)）と結果が揃わないためである。
- wasi-sdkのアーカイブ（数百MB）の取得で、CIの時間が延びる。CIではキャッシュする。

## 参考文献

- HarfBuzz `perf/benchmark-font.cc`、`perf/README.md`、`src/harfbuzz.cc`（15.1.0）
- FreeType `docs/INSTALL.ANY`（2.15.0）
- wasi-sdk https://github.com/WebAssembly/wasi-sdk
- Wasmtime https://wasmtime.dev/
