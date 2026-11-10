# ADR 0021: CFFとCFF2（`sofel-read` の表、`sofel-font` のCIDとグリフ名とフォントの情報、`sofel-outline` のType 2 charstringの輪郭と箱）

ステータス: 採択

## 文脈

[ADR 0001](0001-project-goal-and-scope.md) の実装の順序では、CFFとCFF2は6段階目にある。
順序は利用者の必要に応じて入れかえてよい（同ADRの「実装の順序」）。

sofelはHarfBuzzとFreeTypeを置きかえる汎用のライブラリである。
この ADR は、両者がsfntに包まれたCFFとCFF2について公開するもの（輪郭、グリフの箱、CIDの問い合わせ、グリフ名、PostScriptのフォントの情報）を、可変フォントの既定のインスタンスの範囲で網羅する。
最初の利用者になる予定のGolem（[ADR 0015](0015-c-abi.md)）は、その一部を使う。
Golemは日本語のOpenTypeのフォント（`OTTO`、CFFの版1、多くはCIDキー）を使い、PDFに埋め込むためにCFFの表をそのまま取り出し（`FontFile3`/`CIDFontType0C`）、グリフIDからCIDへの対応（`FT_Get_CID_From_Glyph_Index`）とROS（`FT_Get_CID_Registry_Ordering_Supplement`）を書く。
グリフの箱は、`FT_LOAD_NO_SCALE | FT_LOAD_NO_HINTING` で読んだ輪郭の制御の箱をFreeTypeから取っている。

### FreeType 2.15.0

sfntに包まれたCFFとCFF2を次のように扱う（`src/cff/`、`src/psaux/`）。

- 駆動の選び方：TrueTypeの駆動が先に試され、glyfかlocaがあればTrueTypeとして開く。無ければ、sfntの版が `OTTO` のときに限り、CFFの駆動が `CFF2`、`CFF ` の順に表を探す（`src/cff/cffobjs.c` の `cff_face_init`）。
- 読み込みの検査（`src/cff/cffload.c` の `cff_font_load`）：
  - CFF：版1、`hdrSize` 4以上、`offSize` 4以下。Name INDEXは1つだけ（sfntの中で2つ以上は誤り）、Top DICT INDEXはName INDEX以上の数。
  - CFF2：版2、`hdrSize` 5以上。Top DICTはINDEXではなく、ヘッダの `topDictLength` の長さで直後に置かれる。Name INDEX、String INDEX、charset、encodingは無い。
  - 共通：Top DICT、CharStrings INDEX、CIDキーかCFF2ならFDArray（256以下）とFDSelect、CFF2ならVariationStore、各Private DICTとローカルのサブルーチンを読む。CFFでは、charsetと、CIDキーでなければencodingも読む。どれかが壊れていれば、フェイスを開けない。
  - FDSelectはフォーマット0と3だけを読み、CFF2のフォーマット4は誤りとする（`CFF_Load_FD_Select`）。
- グリフの数は、CharStrings INDEXの数とする（`maxp` ではない）。sfntの中のCIDキーのフォントでは、`FT_Load_Glyph` のグリフIDはCIDではなくグリフIDのままである（CIDへの付けかえは、sfntに包まれていないCFFだけ）。
- charstringの解釈：既定の解釈器は、Adobeが寄贈した `src/psaux/ps*.c`（「Adobe engine」、`FT_HINTING_ADOBE`）である。古い解釈器（`CFF_CONFIG_OPTION_OLD_ENGINE`）は既定では組み込まれない。数はすべて16.16で持つ。オペランドのスタックはCFFで48、CFF2で `maxstack`（既定513、上限513で、それ未満の値は513にする）。サブルーチンの入れ子は16までである。
- CFF2の `blend` と `vsindex`（`cf2_doBlend`、`cff_blend_build_vector`）：既定の値に、各差分を `FT_MulFix(BV[i], 差分)` で足す。`BV` は、正規化した座標が無ければ（fvarの無いフォント）`(1, 0, …, 0)`、あれば各領域の係数とする。領域の係数は、ピークが0か座標と等しい軸を飛ばし、座標が始まり以下か終わり以上なら0、ほかは比例で求める。既定のインスタンスでは座標がすべて0なので、ほとんどの領域の係数は0になるが、すべての軸のピークが0の領域は1、始まりが負でピークが正の領域は0でない値になる。座標の数がVariationStoreの軸の数と違う、`vsindex` が範囲外、VariationStoreの無いフォントで `blend` を使う、`blend` の後に `vsindex` を使う、はいずれも誤りとする。
- ヒンティングをしないときの座標（`src/psaux/psft.c` の `cf2_getScaleAndHintFlag`、`src/psaux/pshints.c`、`src/psaux/psobjs.c` の `ps_builder_add_point`、`src/cff/cffgload.c` の `cff_slot_load`）：解釈器は倍率 `0x400`（1/64）で描き、点を `FT_MulFix(0x400, x)` で求め、`>> 10` でフォントの単位の整数にする。つまり、小数の座標は整数に切り捨てられる。そのあと、FontMatrixが単位行列でなければ `FT_Outline_Transform` をかけ、FontMatrixの平行移動を足し、最後に各点を `FT_MulFix(x, x_scale)` で拡大縮小する。`FT_LOAD_NO_SCALE` では拡大縮小しない。ただし、FDArrayのあるフォントで、Top DICTとFont DICTの `units_per_em` が違えば、`FT_LOAD_NO_SCALE` でも倍率を `FT_MulDiv(x_scale, top_upm, sub_upm)` にして拡大縮小する。
- ステムの太らせ（stem darkening）は、駆動の既定（`no_stem_darkening = TRUE`）で無効である。
- 輪郭の印：`FT_OUTLINE_REVERSE_FILL` と、`y_ppem < 24` のとき `FT_OUTLINE_HIGH_PRECISION` を付ける。`FT_OUTLINE_OVERLAP` は付けない。smoothのラスタライザは前の2つを見ないので、CFFのグリフは常に重なりなしとして描かれる（`src/smooth/ftsmooth.c`）。
- `random` 演算子の種は、駆動を作るときのメモリのアドレスから作る（`src/cff/cffobjs.c` の `cff_driver_init`）。フェイスごとの種（`FT_PARAM_TAG_RANDOM_SEED`）が0なら、Private DICTの `initialRandomSeed` を使う。状態はサブフォントに残り、グリフをまたいで進む。
- CIDの問い合わせ（`src/cff/cffdrivr.c`）：`cff_get_is_cid` はTop DICTにROSがあるか。`cff_get_cid_from_glyph_index` は、ROSが無いか、グリフIDがCharStringsの数以上なら `Invalid_Argument`、そうでなければcharsetの値を返す。`cff_get_ros` は、ROSが無ければ `Invalid_Argument`。登録者と順序は、SIDが390以下なら標準の文字列、391以上ならString INDEXの文字列で、範囲外ならNULLを返す。補遺は、実数なら切り捨てる。CFF2にはROSが無いので、すべて `Invalid_Argument` になる。TrueTypeのフォントには、この問い合わせが無い（`Invalid_Argument`）。
- グリフ名（`cff_get_glyph_name`、`cff_get_name_index`）：CIDキーでないCFFだけが `FT_FACE_FLAG_GLYPH_NAMES` を持ち、名前はcharsetのSIDの文字列とする（postの表は見ない）。SIDの文字列が無ければ、成功して空の名前を返す。名前からグリフへは、グリフIDの順に線形に探し、最初に一致したものを返し、無ければ0を返す。CIDキーのフォントは名前を持たない。CFF2ではpostの表の手順に移る。
- PostScriptのフォントの情報：`FT_Get_PS_Font_Info` は、CFFのTop DICTの `version`、`Notice`、`FullName`、`FamilyName`、`Weight`（SIDの文字列）、`ItalicAngle`、`isFixedPitch`、`UnderlinePosition`、`UnderlineThickness` を返し、CFF2では `Invalid_Argument` とする。`FT_Get_PS_Font_Extra` は、Top DICTの `PostScript` の文字列から `/FSType <数> def` の数を取り出す（数字と空白以外が混じるか、あふれれば0）。`FT_Has_PS_Glyph_Names` はグリフ名の印と同じ。`FT_Get_PS_Font_Private` と `FT_Get_PS_Font_Value` は、CFFでは扱わない（`Invalid_Argument`）。
- PostScript名：sfntに包まれたCFFでは、nameの表だけを使う（`cff_get_ps_name`）。nameの表から得られなくても、CFFのName INDEXには戻らない。
- sfntに包まれていないCFF（素のCFF、Name INDEXの各要素が1つのフェイス）とCEFも、同じ駆動が開く。その場合は、`units_per_em` を1000とし、encodingとグリフ名（Adobe Glyph List、`psnames` のモジュール）から文字の対応を合成し（`src/cff/cffcmap.c`）、CIDキーなら `FT_Load_Glyph` のグリフIDをCIDとしてcharsetの逆引きでグリフIDに付けかえる（`cff_charset_compute_cids`）。

