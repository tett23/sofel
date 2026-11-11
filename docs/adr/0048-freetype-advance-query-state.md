# ADR 0048: FreeType の送り幅を初回の問い合わせに合わせる

ステータス: 採択

## 文脈

[ADR 0027](0027-variable-fonts.md) と [ADR 0029](0029-variation-deltas.md) は、位置つきの送り幅を `FT_Get_Advance` と `FT_Get_Advances` の `FT_LOAD_NO_SCALE` に照合する。一方、現行の `Instance::ft_h_advance` と `ft_v_advance` は、HVAR または VVAR があれば表の差分を足し、なければ元のメトリクスを返す。この値は、初回の FreeType の問い合わせと常に一致するわけではない。

FreeType の TrueType ドライバは、名前つきまたは可変の face で、HVAR/VVAR のサポート印がまだ立っていなければ高速の送り幅取得を拒否する。`FT_Get_Advances` は、その場合、その呼び出し中の全グリフを `FT_Load_Glyph` で読む。グリフの読込では、HVAR/VVAR のサポート印が立てば対応する gvar 幻の点の差分を抑止する。一方、合成グリフの `USE_MY_METRICS` は指定した子の幻の点を採用するため、表の差分を問い合わせる高速取得と結果が異なりうる。

この順序依存性を実機で確認した。`ft-dump --metrics --named-instance=0` の RobotoFlex-Variable では、最初の高速取得は status 7 で拒否され、通常の全グリフ取得で glyph 600 は 430 だった。その後の高速取得は成功し、同じ glyph は 600 だった。glyph 600 は `USE_MY_METRICS` を持つ合成グリフである。`ft-dump --metrics` は face を開いて位置を設定した後、送り幅の全グリフ取得を一度行う。現行の sofel は、この位置の glyph 600 に 600 を返す。`hvar-long-deltas.ttf` のように HVAR の係数だけを調べる場合と、FreeType の送り幅 API の初回値を比べる場合を混同しない。

## 実装すること

- glyf を持つ TrueType face について、`Instance::ft_h_advance` と `Instance::ft_v_advance` の公開契約を、face を開き、名前つきインスタンスまたは座標を設定し、グリフやメトリクスをまだ読まない新しい FreeType face に対する、最初の `FT_Get_Advances(FT_LOAD_NO_SCALE)` の対応するグリフの値とする。縦の問い合わせには `FT_LOAD_VERTICAL_LAYOUT` も付ける。sofel の値は不変とし、問い合わせ順によって変えない。結果が `u16` に収まらないときは `None` とする。C API はこの失敗を既存の非成功 status で返す。glyf 以外の形式の契約と実装は変えない。
- glyf の可変位置では、元のメトリクスから四つの幻の点を作り、FreeType の gvar 算術と `USE_MY_METRICS` を使って点の差を得る。HVAR/VVAR のサポートが有効なら対応する gvar 幻の点の差分を抑止し、その後に `USE_MY_METRICS` の子の値を採用する。正規化座標がすべて 0 のときは、FreeType の `doblend` と同じく gvar を適用しない。
- 輪郭を読む内部処理には、既存の HVAR/VVAR 差分を含む、gvar 適用前のメトリクスを渡す。この用途の内部ヘルパーを公開送り幅から分離する。公開した送り幅を幻の点の初期値に再投入して差分を二度足さない。既存の輪郭とラスタライズの結果を維持する。
- 新しい face に対する初回の問い合わせと、HVAR/VVAR の初期化後の高速問い合わせを区別するテストを置く。コーパスの各位置と全グリフで `ft-dump --metrics` と照合する。

## アルゴリズムと作業領域

HVAR または VVAR が有効なら、対応する送り幅の gvar 幻の点の差分は FreeType によって抑止される。その方向の送り幅は、合成グリフの `USE_MY_METRICS` を持つ最後の成分を深さ 64 までたどり、参照先のメトリクスから求める。この経路は輪郭の点を展開せず、ヒープの作業領域も増やさない。既定位置では、FreeType の高速取得に合わせて元のグリフのメトリクスを返す。

HVAR/VVAR が無い方向は、glyf の輪郭点の数と合成グリフの成分数だけを読む。gvar の組を順に解析し、四つの幻の点に明示された差分だけを積む。輪郭点の配列と IUP の作業配列は作らない。成分に `USE_MY_METRICS` があれば、その子の四点を再帰的に求める。座標の一時配列は呼び出し側が持つ `FtAdvanceScratch` で再利用できるようにする。引数を増やさない従来の関数は内部で一時領域を作る。

縦メトリクス表のない TrueType face は、FreeType の合成縦送り幅を使う。OS/2 の `sTypoAscender` と `sTypoDescender`、または hhea の ascender と descender の差に、MVAR の `hasc` と `hdsc` を反映する。この場合は gvar の縦幻の点の差を送り幅に足さない。

