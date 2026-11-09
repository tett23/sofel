# ADR 0007: `sofel-read` の土台（sfntの目録、head、maxp、hhea、hmtx、cmap）

ステータス: 採択

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の実装の順序の1段階目は、表の読み取りと検証の土台である。
`sofel-read` は、sfntのバイト列の上に表のビューを重ね、コピーも確保もしない。各表の検証器も持つ（[ADR 0003](0003-crate-layers.md)）。
この段階で、以後のすべての表が従う読み取りの方式（境界の検査、壊れた入力の扱い、数値の型、検証器の形）を決める。

参考にした実装の方式は次のとおりである。

- HarfBuzzは、構造体をバイト列の上に直接重ねる（`src/hb-open-type.hh`）。表を初めて使うときに一度だけ全体を検査し（sanitize、`src/hb-sanitize.hh`）、以後は検査なしで読む。範囲外のオフセットは、すべてのフィールドが0の「Null」の構造体を指すことにして、分岐を減らす。
- read-fontsは、`FontData`（`&[u8]` の包み）から、読むたびに境界を検査して `Result` を返す（`read-fonts/src/font_data.rs`）。clippyの `arithmetic_side_effects` を拒否にして、桁あふれを防いでいる。
- FreeTypeは、cmapのサブテーブルを読み込みの時点で検証し、壊れたものを捨てる（`src/sfnt/ttcmap.c`。検証の段階は `FT_VALIDATE_DEFAULT`、`TIGHT`、`PARANOID`）。

65536グリフを超えるフォント（ISO OFF第5版）のために、大文字の表 `MAXP`、`HHEA`、`HMTX` がある。
それぞれ小文字の表と同じ構造で、グリフの数とメトリクスの数を24bitで持つ（HarfBuzz `src/hb-ot-maxp-table.hh`、`src/hb-ot-hhea-table.hh`）。
cmapのフォーマット15は、フォーマット14（異体字セレクタ）のグリフIDを24bitにしたものである（HarfBuzz `src/hb-ot-cmap-table.hh` の `CmapSubtableFormat14_15`）。

壊れた表の扱いは、HarfBuzzとFreeTypeで異なる。
例えば、hmtxで `numberOfHMetrics` 以上のグリフIDについて、HarfBuzzはグリフの数を超えれば0を返し、FreeTypeは最後の送り幅を返す（HarfBuzz `src/hb-ot-hmtx-table.hh`、FreeType `src/sfnt/ttmtx.c` の `tt_face_get_metrics`）。

## 実装すること

### ワークスペースとツールチェーン

- リポジトリ直下に、Cargoのワークスペースの `Cargo.toml` を置く。メンバーは `crates/sofel-read` だけとする。
- `rust-toolchain.toml` で、stableの版（作成時点の最新の1.99）と、`rustfmt`、`clippy`、`wasm32-unknown-unknown`、`wasm32-wasip1` を指定する。版を上げるときはプルリクエストで行う。
- edition 2024とし、ライセンスはMITとする。リポジトリ直下に `LICENSE` と、見出しと説明だけの `THIRD_PARTY_NOTICES.md` を置く（[ADR 0002](0002-license-and-reuse.md)）。

### クレートの規則

- `#![no_std]` とし、`alloc` も使わない（[ADR 0004](0004-wasm-and-no-std.md)）。
- `#![forbid(unsafe_code)]` とする（[ADR 0003](0003-crate-layers.md)）。
- どの入力でもpanicしないことを、clippyの次のlintを `deny` にして強制する：`indexing_slicing`、`arithmetic_side_effects`、`unwrap_used`、`expect_used`、`panic`、`cast_possible_truncation`、`cast_possible_wrap`、`cast_sign_loss`。範囲を確かめた変換は `try_from` か、確かめたことをコメントに書いた明示の関数で行う。

### 基本の型と読み取り