### HarfBuzz 15.1.0

- CFFとCFF2のグリフの箱（`hb_font_get_glyph_extents`）を、自前の解釈器で求める（`src/hb-ot-cff1-table.cc`、`src/hb-ot-cff2-table.cc` の `get_extents`、`src/hb-cff-interp-*.hh`）。
- 数は `double` で持ち、オペランドのスタックは513、サブルーチンの入れ子は10、1グリフの演算子は200000までである。CFFの算術の演算子（`div` など）は未知の演算子としてスタックを捨てる。
- 箱は、曲線の制御点を含むすべての点の最小と最大を、`roundf`（`float` に変換してから半分を0から遠い側へ丸める）で丸める。
- CFF2の `blend` は、フォントに座標が無ければ（既定のインスタンス）差分を使わず、既定の値だけを残す（`do_blend = num_coords && varStore->size`）。FreeTypeと違い、領域の係数を求めない。
- アクセラレータの作成（`src/hb-ot-cff1-table.hh`、`src/hb-ot-cff2-table.hh`）が、charset、FDSelect（CFF2ではフォーマット4も受け付ける）、FDArray、encoding、Private DICTを検査し、ひとつでも壊れていれば、すべてのグリフの箱を求めない。
- グリフ名（`hb_ot_get_glyph_name`）は、postの表を先に、次にCFFのcharsetを引く。CIDキーのCFFは名前を持たない。名前からグリフへは、全グリフの名前を整列した表（`qsort`）を二分探索する。同じ名前が2つ以上あれば、どれが返るかは整列の実装による。
- [ADR 0016](0016-font-and-vertical-metrics.md) は、グリフの箱とグリフ名をHarfBuzzに合わせ、CFFの部分をこのADRに任せた。

### sofelの制約

[ADR 0010](0010-font-foundation.md) の `Font::new` は、表の数とcmapの記録の数に比例する時間で終わり、確保をしない。
CFFの読み込みの検査は、charsetの範囲の数に比例する時間がかかり、グリフごとに繰り返すと遅い。
charstringの解釈は `sofel-outline` が行う（[ADR 0003](0003-crate-layers.md)）。

## 実装すること

### `sofel-read`：CFFとCFF2の表

`sofel-read` に `cff` のモジュールを足す。[ADR 0007](0007-sfnt-read-foundation.md) の方式（確保なし、作るときにヘッダを検査、範囲外は `None`）に従う。

- `Index`：INDEXのビュー。数の幅（CFFは16bit、CFF2は32bit）を引数にとる。`len()`、`get(i) -> Option<&[u8]>` は、2つのオフセットを読み、始まりが終わり以下でデータの中にあるときだけ返す。`end()`（INDEXの直後の位置）を持つ。
- `Header`：版、`hdrSize`、`offSize`（CFF）または `topDictLength`（CFF2）。
- `Cff1`：ヘッダ、Name INDEX、Top DICT INDEX、String INDEX、Global Subrs INDEXを順に読んだ値。`string(sid)` は、390以下なら標準の文字列（391個の表を `sofel-read` に置く）、391以上ならString INDEXの要素を返す。
- `Cff2`：ヘッダ、Top DICT（長さはヘッダの値）、Global Subrs INDEX（32bitの数）を読んだ値。
- DICTの字句：`DictTokens` は、DICTのバイト列から、整数（28、29、32〜254）、実数（30）、演算子（12で始まる2バイトを含む）を順に返すイテレータ。実数はニブルの列のまま返す。CFF2では `blend`（23）と `vsindex`（22）も演算子として返す。
- 数の変換：FreeTypeの `cff_parse_integer`、`cff_parse_real`（指数、桁あふれで0、`scaling` と `power_ten` の扱いを含む）、`cff_parse_num`、`cff_parse_fixed`、`cff_parse_fixed_scaled`、`cff_parse_fixed_dynamic` を、同じ結果を返す関数として移植する。
- `TopDict`、`FontDict`、`PrivateDict`：DICTを1回読み、FreeTypeの `cfftoken.h` の全フィールドを、FreeTypeの既定の値（`cff_subfont_load`）を初期値として持つ固定の大きさの構造体。同じ演算子が2回あれば、後のものが勝つ（FreeTypeと同じ）。版（CFFとCFF2）で受け付ける演算子の組を分ける。
  - Top DICT：`version`、`Notice`、`Copyright`、`FullName`、`FamilyName`、`Weight`、`PostScript`、`BaseFontName`、`FontName`（いずれもSID）、`isFixedPitch`、`ItalicAngle`、`UnderlinePosition`、`UnderlineThickness`、`PaintType`、`CharstringType`、`FontMatrix`（FreeTypeの正規化の前の値と、行列があったか）、`UniqueID`、`FontBBox`、`StrokeWidth`、`XUID`、`charset`、`Encoding`、`CharStrings`、`Private`（大きさとオフセット）、`SyntheticBase`、`ROS`（登録者と順序のSID、補遺）、`CIDFontVersion`、`CIDFontRevision`、`CIDFontType`、`CIDCount`、`UIDBase`、`FDArray`、`FDSelect`。CFF2では `FontMatrix`、`CharStrings`、`FDArray`、`FDSelect`、`vstore`、`maxstack` だけ。
  - Font DICT：`FontMatrix`、`Private`、`FontName`。
  - Private DICT：`BlueValues`、`OtherBlues`、`FamilyBlues`、`FamilyOtherBlues`、`BlueScale`、`BlueShift`、`BlueFuzz`、`StdHW`、`StdVW`、`StemSnapH`、`StemSnapV`、`ForceBold`、`ForceBoldThreshold`、`lenIV`、`LanguageGroup`、`ExpansionFactor`、`initialRandomSeed`、`Subrs`、`defaultWidthX`、`nominalWidthX`。CFF2では `vsindex` と `blend` を含む。ヒンティングの値は、この ADR では読むだけで使わない（7段階目のヒンタが使う）。
  - CFF2のPrivate DICTの `blend` は、既定のインスタンスの値（下の「アルゴリズム」）で解く。値は、その手順を呼び出し側が選べるよう、`blend` を解く関数を引数にとって求める。
  - オペランドのスタックの上限は、FreeTypeの `CFF_MAX_STACK_DEPTH`（96）、CFF2では `maxstack` の扱いに従う。
