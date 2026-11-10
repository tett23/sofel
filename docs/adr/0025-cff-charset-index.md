# ADR 0025: CFFのcharsetの索引（`Cff::cid` などの速さ）

ステータス: 採択

## 文脈

[ADR 0021](0021-cff-outlines.md) の実装で、GIDからCIDへの対応（`Cff::cid`、FreeTypeの `FT_Get_CID_From_Glyph_Index`）を、charsetを引いて求めるようにした。
charsetのフォーマット1と2は、範囲（最初のSIDと残りの数）の列である。あるグリフのSIDを知るには、先頭の範囲から順に数えるしかない。
いまの `Charset::sid` は、グリフごとに先頭から範囲を読むので、全グリフを引くとグリフの数の2乗に比例する時間がかかる。

計測（`docs/benchmarks/2026-11-10-cff.md`）では、SourceHanSans-Regular_subset（CIDキー、10452グリフ）の全グリフのCIDを引くのに、sofelはFreeTypeの約885倍の時間がかかった（機械の負荷が高いときの値だが、桁は変わらない）。

FreeTypeは、フェイスを開くとき（`cff_charset_load`）に、全グリフのSIDの配列（`charset->sids`）を確保して作る。`FT_Get_CID_From_Glyph_Index` と、グリフの名前（`cff_get_glyph_name`、`cff_get_name_index`）は、この配列を引く。CIDからGIDへの逆の配列（`cff_charset_compute_cids`。同じCIDが複数あれば小さいGID）は、素のCIDキーのCFFで `FT_Load_Glyph` のグリフの番号をCIDとして読むときに使う。

`sofel-font` は確保をしない（[ADR 0010](0010-font-foundation.md)）。キャッシュは呼び出し側が持つ値とする（[ADR 0003](0003-crate-layers.md)）。
charsetの範囲の数は、グリフの数よりずっと少ないことが多い（範囲の列を作る理由がそこにある）。範囲ごとの最初のグリフの番号の表があれば、二分探索で引ける。

## 実装すること

### `sofel-read`

- `Charset::num_ranges()`：フォーマット1と2の範囲の数（FreeTypeの読み方で、グリフの数を覆うまでの範囲。`ranges_len` と同じ数え方）。フォーマット0と既定のcharsetは0。
- `Charset::index_into(&self, starts: &mut [u32]) -> Result<CharsetIndex<'_>, usize>`：呼び出し側の配列に、各範囲の最初のグリフの番号を書き、索引を返す。配列が範囲の数より短ければ、何も書かずに要る長さを返す（[ADR 0015](0015-c-abi.md) の可変長の出力と同じ形）。確保しない。
- `CharsetIndex<'a>`：charsetのビューと、範囲の最初のグリフの番号の列を借りる値。`Copy`、`Send`、`Sync`。
  - `sid(glyph)`：フォーマット1と2では、範囲の最初のグリフの番号を二分探索して範囲を見つけ、`最初のSID + (glyph − 範囲の最初のグリフ)` を返す。FreeTypeの「救済」（SIDが0xFFFFを超える範囲の切りつめ、`trim`）も同じにする。フォーマット0と既定のcharsetは、いまの `Charset::sid` と同じく直接引く。値は `Charset::sid` と常に等しい。
  - `glyph(sid)`（SIDから最初のグリフ）：範囲を順に見て、SIDを含む最初の範囲から求める。範囲の数に比例する。
- 範囲を使わないフォーマット0と既定のcharsetでは、`starts` は空でよい（`num_ranges()` が0）。

### `sofel-font`

- `Cff` の問い合わせのうち、charsetを引くもの（`cid`、`charset_name`、`ft_glyph_name`、`glyph_from_name` の候補の照合、`build_name_index`）に、`CharsetIndex` を受け取る版を足す。例えば `Cff::cid_with(&self, index: &CharsetIndex, glyph)`。結果は索引の無い版と同じとする。
- 索引の無い版は残す。1回だけ引く呼び出し側は、索引を作らずに使える。ドキュメントに、繰り返し引くなら索引を作ることと、その費用（範囲の数の配列を1回読む）を書く。
- `build_name_index` は内部で全グリフのSIDを順に読む（`Charset::sids` のイテレータ）ので、索引を使わなくても全体で1回の走査になることを確かめ、遅い経路が残っていれば `sids` に揃える。