- `Tag`（4バイトの表のタグ）、`GlyphId`（`u32`。24bitのグリフIDを最初から扱う）、`Fixed`（16.16の生の値の `i32`）、`F2Dot14`（2.14の生の値の `i16`）、`LongDateTime`（`i64`）を定める。演算は持たず、生の値を返すだけとする。演算は `sofel-geom` の型で行う（[ADR 0003](0003-crate-layers.md)）。
- ビッグエンディアンの整数の読み取り（`u8`、`i8`、`u16`、`i16`、`u24`、`u32`、`i32`）を、`&[u8]` とオフセットから `Option` を返す関数として書く。
- 各表のビューは `&'a [u8]` を持つ値とする。作るときに、ヘッダの長さと、ヘッダが示す配列の範囲を一度だけ検査する（O(1)）。ヘッダは `first_chunk::<N>()` で固定長の配列にして持ち、以後のフィールドの読み取りで境界の検査が起きないようにする。
- 配列は、作るときに長さを確かめた部分スライスを `as_chunks::<N>()` で固定長の要素の配列にして持つ。要素の読み取りの境界の検査は、添字と要素の数の比較の1回だけにする。
- 表の中のオフセットの先（cmapのサブテーブルなど）は、それを取り出すときに同じ方式で検査する。
- 表を作れないほど壊れているとき（ヘッダが足りない、版が未知など）は、ビューの作成が `Err(ReadError)` を返す。`ReadError` は、足りないバイトの位置と数、未知の版、などを区別する列挙型とする。
- 作れたビューの読み取りは、範囲外の入力（グリフの数を超えるグリフIDなど）に対して `None` を返す。0などの既定の値で埋めない。HarfBuzzとFreeTypeで既定の値の扱いが異なるため、方針は `sofel-font` で決める。

### sfntの目録とコレクション

- `Sfnt`：オフセット表（`sfntVersion` が `0x00010000`、`OTTO`、`true`、`typ1`）と表の記録を読む。`table(tag)` で表のバイト列を返す。表の記録は昇順に並ぶべきだが、並んでいないフォントもあるため、線形に探す（表の数は多くても数十）。
- `Collection`：TTCとOTCのヘッダ（版1と版2）を読み、`font(index)` で各フォントの `Sfnt` を返す。表の記録のオフセットは、コレクションの先頭からとして扱う。
- dfont、WOFF、WOFF2は、この段階では扱わない（[ADR 0001](0001-project-goal-and-scope.md) の8段階目と9段階目）。

### 表

| 表 | 読むもの |
|---|---|
| `head` | 版、`fontRevision`、`checksumAdjustment`、`magicNumber`、`flags`、`unitsPerEm`、作成と更新の日時、`xMin`〜`yMax`、`macStyle`、`lowestRecPPEM`、`fontDirectionHint`、`indexToLocFormat`、`glyphDataFormat` |
| `maxp`、`MAXP` | 版（0.5と1.0）、`numGlyphs`（`MAXP` は24bit）、版1.0のTrueTypeの最大値の各フィールド |
| `hhea`、`HHEA` | すべてのフィールド。`numberOfHMetrics`（`HHEA` は24bit） |
| `hmtx`、`HMTX` | `hhea` の `numberOfHMetrics` と `maxp` の `numGlyphs` を引数に作る。`advance(gid)` と `left_side_bearing(gid)` を返す。`numberOfHMetrics` 以上で `numGlyphs` 未満のグリフIDは、最後の送り幅と、続く配列の左のサイドベアリングを返す |
| `cmap` | 版とエンコーディングの記録の列。各記録から、プラットフォーム、エンコーディング、サブテーブルを返す |

- 大文字の表と小文字の表は別の型とせず、グリフの数の幅（16bitか24bit）を引数にとる1つの型とする。どちらを使うかの選択（大文字を優先するHarfBuzzの方式）は、`sofel-font` で決める。
- cmapのサブテーブルは、フォーマット0、2、4、6、8、10、12、13、14、15を読む。
  - 文字からグリフへの対応（0、2、4、6、8、10、12、13）：`glyph(codepoint) -> Option<GlyphId>` と、対応のある組を昇順に返すイテレータ `mappings()`。
  - 異体字シーケンス（14、15）：`variant(codepoint, selector)` が、既定のグリフを使う、特定のグリフを使う、対応なし、の3つを区別して返す。
  - 二分探索は、配列を整数に変換せずに、ビッグエンディアンのバイト列のまま比べる。