- `Charset`：予め定められた3つ（ISOAdobe、Expert、ExpertSubset）と、フォーマット0、1、2のビュー。`sid(gid)`（CIDキーではCID）を返す。フォーマット1と2の範囲は、グリフの数を満たすまで前から読み、FreeTypeと同じく `nLeft` があふれる範囲を切りつめる。予め定められたものは、グリフの数が229、166、87を超えると作れない（FreeTypeと同じ）。
- `Encoding`：予め定められた2つ（Standard、Expert）と、フォーマット0、1と補遺（上位ビット0x80）のビュー。`glyph(code)` を返す。sfntに包まれたCFFでは文字の対応に使わず、FreeTypeが読み込みで行う検査と、検証器と、8段階目の素のCFFのために読む。標準のエンコーディング（seacの番号からSIDへ）の表も置く。
- `FdSelect`：フォーマット0、3、4（CFF2）の列挙型。`fd(gid)` は、FreeTypeの `cff_fd_select_get` と同じ探し方をする。
- `ItemVariationStore`：CFF2の `vstore` の先（2バイトの長さの後に、OpenTypeの共通のItem Variation Storeの形）のビュー。領域の一覧（軸ごとの始まり、ピーク、終わり）と、各 `ItemVariationData` の領域の番号の列を読む。差分の行（`deltaSets`）はCFF2では使わない。3段階目の可変フォントのADRが、HVARなどと共有できる形で `sofel-read` の共通のモジュールに置く。
- `CharStringTokens`：charstringのバイト列から、整数（28、32〜254）、16.16の固定小数点（255）、演算子（12で始まる2バイトを含む）を返す字句の読み手。版を引数にとる。`hintmask` と `cntrmask` の後のバイトは字句から決まらないので、`skip(n)` を呼び出し側が呼ぶ。
- 検証器（重大度は [ADR 0007](0007-sfnt-read-foundation.md) の定義に従う）：
  - ヘッダ：版が1か2でない、`hdrSize` が4（CFF2は5）未満、`offSize` が1〜4でない、CFF2の `topDictLength` が表の外（Error）。
  - INDEX：`offSize` が1〜4でない、最初のオフセットが1でない、オフセットが昇順でない、データが表の外（Error）。
  - Name INDEXが2つ以上、Top DICT INDEXがName INDEXより少ない（Error）。
  - DICT：オペランドの過多、演算子に足りないオペランド、壊れた実数（Error）。未知の演算子、版で許されない演算子、同じ演算子の重複（Warning）。`CharStrings` が無い、`CharstringType` が2でない、`Private` が表の外（Error）。
  - CharStrings：数が0（Error）、`maxp` の `numGlyphs` と違う（Warning）。
  - charset：未知のフォーマット、データの不足、予め定められたものでグリフが多すぎる（Error）。String INDEXの外のSID（Warning）。
  - encoding：未知のフォーマット、データの不足（Error）。
  - FDArray：257以上（Error）。各Font DICTの `Private` が無いか表の外（Error）。CFF2でFDArrayが無い（Error）。
  - FDSelect：未知のフォーマット、最初の範囲が0でない、範囲が昇順でない、番兵がグリフの数と違う（Error）。FDArrayの数以上のFD（Error）。フォーマット4（Warning：FreeTypeが開けない）。
  - VariationStore：Item Variation Storeの検証（形式が1でない、領域の番号が範囲外、データの不足はError）。fvarの軸の数と領域の軸の数が違う（Error：FreeTypeが輪郭を描けない）。
  - ローカルのサブルーチン：INDEXが表の外（Error）。
  - charstring：全グリフを、下の解釈器を描画なしで走らせ、誤り（スタックのあふれと不足、範囲外のサブルーチン、入れ子の上限、マスクのバイトの不足、CFFで `endchar` が無い、範囲外の `vsindex`、VariationStoreの無い `blend`）をグリフIDとバイトの位置つきで報告する（Error）。未知の演算子は Warning とする。
  - `sofel-validate` の対象に `CFF ` と `CFF2` を加え、`validate::sfnt` が `maxp`、`fvar` と組み合わせて呼ぶ（[ADR 0008](0008-validate-command-and-freetype-comparison.md)）。

### `sofel-font`：CFFの問い合わせ

- `Font::is_cff()`：`CFF ` の表があり、長さが0でない。`Font::is_cff2()`：`CFF2` について同じ。HarfBuzzの `hb_face_reference_table` と同じく、ファイルの外にはみ出す表は、はみ出さない部分の長さで判断する。Golemの `golem_font_is_cff`、`golem_font_is_cff2` と同じ結果になる。
- `Cff<'a>`：呼び出し側が一度だけ作り、持ち続ける値（[ADR 0003](0003-crate-layers.md) の「キャッシュは呼び出し側の値」）。`Copy`、確保なし。CFFとCFF2の両方を表し、`version()` で区別する。
  - `Cff::new(font: &Font) -> Result<Cff, CffError>` は、FreeTypeと同じく `CFF2`、`CFF ` の順に表を選び、FreeTypeの `cff_font_load` と `cff_face_init` の検査をする。誤りは、FreeTypeがフェイスを開けない場合とちょうど一致させる（CFF2のFDSelectのフォーマット4を含む）。FontMatrixの正規化（`yy` で割る、`units_per_em` の調整、FDArrayの行列の連結）もここで済ませる。
  - あわせて、HarfBuzzのアクセラレータがCFF2とCFFのそれぞれで有効になるか（`hb_valid`）も求める。FreeTypeとHarfBuzzで受け付ける範囲が違うため、別に持つ。HarfBuzzはCFF2が無効ならCFFを見るので、`hb_valid` は2つの表の分を持つ。
  - 持つもの：表のバイト列、各INDEXの位置、Top DICTの値、正規化した行列と `units_per_em`、CharStringsの数、charset、encoding、FDSelectとVariationStoreの位置、FDArrayの数、fvarの軸の数（FreeTypeの `BV` の作り方に要る）。Font DICTとPrivate DICTは持たず、グリフごとに読む（下の「トレードオフ」）。
  - `Cff::new` は、表を受け取って作る内部の関数 `Cff::from_table(table, units_per_em, Container)` の上に置く。`Container` は、この ADR では `Sfnt` だけとし、8段階目で素のCFFを足す。
- CIDの問い合わせ（FreeTypeに合わせる）：

| 関数 | FreeType | 結果 |
|---|---|---|
| `Cff::is_cid_keyed()` | `FT_Get_CID_Is_Internally_CID_Keyed` | Top DICTにROSがある。CFF2は偽 |
| `Cff::cid(gid) -> Result<u32, CidError>` | `FT_Get_CID_From_Glyph_Index` | ROSが無い、またはグリフIDがCharStringsの数以上なら誤り。そうでなければcharsetの値 |
| `Cff::ros() -> Result<Ros, CidError>` | `FT_Get_CID_Registry_Ordering_Supplement` | ROSが無ければ誤り。登録者と順序は `Option<&[u8]>`（FreeTypeのNULLを `None`）、補遺は `i32`（実数は切り捨て） |

  - `Cff::new` が誤りのフォント、CFFの無いフォント、glyfを持つフォント（FreeTypeではTrueTypeの駆動が開く）では、`Cff` が作れないので、呼び出し側は誤りとして扱う。FreeTypeの `Invalid_Argument` と同じ結果になる。
