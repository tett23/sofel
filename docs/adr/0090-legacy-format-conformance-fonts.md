# ADR 0090: 旧形式の実フォントを取得して FreeType と照合する

Status: accepted-pending

## 文脈

[ADR 0065](0065-legacy-faces-and-webfonts.md) は、素の CFF、Type1、CID、Type42、PFR、BDF、PCF、FNT、FON、WOFF、WOFF2 を開き、文字対応、送り、カーニング、輪郭またはビットマップが FreeType と一致することを求める（[ADR 0065](0065-legacy-faces-and-webfonts.md) の不変条件 I1）。
同じ ADR のテスト設計は、「旧形式の実フォントは [ADR 0005](0005-conformance-tests.md) に従ってライセンスと取得元を決める」と書く。
[ADR 0005](0005-conformance-tests.md) は、旧形式のフォントを、ライセンスを確かめたうえで固定の一覧に加える ADR を、その形式を実装する前に作るとしている。

いまの旧形式のテストは、合成した小さな入力を使う。
FreeType をテストの実行時に動かして比べるのは、合成した Type1 と CID の輪郭（`crates/sofel-conformance/tests/ps_native.rs`、`ps_stems.rs`）だけである。
この ADR は、照合に使う実フォントとその取得元を決め、FreeType と比べる項目を決める。

## 調査した事項

### 候補のフォント

次の二つは、取得してライセンスの本文を読み、SHA-256 を計算した。

| 候補 | 形式 | ライセンス | 取得元、バイト数、SHA-256 |
| --- | --- | --- | --- |
| Spleen 2.3.0 | BDF、PCF、FON、WOFF、WOFF2、dfont ほか | 2条項 BSD。アーカイブの `LICENSE` は「Redistribution and use in source and binary forms, with or without modification, are permitted」で始まり、`Neither` の条項は無い（`grep -c Neither` は 0） | `https://github.com/fcambus/spleen/releases/download/2.3.0/spleen-2.3.0.tar.gz`、693236 バイト、`7b5729aaa8378e9987eccdb98c644180a875dcc22ee7258d8be3791238b87a02` |
| AMS の Computer Modern の Type1（`cmr10`） | PFB、PFM、AFM | SIL OFL 1.1。PFB の中に「This Font Software is licensed under the SIL Open Font License, Version 1.1.」とある | `https://mirrors.ctan.org/fonts/amsfonts/pfb/cmr10.pfb`（35752 バイト、`fdcede8794018df5f2b58f0905fb20a2b418ed8f67b73ee12445855dfbe5b1be`）、`https://mirrors.ctan.org/fonts/amsfonts/pfb/cmr10.pfm`（1304 バイト、`f953f9b5faebd47ca1422fcab7013f1c3f82e7c472381902befa2a6c98d6c2fe`）、`https://mirrors.ctan.org/fonts/amsfonts/afm/cmr10.afm`（10162 バイト、`8df68c822c3217c67dee325d5f7d80c6b9e492293bca366db679a5b0b5e1825b`）。OFL の本文は `https://mirrors.ctan.org/fonts/amsfonts/doc/OFL.txt`（12520 バイト、`aa783fdd47e92743bafe79277ac4067d20339344dc5b64a09c2d0f869b2999dc`） |

- Spleen のアーカイブには、`spleen-8x16.bdf`、`.pcf`、`.fon`、`.woff`、`.woff2`、`.dfont` などがある（`ls`）。
- `cmr10.afm` には `KPX` の行が 181 ある（`grep -c '^KPX'`）。
- `mirrors.ctan.org` は、近くのミラーに転送する（`curl -I` の `Location` は `mirror.aria-on-the-planet.es`）。どのミラーから取っても、上の SHA-256 で確かめる。
- `OFL.txt` の 148 行目は「This Font Software is licensed under the SIL Open Font License, Version 1.1.」である。

### 採らなかった候補

