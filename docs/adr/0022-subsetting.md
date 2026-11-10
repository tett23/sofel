# ADR 0022: サブセット化と表の書き出し（`sofel-write` と `sofel-subset`）

ステータス: 採択（未実装）

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) は、フォントの書き出し（オフセットのあふれを解く再配置を含む）とサブセット化（HarfBuzzの `hb-subset` 相当）を、11段階目に置いた。
sofelは、HarfBuzzとFreeTypeを置きかえる汎用のライブラリである。サブセット化も、`hb-subset` の機能を欠けなく置きかえることを目標とする。
最初の利用者のGolemが `hb-subset` から移るために、この段階を前に出す。順序の入れかえは [ADR 0001](0001-project-goal-and-scope.md) の実装の順序が許している。

[ADR 0005](0005-conformance-tests.md) は、サブセット化の正しさの基準を、HarfBuzzの `test/subset/data/tests/*.tests` の期待する出力とのバイト単位の一致と決めた。

### HarfBuzz 15.1.0の `hb-subset`

- **公開のAPI**（`src/hb-subset.h`、`src/hb-subset-serialize.h`）：入力（`hb_subset_input_*`。集合、フラグ、グリフの対応の指定、軸の位置と範囲、名前の表の上書き、`keep_everything`、オプションの文字列への変換）、計画（`hb_subset_plan_*`。新旧のグリフの対応、文字から元のグリフへの対応）、実行（`hb_subset_or_fail`、`hb_subset_plan_execute_or_fail`）、前処理（`hb_subset_preprocess`）、CFFのcharstringの取り出し（`hb_subset_cff_get_charstring_data`、`hb_subset_cff_get_charstrings_index`）、オブジェクトの列からの表の直列化（`hb_subset_serialize_or_fail`）。
- **サブセット化する表**（`src/hb-subset.cc` と `src/hb-subset-table-*.cc`）：`head`、`maxp`、`hhea`、`hmtx`、`vhea`、`vmtx`、`post`、`name`、`OS/2`、`cmap`、`DMAP`、`glyf`、`loca`、`hdmx`、`CFF `、`CFF2`、`VORG`、`GDEF`、`GSUB`、`GPOS`、`BASE`、`MATH`、`COLR`、`CPAL`、`CBDT`、`CBLC`、`sbix`、`fvar`、`avar`、`gvar`、`HVAR`、`VVAR`、`MVAR`、`cvar`、`STAT`、`VARC`、`cvt `。65536グリフを超えるフォントのための大文字の表（`GLYF`、`LOCA`、`MAXP`、`HHEA`、`HMTX`、`VHEA`、`VMTX`、`GVAR`）も扱う（`HB_NO_BEYOND_64K` は既定で定義されない）。`SVG `、`EBDT`、`EBLC`、`kern` などは扱いを持たず、既定で捨てるか、知らない表として捨てる（`PASSTHROUGH_UNRECOGNIZED` ではそのまま写す）。
- **入力の既定の値**（`src/hb-subset-input.cc`）：フラグはすべて偽。残す名前IDは0〜6、名前の言語は0x409だけ。捨てる表は `morx`、`mort`、`kerx`、`kern`、`JSTF`、`DSIG`、`EBDT`、`EBLC`、`EBSC`、`SVG `、`PCLT`、`LTSH` とGraphiteの表。そのまま写す表は `gasp`、`fpgm`、`prep`、`VDMX`、`DSIG`。残すレイアウトのフィーチャは、fontToolsから写した一覧（`ccmp`、`liga`、`kern`、`vert`、`vrt2`、`vkrn`、`palt`、`vpal`、文字体系ごとのフィーチャなど）。文字体系はすべて残す。
- **既定で外れるもの**：`NOTDEF_OUTLINE` が偽なので、glyfの .notdef の輪郭は空になる（`src/OT/glyf/glyf.hh`）。CFFでは .notdef のcharstringを残す（このフラグは、プログラムをそのまま写せるかの判定にだけ効く。`src/hb-subset-cff1.cc` の `can_copy_programs`）。`GLYPH_NAMES` が偽なので、postは版3になる。
- **計画**（`src/hb-subset-plan.cc`）：文字の集合に鏡像の文字を足す（bidiの閉包）。cmapで、文字の集合にある文字と、グリフの集合にあるグリフに対応する文字を集める。.notdef、cmapのグリフ、要求されたグリフを集合に入れ、異体字シーケンスのグリフ、MATH、GSUBの置換の閉包（最大 `HB_CLOSURE_MAX_STAGES` = 12段）、COLR、VARCの閉包を順に足す。最後に、glyfの合成グリフの成分と、CFFの `seac` の成分を足す。`RETAIN_GIDS` では、グリフIDを変えず、出力のグリフの数を残す最大のグリフID+1とする。
- **組み立て**（`src/hb-face-builder.cc`、`src/hb-open-file.hh`）：表のデータを、長さの昇順、同じ長さはタグの昇順に並べ、それぞれを4バイトに揃えて0で埋める。表の記録はタグの順に並べる。sfntの版は、`CFF ` か `CFF2` があれば `OTTO`、なければ `0x00010000`。各表のチェックサムは埋めた後のデータで計算し、headの `checkSumAdjustment` は0として計算してから `0xB1B0AFBA` から引いた値を書く。
- **直列化**（`src/hb-serialize.hh`）：オフセットでつながる表は、オブジェクトのグラフとして組む。子を先に詰め、後に詰めたものほど前に置く。同じバイト列と同じリンクを持つオブジェクトは1つにまとめる。
- **再配置**（`src/hb-repacker.hh`、`src/graph/`、`docs/repacker.md`）：オフセットがあふれたときだけ動く。まず最短距離による位相の整列（`sort_shortest_distance`）で並べなおす。GSUBとGPOSでは、部分表の分割（PairPos、MarkBasePos、Ligatureなど）と、拡張のlookupへの昇格を行う。それでもあふれれば、32bitのオフセットの下の部分グラフを別の空間に分け（`_try_isolating_subgraphs`）、共有された子の複製や優先度の引き上げ（`_process_overflows`）を、最大32回（全体で `HB_REPACKER_MAX_ITERATIONS` = 500回）繰り返す。GSUBとGPOSで解けなければ、分割と昇格を必ず行う設定でやりなおす。
- **軸の固定**：入力で軸ごとに、既定の値への固定、任意の値への固定、範囲（最小、既定、最大）を指定できる。軸をすべて固定すると、`fvar`、`avar`、`gvar`、`HVAR`、`VVAR`、`MVAR`、`cvar` を捨てる。固定しない軸があれば、これらを新しい範囲に合わせて書きなおす（部分的なインスタンス化。`src/hb-subset-instancer-solver.cc`、`src/hb-subset-instancer-iup.cc`）。軸を指定しなければ、可変フォントの表をサブセット化して残す（`fvar`、`avar`、`MVAR`、`cvar`、`STAT` はそのまま写し、`gvar`、`HVAR`、`VVAR`、`VARC`、`CFF2` と、GDEF、BASE、COLRの変化量の表は、残すグリフと変化量の番号に合わせて書きなおす）。
- **既定の値への固定**：既定の位置の正規化した座標は0なので、変化量は効かない。それでも座標の列が空でないため、glyfのグリフは点から符号化しなおされ（`src/OT/glyf/SimpleGlyph.hh` の `compile_bytes_with_deltas`）、head、hmtx、vmtx、maxpの値はその点から計算しなおされる（`src/hb-subset-plan-var.cc`）。OS/2の `usWeightClass`、`usWidthClass`、`xAvgCharWidth`、postの `italicAngle`、STATの軸の値も改められ、GSUBとGPOSのFeatureVariationsは既定の位置で評価して畳まれる。fvarの名前IDは残さない。CFF2は脱サブルーチン化され、`DOWNGRADE_CFF2` ではCFFに変換される。
- **実験的なAPI**（`HB_EXPERIMENTAL_API`）：`IFTB_REQUIREMENTS`、`RETAIN_NUM_GLYPHS`、名前の表の上書きは、実験的なAPIを有効にしたビルドでだけ働く。