### `sofel-capi`

- `sofel_font_t` は、作るときに `CharsetIndex` を作って持つ（C ABIの層は確保してよい。[ADR 0015](0015-c-abi.md)）。`sofel_font_cid` とグリフの名前の関数は、それを使う。C ABIの関数の形は変えない。
- Rustの `Charset::num_ranges`、`index_into`、`CharsetIndex` は、C ABIでは `sofel_font_t` が内部に持つため公開しない。`crates/sofel-capi/unmapped.txt` に理由を添えて書く。

### 計測

- [ADR 0013](0013-benchmark-harness.md) の項目 `cid` を、索引を使う版で計る（索引を作る時間は、フォントを開く項目の側に入れる）。索引の無い版も別の項目として残し、1回だけ引く場合の費用を記録する。
- 記録は `docs/benchmarks/` に新しく置く（既存の記録は変えない）。機械の負荷を記録に書く。

## 実装しないこと

- CIDからGIDへの逆の配列（FreeTypeの `charset->cids`）。sfntに包まれたCFFではFreeTypeも使わず、素のCFFを開くのは8段階目（[ADR 0021](0021-cff-outlines.md) の「後回しにしたもの」）である。そのADRで、同じ索引の形（呼び出し側の配列）で足す。
- `Cff` の中に索引を持つこと（`Cff` を作るときに確保すること）。`sofel-font` は確保をしない。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。

- **値の一致**：[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方で、フォーマット0、1、2、既定の3つのcharset、0xFFFFを超えて切りつめられる範囲、範囲が1つだけ、グリフの数が範囲の合計より少ない（最後の範囲の途中で終わる）charsetを作り、全グリフについて `CharsetIndex::sid` と `Charset::sid` が等しいことを確かめる。
- **配列の長さ**：短い配列で何も書かずに要る長さが返ること、ちょうどの長さと長い配列で成功することを確かめる。
- **実際のフォント**：`sofel-conformance` で、テストのフォントのすべてのCFFのフェイスについて、索引を使う版と使わない版の `cid`、`charset_name`、`ft_glyph_name` が全グリフで等しいことを確かめる（FreeTypeとの突き合わせの項目 `:cid` は、そのまま通る）。
- **ファジング**：`fuzz/` の `cff` の対象に、索引を作って全グリフの `sid` を引き、`Charset::sid` と等しいことを足す。
- **計測**：`--once` の照合で、索引を使う版の結果がFreeTypeと一致することを確かめる。時間は、FreeTypeと同じ桁（比が1前後）になることを目安とし、記録に残す。

## トレードオフ

- 呼び出し側が配列を用意する手間が増える。代わりに、`sofel-font` は確保をしない約束を保ち、索引の寿命とメモリが呼び出し側に見える。C ABIの利用者には、`sofel_font_t` の中で済ませるので手間は増えない。
- FreeTypeのように全グリフのSIDの配列（グリフの数×2バイト）を持てば、引くのは1回の添字で済む。範囲の表（範囲の数×4バイト）と二分探索を選ぶのは、CJKのフォントでは範囲の数がグリフの数より桁で少なく、メモリが小さいためである。引く時間が問題になれば、計測を添えてSIDの配列の版を足す。
- 索引のある版と無い版の2つの関数を持つ。結果が常に等しいことを、テストとファジングで確かめる。

## 参考文献

- FreeType `src/cff/cffload.c`（`cff_charset_load`、`cff_charset_compute_cids`）、`src/cff/cffdrivr.c`（`cff_get_cid_from_glyph_index`、`cff_get_glyph_name`、`cff_get_name_index`）（2.15.0）
- Adobe Technical Note #5176, "The Compact Font Format Specification"（charsetの形式）
- sofel `docs/benchmarks/2026-11-10-cff.md`