- URW base35: `https://raw.githubusercontent.com/ArtifexSoftware/urw-base35-fonts/master/LICENSE` は、GNU AGPL 3 で配布し、PostScript と PDF に埋め込む例外を足すと書く。[ADR 0002](0002-license-and-reuse.md) は GPL の流用元を使わないと決めており、テストのデータとしての扱いを決めずに入れない。
- PFR と CID キーの Type1: ライセンスの明らかな実フォントを見つけられなかった。合成の入力を続ける。
- Terminus、X.Org の bitstream-type1 と misc-misc、Latin Modern、TeX Gyre: 調べで候補に挙がったが、この ADR では取得して確かめていない（未確認）。必要になったら、この ADR と同じ形で足す。

### いまの照合の道具

- [tools/ft-dump/ftmodule.h](../../tools/ft-dump/ftmodule.h) は、Type1、CFF、CID、PFR、Type42、WinFNT、PCF、BDF のドライバを持つ。
- 既定のビルドの ft-dump は、Brotli を持たないので WOFF2 を開けない。`target/ft-dump/ft-dump spleen-8x16.woff2` は `FreeType error 1` で終わった。
- [tools/ft-dump/build.sh](../../tools/ft-dump/build.sh) の `--webfont-reference` は、Brotli を有効にするが、ドライバを `tools/webfont-reference/compare.c` に差し替える（`build.sh:21-22`）。ft-dump の公開操作の出力は、このビルドでは得られない。
- [tools/ft-dump/ft-dump.c](../../tools/ft-dump/ft-dump.c) は `FT_Get_Kerning` も `FT_Attach_*` も呼ばない（`grep -c -E 'FT_Get_Kerning|FT_Attach'` は 0）。
- [ADR 0005](0005-conformance-tests.md) は、sfnt 以外の形式のフォントを「ライセンスを確かめたうえで `testdata.lock` に加えるADR」を作るとする（`0005:58`）。その後、[ADR 0031](0031-diverse-font-corpus.md) が、ファイルごとに固定する [fonts.lock](../../fonts.lock) と、フォントごとの記録の [conformance/fonts.tsv](../../conformance/fonts.tsv) を足した。
- [scripts/fetch-fonts.sh](../../scripts/fetch-fonts.sh) は、`conformance/fonts.tsv` の「archive member」の列で、zip のアーカイブからメンバーを取り出せる（`fetch-fonts.sh:106-112` の `unzip`）。tar.gz は扱わない。
- `conformance/fonts.tsv` の features の列は、決まった名前だけを受ける（`fetch-fonts.sh:95`）。旧形式を表す名前は無い。

### 照合の前に要る実装

`crates/sofel-outline/src/type1.rs` の `callothersubr` は、OtherSubr 3 と 14〜18 だけを扱い、ほかの番号を `InvalidCharString` にする（`type1.rs:225-240`）。
flex は OtherSubr 0、1、2 なので、flex を使うグリフは描けない。
実在の Type1 は flex を使うことが多いと考えられるので（推測）、`cmr10` の照合では、これが FreeType との差として現れる見込みである。

## 実装すること

### 取得

- 取得は、[ADR 0031](0031-diverse-font-corpus.md) の [fonts.lock](../../fonts.lock) と [conformance/fonts.tsv](../../conformance/fonts.tsv) で行う。ファイルごとに SHA-256 を固定できるためである。[ADR 0005](0005-conformance-tests.md) の `testdata.lock` はリポジトリをコミットで固定する一覧で、単独のファイルの取得に向かない。
- `scripts/fetch-fonts.sh` の「archive member」に、tar.gz のアーカイブを足す。zip と同じく、固定した SHA-256 を確かめてから、書いたメンバーだけを取り出す。
- Spleen 2.3.0 の 8x16 の BDF、PCF、FON、WOFF、WOFF2、dfont と、`cmr10` の PFB、PFM、AFM を加える。
- ライセンスの記録は、[fonts.lock](../../fonts.lock) のいまの形（`*-license` の行）に合わせて、Spleen の `LICENSE` を置く。`cmr10` は、上の表の `OFL.txt` を置く。
- 照合の結果、[docs/specifications.md](../specifications.md) の旧形式の節に、照合したフォントと期待する失敗を書く。
- features の列に、旧形式を表す名前（`legacy`）を足す。

### FreeType と比べる項目