### テスト

- `test/subset/data/tests/` は104ファイルで、フォント、プロファイル、文字の集合、インスタンスの組で1123件ある。`test/subset/meson.build` が走らせるのは99ファイルと、実験的なAPIのときの3ファイル（`iftb_requirements`、`downgrade_cff2_iftb`、`retain-num-glyphs`）で、`colrv1.notoemoji` と `glyf_partial_instancing_iup` は走らせない。
- 作成時点の数えでは、1123件のうち、インスタンスの指定の無い組が1005件、すべての軸を既定の値に固定する組が4件、それ以外のインスタンスの組が91件、IUPの最適化を伴う組が23件である。
- 実行の道具（`test/subset/run-tests.py`）は、すべての組に `--drop-tables+=DSIG,BASE --drop-tables-=sbix` を足し、前処理の有無で2回走らせ、どちらも期待するファイルとバイト単位で一致することを求める。
- `test/subset/data/repack_tests/`（7ファイル）は期待する出力を持たず、`--drop-tables-=GPOS,GSUB,GDEF` で作った出力をfontToolsとOTSが読めることだけを確かめる。

### Golem

Golemは、PDFに埋め込むフォントを次のように作る（`modules/golem-font/cbits/golem/golem_font.c` の `golem_font_subset`、`docs/golem-font/adr/0002-subsetting-and-embedding.md`）。

- 入力はグリフIDの集合だけで、フラグは `RETAIN_GIDS` だけとする。出力は `hb_face_reference_blob` のバイト列である。
- TrueTypeの輪郭のフォントは、出力のsfntをそのまま `FontFile2` に入れる。CFFのフォントは、出力のsfntから `CFF ` の表だけを取り出し、`FontFile3`（`CIDFontType0C`）に入れる。
- 同じグリフの集合からは、x86-64、aarch64、wasm32-wasiのどこでも同じバイト列になることを求める。
- 軸をすべて既定の値に固定することを予定している。CFF2のフォントはPDFの組版で拒む（Golem `docs/golem-font/adr/0003-reject-cff2-fonts-in-pdf.md`）。

Golemの使い方（グリフIDだけを渡し、`RETAIN_GIDS`）の組は、HarfBuzzのテストの中に無い。

### 依存する読み取り

OS/2、name、vhea、vmtx、VORGは [ADR 0016](0016-font-and-vertical-metrics.md)、BASEは [ADR 0017](0017-base-baselines.md)、鏡像の文字は [ADR 0019](0019-unicode-character-data.md)、GSUB、GPOS、GDEFは [ADR 0020](0020-shaping-foundation.md)、CFFと既定のインスタンスのCFF2は [ADR 0021](0021-cff-outlines.md)、COLR、CPAL、CBDT、CBLC、sbixは [ADR 0023](0023-color-glyphs.md) で読めるようになる。
gvar、HVAR、VVAR、MVAR、cvar、ItemVariationStore、DeltaSetIndexMapの読み取りは、3段階目（可変フォント）のADR（未作成）で作る。VARCは13段階目である。

## 実装すること

### 基準

- 目標は、`hb-subset` 15.1.0の機能のすべてである。「後回しにしたもの」に挙げたインスタンス化を除き、この ADR で設計を定める。
- 出力は、HarfBuzz 15.1.0の `hb-subset` とバイト単位で一致させる（[ADR 0005](0005-conformance-tests.md)）。表の並び、埋め、チェックサム、オブジェクトの詰め方、形式の選び方（cmapやカバレッジの形式、locaの長さ、CFFのcharsetやFDSelectの形式、INDEXのオフセットの幅）、再配置の結果まで合わせる。
- バイト単位の一致は決定性も兼ねる。同じ入力からは、どの環境でも同じバイト列を出す。
- 出力は、`sofel-validate` の検証器（[ADR 0007](0007-sfnt-read-foundation.md)）が `Error` を報告しないものとする。
- 実装の途中の段階では、まだ書けない表やフラグを `Unsupported` の誤りとする。黙って捨てたり、違う形で書いたりしない。すべての段階を終えた後に `Unsupported` を返すのは、「後回しにしたもの」の入力だけである。

### 表

| 表 | 扱い | 待つ読み取り |
|---|---|---|
| `head`、`maxp`、`hhea`、`hmtx`、`vhea`、`vmtx`、`post`、`name`、`OS/2` | サブセット化。postの版2のグリフ名は `GLYPH_NAMES` のときだけ残す | [ADR 0016](0016-font-and-vertical-metrics.md)。postはこの ADR で足す |
| `cmap`、`DMAP` | サブセット化（`DMAP` はcmapと同じ構造で、HarfBuzzの `cmap::subset` の `is_dmap`） | なし |
| `glyf`、`loca`、大文字の表 | サブセット化。headの `indexToLocFormat` もここで決める | なし |
| `hdmx` | サブセット化。`NO_HINTING` では捨てる | この ADR で足す |
| `cvt `、`fpgm`、`prep`、`gasp`、`VDMX` | そのまま写す。`NO_HINTING` では `cvt `、`fpgm`、`prep`、`VDMX` を捨てる | なし |
| `CFF `（名前のキーとCIDのキー）、`VORG` | サブセット化（下の「CFF」） | [ADR 0021](0021-cff-outlines.md) |
| `CFF2` | サブセット化（下の「CFF2」） | [ADR 0021](0021-cff-outlines.md)。変化量の表は3段階目 |
| `GDEF`、`GSUB`、`GPOS` | サブセット化し、レイアウトの閉包を行う | [ADR 0020](0020-shaping-foundation.md) |
| `BASE` | サブセット化 | [ADR 0017](0017-base-baselines.md) |
| `MATH` | 閉包とサブセット化 | この ADR で足す（ビューと検証器） |
| `COLR`、`CPAL` | 閉包（層、パレット、変化量の番号）とサブセット化 | [ADR 0023](0023-color-glyphs.md) |
| `CBDT`、`CBLC`、`sbix` | サブセット化。CBDTの複写は長さ×`HB_SUBSET_CBDT_MAX_COPY_FACTOR`（64）を上限とする | [ADR 0023](0023-color-glyphs.md) |
| `fvar`、`avar`、`MVAR`、`cvar`、`STAT` | 軸を指定しなければそのまま写す。すべて既定の値に固定すれば、`STAT` 以外を捨て、`STAT` をサブセット化する | fvar、avar、STATはこの ADR で足す |
| `gvar`、`HVAR`、`VVAR`、`VARC` | 軸を指定しなければ、残すグリフと変化量の番号に合わせて書きなおす。すべて固定すれば捨てる | 3段階目（`VARC` は13段階目） |
| HarfBuzzの既定で捨てる表、HarfBuzzが扱いを持たない表 | 捨てる。`PASSTHROUGH_UNRECOGNIZED` では、扱いを持たない表をそのまま写す | なし |

読み取りの無い表を足すときは、[ADR 0001](0001-project-goal-and-scope.md) と [ADR 0007](0007-sfnt-read-foundation.md) に従い、同じプルリクエストでビューと検証器を `sofel-read` に足す。

### `sofel-write`