## 実装しないこと

- FreeType face の可変な初期化状態や問い合わせ順を `Instance` に持ち込まない。
- 初回の問い合わせの後で HVAR/VVAR が初期化された face の高速取得値を、公開する送り幅の契約にしない。この値は `item_delta::<Ft>` のテストで別に確かめる。
- HarfBuzz の送り幅、gvar の差分の算術、輪郭の拡大縮小を変更しない。

## 不変条件

1. I01: 同じ `Font`、`Location`、glyph に対する `ft_h_advance` と `ft_v_advance` は、呼び出し順によらず同じ値を返す。
2. I02: 有効な glyf と位置について、`u16` に収まる初回の `FT_Get_Advances(FT_LOAD_NO_SCALE)` の横・縦送り幅と、それぞれの公開関数の結果が等しい。HVAR/VVAR がある場合、ない場合、共有・組固有の gvar 点番号、`USE_MY_METRICS` の場合を含む。サポート表の存在だけでなく、FreeType がその表を有効と認めたかを判定に使う。
3. I03: 輪郭内部の幻の点は元のメトリクスから一度だけ変化し、公開する送り幅を先に呼んでも輪郭とラスターの結果が変わらない。
4. I04: グリフ番号が範囲の外、または初回の送り幅が `u16` の範囲外なら、panic せず `None` を返す。既存の C API は非成功 status を返す。破損した glyf/gvar は panic せず、FreeType が読込を拒否するか既存メトリクスに戻すかに従って扱う。

| 境界の入力 | 期待する結果 |
| --- | --- |
| 既定位置で gvar の差分なし | 元の送り幅 |
| 名前つき位置で正規化座標がすべて 0 | gvar の差分なし。合成グリフでは `USE_MY_METRICS` の子を採用する場合がある |
| HVAR/VVAR なしで gvar の幻の点だけ変化 | 幻の点の差 |
| HVAR/VVAR のサポート印が未設定で初回は低速読込 | グリフ読込中に有効化された表の差分と、子の `USE_MY_METRICS` を反映する |
| 合成グリフに `USE_MY_METRICS` | 指定した子の送り幅 |
| 結果が 0、65535、範囲を一つ超える | 順に `Some(0)`、`Some(65535)`、`None` |
| グリフ番号がちょうど範囲外 | `None` |

## テスト設計

まず、HVAR/VVAR の有無と gvar の幻の点の差、`USE_MY_METRICS`、0 と 65535 の境界を手で組んだフォントで I01、I02、I04 の失敗するテストにする。`ft-dump` の同じ face で初回の `FT_Get_Advances` と後続の `FT_ADVANCE_FLAG_FAST_ONLY` を順に呼び、値の違いを記録する。次に実装し、単体テストとコーパスの位置・全グリフの照合を行う。

I03 は、送り幅を呼ぶ前後の輪郭とラスターを FreeType と比較する。少なくとも Mada-VF、RobotoFlex-Variable、VVAR を持つフォントと、空・単純・合成グリフを含める。C API の同じ値と失敗 status を確かめる。性能は、位置ごとの全グリフ取得時間と、作業領域の確保回数・累計バイト数・同時に生きる最大バイト数を変更前後で測る。

## トレードオフ

FreeType の face は状態を持つが、sofel の `Instance` は不変である。初回の問い合わせを契約にすると、呼び出し順に依存しない値を返せる。その代わり、初期化後の FreeType の高速問い合わせと値が異なるフォントでは、その高速値をこの API で取得できない。HVAR/VVAR 自体の整数の差分は `item_delta::<Ft>` で調べられる。この選択は [ADR 0027](0027-variable-fonts.md) の HVAR 優先規則を保ち、初回の低速読込で `USE_MY_METRICS` の子の幻の点を採用する場合を追加する。

HVAR/VVAR の無い方向は、幻の点だけを求めても従来の表の直読より遅い。試作を Apple Silicon の release build で測ると、Mada-VF の 529 グリフを 100 回読む時間は従来 363µs、幻の点だけを読む方法が 27,332µs、作業領域の再利用を指定した方法が 25,873µs だった。同じ順番の測定でヒープ確保は 0 回、52,900 回、1 回、累計 0、846,400、16 バイト、最大同時増分は 0、16、16 バイトだった。これらのメモリ値は Rust の `System` allocator を計数したもので、プロセスの RSS は含まない。旧実装は可変位置の値が誤るため、この速度差は機能が同じ二つの実装の比較ではない。