- グリフ名：
  - `Cff::charset_name(gid) -> Option<&[u8]>`：CIDキーでないCFFで、charsetのSIDの文字列。CIDキー、CFF2、グリフIDがCharStringsの数以上なら `None`。
  - [ADR 0016](0016-font-and-vertical-metrics.md) の `glyph_name` は、HarfBuzzの順（postの次に `charset_name`。空の名前は無しとする）でこれを使う。`ft_glyph_name` は、FreeTypeの手順（CFFならcharsetだけを見て、SIDの文字列が無ければ空の名前で成功。CIDキーなら誤り。CFF2ならpostの手順）で使う。
  - 名前からグリフへ：`Cff::ft_glyph_from_name(name) -> GlyphId` は、FreeTypeと同じくグリフIDの順に線形に探し、無ければ0を返す。HarfBuzzの手順のための索引は、[ADR 0016](0016-font-and-vertical-metrics.md) の「名前から引く索引は呼び出し側が持つ値」に従い、`Cff::build_name_index(buf: &mut [u32])` で呼び出し側の配列（グリフの数の長さ）に、名前の順（同じ名前はグリフIDの小さい順）にグリフIDを並べる。`Cff::glyph_from_name(index, name)` は二分探索で引く。
- PostScriptのフォントの情報（FreeTypeに合わせる）：
  - `Cff::ps_font_info() -> Result<PsFontInfo, CffError>`：`FT_Get_PS_Font_Info` と同じ9つの値。文字列は `Option<&[u8]>`。CFF2では誤り。
  - `Cff::ps_fs_type() -> u16`：`FT_Get_PS_Font_Extra` の `fs_type`（`PostScript` の文字列の `/FSType` の手順）。
  - `Cff::top_dict()`、`Cff::font_dict(fd)`、`Cff::private_dict(fd)`：読んだ構造体をそのまま返す。FreeTypeの `FT_Get_PS_Font_Private` はCFFで使えないが、PDFの書き出し（`StemV` など）とヒンタが使えるよう、生の値を公開する。
- PostScript名は、[ADR 0016](0016-font-and-vertical-metrics.md) のnameの表の手順のままとする。CFFのName INDEXの名前には戻らない（`cff_get_ps_name`）。

### `sofel-outline`：CFFとCFF2の輪郭

- `Outlines<'a>`：フォントごとに呼び出し側が一度作る値。`Outlines::new(font: &Font) -> Outlines` は、FreeTypeの駆動の選び方で形式を決める（glyfかlocaがあればglyf、無くてsfntの版が `OTTO` なら `Cff::new` の結果）。
- `Outlines::draw(glyph, scale, scratch, pen) -> Result<OutlineInfo, OutlineError>` を足す。[ADR 0011](0011-glyf-outlines.md) の `draw(font, ...)` は残し、中で `Outlines::new` を呼ぶ形にする。glyfのフォントでは費用は変わらない。CFFのフォントで繰り返し描く呼び出し側は `Outlines` を持つ。
- 可変フォントの座標は、この ADR では受け取らない。CFF2は既定のインスタンスで描く。3段階目のADRが、座標を持つ値を `Outlines::draw` に足す。
- `OutlineError` に、`UnsupportedFormat`（輪郭の表が無いなど）、`InvalidCff`（`Cff::new` の誤り。FreeTypeではフェイスを開けない）、`InvalidCharString`（解釈の誤り）を足す。
- CFFとCFF2のグリフは、下の「アルゴリズム」の手順で点と輪郭を作り、[ADR 0011](0011-glyf-outlines.md) と同じ `FT_Outline_Decompose` の手順で `pen` に渡す。`OutlineInfo` の `overlap` は常に `false`、`cbox` は `FT_Outline_Get_CBox` と同じ箱とする。Golemのグリフの箱（`Unscaled` の `cbox`）は、FreeTypeの値と一致する。
- `Scratch` に、CFFの作業領域（型つきのオペランドのスタック513、サブルーチンの呼び出しのスタック17、ステムの数、マスク、`glyphpath` の状態、`BV`）を足す。固定の大きさの配列とし、点と輪郭の列は既存のものを使う。
- `random` の状態は、描くたびに、FreeTypeでフェイスの種を0にしたときの値（Private DICTの `initialRandomSeed`。0なら987654321、負なら符号を反転）から始める。FreeTypeの状態はグリフをまたいで進むが、sofelは呼び出しの順に結果を依存させない（決定性）。
- グリフの箱は、charstringの解釈が要るため `sofel-font` には置かず、`sofel-outline` に置く（[ADR 0003](0003-crate-layers.md)）。下の「アルゴリズム」の「HarfBuzzの箱」を参照。
- 移植の元のうち `src/psaux/ps*.c` は、Adobeの著作で、FTLに特許の許諾の条項が加わっている。[ADR 0002](0002-license-and-reuse.md) に従い、流用元とFTLに加えて、この条項も `THIRD_PARTY_NOTICES.md` に載せる。

### `sofel-raster` と `sofel-render`

- ラスタライザは変えない（[ADR 0012](0012-antialiased-rasterizer.md)、[0014](0014-raster-sweep-touched-cells.md)）。CFFのグリフは `overlap = false`、非ゼロ規則で描く。FreeTypeも、CFFでは `FT_OUTLINE_OVERLAP` を付けず、`REVERSE_FILL` と `HIGH_PRECISION` はsmoothで使われないので、同じ結果になる。
- `sofel-render` は `Outlines` を使い、CFFとCFF2のフォントも描く。

### FreeTypeとの突き合わせ

- `ft-dump` に次を足す。
  - `--cid FILE`：各フェイスについて、`FT_Get_CID_Is_Internally_CID_Keyed` の結果、`FT_Get_CID_Registry_Ordering_Supplement` の結果（誤りの番号か、登録者と順序（NULLは `-`、文字列は16進）と補遺）、全グリフと `num_glyphs` そのもの（範囲外）について `FT_Get_CID_From_Glyph_Index` の結果。
  - `--ps-info FILE`：各フェイスについて、`FT_Has_PS_Glyph_Names`、`FT_Get_PS_Font_Info`（誤りの番号か9つの値。文字列は16進、NULLは `-`）、`FT_Get_PS_Font_Extra` の `fs_type`。
  - グリフ名は、[ADR 0016](0016-font-and-vertical-metrics.md) の `ft-dump` のグリフ名の書き出し（`FT_Get_Glyph_Name` と `FT_Get_Name_Index`）を、CFFのフェイスにもそのまま使う。
- `ft-dump` は、`--outline` と `--raster` で、フェイスごとの `random-seed`（`FT_PARAM_TAG_RANDOM_SEED`）を0にして開く。アドレスに依存する種を除くためである。
- `sofel-conformance` で、[ADR 0011](0011-glyf-outlines.md) の輪郭（`unscaled`、16、48画素）と [ADR 0012](0012-antialiased-rasterizer.md) のラスタライズ（12、24、48画素）の比較の対象を、`Outlines::new` がCFFかCFF2を選ぶフェイスにも広げる。項目の名前は既存のままとする。可変フォントは既定のインスタンスだけを比べる（[ADR 0011](0011-glyf-outlines.md) と同じ）。
- `--cid` と `--ps-info` の結果を全フェイスで比べる項目 `<フォント>#<フェイス>:cid`、`<フォント>#<フェイス>:ps-info` を足す。glyfのフォントでは、FreeTypeもsofelも誤りになることを確かめる。
- 比べる対象は、[ADR 0008](0008-validate-command-and-freetype-comparison.md) のテストのフォント（`test/subset/data/fonts/` と `test/shape/data/*/fonts/`）のうち、`CFF ` を持つ286のフェイス（subset 29、aots 205、in-house 38、text-rendering-tests 14）と、`CFF2` を持つ11のフェイス。CIDキーのものは9つ（SourceHanSans-Regular_subset、FDArrayTest257、FDArrayTest65535、TestVRTR-CID-Regularなど）。数は作成時点の走査による（HarfBuzz 15.1.0）。
- 期待する失敗：[ADR 0011](0011-glyf-outlines.md) と同じ `conformance/expected-failures-freetype.txt` に、理由を添えて載せる。載せてよい理由は、`random` の状態がFreeTypeでグリフをまたいで進むこと、だけとする。それ以外の不一致は、実装を直す。理由の分からない不一致を載せない。