- `crates/sofel-write` を作る。`#![no_std]` と `alloc`、`#![forbid(unsafe_code)]`。依存は `sofel-read` だけとする（[ADR 0003](0003-crate-layers.md)）。
- **sfntの組み立て**：`build_sfnt(tables: &[(Tag, &[u8])]) -> Result<Vec<u8>, WriteError>`。上のHarfBuzzの手順（版、表の記録、検索の補助の値、データの並び、埋め、チェックサム、`checkSumAdjustment`）のとおりに書く。同じタグの重複、65535を超える表の数、2GiB以上の出力は誤りとする。表の並びを呼び出し側が指定する形（`hb_face_builder_sort_tables`）も持つ。
- **直列化**：`Serializer` は、`hb_serialize_context_t` の `push`、`pop_pack`、`pop_discard`、スナップショットと巻き戻し、リンクの追加（幅16、24、32bit、符号の有無、起点 `Head`、`Tail`、`Absolute`、偏り）、`end_serialize` と同じ規則で、オブジェクトのグラフを組み、バイト列に詰める。
- **再配置**：`Graph` は、詰めたオブジェクトのグラフから作り（`graph_t::create`）、`hb_resolve_overflows` と同じ手順で並べなおし、分割し、複製して、バイト列に戻す（`graph::serialize`）。GSUBとGPOSの専用の処理（`src/graph/gsubgpos-graph.hh`、`pairpos-graph.hh`、`markbasepos-graph.hh`、`ligature-graph.hh`、`coverage-graph.hh`、`classdef-graph.hh`、`split-helpers.hh`）も移す。解けなければ `WriteError::OffsetOverflow` を返す。
- **オブジェクトの列からの直列化**：`hb_subset_serialize_or_fail` と同じく、呼び出し側が作ったオブジェクトとリンクの列から、再配置を含めて表のバイト列を作る関数を公開する。サブセット化に限らず、表を一から書く利用者が使える。

### `sofel-subset` の入力と出力

- `crates/sofel-subset` を作る。`#![no_std]` と `alloc`、`#![forbid(unsafe_code)]`。依存は `sofel-font` と `sofel-write`、それに鏡像の文字（`mirroring`）のための `sofel-unicode`（[ADR 0019](0019-unicode-character-data.md)）とする。`sofel-unicode` への依存は [ADR 0003](0003-crate-layers.md) の表に無いので、この ADR が足す。実装のプルリクエストで `scripts/deps.txt` と `docs/specifications.md` の構成の図を改め、[ADR 0003](0003-crate-layers.md) のステータスに「`sofel-subset` の `sofel-unicode` への依存は [ADR 0022](0022-subsetting.md)」と書く。
- `Input`：`hb_subset_input_t` に相当する不変の値。値を変える関数は `self` を受け取って新しい `Input` を返す（[ADR 0003](0003-crate-layers.md) の関数型の方針）。
  - `Input::new()` はHarfBuzzの既定の値を持つ。`keep_everything()` は `hb_subset_input_keep_everything` と同じ値にする。
  - 集合：文字、グリフ、名前ID、名前の言語、レイアウトのフィーチャ、文字体系、捨てる表、写す表。どれも「すべて」（HarfBuzzの反転した集合）を表せる。
  - `Flags`：`u32` で、ビットの値はHarfBuzzの `hb_subset_flags_t` と同じにする。すべてのフラグを持つ。`IFTB_REQUIREMENTS` と `RETAIN_NUM_GLYPHS` は、HarfBuzzの実験的なAPIを有効にしたビルドと同じ結果を出す（テストの期待する出力がそのビルドで作られているため）。`OPTIMIZE_IUP_DELTAS` は部分的なインスタンス化でだけ意味を持つので、「後回しにしたもの」とともに働く。
  - グリフの対応の指定（`hb_subset_input_old_to_new_glyph_mapping`）。名前の表の上書き（`hb_subset_input_override_name_table`）。
  - 軸：軸のタグから `AxisRange { min, default, max }` への対応。`with_axis_pinned_to_default(tag, font)`、`with_all_axes_pinned_to_default(font)`、`with_axis_pinned(tag, value)`、`with_axis_range(tag, min, max, default)` と、`axis_range(tag)` を持つ（`hb_subset_input_pin_axis_to_default`、`pin_all_axes_to_default`、`pin_axis_location`、`set_axis_range`、`get_axis_range`）。fvarが無いフォントや無い軸では、HarfBuzzと同じく失敗を返して値を変えない。この ADR で実行できるのは、すべての軸を既定の値に固定したときと、軸を指定しないときである。そのほかの指定は、`Plan::new` が `Unsupported` を返す（「後回しにしたもの」）。
  - `to_args()`：`hb_subset_input_to_string_or_fail` と同じく、入力を `hb-subset` のオプションの列にする。
- `Plan::new(font: &Font, input: &Input) -> Result<Plan, SubsetError>`：閉包とグリフの対応を求める。新旧のグリフの対応（両向き）、文字から元のグリフへの対応、残したグリフの集合、出力のグリフの数を公開する（`hb_subset_plan_*_mapping`）。
- `Plan::execute(&self, font: &Font) -> Result<Vec<u8>, SubsetError>`：表を書き、sfntを組む。`subset(font, input)` は、この2つを続けて呼ぶ。
- **前処理**：`Prepared::new(font)` は、`hb_subset_preprocess` と同じく、cmapの対応、文字とグリフの対応の逆引き、CFFの解析済みのcharstringとサブルーチン、`seac` の有無などを作り、呼び出し側が持つ値として返す。`Plan::with_prepared(font, &prepared, input)` は、これを使って速く計画を作る。結果のバイト列は、前処理の無いときと同じでなければならない。値は不変で、スレッドの間で共有できる（[ADR 0003](0003-crate-layers.md) の「キャッシュは呼び出し側の値」）。
- **CFFのcharstringの取り出し**：`cff_charstrings_index(font)` と `cff_charstring_data(font, gid)`（`hb_subset_cff_get_charstrings_index`、`hb_subset_cff_get_charstring_data`）。
- `SubsetError`：`NoGlyphs`（グリフの数が0）、`InvalidInput`（重複するグリフの対応、`RETAIN_GIDS` とグリフの対応の併用など、HarfBuzzが失敗する入力）、`Malformed { tag }`（表のビューを作れない）、`Unsupported { tag, reason }`、`OffsetOverflow { tag }`（再配置でも解けない）、`Limit { tag }`（HarfBuzzの上限に当たった）。
- 可変フォントのインスタンスを持つ `Font` を渡しても、サブセット化は元のデータと `Input` の軸の指定から行う。

### 計画（閉包とグリフの対応）

`src/hb-subset-plan.cc` の手順と順序をそのまま写す。

1. 文字の集合に、`NO_BIDI_CLOSURE` でなければ鏡像の文字を足す。
2. cmapから、文字とグリフの対を集める。グリフの集合が空で、文字の数がグリフの数より少ないときと、それ以外のときで、HarfBuzzは集め方を分ける。結果が同じになることを、両方の経路のテストで確かめる。異体字セレクタは、対応するグリフが無くても文字の集合に残す。OS/2の `usFirstCharIndex` と `usLastCharIndex` は、異体字セレクタを足す前の最小と最大から求める。
3. 閉包の対象の集合を、.notdef、cmapのグリフ、要求したグリフ（グリフの数以上は捨てる）とする。cmapの異体字シーケンスのグリフを足す。MATHを捨てないなら、MATHの閉包を足す。
4. GSUBを捨てないなら、フィーチャと文字体系で選んだlookupについて置換の閉包を求め（`NO_LAYOUT_CLOSURE` では省く）、GSUBとGPOSで使うlookup、フィーチャ、言語系を決める。重複するフィーチャをまとめる。軸を固定するときは、FeatureVariationsの条件を評価する。
5. COLRを捨てないなら、COLRv0の層とCOLRv1の塗りの閉包を足し、使う層、パレット、変化量の番号を集める。VARCを捨てないなら、VARCの閉包を足す。
6. 名前IDの閉包：STAT、（軸を固定しないとき）fvar、CPAL、GSUBとGPOSのフィーチャの引数から、残す名前IDを足す。
7. glyfがあれば、合成グリフの成分を再帰的に足す。操作の数は `HB_MAX_COMPOSITE_OPERATIONS_PER_GLYPH`（64）×グリフの数、読むバイト数はglyfの長さ×`HB_SUBSET_GLYF_MAX_BYTES_FACTOR`（16）を上限とし、超えれば `Limit` とする。
8. CFFなら、`seac`（4つの引数を持つ `endchar`）の基底とアクセントのグリフを、StandardEncodingの対応から足す。
9. GDEF、BASEの変化量の番号を集め、新しい番号を振る。
10. グリフの対応を作る。既定では残すグリフを昇順に0から詰める。`RETAIN_GIDS` では恒等とし、出力のグリフの数を最大のグリフID+1とする（`RETAIN_NUM_GLYPHS` を併せれば元の数）。対応の指定があれば、指定を先に置き、残りをその後に詰める。新しいグリフID 0は元の0でなければならない。