HVAR のある RobotoFlex-Variable は、947 グリフを 100 回読む時間が従来 8,559µs、新しい通常の関数が 10,813µs、作業領域を指定した関数が 10,433µs だった。三つとも追加のヒープ確保は 0 回だった。合成グリフの参照先をたどる時間が増えるが、輪郭点を展開しない。測定コマンドは `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/wt/adr-0027-gvar/target cargo build --release --manifest-path /tmp/sofel-adr0048-membench/Cargo.toml` と、環境変数 `SOFEL_TESTDATA`、`SOFEL_ADR0048_BENCH_FONT`、`SOFEL_ADR0048_BENCH_MODE` を設定した `target/wt/adr-0027-gvar/target/release/sofel-adr0048-membench` である。計測用のコードは `/tmp/sofel-adr0048-membench` にあり、リポジトリへは入れない。

`u16` の範囲外を `None` とすることは、現在の C API の出力型を保つ代わりに、FreeType の符号つきの値を表せない限界を明示する。

## 試作の照合結果

試作前の FreeType メトリクス全コーパス照合では、3,835 face-location、参照ツールが拒否した 6 件、送り幅 10,603 差だった。参照ツールを memory face に対応させ、試作の幻の点だけを読む経路を使った照合では、3,841 face-location、拒否 0 件、差 0 件だった。試作 worktree で `SOFEL_TESTDATA` にコーパス、`SOFEL_FT_DUMP` に memory face 対応版を指定し、`cargo test --release -p sofel-conformance --test variation_metrics_corpus adr0027_i03_freetype_metrics_at_all_corpus_locations -- --ignored --nocapture` を実行した。テストは 1 件成功した。参照ツールの `bearing` 行は、この試作 worktree の古いパーサーが未対応なので、試験用コピーの入力から除いた。採択後の実装でも、この全位置照合と不変条件の境界テストを再実行する。

別の初期状態の face を使う `ft-dump --advances`（[ADR 0030](0030-variation-evaluation-corrections.md)）の試作でも、Mada-VF と RobotoFlex-Variable の名前つき位置 0 について、全グリフの横・縦の単体問い合わせと範囲問い合わせの 5,904 件が sofel と一致した。参照ツールを `/tmp/sofel-ft-adr0030-advances` にビルドし、`SOFEL_FT_DUMP` に指定して `cargo test --release -p sofel-conformance --test variation_metrics_corpus adr0048_i01_first_single_and_batch_advances_match -- --ignored --nocapture` を実行し、1 件成功、差 0 件だった。この照合は二つのフォントの位置 0 に限る。

## 採択前の確認事項

FreeType は HVAR/VVAR 表の読込と検証が成功した場合にサポート印を立てる。RobotoFlex-Variable.ABC の HVAR majorVersion を 1 から 2 に変えると、FreeType の名前つき位置の glyph 1、2、3 の横送り幅は 1272、1200、1313 になった。表の存在だけで gvar の幻の点を抑止する試作は 1280、1260、1359 を返した。表の版、ItemVariationStore、advance map の読込成功を判定に加えると、この三つは一致した。SourceSerifVariable-Roman-VVAR の VVAR majorVersion を 1 から 2 に変えると、縦送り幅 glyph 1 と 3 は FreeType と試作の双方で 996 になった。正常な VVAR では双方とも 1000 だった。ほかの破損の仕方について FreeType と同じ判定になるかは未確認であり、I02 と I04 の手組みテストで確かめる。

`FT_Get_Advances` は符号つきの値を返すが、既存の Rust と C の API は `u16` を返す。Mada-VF の glyph 1 の元の横送り幅を 205 から 0 に変えると、名前つき位置で FreeType は -13 を返し、試作は `None` を返した。glyph 243 の元の横送り幅を 833 から 65535 に変えると、FreeType は 65550 を返し、試作は `None` を返した。一方、HVAR の算術が `FT_UShort` に切り詰めた 65533 はそのまま `Some(65533)` を返す。C API の試作は前二例とも既存の `SOFEL_ERROR_INVALID_GLYPH`（-6）を返したが、二つの glyph 番号は有効である。範囲外を `None` にする提案は既存の型を保つ一方、符号つきの値を利用者へ渡せない。別に符号つきの API と専用のエラーを追加するかは本人の判断を要する。

## 参考文献

- [ADR 0027](0027-variable-fonts.md)
- [ADR 0029](0029-variation-deltas.md)
- [FreeType: ftadvanc.c](https://github.com/freetype/freetype/blob/c4a3c188e82bc5b457390a87630ac782316421a1/src/base/ftadvanc.c)
- [FreeType: ttdriver.c](https://github.com/freetype/freetype/blob/c4a3c188e82bc5b457390a87630ac782316421a1/src/truetype/ttdriver.c)
- [FreeType: ttgload.c](https://github.com/freetype/freetype/blob/c4a3c188e82bc5b457390a87630ac782316421a1/src/truetype/ttgload.c)
- [FreeType: ttgxvar.c](https://github.com/freetype/freetype/blob/c4a3c188e82bc5b457390a87630ac782316421a1/src/truetype/ttgxvar.c)
