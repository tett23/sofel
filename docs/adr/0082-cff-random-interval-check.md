# ADR 0082: CFFの `random` の決まりと、区間による参照の確かめ

Status: proposed

## 文脈

CFFのCharStringの演算子 `random`（12 23）は、(0, 1] の値を積む。
仕様（Adobe TN#5177）は値の範囲だけを定め、乱数の作り方を定めない。
そのため、同じフォントとグリフでも、処理系や読み方によって輪郭が変わりうる。

Golemは、PDFの出力のバイト列を入力だけで決めるために、`random` の扱いを Golem root ADR 0390 で決めた。
Golemは、FreeTypeとHarfBuzzをsofelに置きかえる予定である（[ADR 0015](0015-c-abi.md)）。
置きかえたあとも同じ決まりが保たれるよう、sofelでも `random` の決まりを明文にし、Golemが自前で持つ参照の確かめ（Golem root ADR 0390 の「参照の確かめ」）をsofelの検証器に移す。

sofelは汎用のライブラリであり、この確かめは、GolemのほかにもCFFのフォントを検証する利用者が使う。

## 調査した事項

### sofelのいまの実装

[ADR 0021](0021-cff-outlines.md) の実装で、次のとおりになっている。

- Private DICTの `initialRandomSeed` は、負なら符号を反転し、0なら987654321にする（`crates/sofel-read/src/cff/dict.rs:1492-1496`）。
- `random` の状態は、グリフを描くたびに、その値から始める（`crates/sofel-read/src/cff/charstring.rs:813` の `work.random = ctx.random_seed`、`crates/sofel-outline/src/cff.rs:216`）。読む順、読む回数、スレッドに依らない。
- `random` は `((state & 0xFFFF) + 1)` を16.16の値として積み、状態を32bitのxorshiftで進める（`charstring.rs:1358` 以降、`random_next` は `charstring.rs:1444`）。
- 整数を求める取り出し（`pop_int`）は、16.16の値を受けると `TypeMismatch` を返す（`charstring.rs:589-601`）。`random` の値は16.16なので、`callsubr` などに渡すと、そのグリフは描けない。
- ライブラリは環境変数を読まない（[ADR 0004](0004-wasm-and-no-std.md) の `no_std`）。

検証器（`crates/sofel-read/src/cff/check.rs`）は、各グリフを種から始めた1本の道筋でだけたどる。`random` の値で `ifelse` の選ぶ側が変わるグリフでは、たどらなかった道筋の誤りを報告しない。

### FreeType 2.15.0

- `cf2_escRANDOM` は、`(subfont->random & 0xFFFF) + 1` を16.16の値として積み、状態を `cff_random` で進める（`src/psaux/psintrp.c:2249-2265`）。
- `cff_random` は、`r ^= r << 13; r ^= r >> 17; r ^= r << 5` の32bitのxorshiftである（`src/psaux/psobjs.c:2552-2559`）。
- `initialRandomSeed` は、負なら符号を反転し、0なら987654321にする（`src/cff/cffload.c:1941-1946`）。
- フェイスの種（`random-seed` の性質）が既定の−1なら、サブフォントの状態の初めにドライバの種を使い、ドライバの種は使うたびに進む。ドライバの種が0のときだけ、`initialRandomSeed` を使う（`src/cff/cffload.c:2105-2118`）。ドライバの種の初めは、メモリのアドレスから作る（`src/cff/cffobjs.c:1158`、`src/psaux/psobjs.c:2525`）。状態はサブフォントに残り、グリフをまたいで進む。
- `cf2_stack_popInt` は、型が整数でない値を受けると `Syntax_Error` にする（`src/psaux/psstack.c:138-149`）。`callsubr` と `callgsubr`（`psintrp.c:1011`）、`put`（2207）、`get`（2222）、`index`（2339）、`roll`（2371-2372）は、この関数で番号や位置を取り出す。

### HarfBuzz 15.1.0