閉包の上限（`HB_CLOSURE_MAX_STAGES`、`HB_MAX_LOOKUP_VISIT_COUNT`、`HB_MAX_CONTEXT_LENGTH` など）は、`src/hb-limits.hh` と同じ値を定数として持つ。

### 表の書き出し

- 表は、元のフォントの目録の順に処理する。捨てる表を先に除き、HarfBuzzの依存の規則（`_dependencies_satisfied`）に従って処理を遅らせる。glyfがあれば、headはglyfの処理が書く。
- 各表の書き出しは、HarfBuzzの各表の `subset` と同じ値を同じ順で書く。HarfBuzzが直列化を使う表は `Serializer` で、そうでない表はバイト列を直接組む。直列化した表であふれが起きれば、HarfBuzzと同じく再配置する（`_hb_subset_repack`）。
- 主な決まりを挙げる。細部は参考文献のファイルに従う。
  - glyf：各グリフを、構造から求めた長さに切り詰める。`NO_HINTING` では命令を除き、`SET_OVERLAPS_FLAG` では最初の点のフラグに0x40を立てる。`RETAIN_GIDS` の空きのグリフは長さ0とする。埋めた後の合計が0x1FFFF未満なら短いlocaとし、各グリフを2バイトに揃える。`IFTB_REQUIREMENTS` では長いlocaとする。
  - hmtx、vmtx：長いメトリクスの数は、末尾から同じ送り幅が続く分を縮める。空きのグリフは送り幅0、サイドベアリング0とする。
  - maxp：グリフの数を出力の数とし、`NO_HINTING` では版1.0の命令に関わる値を改める。
  - OS/2：`usFirstCharIndex`、`usLastCharIndex` を改め、`NO_PRUNE_UNICODE_RANGES` でなければ `ulUnicodeRange` を残した文字にあるビットだけにする。
  - cmap：元の符号化の記録のうち (0,3)、(0,4)、(3,1)、(3,10) と異体字シーケンス（形式14）の記録だけを残し、形式4、形式12、形式14のサブテーブルをHarfBuzzと同じ規則で作りなおす。同じ内容のサブテーブルは共有する。残す記録が無いとき、形式12が無くて (0,3) も (3,1) も無いとき、形式12があって (0,4) も (3,10) も無いときは、cmapを出力に入れない。
  - name：残す名前IDと言語の記録だけを残す。`NAME_LEGACY` でなければUnicodeでない記録を捨てる。上書きの指定があれば、その記録を置きかえる。
  - gvar：残すグリフの変化量のデータを写し、共有の組を保つ。空きのグリフのデータは空とする。
  - HVAR、VVAR、GDEF、BASE、COLRの変化量の表（ItemVariationStore、DeltaSetIndexMap）：使う番号だけを残して詰め、使わない領域を除く。
- 表の書き出しが空を返したとき（残すものが無いとき）は、その表を出力に入れない。HarfBuzzと同じである。

### CFF

[ADR 0021](0021-cff-outlines.md) のCFFの読み取り（INDEX、DICT、Private、charset、Encoding、FDSelect、FDArray）を使い、`src/hb-subset-cff1.cc` と `src/hb-subset-cff-common.{hh,cc}` のとおりに書く。

- **charstringの走査**：サブルーチンの閉包、ヒントの除去、`seac` の検出、脱サブルーチン化には、charstringを命令と数に分け、`callsubr` と `callgsubr` をたどる走査が要る。図形の意味は要らない。[ADR 0021](0021-cff-outlines.md) が `sofel-read` に置く字句の読み手 `CharStringTokens`（輪郭の解釈器も使う）の上に、`sofel-subset` が走査を持つ。これにより、`sofel-subset` は `sofel-outline` に依存しない。
- **サブルーチン**：既定では脱サブルーチン化しない。残すグリフから到達するサブルーチンだけを残し、番号を詰め、偏り（107、1131、32768）を改めてcharstringを符号化しなおす。サブルーチンの番号は直前の数の字句でなければならず、そうでないプログラムは失敗とする（HarfBuzzと同じ）。グリフとサブルーチンの対応が恒等で、ほかの条件も満たせば、元のバイト列を写す（`can_copy_programs`）。
- **`DESUBROUTINIZE`**：サブルーチンを展開して平らにする。`NO_HINTING`：ヒントの命令と数を除く。
- **空きのグリフ**：`RETAIN_GIDS` の空きのグリフは、`endchar` だけのcharstringとする。
- **名前のキー**：使うSIDを集めて振りなおし、charsetとEncodingを必要なら作りなおす。
- **CIDのキー**：FDSelectを残すグリフについて作りなおし、使わないFont DICTを除く。charsetは元のCIDを保ち、空きのグリフには元のグリフIDのCIDを入れる。`CFF_IDENTITY_CHARSET` では、CIDを新しいグリフIDとする。
- charset、FDSelectの形式は、HarfBuzzと同じく大きさの比較で選ぶ。`IFTB_REQUIREMENTS` では、CharStrings INDEXのオフセットの幅を4とする。
- 出力から取り出した `CFF ` の表は、それだけでCFFとして正しくなければならない（PDFの `FontFile3` などに入れる用途のため）。

### CFF2

`src/hb-subset-cff2.cc`、`src/hb-subset-cff2-round.hh`、`src/hb-subset-cff2-to-cff1.hh` のとおりに書く。

- 軸を指定しなければ、CFFと同じくサブルーチンを閉包して詰め、`blend` と `vsindex` を保ったまま符号化しなおす。VariationStoreは、使う `vsindex` に合わせて書きなおす。この部分は、3段階目のItemVariationStoreの読み取りを待つ。
- すべての軸を既定の値に固定すれば、`blend` を既定の値（変化量の0倍）で畳み、脱サブルーチン化し、VariationStoreを除く。グリフの送り幅と縦の原点の値は、HarfBuzzと同じくCFF2から計算しなおす（`update_instance_metrics_map_from_cff2`）。この部分は、[ADR 0021](0021-cff-outlines.md) の既定のインスタンスのCFF2の読み取りで足りる。
- `DOWNGRADE_CFF2`：すべての軸を固定したとき、CFF2をCFFに変換し、表のタグを `CFF ` とする。

### 既定の値への固定