### HarfBuzzとの突き合わせ

- [ADR 0016](0016-font-and-vertical-metrics.md) の `hb-dump` のグリフの箱とグリフ名の比較を、CFFとCFF2のフェイスにも広げる。[ADR 0016](0016-font-and-vertical-metrics.md) が期待する失敗に載せたCFFの箱と名前の項目は、この実装のプルリクエストで外す。
- 名前からグリフへの比較で、同じ名前を2つ以上持つフォントは、HarfBuzzの結果が整列の実装によるため、返ったグリフの名前が引いた名前と等しいことだけを確かめる。

### C ABI

[ADR 0015](0015-c-abi.md) の決まりに従い、次を足す。`sofel_font_t` は、作るときに `Cff::new` の結果を中に持つ（CFFが壊れていても、フォントの作成は失敗させない）。

| 関数 | 中身 |
|---|---|
| `int32_t sofel_font_is_cff(const sofel_font_t *)`、`sofel_font_is_cff2` | `Font::is_cff`、`is_cff2`（1か0） |
| `int32_t sofel_font_is_cid_keyed(const sofel_font_t *, int32_t *out)` | `Cff::is_cid_keyed`。`Cff` が無ければ誤り |
| `int32_t sofel_font_cid(const sofel_font_t *, uint32_t gid, uint32_t *cid)` | `Cff::cid`。誤りのとき `*cid` に0を書く（FreeTypeと同じ） |
| `int32_t sofel_font_ros(const sofel_font_t *, char *registry, size_t, char *ordering, size_t, int32_t *supplement)` | `Cff::ros`。文字列はNUL終端で書き、最初のNULで切る（FreeTypeのCの文字列と同じ）。NULLの文字列は長さ0とし、別の出力の引数で区別する |
| `int32_t sofel_font_ps_font_info(const sofel_font_t *, ...)`、`sofel_font_ps_fs_type` | `Cff::ps_font_info`、`ps_fs_type`。文字列は呼び出し側のバッファに書く |

- 輪郭とラスタライズの関数（`sofel_outline_draw`、`sofel_render_glyph`）は、中で `Outlines` を使い、CFFとCFF2も描く。グリフ名の関数は [ADR 0016](0016-font-and-vertical-metrics.md) で足し、CFFの名前もそこから返る。

### 計測

[ADR 0013](0013-benchmark-harness.md) の項目に、CFFとCFF2のフォントを足す。フォントは、HarfBuzzの `perf/benchmark-font.cc` が使うCFFとCFF2のフォント（`test/subset/data/fonts/SourceSansPro-Regular.otf`、`NotoSerifMyanmar-Regular.otf`、`AdobeVFPrototype.otf` の既定のインスタンス）と、CIDキーの日本語のフォント（`SourceHanSans-Regular_subset.otf`、10452グリフ）とする。

| 項目 | sofel | FreeType | HarfBuzz |
|---|---|---|---|
| 輪郭（拡大縮小なし、48画素） | `Outlines::draw` | `FT_Load_Glyph` と `FT_Outline_Decompose` | `hb_font_draw_glyph` |
| ラスタライズ（12、24、48、96画素） | `Outlines::draw` と `render` | `FT_Load_Glyph` と `FT_Render_Glyph` | なし |
| グリフの箱 | `Outlines::glyph_extents` | なし | `hb_font_get_glyph_extents`（拡大縮小なし） |
| GIDからCID | `Cff::cid` | `FT_Get_CID_From_Glyph_Index` | なし |
| グリフ名と名前からグリフ（CIDキーでないもの） | `charset_name`、`glyph_from_name` | `FT_Get_Glyph_Name`、`FT_Get_Name_Index` | `hb_font_get_glyph_name`、`hb_font_get_glyph_from_name` |
| 開く | `Cff::new` | `FT_New_Memory_Face` | `hb_face_create` と箱の1回目の問い合わせ |

- 最初の記録を `docs/benchmarks/<YYYY-MM-DD>-cff.md` に置く（ネイティブとwasm）。

## データ構造

- 解釈器のオペランドは、FreeTypeの `CF2_StackNumber` と同じく、型（整数、16.16、2.30）と値の組とする。`callsubr` などで整数を求める箇所に16.16が来ると、FreeTypeと同じく誤りにする（`cf2_stack_popInt`）。
- `Cff` は固定の大きさで、おおむね数百バイトとする。Font DICTとPrivate DICTの読みの結果（行列、ローカルのサブルーチンの位置と偏り、`nominalWidthX`、CFF2の `vsindex`）は、グリフを描くたびにFDSelectから引いて作り、`Scratch` には残さない。
- `BV`（CFF2の各領域の係数、16.16）は、`vsindex` が決まったときに `Scratch` に作る。既定のインスタンスでは、FreeTypeの手順で座標0から求める。
- HarfBuzzの箱の解釈器は、数を `double` ではなく、16.16を `i64` に広げて持つ。オペランドは整数か16.16で、既定のインスタンスでは `blend` が乗算を生まないので、和と差は `double` と同じ値になる（`double` の仮数は53bitある）。座標を持つ `blend` は3段階目のADRで扱い、そこで数の型を決めなおす。

## アルゴリズム

### FreeTypeの輪郭（ヒンティングなし）

FreeTypeの `cf2_interpT2CharString`（`src/psaux/psintrp.c`）と `cf2_glyphpath_*`（`src/psaux/pshints.c`）を、ヒンティングと太らせを外して移植する。外しても、次の手順はそのまま残す。