- `OpCode_random` は定義だけがあり（`src/hb-cff-interp-common.hh:200`）、処理する分岐が無い。知らない演算子として、引数を捨てる（`hb-cff-interp-common.hh:640-647` の `env.clear_args ()`）。
- そのため、HarfBuzzの箱（[ADR 0021](0021-cff-outlines.md) の `Outlines::glyph_extents`）は、`random` の値に依らない。

### Golem root ADR 0390

Golemの調べと決定は、`target/handoff/golem-cff-random/` に写した（Golemのセッションの調査の記録と、試験のフォント）。
決定のうち、この ADR に関わるのは次のとおりである。

- 種と状態の決まりは、上のsofelの実装と同じである。
- 参照の確かめでは、`random` の値を区間 (0, 1] とし、算術を区間で行い、結果が一つに決まらない比較と `ifelse` は両方の枝を、番号や位置に使う区間は下端の床から上端の天井までの各整数を、別の枝としてたどる。INDEXの外の参照、確かめられない演算、実行の数の上限（すべての枝で合わせて100000）を、それぞれ別の診断にする。

## 実装すること

### 決まり（いまの実装を明文にする）

- `random` の状態は、グリフを描くたびに、Private DICTの `initialRandomSeed`（負なら符号を反転、0なら987654321）から始める。
- `random` の値と状態の進め方は、FreeTypeの `cf2_escRANDOM` と `cff_random` と同じにする。
- ライブラリは、種を外から変える手段（環境変数、プロセスの状態、設定の値）を持たない。
- `random` の値（16.16）を、整数を求める演算子（`callsubr`、`callgsubr`、`put`、`get`、`index`、`roll`）に渡したグリフは、FreeTypeと同じく描けない（`TypeMismatch`）。
- HarfBuzzの箱は、HarfBuzzと同じく `random` を処理せず、引数を捨てる。

### 検証器：区間による参照の確かめ

`sofel-read` のCFFの検証器に、次の確かめを足す。CFF2には `random` が無いので、CFFだけに当てる。

1. 各グリフをいまのとおり種から始めた道筋でたどり、その道筋で `random` を一度でも実行したグリフだけを、2. の確かめにかける。`random` を実行しない道筋は、比較と `ifelse` の結果が値で決まるので、道筋は1本だけである。
2. グリフを、値を区間として持つ解釈器でたどりなおす。
   - 区間は、下端と上端と、それぞれの端を含むかを持つ。端は無限大でもよい。`random` は (0, 1]（下端を含まず、上端を含む）を積む。ほかの値は、幅0の区間（両端を含む）である。
   - 区間の値には、FreeTypeの型（整数か16.16か）も持たせる。`random` の値と、それから算術で作った値は16.16である。
   - `add`、`sub`、`neg`、`abs`、`mul` は区間の演算で結果を求める。`div` は除数の区間が0を含まなければ区間の演算で、除数の端が0の側に開いていれば結果の端を無限大にする。`sqrt` は区間がすべて0以上なら各端の平方根とする。端の値は、FreeTypeの演算（`FT_MulFix`、`FT_DivFix`、`FT_SqrtFixed`）で16.16のまま求める。`mul` と `div` は、2つの区間の端の4つの組をそれぞれ計算し、その最小と最大を結果の端とする。結果の端を含むかは、その端を作った2つの端が両方とも含まれるときだけ含むとする。
   - 比較と論理の演算子（`eq`、`and`、`or`、`not`）と `ifelse` で結果が一つに決まらないときは、とりうる結果ごとに別の枝としてたどる。
   - 幅を持つ区間を番号や位置に使う演算子（`callsubr`、`callgsubr`、`put`、`get`、`index`、`roll`）は、区間の下端の床から上端の天井までの各整数を、別の枝としてたどる。
   - 枝ごとに、スタック、ステムの数、一時の配列、読む位置を持つ。
   - 枝をたどる順は、番号や位置の小さい順、`ifelse` は最初の引数を選ぶ枝が先、比較と論理は結果0の枝が先、`roll` は数の小さい順で、同じ数では量の小さい順とする。
   - 実行の数（数と演算子をそれぞれ一つと数え、すべての枝を足す）は、100000までとする。