- すべての軸を既定の値に固定した入力は、`hb_subset_input_pin_all_axes_to_default` と同じ結果を出す。glyf、CFF2のどちらの可変フォントも扱う。
- 必要な読み取りは、fvar、avar（版1）、STATの表で、この ADR でビューと検証器を `sofel-read` に足す（3段階目のADRが先に採択されていれば、それを使う）。既定の位置では変化量が0になるので、gvar、HVAR、VVAR、MVARの変化量は適用しない。avarが既定の値を0以外に写すフォントと、avarの版2の固定は、「後回しにしたもの」とし、それまでは `Unsupported` とする。
- HarfBuzzと同じく、glyfのグリフを点から符号化しなおし（フラグの繰り返しと座標の短い形式の選び方を含む）、合成グリフの箱を成分の点から求める。headの箱と `flags` のビット1、hmtxとvmtxのサイドベアリング、maxpの値、OS/2の `usWeightClass`、`usWidthClass`、`xAvgCharWidth`、postの `italicAngle`、STATの軸の値を改める。点の計算に浮動小数点を使う箇所は、HarfBuzzと同じ型（`f32`）と同じ演算の順で書き、FMAへの融合や超越関数を使わない。
- GSUBとGPOSのFeatureVariationsは、既定の位置で条件を評価して畳む。GDEF、BASE、COLRの変化量の表と、ValueRecordとアンカーのVariationIndexのDeviceを、HarfBuzzと同じく除く。

### `sofel-capi`

[ADR 0015](0015-c-abi.md) の決まり（公開のRustのAPIのすべてをCから使えるようにする）に従い、`sofel-subset` と `sofel-write` の公開のAPIに対応する関数を足す。ヘッダの各関数のコメントに、対応するHarfBuzzの関数の名前を書く。

- **入力**（`sofel_subset_input_t`、不透明な型）：
  - `sofel_subset_input_create(sofel_subset_input_t **out)`（`hb_subset_input_create_or_fail`。HarfBuzzの既定の値で作る）、`sofel_subset_input_destroy`（`hb_subset_input_destroy`）、`sofel_subset_input_keep_everything`（`hb_subset_input_keep_everything`）。
  - 集合：`sofel_subset_set_t` の種類（グリフ、文字、写す表、捨てる表、名前ID、名前の言語、レイアウトのフィーチャ、文字体系。`hb_subset_sets_t` と同じ番号）を引数にとる `sofel_subset_input_set_add`、`_add_range`、`_remove`、`_clear`、`_invert`、`_contains`、`_count`、`_next`（`hb_subset_input_set`、`hb_subset_input_glyph_set`、`hb_subset_input_unicode_set` と、それが返す `hb_set_t` の `hb_set_add`、`add_range`、`del`、`clear`、`invert`、`has`、`get_population`、`next`）。
  - フラグ：`sofel_subset_input_get_flags`、`sofel_subset_input_set_flags`（`hb_subset_input_get_flags`、`set_flags`）。ビットの値はHarfBuzzと同じ。
  - グリフの対応の指定：`sofel_subset_input_set_glyph_mapping`、`_clear_glyph_mapping`（`hb_subset_input_old_to_new_glyph_mapping`）。
  - 軸：`sofel_subset_input_pin_all_axes_to_default`、`_pin_axis_to_default`、`_pin_axis_location`、`_set_axis_range`、`_get_axis_range`（同名の `hb_subset_input_*`）。どれもフォントを引数にとる。
  - 名前の上書き：`sofel_subset_input_override_name_table`（`hb_subset_input_override_name_table`）。
  - 文字列：`sofel_subset_input_to_args`（`hb_subset_input_to_string_or_fail`。呼び出し側の配列に書く）、`sofel_subset_axis_range_from_string`、`sofel_subset_axis_range_to_string`（同名の `hb_subset_*`）。
- **前処理**（`sofel_subset_prepared_t`）：`sofel_subset_prepared_create(font, out)`（`hb_subset_preprocess`）、`sofel_subset_prepared_destroy`。
- **計画**（`sofel_subset_plan_t`）：`sofel_subset_plan_create(font, prepared_or_null, input, out)`（`hb_subset_plan_create_or_fail`）、`sofel_subset_plan_destroy`、`sofel_subset_plan_execute(plan, font, sofel_bytes_t *out)`（`hb_subset_plan_execute_or_fail`）、`sofel_subset_plan_old_to_new_glyph`、`_new_to_old_glyph`、`_unicode_to_old_glyph` と、それぞれの対を順に返す関数（`hb_subset_plan_old_to_new_glyph_mapping`、`new_to_old_glyph_mapping`、`unicode_to_old_glyph_mapping`）。
- **実行**：`int32_t sofel_subset(const sofel_font_t *font, const sofel_subset_input_t *input, sofel_bytes_t *out)`（`hb_subset_or_fail` と `hb_face_reference_blob`）。出力は `sofel_bytes_destroy` で解放する。
- **CFF**：`sofel_subset_cff_charstrings_index`、`sofel_subset_cff_charstring_data`（`hb_subset_cff_get_charstrings_index`、`hb_subset_cff_get_charstring_data`）。
- **直列化**：`sofel_serialize_objects`（`hb_subset_serialize_or_fail`。オブジェクトとリンクの配列を受け取り、再配置を含めて表のバイト列を `sofel_bytes_t` で返す）。
- 各関数は、Rustの対応する関数が実装された段階（下の「作業の分け方」）のプルリクエストで足す。
- Golemの `golem_font_subset` は、次の順の呼び出しで置きかえられる：`sofel_subset_input_create`、グリフの集合への `sofel_subset_input_set_add` の繰り返し、`sofel_subset_input_set_flags`（`RETAIN_GIDS`）、軸を固定するなら `sofel_subset_input_pin_all_axes_to_default`、`sofel_subset`、`sofel_subset_input_destroy`。

### `sofel-cli` の `sofel-subset`

- `hb-subset`（`util/hb-subset.cc`）のオプションをすべて受け付ける：`--font-file`、`--output-file`（`-o`）、`--unicodes`、`--gids`、`--glyphs`、`--text`、`--name-IDs`、`--name-languages`、`--layout-features`、`--layout-scripts`、`--drop-tables`（それぞれ `+` と `-` の形、`*`、`-file` の形のあるものはそれも）、`--gid-map`、各フラグのオプション、`--keep-everything`、`--variations` と `--instance`、`--preprocess`、`--preprocess-face`、`--num-iterations`、`--optimize`、`--batch`。
- `--glyphs` はグリフの名前で指定する。名前は、HarfBuzzの `hb_font_get_glyph_from_name` と同じく、postの版2とCFFのcharsetから引く。
- `--instance` のうち「後回しにしたもの」に当たる指定は、それが済むまで誤りを書いて終了の状態1で終わる。
- 引数の解析は `sofel_cli::subset` に純粋な関数として置き、バイナリと `sofel-conformance` が共有する。
- `--batch` は、`hb-subset` と同じく、1行に `;` で区切った引数を読み、成功なら `success` を書く。

### 適合テスト

- `sofel-conformance` に、[ADR 0005](0005-conformance-tests.md) のサブセット化の適合テストを置く。`*.tests` の104ファイルすべてを `subset_test_suite.py` と同じ規則で解析し、組ごとに、`run-tests.py` と同じ引数（`--drop-tables+=DSIG,BASE --drop-tables-=sbix` とプロファイルとインスタンス）を `sofel_cli::subset` で解析して `sofel-subset` を呼ぶ。前処理の無い場合と、`Prepared` を使う場合の2回を走らせる。
- テストの識別子は `<testsのファイル名>/<期待する出力のファイル名>` とする。後者はフォント、プロファイル、文字の集合、インスタンスを含む（`get_font_name`）。前処理を使う回は末尾に `+prepared` を付ける。
- 各組で2つを確かめる。`:bytes` は出力が期待するファイルとバイト単位で一致すること、`:validate` は出力が検証器で `Error` を出さないこと。一致しないときは、表ごとに一致しない最初の位置を報告する（[ADR 0005](0005-conformance-tests.md)）。
- 期待する失敗の一覧は `conformance/expected-failures-subset.txt` とし、各行に理由（`stage:cff2`、`stage:colr`、`deferred:instance` など）をコメントで添える。最初の一覧は、まだ書けない表、フラグ、インスタンスを持つ組から機械的に作り、実装の段階が進むたびに行を消す。すべての段階を終えた後に残るのは、「後回しにしたもの」の約114件（インスタンスの91件とIUPの23件）だけとする。
- `repack_tests` は、`run-repack-tests.py` と同じ引数で `sofel-subset` を呼び、成功することと `:validate` を確かめる。これらの出力は、`tools/hb-dump/`（下）でHarfBuzzの出力を書き出して、バイト単位でも比べる。
- **HarfBuzzの出力との突き合わせ**：[ADR 0016](0016-font-and-vertical-metrics.md) の `tools/hb-dump/` に `--subset` を足す。`hb-subset` と同じオプションの列を受け取り、HarfBuzzを実験的なAPIを有効にしてビルドしたもので `hb_subset_or_fail` の出力を書き出す。これで、テストに期待する出力の無い組を比べる。
  - Golemの使い方：`test/subset/data/fonts/` の全フォントについて、HarfBuzzの `perf/texts/ja-runs.txt` と `en-paragraph.txt` の文字をcmapで引いたグリフと、グリフID 1と最後のグリフを加えた集合を、`RETAIN_GIDS` で、軸の固定の有無の2通りでサブセット化する。識別子は `hb-dump/golem/<フォント>:<pin|keep>` とする。
  - 再配置：`repack_tests` の各組。識別子は `hb-dump/repack/<testsのファイル名>` とする。
