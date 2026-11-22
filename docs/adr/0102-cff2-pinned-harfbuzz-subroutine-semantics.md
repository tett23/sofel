# ADR 0102: CFF2 の共有サブルーチンと変化量ストアを固定版 HarfBuzz に合わせる

Status: accepted

## 文脈

root [ADR 0022](0022-subsetting.md) は、HarfBuzz 15.1.0 とのバイト単位の一致と、サブセット化前後の輪郭の保存を求める。
同 ADR の「CFF2」は、軸を指定しないときに使う `vsindex` に合わせて VariationStore を書き直し、`blend` と `vsindex` を保持すると記している。
実装の残範囲を調べたところ、固定した HarfBuzz の実際の出力と両立しない箇所があった。

この ADR は、この3点を固定版の出力へ揃える。
共有 global サブルーチンの local 呼び出しの再採番で HarfBuzz 自身が輪郭を変えるケースでは、バイト一致を優先する。

## 調査した事項

参照コミットは HarfBuzz 15.1.0 の `3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372` である。
`src/hb-subset-cff-common.hh` の `encode_globalsubrs` は、global サブルーチンの符号化に Font DICT 0 の local 再採番表を渡す。
local サブルーチンの符号化には、それぞれの Font DICT の表を渡す。

2つの Font DICT が同じ global サブルーチンを使い、その global が local サブルーチン1を呼ぶ合成入力を作った。
Font DICT 0 は local 1だけを残し、Font DICT 1は local 0と1を残す。
`hb-dump --subset --gids=1,2` は成功し、global の local 呼び出しを Font DICT 0 の再採番で書く。
元の gid 2の第2輪郭の開始点は `(20,10)` だが、HarfBuzz 出力では `(30,10)` になる。
両方の入力を `sofel-outline` で描画してこの差を確認した。
現在の sofel は、Font DICT ごとに global の書き換え結果が異なると `Unsupported` を返す。
同じ入力で `--gids=2` だけを指定すると HarfBuzz は成功するが、出力の検証器は global 内の local 呼び出しを範囲外と報告する。
この場合は、固定版とのバイト一致と、検証器が `Error` を報告しない条件も両立しない。

未使用の ItemVariationData を含む、2つの ItemVariationData を持つ CFF2 の VariationStore でも比較した。
軸を指定しない `hb-dump --subset --gids=1` は、未使用行を含む50バイトのストアをそのまま残す。
`src/hb-subset-cff2.cc` の `cff2::subset` も、部分的なインスタンス化でない分岐では元のストアを直列化する。

同じ合成入力の charstring は `vsindex 0` と `moveto` を持つ。
通常のサブセット化はこの `vsindex` を保持し、`--desubroutinize` では省く。
通常の出力は384バイト、脱サブルーチン化した出力は380バイトだった。
通常と脱サブルーチン化で異なる結果を求める回帰テストを PR #391 に追加し、固定版の期待バイト列と一致することを確認した。

## 実装すること

### 軸を指定しない VariationStore

軸を指定しない CFF2 では、使用するデータ行と領域が一部だけでも、VariationStore を固定版 HarfBuzz と同じ内容で保持する。
この場合に未使用行を詰めるための再採番を行わない。
軸の固定と範囲変更に伴うストアの射影は、root [ADR 0061](0061-subset-axis-instancing.md) のままとする。

### `vsindex` と脱サブルーチン化

通常の軸未指定の出力では、明示された `vsindex` を保持する。
`DESUBROUTINIZE` では、展開して符号化し直したプログラムが使わない `vsindex` を固定版 HarfBuzz と同じく省く。
描画の `blend` に必要な変化量の選択を保持する。
`NO_HINTING` による `vsindex` の先頭への移動も、固定版 HarfBuzz の規則に従う。

### 共有 global サブルーチン

global サブルーチンは、Font DICT ごとに複製せず、固定版 HarfBuzz と同じく Font DICT 0 の local 再採番表で符号化する。
必要な local の閉包は、各 Font DICT の呼び出しをたどって求める。
Font DICT ごとの再採番結果が異なることだけを理由に `Unsupported` にしない。

この構造で固定版 HarfBuzz が元の輪郭を変える場合に限り、root [ADR 0022](0022-subsetting.md) の輪郭保存条件から除外する。
sofel は、その場合も固定版 HarfBuzz と同じバイト列を返す。
輪郭が変わる入力には回帰テストを置き、元との不一致と HarfBuzz 出力との一致を両方記録する。
出力の検証器が `Error` を報告しないという条件は変えない。
固定版と同じ再採番で存在しない local を呼ぶ出力になる場合は、バイト一致の例外として `SubsetError::Malformed { tag: CFF2 }` を返す。
このエラーは、この共有構造で参照どおりの出力を正しく読めないことを示し、入力全体が不正だったと断定するものではない。
検証に失敗する出力を成功として返したり、別の図形になる独自の修復を行ったりしない。