3. 確かめで見つけたものを、次の診断として報告する。グリフごとに、たどる順で最初に見つけたものだけを報告する。

| 診断（`Kind`） | 重さ | 当てはまるとき |
| --- | --- | --- |
| `CffRandomSubroutineOutOfRange { glyph, position }` | Error | ある枝の `callsubr` か `callgsubr` の番号（偏りを足した位置）が、INDEXの外にある。番号の区間の端が無限大のときも含む |
| `CffRandomUnchecked { glyph, position, reason }` | Warning | 幅を持つ区間が、0での `div`、負の数の `sqrt`、スタックの深さを超える `index` の位置、負かスタックの深さを超える `roll` の数を含む。無限大の端を持つ区間を `mul`、`div`、または位置に使う。実行の数が100000を超える。`reason` は、この4つを区別する |
| `CffRandomOperandRejected { glyph, position, operator }` | Warning | ある枝で、16.16の値（`random` に由来する値）を、整数を求める演算子が受ける。仕様には反しないが、FreeTypeとsofelはそのグリフを描けない（`Syntax_Error`、`TypeMismatch`） |

- 診断の位置は、いまの `char_string_error` と同じく、グリフのCharStringの中の位置とする。
- 種から始めた道筋で見つかる誤り（いまの診断）は、これまでどおり報告する。区間の確かめは、いまの診断を変えない。
- `sofel-validate` の出力と `--json` に、この3つの診断を足す（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。

### C ABI

検証の診断は、いまの `sofel_validate` の関数ポインタで渡る（[ADR 0015](0015-c-abi.md)）。診断の番号の列挙に3つを足す。新しい関数は足さない。

## 実装しないこと

- グリフを描くときの振る舞いを変えること。描くときはFreeTypeと同じであり、区間の確かめで見つけたグリフも、これまでどおり描くか、描けないかが決まる。
- HarfBuzzの箱で `random` を処理すること。
- 種を利用者が変える手段。
- seacの部品のグリフを区間でたどること。部品は種から始めた道筋でだけたどる。
- CFF2とType1のCharString。どちらにも `random` が無い（CFF2は [ADR 0021](0021-cff-outlines.md)、Type1はFreeTypeの `src/type1/t1decode.c` に `random` の処理が無い）。

## 不変条件

- I1: 同じフォントの同じグリフを、読む順、読む回数、読むスレッド、先に開いたフォントの数を変えて描いても、輪郭の命令の列は等しい。
- I2: `initialRandomSeed` が負なら、符号を反転した値の種で描いた輪郭と等しい。0なら987654321の種で描いた輪郭と等しい。
- I3: `random` が積む値の列は、FreeTypeの `cf2_escRANDOM` と `cff_random` が同じ種から積む値の列と等しい。
- I4: 同じフォントの検証を何度走らせても、報告される診断の列（種類、グリフ、位置、順）は等しい。
- I5: 種から始めた道筋で `random` を実行しないグリフでは、この ADR の前とあとで、報告される診断は等しい。
- I6: 区間の確かめを足しても、グリフの輪郭とラスタライズの結果は、FreeTypeとの突き合わせ（[ADR 0021](0021-cff-outlines.md)）でこれまでどおり一致する。
- I7: `random` を一度だけ実行し、その値を `ifelse` で0.5と比べて、片方の枝でだけINDEXの外の `callsubr` に届くグリフでは、種によらず `CffRandomSubroutineOutOfRange` が報告される。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| `random` を使わないグリフ | 診断はこの ADR の前と等しい | I5 |
| `random` を1回使い、値を捨てるグリフ | 区間の確かめをかけ、診断は無い | I4, I5 |
| `random` を2回以上使い、座標に使うグリフを、2回、逆の順、2つのスレッドで描く | 輪郭はすべて等しい | I1 |
| `initialRandomSeed` が 0、1、−1、i32の最大 | 0は987654321、−1は1の種と等しい | I2 |
| 種987654321から `random` を1000回 | FreeTypeの値の列と等しい | I3 |
| 片方の枝でだけINDEXの外の `callsubr` | `CffRandomSubroutineOutOfRange` | I7 |
| 区間 (0, 1] で `div` の除数にする | `CffRandomUnchecked`（0での `div`） | I4 |
| 区間 (0, 1] の逆数を `mul` に使う（端が無限大） | `CffRandomUnchecked`（無限大の端） | I4 |
| `random` の値をそのまま `callsubr` に渡す | `CffRandomOperandRejected` | I4 |
| 枝を合わせた実行の数がちょうど100000 | 上限の診断は無い | I4 |
| 枝を合わせた実行の数が100001 | `CffRandomUnchecked`（上限） | I4 |