- ft-dump に、グリフ対のカーニング（`FT_Get_Kerning`）と、AFM か PFM を付けてから開く指定（`FT_Attach_File` に当たる `FT_Attach_Stream`）を足す。
- [tools/ft-dump/build.sh](../../tools/ft-dump/build.sh) の三つめの引数に `--brotli` を足し、ft-dump.c のまま `FT_CONFIG_OPTION_USE_BROTLI` を定義したビルドを作る。WOFF2 の公開操作は、このビルドと比べる。
- 適合テスト `legacy_faces` を足し、各フォントで次を比べる。
  - フェイスの数、グリフの数、文字対応（すべての cmap の項目）
  - 送りと、ストライクの有無と寸法
  - 輪郭（Type1）とビットマップ（BDF、PCF、FON）。サイズは [ADR 0065](0065-legacy-faces-and-webfonts.md) の境界の表に合わせて選ぶ
  - `cmr10` に AFM と PFM をそれぞれ付けたときと、付けないときのカーニング。AFM の `KPX` のすべての対と、登録されていない対を比べる
  - WOFF と WOFF2 は、展開した表の内容と、上の公開操作の結果
- 差が出た項目は、[ADR 0005](0005-conformance-tests.md) の期待する失敗の一覧に理由を書いて置く。

## 実装しないこと

- フォントのファイルのリポジトリへの同梱（[ADR 0002](0002-license-and-reuse.md)）。
- PFR と CID キーの Type1 の実フォント。合成の入力を続ける。
- AGPL のフォントの取得。

## 不変条件

- I1: 取得したファイルの SHA-256 とバイト数は、一覧に書いた値と一致する。一致しなければ、取得は失敗し、ファイルを置かない。
- I2: アーカイブからは、一覧に書いたメンバーだけを置く。
- I3: 一覧の各フォントについて、比べる項目のうち期待する失敗に書いていないものは、FreeType と一致する。

| 入力 | 期待 | 不変条件 |
| --- | --- | --- |
| SHA-256 が一致するアーカイブ | 書いたメンバーだけを置く | I1, I2 |
| 1 バイト違うアーカイブ | 失敗し、何も置かない | I1 |
| 一覧に書いたメンバーがアーカイブに無い | 失敗する | I2 |
| メンバーのパスが `../` を含む | 失敗する | I2 |
| AFM を付けない `cmr10` の対 | カーニングが FreeType と同じ 0 | I3 |
| AFM の `KPX` に登録された対 | カーニングが FreeType と一致する | I3 |
| WOFF2 の Spleen | 展開した表と公開操作が FreeType と一致する | I3 |

## 受けいれ条件

- A1: CI で `legacy_faces` が動き、通る。
- A2: 期待する失敗の各行に、理由と、直す ADR か作業がある。

## テスト設計

- `ADR 0090 I1: ...`、`ADR 0090 I2: ...`: `scripts/tests/` のシェルのテストで、一時ディレクトリに作ったアーカイブと一覧で、境界の表の取得の行を確かめる。
- `adr0090_i3_...`: `crates/sofel-conformance/tests/legacy_faces.rs` で、ft-dump の出力と sofel の結果を項目ごとに比べる。WOFF2 は `--brotli` のビルドの ft-dump を使う。

## トレードオフ

- CTAN のファイルは版の固定された URL を持たないので、更新されると SHA-256 が合わなくなり、取得が失敗する。そのときは、新しい値を確かめて一覧を直す。版を固定した取得元に移れるなら移る。
- 実フォントを足すと、合成の入力では出なかった差（flex など）が期待する失敗として並ぶ。一方、実際に使われる入力で、どこが FreeType と違うかが一覧で分かる。

## 参考文献

- [ADR 0065: 旧形式と Web フォントを既存のフェイスとして開く](0065-legacy-faces-and-webfonts.md)
- [ADR 0005: HarfBuzzとFreeTypeの出力を正しさの基準にする](0005-conformance-tests.md)
- [ADR 0031: ライセンスを確認した多機能フォントを検証に追加する](0031-diverse-font-corpus.md)
- [ADR 0002: ライセンスと、既存の実装とテストの流用](0002-license-and-reuse.md)
- Spleen: https://github.com/fcambus/spleen
- AMS fonts: https://ctan.org/pkg/amsfonts