- CIでは、Linux（x86-64）とmacOS（aarch64）でこれらを走らせる。`hb-dump` と `ft-dump` を要しない部分は別のテストの対象に分け、`wasm32-wasip1` でもwasmtimeで走らせる。期待する出力との一致を3つの環境で確かめることで、決定性を確かめる。

### 計測

[ADR 0013](0013-benchmark-harness.md) の計測の道具に、次の項目を足す。

| 項目 | sofel | HarfBuzz |
|---|---|---|
| サブセット化（グリフID、`RETAIN_GIDS`） | `subset` | `hb_subset_or_fail` と `hb_face_reference_blob` |
| サブセット化（文字、既定のフラグ） | `subset` | 同上 |
| 前処理つきのサブセット化 | `Prepared` と `Plan::with_prepared` | `hb_subset_preprocess` のあとの `hb_subset_or_fail` |
| 前処理 | `Prepared::new` | `hb_subset_preprocess` |

- フォントは `test/subset/data/fonts/` の Roboto-Regular.ttf（glyf）、SourceHanSans-Regular_subset.otf（CIDのCFF）、NotoNastaliqUrdu-Bold.ttf（大きなGSUBとGPOS。再配置が働く）とし、可変フォントとカラーのフォントは、その段階が済んだときに加える。文字とグリフの集合は、上の突き合わせと同じ作り方とする。
- 計測の前の照合は、出力のバイト列の一致とする。

### 作業の分け方

設計は1つとし、実装は次の順にプルリクエストを分ける。各段階で、まだ書けない入力は `Unsupported` を返すので、途中でも誤った出力は出ない。期待する失敗の一覧が、進み具合を示す。順序は、依存する読み取りの段階と、Golemが使う範囲を先にすることで決める。

1. `sofel-write`（組み立てと直列化）、`Input`、`Plan` のうちcmapと合成グリフの閉包とグリフの対応、glyf系の表（大文字の表を含む）、cmap、DMAP、post、name、OS/2、hdmx、写す表、`sofel-subset` のコマンド、適合テストの仕組みと一覧。[ADR 0016](0016-font-and-vertical-metrics.md) の後に始められる。鏡像の文字は [ADR 0019](0019-unicode-character-data.md) の後に足す。
2. CFFとVORG（[ADR 0021](0021-cff-outlines.md) の後）。
3. GDEF、GSUB、GPOSとレイアウトの閉包、名前IDの閉包（[ADR 0020](0020-shaping-foundation.md) の後）。BASE（[ADR 0017](0017-base-baselines.md) の後）。
4. 再配置。3と同時か直後に入れる。日本語のフォントや大きなGSUBとGPOSのフォントで要るためである。
5. ここまでに実装した関数の `sofel-capi`（入力、計画、`sofel_subset`、直列化）と、Golemの使い方の突き合わせと計測。ここまでで、Golemの日本語と欧文のフォントを置きかえられる。以後の段階は、Rustの関数と同じプルリクエストで `sofel-capi` の関数を足す。
6. すべての軸の既定の値への固定（glyf）と、fvar、avar、STATの読み取り。FeatureVariationsの畳み込みは3の後に足す。
7. CFF2の既定の値への固定と `DOWNGRADE_CFF2`（[ADR 0021](0021-cff-outlines.md) の後）。
8. MATH。
9. COLR、CPAL、CBDT、CBLC、sbix（[ADR 0023](0023-color-glyphs.md) の後）。
10. 軸を指定しないときの可変フォントの表（gvar、HVAR、VVAR、CFF2の変化量、GDEF、BASE、COLRの変化量の表）。3段階目の読み取りの後。VARCは13段階目の後。
11. 前処理（`Prepared`）と、CFFのcharstringの取り出し。
12. `--glyphs`（postの版2とCFFのグリフ名の引き方）。

## データ構造

- `RangeSet`：`u32` の昇順で重ならない範囲の列と、反転の印。文字、グリフ、名前ID、言語の集合に使う。HarfBuzzの `hb_set_t` の意味（反転した集合、範囲の追加と削除）を持つ。タグの集合は昇順の `Vec<Tag>` とする。
- `GlyphMap`：新しいグリフIDの昇順の `(new, old)` の列と、古いIDから新しいIDを引く表。引く表は、元のグリフの数が65536以下なら密な配列、それを超えれば昇順の列の二分探索とする。
- `Plan`：グリフの対応、文字から新しいグリフへの昇順の列、閉包の各段階の集合（`glyphset_gsub`、`glyphset_colred` など）、残す名前ID、lookupとフィーチャの新旧の対応、言語系、変化量の番号の対応、COLRの層とパレットの対応、CFFの計画（fdの対応、SIDの対応、サブルーチンの対応）、OS/2、head、maxpのために集めた値、軸の正規化した位置。どれも作った後は変えない。
- `Prepared`：`hb_subset_accelerator_t` の内容（cmapの対応とその逆引き、CFFの解析済みのcharstring、`seac` の有無）。不変の値とする。
- `Serializer` の内部：オブジェクトの列（バイト列の範囲とリンクの列）、作業中のスタック、統合のための表。統合の表は `alloc::collections::BTreeMap` でバイト列とリンクを鍵とする。ハッシュの表を使わないので、乱数の種に結果が依存しない。
- `Graph`：頂点（オブジェクトのバイト列、子へのリンク、親の数、距離、空間の番号、優先度）の列。HarfBuzzの `graph_t` と同じ情報を持つ。整列の優先度つきの待ち行列は、HarfBuzzの `hb_priority_queue_t` と同じ同点の扱いの二分ヒープとして自分で書く。
- 出力は `Vec<u8>` とし、表ごとの書き出しは呼び出し側の作業領域を再利用できる形（`&mut Vec<u8>` に追記して範囲を返す）とする。

## アルゴリズム