## 実装しないこと

参照バージョンを上げない。
他の構造で起きる輪郭の変化を、輪郭保存条件の例外に加えない。
使用していないデータを詰めるために、固定版と異なる CFF2 を書かない。
この補足だけで root [ADR 0022](0022-subsetting.md) の全範囲を実装済みにしない。

## 不変条件

- I1: 軸を指定しない CFF2 の VariationStore は、固定版 HarfBuzz の出力と同じバイト列になる。
- I2: 通常と `DESUBROUTINIZE` の各出力で、`vsindex` の有無と charstring のバイト列が固定版 HarfBuzz と一致する。
- I3: 複数の Font DICT が共有 global サブルーチンを使う入力で、global と各 local の再採番後のバイト列が固定版 HarfBuzz と一致する。ただし I5 の不正参照を作る場合を除く。
- I4: I3 の構造で固定版 HarfBuzz が輪郭を変え、出力の local 参照が有効な場合、sofel の出力輪郭は HarfBuzz 出力と一致し、出力の検証器は `Error` を報告しない。
- I5: I3 の再採番によって存在しない local を呼ぶ出力になる場合、sofel は CFF2 の `Malformed` を返し、出力バイト列を返さない。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| 変化量ストアの参照0件、一部だけ、全件 | 固定版と同じストアを出す | I1 |
| 使わない `vsindex` と、描画の `blend` が使う `vsindex` | 通常と脱サブルーチン化の各参照結果に一致 | I2 |
| Font DICT 1つ、2つで同じ local 再採番、異なる local 再採番 | global と local が固定版と一致 | I3 |
| 2つの Font DICT で異なる図形の local を持つ共有 global | 固定版と同じ輪郭を返し、出力検証が成功 | I3, I4 |
| 共有 global の local が再採番後に存在しない | CFF2 の Malformed を返す | I5 |

## 受けいれ条件

- A1: 合成入力の期待バイト列を固定した `hb-dump` から生成して比較し、VariationStore、通常と脱サブルーチン化、共有 global の各回帰テストが成功する。
- A2: 既存のサブセット化の適合テスト、輪郭保存テスト、出力の検証、native と wasm の検査が成功する。

## テスト設計

各ケースは合成したフォントを使い、固定版 `hb-dump --subset` の出力を期待値とする。
`adr0102_i1_unpinned_variation_store` は、未使用のデータ行と領域を含む入力でストア全体を比較する。
`adr0102_i2_vsindex_desubroutinize` は、描画の `blend` がある場合とない場合を通常と `DESUBROUTINIZE` で比べる。

`adr0102_i3_i4_shared_global_local_remapping` は、再採番先の local が異なる図形になる2つの Font DICT を作る。
現行の `Unsupported` を最初の失敗として確認してから実装する。
元の gid 2の第2輪郭の開始点 `(20,10)` と、参照出力の `(30,10)` の両方を検査する。
同じ再採番先になる対照ケースでは、元の輪郭を保存することも確かめる。
`adr0102_i5_invalid_reference_output_is_rejected` は同じ入力で gid 2だけを残し、固定版の出力が local の範囲外を参照することと、sofel が CFF2 の `Malformed` を返すことを確かめる。
出力を検証器に通し、例外をこの共有構造以外へ広げない。

期待値の出典と再生成手順は root [ADR 0088](0088-synthetic-test-fixtures.md) に従う。
既存の root [ADR 0022](0022-subsetting.md) の適合テストと、通常の輪郭保存の検査も維持する。

## トレードオフ

固定版とのバイト一致を優先すると、共有 global の特定の構造では元の輪郭を保存できない。
これを明記すると参照実装の出力で置き換えられるが、輪郭保存を必要とする利用者はこの例外を考慮する必要がある。
固定版が不正な local 参照を出す場合はバイト一致を断念して失敗するため、その入力では置き換え時に成否が変わる。
参照実装が将来この挙動を変えた場合は、参照の更新時に別の ADR で追う。

## 参考文献

- root [ADR 0022](0022-subsetting.md)
- root [ADR 0061](0061-subset-axis-instancing.md)
- HarfBuzz 15.1.0 `src/hb-subset-cff-common.hh` の `encode_globalsubrs` と `encode_str`
- HarfBuzz 15.1.0 `src/hb-subset-cff2.cc` の `cff2::subset`