1. 字句を読み、型つきのスタックに積む。スタックのあふれ（CFFは48、CFF2は `maxstack`）、不足、型の違いは誤りとする。
2. 送り幅の判定（CFFだけ）：最初のステム、移動、`hintmask`、`endchar` で、引数の数が奇数（または演算子ごとの数より多い）なら先頭を送り幅として取り除く。送り幅は、hmtxが無いときだけ使う。CFF2は送り幅を持たない。
3. ステムの演算子と `hintmask`/`cntrmask`：ステムの数を数え（96を超えれば誤り）、マスクのバイト `(数 + 7) / 8` を読み飛ばす。引数が残る `hintmask` は暗黙の `vstemhm` とする。マスクが新しいかどうか（`isNew`）は、下の長さ0の線の扱いに効くので保つ。
4. サブルーチン：偏りは、数が1240未満なら107、33900未満なら1131、それ以上は32768。範囲外は誤り。入れ子は16まで。`return` と、サブルーチンの中の `endchar` を扱う。CFF2には `return` と `endchar` が無く、charstringの終わりで戻る。
5. `endchar` の引数が4つ（送り幅を除く）なら、seacとして扱う（CFFだけ）。標準のエンコーディングの番号をSIDにし、charsetで同じ値を持つ最初のグリフを探す。アクセントを `(adx, ady)` ずらして先に、基のグリフを後に、同じ輪郭へ描く。seacの入れ子は誤り。ローカルのサブルーチンは、元のグリフのFDのものを使う（FreeTypeと同じ）。
6. flex（`flex`、`hflex`、`hflex1`、`flex1`）は2つの三次曲線として描く。`flex1` の最後の点の向きは、dxの和の絶対値とdyの和の絶対値の大小で決める（`cf2_escFLEX1`）。
7. 算術の演算子（`and`、`or`、`not`、`abs`、`add`、`sub`、`div`、`neg`、`eq`、`drop`、`put`、`get`、`ifelse`、`random`、`mul`、`sqrt`、`dup`、`exch`、`index`、`roll`）は、CFFでFreeTypeと同じ演算（`FT_DivFix`、`FT_MulFix`、`FT_SqrtFixed`）で行う。保管の領域は32。CFF2では未知の演算子とする。
8. CFF2の `vsindex` と `blend`：`vsindex` はFDのPrivate DICTの値（既定0）を置きかえ、`blend` の後なら誤り。`blend` は、スタックの上の `n × (領域の数 + 1) + 1` 個から `n` 個の値を作る。各値は `既定 + Σ FT_MulFix(BV[i], 差分[i])`（和は `ADD_INT32`）。`BV` は、fvarが無ければ `(1, 0, …, 0)`、あれば座標をすべて0として、上の「文脈」の領域の係数の規則で求める。fvarの軸の数と領域の軸の数が違う、`vsindex` が範囲外、VariationStoreが無い、はいずれも誤りとする。CFFでは `blend` と `vsindex` はスタックを捨てるだけとする。
9. 経路（`glyphpath`）：文字の空間で長さ0の線は、マスクが新しくなければ捨てる。点は `x' = FT_MulFix(0x400, x)`、`y' = FT_MulFix(y, 0x400)` として装置の空間に移し、装置の空間で長さ0になる線も捨てる。曲線は捨てない。`closepath` に相当する閉じる線は、文字の空間で始点へ引く。移動だけの輪郭は作らない。
10. 点の作成（`ps_builder_*`）：`x' >> 10`、`y' >> 10`（算術右シフト）をフォントの単位の点とする。輪郭を閉じるとき、最後の点が最初の点と同じで曲線上なら取り除く。1点だけの輪郭は取り除く。
11. 解釈の誤りは、すべて `InvalidCharString` とする（FreeTypeは `Invalid_File_Format` などを返す。番号は比べない）。
12. 仕上げ（`cff_slot_load`）：FontMatrixが単位行列でなければ `FT_Outline_Transform` と同じ変換をかけ、平行移動（整数のフォントの単位）を足す。`Scaled` なら、各点を `FT_MulFix(x, x_scale)` とする。FDArrayのあるフォントでTop DICTとFont DICTの `units_per_em` が違えば、倍率を `FT_MulDiv(scale, top_upm, sub_upm)` とし、`Unscaled` でも拡大縮小する。`unitsPerEm` が0x7FFFを超えるフォントで拡大縮小するときは、FreeTypeと同じく誤りとする（`cf2_checkTransform`）。
13. グリフIDがCharStringsの数以上なら `InvalidGlyphIndex` とする。

### HarfBuzzの箱

HarfBuzzの `cff1_cs_interpreter_t`、`cff2_cs_interpreter_t` と、それぞれの `opset_extents` を移植し、`sofel-outline` に置く。

- `Outlines::glyph_extents(glyph) -> Option<GlyphExtents>` は、HarfBuzzの `hb_ot_get_glyph_extents` の順のうち、カラーとビットマップを除いた部分（VARC、glyf、CFF2、CFF）を、拡大縮小の前の値で返す。glyfの箱は [ADR 0016](0016-font-and-vertical-metrics.md) の `sofel-font` の `glyf_extents` を呼ぶ。CFF2とCFFの箱は下の解釈器で求め、CFF2の `hb_valid` が偽ならCFFへ進む。VARCは、そのADRで足すまで `None` とする。カラーとビットマップを前に足した全体の順は、[ADR 0023](0023-color-glyphs.md) の `sofel_color::glyph_extents` が受け持つ。
- スタックは513、サブルーチンの入れ子は10、演算子の数は1グリフ200000まで（`HB_CFF_MAX_OPS`）。超えれば箱は無い。
- 移動では経路を閉じ、線と曲線の始まりで現在の点を、その後に各点（曲線は2つの制御点を含む）を箱に足す。
- seac（CFFだけ）は、基とアクセントの箱を、アクセントをずらして合わせる。どちらかが無ければ箱は無い。
- CFF2の `blend` は、既定のインスタンスでは差分を捨て、既定の値だけを残す（HarfBuzzの `do_blend` が偽の場合）。
- 丸め：各端の値（16.16）を、まず `float` への変換（最近接、偶数への丸め、24bitの仮数）と同じ規則で丸め、次に半分を0から遠い側へ丸める（`roundf`）。`x_bearing` は丸めた最小のx、幅は丸めた最大のx との差。`y_bearing` は丸めた最大のy、高さは丸めた最小のyとの差（負）。最小が最大以上なら、その軸は0とする。
- 拡大縮小（`scale_glyph_extents`）は [ADR 0016](0016-font-and-vertical-metrics.md) の手順による。

### CFFとCFF2の共有

- `Index` は数の幅を、`DictTokens` と `CharStringTokens` は版を引数にとる。解釈器は1つの関数で、版の違い（送り幅、`endchar`、`return`、seac、算術の演算子、`blend`、`vsindex`、スタックの上限）を版の分岐で扱う。FreeTypeの解釈器も、同じ関数の中で `isCFF2` で分けている。
- `BV` を求める関数は、座標の列を引数にとる。この ADR では、呼び出し側は空の列（fvarが無い）か、軸の数だけの0の列（fvarがある）だけを渡す。3段階目のADRが、正規化した座標を渡す。

## 実装しないこと

- `FT_Get_PS_Font_Private` と `FT_Get_PS_Font_Value` に相当する問い合わせ。FreeTypeもCFFでは扱わない。生の値は `Cff::private_dict` で読める。
- Multiple MasterのCFF（版1の `blend` の演算子、`MultipleMaster` のTop DICTの演算子）。FreeTypeもsfntの中では扱わず、Adobeも廃止している。CFFの `blend` はスタックを捨てる。
- FreeTypeの古い解釈器（`CFF_CONFIG_OPTION_OLD_ENGINE`、`FT_HINTING_FREETYPE`）。既定で組み込まれず、出力も違う。
- FreeTypeの `Glyph_Too_Big` の再試行（2000画素を超える大きさ）。ヒンティングをしないので、拡大縮小の倍率がこの検査にかからない。ヒンティングの段階で、改めて扱いを決める。
- CFFの書き出しとサブセット化（[ADR 0022](0022-subsetting.md)）。

## 後回しにしたもの

この ADR の実装の後に、それぞれのADRを書く。各ADRに含める内容は次のとおりとする。