- サブテーブルの選び方、記号フォントの文字の付けかえ（U+F0xx）、Macのエンコーディングの変換は、`sofel-read` では行わない。`sofel-font` で決める。

### 検証器

- 各表に `validate(data, report)` を置く。`report` は `FnMut(Diagnostic)` で、見つけた誤りを1つずつ渡す。確保をしない（[ADR 0003](0003-crate-layers.md)）。
- `Diagnostic` は、表のタグ、表の先頭からのオフセット、重大度、種類を持つ。種類は誤りごとの列挙型の値とし、期待した値と実際の値などを持つ。
- 重大度は2つとする。
  - `Error`：その表または部分を、ビューとして作れない。または、読み取りの結果が仕様の意図と異なる（範囲外を指すオフセット、ヘッダの数と合わない長さ、二分探索の前提を崩す並びなど）。
  - `Warning`：仕様に反するが、読み取りの結果には影響しない（予約のフィールドが0でない、表の記録が昇順でない、チェックサムが合わない、`magicNumber` が違うなど）。
- 次を不変条件とし、テストで確かめる。
  - 検証器が `Error` を報告しない表では、ビューの作成が成功する。
  - 検証器が `Error` を報告しない表では、範囲内の入力（グリフの数未満のグリフIDなど）に対する読み取りが `None` を返さない。
- 検証は、入力の大きさに比例する時間で終わる。オフセットをたどる表で循環があっても止まるよう、たどる回数に上限を設ける（HarfBuzzのsanitizeの `max_ops` と同じ考え）。この段階の表には循環しうるオフセットが無いが、方式をここで決めておく。

### CI

- `main` へのpushとプルリクエストで、次を実行するジョブを足す（[ADR 0004](0004-wasm-and-no-std.md)）。
  - `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`
  - `cargo build --release --target wasm32-unknown-unknown -p sofel-read`
  - `cargo test --target wasm32-wasip1 -p sofel-read`。実行器はwasmtimeとし、Bytecode Allianceのセットアップ用のactionをコミットのSHAで固定して入れる。
- ファジングのジョブ（後述）を足す。

### ファジング

- ワークスペースの外に `fuzz/` を置き、cargo-fuzz（libFuzzer）で次の対象を作る。`fuzz/` はライブラリの依存ではないため、`libfuzzer-sys` に依存してよい。
  - 各表：任意のバイト列からビューを作り、すべての読み取り（cmapは `glyph`、`variant`、`mappings` を含む）を呼ぶ。検証器も呼ぶ。panicしないことと、上の検証器の不変条件を確かめる。
  - `Sfnt` と `Collection`：任意のバイト列から作り、すべての表を取り出す。
- CIでは、nightlyのツールチェーンで、各対象を短い時間（1対象あたり60秒）走らせる。見つかった入力は、単体テストの入力として `crates/sofel-read/tests/` に加える。

## 実装しないこと