- **閉包**：HarfBuzzの各表の `closure_glyphs`、`closure_lookups`、合成グリフの再帰、`seac`、COLRとMATHとVARCの閉包を、同じ順と同じ上限で行う。集合を順に広げる処理なので、途中の集合の順序が結果に影響しない形（集合の和）で書く。
- **直列化**：`hb-serialize.hh` と同じく、オブジェクトを子から順に作業領域の末尾側に詰め、最後に根を詰める。リンクの値は `end_serialize` で解く。同じオブジェクトの統合は、共有を許したオブジェクトだけを対象とする。
- **再配置**：`hb_resolve_graph_overflows` の手順（上の文脈）を、同じ繰り返しの上限、同じ優先度の計算、同じ同点の扱いで写す。分割と昇格は、HarfBuzzと同じ大きさの見積もりで決める。結果がHarfBuzzと同じ並びになることは、バイト単位の一致で確かめる。
- **組み立て**：表を長さとタグで整列し、4バイト単位の和でチェックサムを求める。
- **CFFのサブルーチン**：各グリフのcharstringを走査し、到達するサブルーチンの集合（全体とfdごと）を求める。集合の昇順で新しい番号を振り、偏りを数から決め、charstringとサブルーチンを符号化しなおす。数の符号化（1、2、3、5バイトの形）は、HarfBuzzの `str_encoder_t` と同じ規則で選ぶ。
- **浮動小数点**：使うのは、HarfBuzzが使う箇所（軸の固定での点の計算、`usWeightClass`、`usWidthClass`、`italicAngle`、CFF2の `blend` の丸め）だけとし、HarfBuzzと同じ型と演算と丸め（`roundf`）を使う。IEEE 754の基本の演算は、どの環境でも同じ結果になる。

## 実装しないこと

- 「後回しにしたもの」に挙げたもの。この ADR の範囲では、`Unsupported` を返す。
- HarfBuzzが扱いを持たない表のサブセット化（`SVG `、`EBDT`、`EBLC`、`EBSC`、`kern`、`morx` など）。HarfBuzzと同じく捨てるか、`PASSTHROUGH_UNRECOGNIZED` でそのまま写す。
- 壊れた表についてのHarfBuzzとの一致。HarfBuzzのsanitizeは、壊れたオフセットを0に書きかえて先へ進むことがある。sofelは、ビューを作れない表を `Malformed` とし、panicせず、検証器の通る出力か誤りを返すことだけを保証する。
- WOFFとWOFF2への書き出し（[ADR 0001](0001-project-goal-and-scope.md)）。
- HarfBuzzの `hb_subset_input_set_user_data` などの利用者のデータ。Rustでは、呼び出し側が値と一緒に持てばよい。

## 後回しにしたもの

実装の後に、次のADRを作成する。

### 可変フォントのインスタンス化（任意の位置への固定と、軸の範囲の絞り込み）

3段階目（可変フォント）のADRで変化量の適用ができた後に作成する。次を含める。

- 入力：`with_axis_pinned(tag, value)` と `with_axis_range(tag, min, max, default)` の実行。`hb_subset_axis_range_from_string` と `to_string`、`--instance` の指定（値、`min:max`、`min:def:max`、空の部分は元の値）。
- 正規化：fvarの範囲への切り詰め、avar版1の写像、avar版2（変化量による写像。HarfBuzzの `has_avar2` の経路と、自己完結した固定の判定）。avarが既定の値を0以外に写すフォントの既定の値への固定も、ここで扱う。
- 部分的なインスタンス化の解法：`src/hb-subset-instancer-solver.cc`（tentの組みなおし、`rebase_tent`、軸の距離 `axes_triple_distances`）。
- 変化量の表の書きなおし：gvar（変化量の適用、残る組の作りなおし、推定した点の変化量、パディング）、`OPTIMIZE_IUP_DELTAS` のIUPの最適化（`src/hb-subset-instancer-iup.cc`）、cvarとcvtへの適用、ItemVariationStoreのインスタンス化（領域の組みなおし、変化量の丸め、行の統合。HVAR、VVAR、MVAR、GDEF、BASE、COLR）、DeltaSetIndexMapの作りなおし。
- 値の更新：任意の位置でのhead、hhea、hmtx、vmtx、maxp、OS/2、post、MVARで変わる値、STAT、fvar（範囲の書きなおしと名前ID）、avarの書きなおし。
- レイアウト：FeatureVariationsの部分的な評価（条件の書きなおしと、すべてに当たる記録の挿入）、ValueRecordとアンカーのDeviceの変化量の適用。
- CFF2：任意の位置での `blend` の評価と丸め（`src/hb-subset-cff2-round.hh`）、部分的なインスタンス化でのVariationStoreの書きなおし、`DOWNGRADE_CFF2` の任意の位置。
- COLRv1の部分的なインスタンス化（変化量の番号と、固定後に定数になる塗りの扱い）。
- VARCのインスタンス化（13段階目の後）。
- テスト：HarfBuzzのテストのインスタンスの91件とIUPの23件を、期待する失敗の一覧から外すこと。`tools/hb-dump/` での追加の突き合わせ（任意の位置と範囲の組）。計測の項目に、可変フォントのインスタンス化を足す。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。フォントは、[ADR 0007](0007-sfnt-read-foundation.md) のテストの組み立て方で作る。

- **組み立て**：表の数が0、1、2の累乗、そうでない数のときの検索の補助の値。データの並び（長さが同じ表、違う表）、指定した並び、4バイトの埋め、各表のチェックサム、headの `checkSumAdjustment` を、手で計算した値と比べる。出力を `sofel-read` の目録の検証器に通し、`Error` が無いことを確かめる。
- **直列化**：子を持つオブジェクト、共有される子、同じ内容の2つの子（統合される）、共有を許さないオブジェクト（統合されない）、16、24、32bitと符号つきのリンク、3つの起点、スナップショットへの巻き戻し、あふれの検出で、出力のバイト列を手で組んだ期待の値と比べる。
- **再配置**：HarfBuzzの `src/test-repacker.cc` の各テスト（グラフの組み立てと期待するグラフ）を、同じグラフと同じ期待の値でRustに写す。位相の整列、空間の割り当て、複製、優先度の引き上げ、PairPos、MarkBasePos、Ligatureの分割、拡張への昇格を含む。優先度つきの待ち行列の同点の扱いを、単体で確かめる。
- **`RangeSet`**：範囲の追加と削除、反転、反転した集合への追加、境界の値（0、`u32::MAX`）を確かめる。
- **計画**：手で組んだフォントで、cmapの2つの集め方が同じ対を出すこと、異体字セレクタの扱い、要求したグリフのうちグリフの数以上のもの、合成グリフの入れ子と自己参照と上限、`seac`、`RETAIN_GIDS` と `RETAIN_NUM_GLYPHS` と対応の指定によるグリフの数、`InvalidInput` になる入力を確かめる。
- **入力**：`Input::new()` と `keep_everything()` の各集合とフラグが、`src/hb-subset-input.cc` から写した一覧と一致すること。軸の指定の関数が、fvarの無いフォントと無い軸で失敗し、値を変えないこと。`to_args()` の結果を `sofel_cli::subset` で解析すると元の `Input` に戻ること。
- **前処理**：適合テストの全組を `Prepared` の有無で走らせ、結果が同じであることを確かめる（上の `+prepared`）。
- **段階の途中の扱い**：まだ書けない表を持ち、それを捨てないとき `Unsupported` になり、捨てる表に入れれば成功すること。「後回しにしたもの」の軸の指定で `Unsupported` になること。
- **適合テスト**：上の `sofel-conformance` のテスト。一覧の判定の仕組みは [ADR 0005](0005-conformance-tests.md) の既存のテストで確かめてある。
- **輪郭と送り幅の保存**：適合テストの全フォントの出力（軸を固定しない組）について、残した各グリフの輪郭（[ADR 0011](0011-glyf-outlines.md)、[0021](0021-cff-outlines.md) の `draw` の命令の列、拡大縮小なし）と送り幅が元のフォントと一致し、`RETAIN_GIDS` の空きのグリフの輪郭が空であることを確かめる。glyfの .notdef は、`NOTDEF_OUTLINE` でなければ空であることを確かめる。既定の値に固定した組では、元のフォントの既定のインスタンスの輪郭と比べる。CFFでは、出力から取り出した `CFF ` の表だけで検証器が `Error` を出さないことも確かめる。
- **決定性**：同じ入力を2回与えて同じバイト列が出ること、グリフの集合を与える順を変えても同じであることを確かめる。環境の間の一致は、上のCIの3つの環境での適合テストで確かめる。
- **引数の解析**：`sofel_cli::subset` が、HarfBuzzの `test/subset/data/profiles/` のすべてのプロファイル、`run-tests.py` と `run-repack-tests.py` の固定の引数、すべてのインスタンスの指定を解析できることを確かめる。`--batch` の入出力を確かめる。
- **`sofel-capi`**：[ADR 0015](0015-c-abi.md) の決まりに従い、Cのテストで、入力の各関数で作った入力と同じ値を `sofel_subset_input_to_args` が返すこと、`sofel_subset` と計画を経た `sofel_subset_plan_execute` と前処理つきの計画が `sofel-subset` と同じバイト列を返すこと、計画の対応を引く関数が `Plan` と同じ値を返すこと、Golemの呼び出しの順が `hb-dump` の出力と同じバイト列を返すこと、NULLと不正なフラグと不正な集合の種類で誤りが返ることを確かめる。
- **ファジング**：`fuzz/` に、任意のバイト列のフォントと、任意のグリフの集合とフラグで `subset` を呼ぶ対象と、任意のオブジェクトのグラフで再配置を呼ぶ対象を足す。panicしないこと、成功した出力を `sofel-font` が開け、検証器が `Error` を出さないこと（そのまま写す表のうち元から `Error` のあるものを除く）、再配置の結果のオフセットがあふれないことを確かめる。
- **wasm**：`sofel-write` と `sofel-subset` の単体テストを `wasm32-wasip1` で通し、`wasm32-unknown-unknown` でビルドできることを確かめる。