- **可変のCFF2（[ADR 0001](0001-project-goal-and-scope.md) の3段階目の可変フォントのADR）**：fvarとavarから作った正規化した座標を `Outlines::draw` と `glyph_extents` に渡す。FreeTypeの `cff_blend_build_vector` の領域の係数（`FT_MulDiv` による比例）と、HarfBuzzの `VarRegionList::evaluate`（`float` の係数）を、それぞれ輪郭と箱に移植し、箱の数の型（`i64` の16.16で足りるか）を決めなおす。座標ごとのPrivate DICTの読みなおし（`cff_blend_check_vector` と `load_private_dict`）、HVARとVVARによる送り幅、名前つきインスタンス（`FT_Set_Named_Instance`）、`ft-dump` の座標の指定と、名前つきインスタンスごとの突き合わせを含める。`ItemVariationStore` のビューは、この ADR で作ったものを使う。
- **CFFのヒンティング（7段階目のヒンティングのADR）**：Adobe engineのヒンティング（`src/psaux/pshints.c` の `cf2_hintmap_*`、`psblues.c` のblue zone、`cntrmask` の扱い、`initialHintMap` と `firstHintMap` による点の写像）と、ステムの太らせ（`cf2_computeDarkening` と `darkening-parameters`、`no-stem-darkening` の属性、`FT_LOAD_TARGET_LIGHT` など）。`FT_LOAD_NO_HINTING` を外した `ft-dump` の突き合わせと、`Glyph_Too_Big` の再試行を含める。Private DICTのヒンティングの値は、この ADR で読む構造体を使う。
- **素のCFFとCEF（8段階目のsfnt以外の形式のADR）**：`sofel-font` の `FontFile` の判別に、sfntに包まれていないCFF（Name INDEXの各要素をフェイスとし、Top DICTの数が足りないもの、ROSのSIDの扱い）を足す。`Cff::from_table` の `Container` に素のCFFを足し、`units_per_em` を1000とし、FreeTypeの `cff_charset_compute_cids` によるCIDからグリフIDへの付けかえ（`FT_Load_Glyph` と `FT_FACE_FLAG_CID_KEYED`）、Name INDEXのPostScript名、Top DICTからのファミリー名とスタイルの印（`cff_face_init`）、encodingとAdobe Glyph Listによる文字の対応の合成（`src/cff/cffcmap.c`、`src/psnames/`）を含める。Adobe Glyph Listは、Type1と共有する。
- **VARC（13段階目）**：VARCの合成グリフと、FreeTypeでCFF2のCharStringsの数を超えるグリフ。`Outlines::glyph_extents` のVARCの部分を含める。

## テスト設計

実装のまえに、次のテストを書いて失敗させる。フォントは、[ADR 0007](0007-sfnt-read-foundation.md) のテストの組み立て方に、CFFとCFF2の組み立て（ヘッダ、INDEX、DICTの符号化、charset、FDSelect、VariationStore、charstringの符号化）を足して作る。組み立ての道具は `sofel-read` のテストの補助に置く。

- `Index`：数0、`offSize` 1〜4、16bitと32bitの数、昇順でないオフセット、オフセット0、データの外の要素で、`get` の結果を確かめる。
- DICTの字句と数の変換：28、29、32〜254の各範囲の境界、実数のすべてのニブル（`E`、`E-`、`-`、終わり）、指数の大きい実数（桁あふれで0）、`FontMatrix` の値（`cff_parse_fixed_dynamic` の倍率の決め方）を、FreeTypeの式を直接書いた参照の値と比べる。
- `TopDict` など：全フィールドの既定の値、重複（後が勝つ）、オペランドのあふれ、足りないオペランド、版で許されない演算子、CFF2の `maxstack`（513未満は513、513を超えれば513）、Private DICTの `blend`（`BV` が `(1, 0)` と、軸のピークがすべて0の領域）を確かめる。
- `Charset`：予め定められた3つとグリフの数の上限、フォーマット0、1、2、`nLeft` の切りつめ、範囲がグリフの数を超える場合を確かめる。
- `Encoding`：予め定められた2つ、フォーマット0と1、補遺を確かめる。
- `FdSelect`：フォーマット0、3、4、範囲の外のグリフ、FDの数以上の値（FreeTypeは最後のFDに丸める）を確かめる。
- `ItemVariationStore`：領域の一覧、`ItemVariationData` の領域の番号、範囲外の番号を確かめる。
- `Cff::new`：FreeTypeがフェイスを開けない各条件（版、`hdrSize`、`offSize`、Name INDEXの数、Top DICTの不足、CharStringsの無いこと、FDArrayの数、charsetとencodingの不足、CFF2のFDSelectのフォーマット4）で誤りになり、条件を1つ外すと成功することを確かめる。`CFF2` と `CFF ` の両方を持つフォントで `CFF2` が選ばれることを確かめる。`hb_valid` も、HarfBuzzのアクセラレータの各条件で、CFFとCFF2のそれぞれについて確かめる。
- CIDの問い合わせ：CIDキーのフォントで `cid` がcharsetの値を返すこと、グリフIDがCharStringsの数のとき誤りになること、CIDキーでないフォント、CFF2、glyfのフォントで誤りになることを確かめる。`ros`：SIDが390以下（標準の文字列）、391以上、String INDEXの外（`None`）、実数の補遺（切り捨て）、負の補遺を確かめる。
- グリフ名：`charset_name` の標準の文字列とString INDEXの文字列、範囲外のSID（`None`）、CIDキーとCFF2（`None`）。`ft_glyph_from_name` の最初の一致と、無いときの0。`build_name_index` と `glyph_from_name` の、同じ名前を2つ持つフォント（小さいグリフIDが先）、空の名前、配列の長さが足りない場合を確かめる。
- フォントの情報：`ps_font_info` の各値と、CFF2の誤り。`ps_fs_type` は `/FSType 8 def`、空白と改行、数字以外が混じる、`def` が無い、あふれる値で確かめる。
- 解釈器：各演算子について、手で組んだcharstringと、FreeTypeの式で手で計算した期待の命令の列を比べる。
  - `rmoveto`、`hmoveto`、`vmoveto`、`rlineto`、`hlineto`、`vlineto`（向きの交互）、`rrcurveto`、`hhcurveto`、`vvcurveto`（奇数の引数）、`hvcurveto`、`vhcurveto`（最後の `df`）、`rcurveline`、`rlinecurve`、4つのflex。
  - 送り幅の判定（各演算子の引数の数）、ステムの数とマスクのバイト数（暗黙の `vstemhm` を含む）、ステム97個の誤り。
  - サブルーチンの偏りの境界（1239と1240、33899と33900のサブルーチンの数）、範囲外、入れ子の16と17、サブルーチンの中の `endchar`。
  - スタックの48と49（CFF）、`maxstack` とその1つ上（CFF2）、不足、16.16を整数として使う型の誤り。
  - seac：通常、アクセントのずらし、標準のエンコーディングに無い番号、入れ子。
  - 算術の演算子それぞれと、`random` が描くたびに同じ値から始まること。CFF2では未知の演算子になること。
  - CFF2：送り幅を取らないこと、`return` と `endchar` の無いこと、`vsindex`（範囲外、`blend` の後）、`blend` の数（`n × (領域の数 + 1) + 1`）と不足、fvarの無いフォントの `BV`、fvarのあるフォントの座標0での `BV`（ピークが正の領域で0、すべての軸のピークが0の領域で1、始まりが負でピークが正の領域で比例の値）、fvarの軸の数と領域の軸の数が違う誤り、VariationStoreの無い `blend` の誤り。
  - 座標：小数の座標の切り捨て（正と負、`FT_MulFix` の丸めの境界の値）、文字の空間と装置の空間で長さ0の線、移動だけの輪郭、始点に戻る最後の点、1点だけの輪郭。
  - FontMatrixの正規化と変換、FDArrayの `units_per_em` の違い（`Unscaled` でも拡大縮小されること）、`Scaled` の倍率。