- この段階にない表（name、OS/2、post、vhea、vmtxなど）。以後の段階のADRで加える。
- dfont、WOFF、WOFF2、Type1などの形式。
- サブテーブルの選び方などの方針（`sofel-font` の仕事）と、cmapのキャッシュ。
- コードの自動生成（read-fontsの `font-codegen` のような、表の定義からの生成）。表を手で書いて、繰り返しが重荷になったらADRで判断する。
- 性能の計測。計測の道具（[ADR 0006](0006-performance-measurement.md) の `bench/`）は、2段階目（glyfの輪郭とラスタライズ）のADRで作り、そのときにcmapとhmtxの引き方も計測の項目に加える。
- 形式手法（Kaniなど）の導入。上のlintとファジングで足りない箇所が見つかったら、ADRで判断する。
- 検証のコマンドと、FreeTypeとの突き合わせ。[ADR 0008](0008-validate-command-and-freetype-comparison.md) で扱う。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- 整数の読み取り：各型について、境界ちょうど、1バイト足りない、オフセットが `usize` の上限付近、の場合に正しい値か `None` を返す。
- `Sfnt` と `Collection`：手で組んだバイト列で、各 `sfntVersion`、表の探索、昇順でない表の記録、TTCの版1と版2、範囲外のオフセットを確かめる。
- 各表：手で組んだバイト列で、全フィールドの値を確かめる。ヘッダの不足と未知の版で `Err` になることを確かめる。
- `MAXP`、`HHEA`、`HMTX`：24bitの数で、65536を超えるグリフIDの送り幅を読めることを確かめる。
- hmtx：`numberOfHMetrics` 未満、以上で `numGlyphs` 未満、`numGlyphs` 以上の3つの範囲を確かめる。
- cmapの各フォーマット：手で組んだサブテーブルで、対応のある文字、対応のない文字、範囲の端、フォーマット4の `idRangeOffset` と `idDelta` の桁の巻き戻り、フォーマット12と13の範囲、フォーマット14と15の3つの結果を確かめる。`mappings()` が返す組が、すべて `glyph()` の結果と一致することを確かめる。
- 検証器：わざと壊したバイト列ごとに、期待する種類、重大度、オフセットの `Diagnostic` が報告されることを確かめる。正しいバイト列で何も報告されないことを確かめる。
- 不変条件：自分で書く小さな疑似乱数（xorshift）で作った大量のバイト列について、検証器の2つの不変条件が成り立つことを確かめる。
- wasm：同じ単体テストを `wasm32-wasip1` で通す。
- 実際のフォントでの確かめは、[ADR 0008](0008-validate-command-and-freetype-comparison.md) の突き合わせで行う。

## トレードオフ

- HarfBuzzのように一度の検査の後に検査なしで読むことは、`unsafe` を使わない限りできない。固定長の配列への変換と `as_chunks` で境界の検査を1回に減らすが、ゼロにはできない。その差は、計測の道具ができた段階で測る。
- 読み取りが `None` を返すため、呼び出し側（`sofel-font`）で既定の値を決める分岐が要る。HarfBuzzのNullの構造体のほうが分岐は少ないが、HarfBuzzとFreeTypeの挙動の違いを `sofel-read` に埋め込まないことを優先する。
- 大文字と小文字の表を1つの型にまとめるため、グリフの数の幅を実行時の値として持つ。型を分けるより読み取りの分岐が1つ増えるが、24bitの表は稀で、分岐の予測は外れにくい。
- clippyのlintで `indexing_slicing` と `arithmetic_side_effects` を拒否するため、コードの書き方が冗長になる。代わりに、panicしないことをコンパイルの段階で強く保証できる。
- ファジングはnightlyと `libfuzzer-sys` に依存する。ライブラリには入らないが、CIの時間が延びる。

## 参考文献

- HarfBuzz `src/hb-open-type.hh`、`src/hb-sanitize.hh`、`src/hb-ot-cmap-table.hh`、`src/hb-ot-hmtx-table.hh`、`src/hb-ot-maxp-table.hh`、`src/hb-ot-hhea-table.hh`（15.1.0）
- FreeType `src/sfnt/ttcmap.c`、`src/sfnt/ttmtx.c`、`include/freetype/internal/ftvalid.h`（2.15.0）
- read-fonts `src/font_data.rs`（0.46.0） https://github.com/googlefonts/fontations
- OpenType Specification：Font file（table directory、TTC）、cmap、head、hhea、hmtx、maxp https://learn.microsoft.com/en-us/typography/opentype/spec/
- ISO/IEC 14496-22 第5版（24bitのグリフID）
- cargo-fuzz https://github.com/rust-fuzz/cargo-fuzz