## トレードオフ

- バイト単位の一致を求めるため、HarfBuzzの直列化の詰め方、形式の選び方、表の並べ方、再配置の手順を、より速いか小さい方法があっても写す。代わりに、出力の正しさを1123件の期待する出力と `hb-dump` の出力で機械的に確かめられ、`hb-subset` の利用者は出力を変えずに移れる。HarfBuzzの版を上げるたびに、期待する出力の差分を追う費用がかかる。
- バイト単位の一致は、決定性の確かめ方も兼ねる。3つの環境で同じ期待する出力と一致すれば、環境の間の一致も示せる。
- 再配置をこの ADR に含めるため、`src/hb-repacker.hh` と `src/graph/` の約7000行を、同点の扱いまで写す必要がある。含めなければ、大きなCJKのフォントやGSUBとGPOSの多いフォントで、HarfBuzzが成功する入力に失敗する。汎用のサブセット化として、これは受け入れられない。
- HarfBuzzの実験的なAPIの `IFTB_REQUIREMENTS` と `RETAIN_NUM_GLYPHS` を、常に有効として扱う。HarfBuzzの既定のビルドでは働かないため、そのビルドと結果が違う。テストの期待する出力が実験的なAPIのビルドで作られていることと、機能を欠かないことを優先する。HarfBuzzがこれらを変えたら、固定のコミットを上げるときに追う。
- 実装の途中では、まだ書けない表を誤りにするため、COLRなどを持つフォントは、その段階が済むまで、利用者が捨てる表を指定しないと失敗する。黙って捨てると、HarfBuzzと違う出力を誤りなく返すことになり、基準と矛盾するためである。
- charstringの字句の読み手（`sofel-read`）を共有し、サブルーチンをたどる走査は `sofel-subset` に持つ。`sofel-outline` の解釈器と、サブルーチンをたどる処理が重なる。代わりに、[ADR 0003](0003-crate-layers.md) の依存の表を変えずに済み、サブセット化が輪郭の層に依存しない。
- fvar、avar、STATの読み取りを、3段階目のADRを待たずにこの ADR で足す。既定の値への固定を早く使えるようにするためで、3段階目のADRはこれを土台にする。
- 任意の位置へのインスタンス化を後回しにするため、それを要する入力は、3段階目の後まで使えない。変化量の適用と、tentの解法の両方が要り、可変フォントの読み取りと同時に設計するほうが、表の扱いが揃うためである。
- `RETAIN_GIDS` のグリフの対応を、元のグリフの数が65536以下なら密な配列で持つ。24bitのグリフIDのフォントでは二分探索になり、遅くなる。
- Golemは、いまHarfBuzz 14.6.0の出力を使っており、sofelの基準の15.1.0とは出力が違いうる。Golemのテストは決定性と含まれるグリフを確かめるもので、特定のバイト列には依存しない。

## 参考文献

- HarfBuzz 15.1.0（`3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`）
  - `src/hb-subset.h`、`src/hb-subset-serialize.h`、`src/hb-subset-serialize.cc`、`src/hb-subset.cc`、`src/hb-subset-input.cc`、`src/hb-subset-input.hh`、`src/hb-subset-plan.cc`、`src/hb-subset-plan.hh`、`src/hb-subset-plan-layout.cc`、`src/hb-subset-plan-var.cc`、`src/hb-subset-accelerator.hh`、`src/hb-subset-table.hh`、`src/hb-subset-table-other.cc`、`src/hb-subset-table-layout.cc`、`src/hb-subset-table-var.cc`、`src/hb-subset-table-cff.cc`、`src/hb-subset-table-color.cc`、`src/hb-subset-instancer-solver.cc`、`src/hb-subset-instancer-iup.cc`
  - `src/hb-subset-cff1.cc`、`src/hb-subset-cff-common.hh`、`src/hb-subset-cff-common.cc`、`src/hb-subset-cff2.cc`、`src/hb-subset-cff2-round.hh`、`src/hb-subset-cff2-to-cff1.hh`
  - `src/hb-face-builder.cc`、`src/hb-open-file.hh`、`src/hb-serialize.hh`、`src/hb-repacker.hh`、`src/graph/`、`docs/repacker.md`、`src/hb-limits.hh`、`src/hb-config.hh`、`src/test-serialize.cc`、`src/test-repacker.cc`
  - `src/OT/glyf/glyf.hh`、`src/OT/glyf/glyf-helpers.hh`、`src/OT/glyf/Glyph.hh`、`src/OT/glyf/SimpleGlyph.hh`、`src/OT/glyf/CompositeGlyph.hh`、`src/OT/glyf/SubsetGlyph.hh`、`src/hb-ot-hmtx-table.hh`、`src/hb-ot-maxp-table.hh`、`src/hb-ot-os2-table.hh`、`src/hb-ot-post-table.hh`、`src/hb-ot-stat-table.hh`、`src/hb-ot-cmap-table.hh`、`src/OT/name/name.hh`、`src/hb-ot-var-gvar-table.hh`、`src/hb-ot-var-hvar-table.hh`、`src/OT/Color/CBDT/CBDT.hh`
  - `util/hb-subset.cc`
  - `test/subset/meson.build`、`test/subset/run-tests.py`、`test/subset/subset_test_suite.py`、`test/subset/run-repack-tests.py`、`test/subset/data/tests/*.tests`、`test/subset/data/profiles/`、`test/subset/data/expected/`、`test/subset/data/repack_tests/`
- Golem `modules/golem-font/cbits/golem/golem_font.c`（`golem_font_subset`）、`docs/golem-font/adr/0002-subsetting-and-embedding.md`、`docs/golem-font/adr/0003-reject-cff2-fonts-in-pdf.md`
- OpenType Specification：sfntの目録、head、glyf、loca、hmtx、cmap、name、OS/2、post、STAT、fvar、avar、MATH、COLR、CPAL、CBDT、CBLC、sbix、gvar、HVAR、MVAR、CFF2 https://learn.microsoft.com/en-us/typography/opentype/spec/
- Adobe Technical Note #5176（The Compact Font Format Specification）、#5177（Type 2 Charstring Format）
- ISO/IEC 14496-22 第5版（`GLYF`、`LOCA` などの大文字の表）
- ISO 32000-1:2008 §9.9（埋め込みフォントのプログラム）