## 受けいれ条件

- A1: HarfBuzzのテストのフォントのすべてのCFFのフェイスで、FreeTypeとの輪郭とラスタライズの突き合わせが、この ADR の前と同じ結果で通る。
- A2: Golemの調べで作った `random` を含む試験のフォント（`target/handoff/golem-cff-random/random-audit/out/*-random.otf`）を `sofel-validate --json` で検証し、Golem root ADR 0390 の確かめ（G08107、G08108、G08109）が報告するグリフと、この ADR の3つの診断が報告するグリフが対応する。対応しないものは、理由を添えて記録する。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。テストの名前は `adr0082_i1_...` の形とする。

- 不変条件の表の各行を、[ADR 0007](0007-sfnt-read-foundation.md) の組み立て方で作ったCFFのフォントで確かめる。
- I1：同じグリフを、2回、逆の順、2つのスレッドで描いて、命令の列を比べる。
- I3：`tools/ft-dump` に、種を与えて `cff_random` の値の列を書く形を足し、sofelの値の列と比べる。
- I5：HarfBuzzのテストのフォントのすべてのCFFのフェイスで、検証の診断の列が、この ADR の前のコミットの `sofel-validate --json` の出力と等しいことを確かめる（`random` を含むグリフの新しい診断を除く）。
- I6、A1：既存の `sofel-conformance` の突き合わせを、そのまま通す。
- A2：Golemの試験のフォントを `sofel-conformance` のテストに加え、報告されるグリフと診断を期待の一覧と比べる。試験のフォントのライセンスと出どころを確かめ、`testdata.lock` か、組み立てで作りなおせる形にする。確かめられなければ、同じ形のフォントを組み立てて作る。
- ファジング：`fuzz/` の `cff` の対象に、区間の確かめを加え、どの入力でもpanicせず、実行の数の上限で終わることを確かめる。

## トレードオフ

- 区間の確かめは、枝の数が値の区間の幅に比例して増える。実行の数の上限（100000）で費用を抑えるが、上限に当たったグリフは「確かめきれない」と報告するだけになる。
- 診断の種類をGolemの3つのコードに合わせて分けるため、`Kind` が3つ増える。Golemはsofelの診断をそのまま自分のコードに写せる。
- 区間の確かめは仕様の意味（型の無い数）でたどり、描くときはFreeTypeの意味（整数と16.16の型）で描く。そのため、検証器は仕様には反しないがFreeTypeとsofelが描けないグリフを、別の診断（`CffRandomOperandRejected`）で報告する。
- 種から始めた道筋で `random` を実行したグリフだけを区間でたどるので、`random` を使わないフォントの検証の費用は変わらない。

## 参考文献

- Adobe Technical Note #5177, "The Type 2 Charstring Format"（`random`、`ifelse`、`callsubr`）
- Adobe Technical Note #5176, "The Compact Font Format Specification"（`initialRandomSeed`）
- FreeType `src/psaux/psintrp.c`、`src/psaux/psobjs.c`（`cff_random`）、`src/psaux/psstack.c`（`cf2_stack_popInt`）、`src/cff/cffload.c`、`src/cff/cffobjs.c`（2.15.0）
- HarfBuzz `src/hb-cff-interp-common.hh`（15.1.0）
- Golem root ADR 0390（`docs/adr/0390-deterministic-and-exhaustive-cff-random.md`、tett23/golem のブランチ `adr-0390`）と、Golemのセッションの調査の記録（`target/handoff/golem-cff-random/`）