- HarfBuzzの箱：制御点を含むこと、移動だけの経路が箱に入らないこと、seac、空の軸、`float` への丸めで結果が変わる値（例えば256より大きく、16.16の端数が0.5の近くの値）、入れ子の10と11、演算子の数の上限、CFF2の `blend` が既定の値だけを残すこと、CFF2が無効なときにCFFへ進むことを確かめる。
- 検証器：各誤りについて、期待する種類、重大度、オフセットを確かめる。[ADR 0007](0007-sfnt-read-foundation.md) の不変条件（Errorが無ければ `Cff::new` が成功する）を、変異させた入力で確かめる。
- `ft-dump --cid`、`--ps-info` と、`random-seed` を0にした `--outline` の正しさは、手で組んだ小さなCFFとCFF2のフォント（CIDキーとそうでないもの）で、出力が手で書いた期待の出力と一致することを確かめる（[ADR 0008](0008-validate-command-and-freetype-comparison.md) と同じ方式）。
- 実際のフォントでの完全一致は、上の `sofel-conformance` の輪郭、ラスタライズ、CID、フォントの情報の項目と、[ADR 0016](0016-font-and-vertical-metrics.md) の箱とグリフ名の項目で確かめる。
- ファジング：`fuzz/` に `cff` の対象を足す。任意のバイト列を `CFF ` または `CFF2` の表とするsfntを組み（fvarの有無も入力で選ぶ）、`Cff::new`、全グリフの `draw`、`Outlines::glyph_extents`、`cid`、`ros`、`charset_name`、`build_name_index`、`ps_font_info`、検証器を呼び、panicしないこと、検証器がErrorを報告しなければ `Cff::new` が成功することを確かめる。既存の `outline` の対象も `Outlines` を通して、CFFのフォントを描くようにする。
- C ABI：[ADR 0015](0015-c-abi.md) のCのテストに、`sofel_font_is_cff`、`sofel_font_cid`、`sofel_font_ros`、`sofel_font_ps_font_info`（バッファが足りない場合を含む）を足す。
- wasm：`sofel-read`、`sofel-font`、`sofel-outline` の単体テストを `wasm32-wasip1` で通す。浮動小数点を使わないので、x86-64、aarch64、wasm32で結果が同じになる。CIでは、`sofel-conformance` の一部（CIDキーのフォント1つとCFF2のフォント1つ）の輪郭の出力のハッシュを、ネイティブとwasmで比べる。

## トレードオフ

- 輪郭はFreeTypeに、箱はHarfBuzzに合わせるため、解釈器を2つ持つ。2つは、スタックの上限、入れ子の上限、算術の演算子、数の型（16.16と `double`）、既定のインスタンスでの `blend`、壊れたフォントの受け付け方が違い、1つの解釈器に設定を足して両方を出すと、分岐が絡み合う。字句の読み手（`CharStringTokens`）とDICT、charset、FDSelect、VariationStoreのビューは共有する。
- FreeTypeの解釈器は小数の座標をフォントの単位の整数に切り捨てるため、HarfBuzzの描画（`hb_font_draw_glyph`）と座標が違うことがある。輪郭の基準はFreeTypeとする（[ADR 0005](0005-conformance-tests.md)）。
- 既定のインスタンスで、FreeTypeは領域の係数を求め、HarfBuzzは差分を捨てる。ピークがすべて0の領域などを持つCFF2のフォントでは、輪郭と箱が食い違う。それぞれの基準に合わせるため、sofelも同じく食い違う。
- FreeTypeはCFF2のFDSelectのフォーマット4を読めないので、そのフォントは輪郭が誤り、箱は得られる。FreeTypeに合わせるため、sofelも輪郭を誤りとする。
- `Cff` と `Outlines` を呼び出し側が持つため、APIの使い方が1段増える。`Font::new` の費用を保ち、FreeTypeの検査（charsetの範囲の数に比例する）をグリフごとに繰り返さないために選ぶ。
- Font DICTとPrivate DICTをグリフごとに読むため、FDArrayのあるフォントでは描くたびに数十バイトのDICTを読む。グリフの解釈に比べて小さいと見込む。計測で効くと示せたら、FDごとの読みの結果を呼び出し側の値として持つADRを作成する。
- 名前からグリフへの索引で、同じ名前のグリフはグリフIDの小さい順に並べる。HarfBuzzの `qsort` の結果とは一致しないことがあるが、決定性を優先する。
- `random` の状態を描くたびに戻すため、FreeTypeで `random` を2回以上の読み込みで使うフォントは結果が違う。テストのフォントで見つかれば、期待する失敗に載せる。
- FreeTypeが開けないCFFのフォントでも、sofelは `Font` を作り、シェーピング（HarfBuzzに合わせる）に使える。輪郭、CID、フォントの情報の問い合わせだけが誤りになる。
- 素のCFFを8段階目に回すため、それまでは `.cff` のファイルを開けない。素のCFFは、Adobe Glyph Listによる文字の対応の合成を要し、Type1と同じ部品を使う。OpenTypeのフォントの利用者には影響しない。
- CIDキーのフォントの比較には、65535グリフのフォント（FDArrayTest65535）が含まれ、輪郭とラスタライズの6つの大きさで約40万の項目になる。突き合わせの時間は延びるが、標本を間引くと完全一致の確かめにならないため、すべてを比べる。
- Adobeの解釈器の移植は、FTLに加えて特許の許諾の条項を表示する。条項は利用者に義務を課さないが、`THIRD_PARTY_NOTICES.md` が長くなる。

## 参考文献

- FreeType `src/cff/cffload.c`（`cff_font_load`、`cff_blend_build_vector`、`CFF_Load_FD_Select`、`cff_vstore_load`、`cff_charset_compute_cids`）、`src/cff/cffparse.c`、`src/cff/cffparse.h`、`src/cff/cfftoken.h`、`src/cff/cffobjs.c`、`src/cff/cffdrivr.c`、`src/cff/cffgload.c`、`src/cff/cffcmap.c`、`src/psaux/psintrp.c`、`src/psaux/pshints.c`、`src/psaux/psft.c`、`src/psaux/psfont.c`、`src/psaux/psstack.c`、`src/psaux/psfixed.h`、`src/psaux/psobjs.c`、`src/psaux/cffdecode.c`（`cff_lookup_glyph_by_stdcharcode`）、`src/base/ftcid.c`、`src/base/ftobjs.c`（`FT_Get_Glyph_Name`、`FT_Get_Name_Index`）、`src/truetype/ttgxvar.c`（`tt_get_var_blend`）、`src/sfnt/sfobjs.c`、`src/truetype/ttobjs.c`、`src/smooth/ftsmooth.c`、`include/freetype/config/ftoption.h`、`include/freetype/internal/cfftypes.h`（2.15.0）
- HarfBuzz `src/hb-ot-cff-common.hh`、`src/hb-ot-cff1-table.hh`、`src/hb-ot-cff1-table.cc`、`src/hb-ot-cff2-table.hh`、`src/hb-ot-cff2-table.cc`、`src/hb-cff-interp-common.hh`、`src/hb-cff-interp-cs-common.hh`、`src/hb-cff-interp-dict-common.hh`、`src/hb-cff1-interp-cs.hh`、`src/hb-cff2-interp-cs.hh`、`src/hb-ot-cff1-std-str.hh`、`src/hb-ot-font.cc`（`hb_ot_get_glyph_name`、`hb_ot_get_glyph_from_name`）、`src/hb-limits.hh`、`perf/benchmark-font.cc`（15.1.0）
- Adobe Technical Note #5176（The Compact Font Format Specification）
- Adobe Technical Note #5177（The Type 2 Charstring Format）
- OpenType Specification：CFF、CFF2、Item Variation Store https://learn.microsoft.com/en-us/typography/opentype/spec/
- Golem `modules/golem-font/cbits/golem/golem_font.{h,c}`（CFFとCIDの問い合わせ、グリフの箱）
