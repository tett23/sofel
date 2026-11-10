# Third-party notices

sofel is licensed under the MIT License (see [LICENSE](LICENSE)).

This file lists the third-party code that sofel copies, ports or adapts, with the copyright notices and license texts that their licenses require ([ADR 0002](docs/adr/0002-license-and-reuse.md)).
Each file that contains such code also names its source (repository, version or commit, path) and license in a comment at the top.

## FreeType

Portions of this software are copyright © 2026 The FreeType Project (https://freetype.org). All rights reserved.

sofel ports algorithms and arithmetic from FreeType 2.15.0 (commit `37515cf56ba3349656049edbed89ffc6f6466f2d`), choosing the FreeType License (FTL) of FreeType's dual license. The crates that contain such code declare `MIT AND FTL`.

| Crate | Ported from |
|---|---|
| `sofel-geom` | `src/base/ftcalc.c`, `include/freetype/internal/ftcalc.h`, `src/base/fttrigon.c`, `src/base/ftoutln.c` |
| `sofel-read` | `src/cff/cffparse.c`, `src/cff/cfftoken.h`, `src/cff/cffload.c` (DICT parsing, INDEX access, charset, encoding, FDSelect, the blend vector and the predefined charsets and encodings), `src/base/ftcalc.c` (64-bit `FT_MulFix`, `FT_DivFix`, `FT_MulDiv`, `FT_RoundFix`, `FT_SqrtFixed`, `FT_Matrix_Check`), and the Adobe engine `src/psaux/psintrp.c`, `src/psaux/pshints.c`, `src/psaux/psstack.c`, `src/psaux/psfont.c`, `src/psaux/psft.c`, `src/psaux/psobjs.c` (`cff_random`), `src/psaux/cffdecode.c` (`cff_lookup_glyph_by_stdcharcode`) ([ADR 0021](docs/adr/0021-cff-outlines.md)); the Adobe files carry the patent license below |
| `sofel-font` | `src/sfnt/ttmtx.c`, `src/sfnt/sfobjs.c` (the left side bearing for phantom point 1; the `FT_Face` values), `src/sfnt/ttload.c` (`tt_face_load_name`, `tt_face_load_post`), `src/sfnt/sfdriver.c` (the PostScript name, `sfnt_get_name_index`), `src/sfnt/ttpost.c`, `src/truetype/ttgload.c` (`TT_Get_VMetrics`) |
| `sofel-outline` | `src/truetype/ttgload.c`, `src/truetype/ttpload.c`, `src/base/ftoutln.c` |
| `sofel-raster` | `src/smooth/ftgrays.c`, `src/smooth/ftsmooth.c`, `src/base/ftobjs.c` |

The FreeType License follows.

```
                    The FreeType Project LICENSE
                    ----------------------------

                            2006-Jan-27

                    Copyright 1996-2002, 2006 by
          David Turner, Robert Wilhelm, and Werner Lemberg



Introduction
============

  The FreeType  Project is distributed in  several archive packages;
  some of them may contain, in addition to the FreeType font engine,
  various tools and  contributions which rely on, or  relate to, the
  FreeType Project.

  This  license applies  to all  files found  in such  packages, and
  which do not  fall under their own explicit  license.  The license
  affects  thus  the  FreeType   font  engine,  the  test  programs,
  documentation and makefiles, at the very least.

  This  license   was  inspired  by  the  BSD,   Artistic,  and  IJG
  (Independent JPEG  Group) licenses, which  all encourage inclusion
  and  use of  free  software in  commercial  and freeware  products
  alike.  As a consequence, its main points are that:

    o We don't promise that this software works. However, we will be
      interested in any kind of bug reports. (`as is' distribution)

    o You can  use this software for whatever you  want, in parts or
      full form, without having to pay us. (`royalty-free' usage)

    o You may not pretend that  you wrote this software.  If you use
      it, or  only parts of it,  in a program,  you must acknowledge
      somewhere  in  your  documentation  that  you  have  used  the
      FreeType code. (`credits')

  We  specifically  permit  and  encourage  the  inclusion  of  this
  software, with  or without modifications,  in commercial products.
  We  disclaim  all warranties  covering  The  FreeType Project  and
  assume no liability related to The FreeType Project.


  Finally,  many  people  asked  us  for  a  preferred  form  for  a
  credit/disclaimer to use in compliance with this license.  We thus
  encourage you to use the following text:

   """
    Portions of this software are copyright © <year> The FreeType
    Project (https://freetype.org).  All rights reserved.
   """

  Please replace <year> with the value from the FreeType version you
  actually use.


Legal Terms
===========

0. Definitions
--------------

  Throughout this license,  the terms `package', `FreeType Project',
  and  `FreeType  archive' refer  to  the  set  of files  originally
  distributed  by the  authors  (David Turner,  Robert Wilhelm,  and
  Werner Lemberg) as the `FreeType Project', be they named as alpha,
  beta or final release.

  `You' refers to  the licensee, or person using  the project, where
  `using' is a generic term including compiling the project's source
  code as  well as linking it  to form a  `program' or `executable'.
  This  program is  referred to  as  `a program  using the  FreeType
  engine'.

  This  license applies  to all  files distributed  in  the original
  FreeType  Project,   including  all  source   code,  binaries  and
  documentation,  unless  otherwise  stated   in  the  file  in  its
  original, unmodified form as  distributed in the original archive.
  If you are  unsure whether or not a particular  file is covered by
  this license, you must contact us to verify this.

  The FreeType  Project is copyright (C) 1996-2000  by David Turner,
  Robert Wilhelm, and Werner Lemberg.  All rights reserved except as
  specified below.

1. No Warranty
--------------

  THE FREETYPE PROJECT  IS PROVIDED `AS IS' WITHOUT  WARRANTY OF ANY
  KIND, EITHER  EXPRESS OR IMPLIED,  INCLUDING, BUT NOT  LIMITED TO,
  WARRANTIES  OF  MERCHANTABILITY   AND  FITNESS  FOR  A  PARTICULAR
  PURPOSE.  IN NO EVENT WILL ANY OF THE AUTHORS OR COPYRIGHT HOLDERS
  BE LIABLE  FOR ANY DAMAGES CAUSED  BY THE USE OR  THE INABILITY TO
  USE, OF THE FREETYPE PROJECT.

2. Redistribution
-----------------

  This  license  grants  a  worldwide, royalty-free,  perpetual  and
  irrevocable right  and license to use,  execute, perform, compile,
  display,  copy,   create  derivative  works   of,  distribute  and
  sublicense the  FreeType Project (in  both source and  object code
  forms)  and  derivative works  thereof  for  any  purpose; and  to
  authorize others  to exercise  some or all  of the  rights granted
  herein, subject to the following conditions:

    o Redistribution of  source code  must retain this  license file
      (`FTL.TXT') unaltered; any  additions, deletions or changes to
      the original  files must be clearly  indicated in accompanying
      documentation.   The  copyright   notices  of  the  unaltered,
      original  files must  be  preserved in  all  copies of  source
      files.

    o Redistribution in binary form must provide a  disclaimer  that
      states  that  the software is based in part of the work of the
      FreeType Team,  in  the  distribution  documentation.  We also
      encourage you to put an URL to the FreeType web page  in  your
      documentation, though this isn't mandatory.

  These conditions  apply to any  software derived from or  based on
  the FreeType Project,  not just the unmodified files.   If you use
  our work, you  must acknowledge us.  However, no  fee need be paid
  to us.

3. Advertising
--------------

  Neither the  FreeType authors and  contributors nor you  shall use
  the name of the  other for commercial, advertising, or promotional
  purposes without specific prior written permission.

  We suggest,  but do not require, that  you use one or  more of the
  following phrases to refer  to this software in your documentation
  or advertising  materials: `FreeType Project',  `FreeType Engine',
  `FreeType library', or `FreeType Distribution'.

  As  you have  not signed  this license,  you are  not  required to
  accept  it.   However,  as  the FreeType  Project  is  copyrighted
  material, only  this license, or  another one contracted  with the
  authors, grants you  the right to use, distribute,  and modify it.
  Therefore,  by  using,  distributing,  or modifying  the  FreeType
  Project, you indicate that you understand and accept all the terms
  of this license.

4. Contacts
-----------

  There are two mailing lists related to FreeType:

    o freetype@nongnu.org

      Discusses general use and applications of FreeType, as well as
      future and  wanted additions to the  library and distribution.
      If  you are looking  for support,  start in  this list  if you
      haven't found anything to help you in the documentation.

    o freetype-devel@nongnu.org

      Discusses bugs,  as well  as engine internals,  design issues,
      specific licenses, porting, etc.

  Our home page can be found at

    https://freetype.org


--- end of FTL.TXT ---
```

The files of FreeType's Adobe engine (`src/psaux/ps*.c` and `ps*.h`, Copyright 2007-2014 Adobe Systems Incorporated) add the following patent license to the FTL. Ports of them in sofel carry it as well.

```
This software, and all works of authorship, whether in source or
object code form as indicated by the copyright notice(s) included
herein (collectively, the "Work") is made available, and may only be
used, modified, and distributed under the FreeType Project License,
LICENSE.TXT.  Additionally, subject to the terms and conditions of the
FreeType Project License, each contributor to the Work hereby grants
to any individual or legal entity exercising permissions granted by
the FreeType Project License and this section (hereafter, "You" or
"Your") a perpetual, worldwide, non-exclusive, no-charge,
royalty-free, irrevocable (except as stated in this section) patent
license to make, have made, use, offer to sell, sell, import, and
otherwise transfer the Work, where such license applies only to those
patent claims licensable by such contributor that are necessarily
infringed by their contribution(s) alone or by combination of their
contribution(s) with the Work to which such contribution(s) was
submitted.  If You institute patent litigation against any entity
(including a cross-claim or counterclaim in a lawsuit) alleging that
the Work or a contribution incorporated within the Work constitutes
direct or contributory patent infringement, then any patent licenses
granted to You under this License for that Work shall terminate as of
the date such litigation is filed.

By using, modifying, or distributing the Work you indicate that you
have read and understood the terms and conditions of the
FreeType Project License as well as those provided in this section,
and you accept them fully.
```

## HarfBuzz

sofel follows rules of HarfBuzz 15.1.0 (commit `3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`), which is licensed under the "Old MIT" license.

| Where | Ported from |
|---|---|
| `tools/gen-unicode/` | The rules of `src/gen-ucd-table.py` (which decompositions are kept, which pairs compose) and `src/gen-arabic-pua.py` ([ADR 0019](docs/adr/0019-unicode-character-data.md)) |
| `sofel-unicode` | `src/hb-ucd.cc` (Hangul composition and decomposition), `src/hb-common.cc` (`hb_script_from_iso15924_tag`) |
| `sofel-read` | `src/cff/data.rs`: the 391 CFF standard strings from `src/hb-ot-cff1-std-str.hh` (Copyright © 2019 Adobe, Inc.) ([ADR 0021](docs/adr/0021-cff-outlines.md)); `src/cff/hb.rs`: HarfBuzz's CFF reading (the sanitizer and its budget, `CFFIndex`, charsets, encodings, FDSelects, the CFF2 Item Variation Store, the DICT interpreters and the BCD number parser) from `src/hb-sanitize.hh`, `src/hb-open-type.hh`, `src/hb-ot-cff-common.hh`, `src/hb-ot-cff1-table.hh`, `src/hb-ot-cff2-table.hh`, `src/hb-ot-layout-common.hh`, `src/hb-cff-interp-common.hh`, `src/hb-cff-interp-dict-common.hh` and `src/hb-number-parser.hh` (Copyright © 2018 Adobe Inc.; © 2007-2010 Red Hat, Inc.; © 2010-2018 Google, Inc.; © 2019-2026 Ebrahim Byagowi) ([ADR 0021](docs/adr/0021-cff-outlines.md)) |
| `sofel-font` | `src/tables/arabic_pua.rs`, generated from HarfBuzz's `src/ArabicPUASimplified.txt` and `src/ArabicPUATraditional.txt`; `src/names.rs` and `src/name_language.rs` from `src/OT/name/name.hh`, `src/hb-ot-name.cc`, `src/hb-ot-name-language-static.hh`, `src/hb-common.cc`, `src/hb-aat-ltag-table.hh`, `src/hb-utf.hh` and `src/hb-ot-post-table.hh`; `src/metrics.rs` and `src/glyf_data.rs` from `src/hb-ot-font.cc`, `src/hb-ot-metrics.cc`, `src/hb-font.hh`, `src/hb-algs.hh` (`_hb_roundf`, `hb_bsearch`, `hb_qsort_inline`), `src/hb-decycler.hh` and `src/OT/glyf/` ([ADR 0016](docs/adr/0016-font-and-vertical-metrics.md)); `src/layout.rs` and `src/base_sanitize.rs` from `src/hb-ot-layout.cc`, `src/hb-ot-layout-base-table.hh`, `src/hb-ot-layout-common.hh` (`Device`, `ItemVariationStore`), `src/hb-ot-tag.cc` (`hb_ot_tag_to_script`) and `src/hb-sanitize.hh` ([ADR 0017](docs/adr/0017-base-baselines.md)); `src/hb_cff.rs` (HarfBuzz's CFF and CFF2 accelerators) and the CFF glyph names of `src/names.rs` from `src/hb-ot-cff1-table.hh`, `src/hb-ot-cff2-table.hh` and `src/hb-face.cc` (Copyright © 2018 Adobe Inc.) ([ADR 0021](docs/adr/0021-cff-outlines.md)) |
| `sofel-outline` | `src/hb_cff.rs`: HarfBuzz's CFF and CFF2 glyph boxes from `src/hb-cff-interp-common.hh`, `src/hb-cff-interp-cs-common.hh`, `src/hb-cff1-interp-cs.hh`, `src/hb-cff2-interp-cs.hh`, `src/hb-ot-cff1-table.cc`, `src/hb-ot-cff2-table.cc` and `src/hb-ot-font.cc` (`hb_ot_get_glyph_extents`) (Copyright © 2018 Adobe Inc.; © 2011, 2014 Google, Inc.) ([ADR 0021](docs/adr/0021-cff-outlines.md)) |

The HarfBuzz license follows.

```
Copyright © 2010-2022  Google, Inc.
Copyright © 2015-2020  Ebrahim Byagowi
Copyright © 2019,2020  Facebook, Inc.
Copyright © 2012,2015  Mozilla Foundation
Copyright © 2011  Codethink Limited
Copyright © 2008,2010  Nokia Corporation and/or its subsidiary(-ies)
Copyright © 2009  Keith Stribley
Copyright © 2011  Martin Hosken and SIL International
Copyright © 2007  Chris Wilson
Copyright © 2005,2006,2020,2021,2022,2023  Behdad Esfahbod
Copyright © 2004,2007,2008,2009,2010,2013,2021,2022,2023  Red Hat, Inc.
Copyright © 1998-2005  David Turner and Werner Lemberg
Copyright © 2016  Igalia S.L.
Copyright © 2022  Matthias Clasen
Copyright © 2018,2021  Khaled Hosny
Copyright © 2018,2019,2020  Adobe, Inc
Copyright © 2013-2015  Alexei Podtelezhnikov

Permission is hereby granted, without written agreement and without
license or royalty fees, to use, copy, modify, and distribute this
software and its documentation for any purpose, provided that the
above copyright notice and the following two paragraphs appear in
all copies of this software.

IN NO EVENT SHALL THE COPYRIGHT HOLDER BE LIABLE TO ANY PARTY FOR
DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES
ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS DOCUMENTATION, EVEN
IF THE COPYRIGHT HOLDER HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH
DAMAGE.

THE COPYRIGHT HOLDER SPECIFICALLY DISCLAIMS ANY WARRANTIES, INCLUDING,
BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS FOR A PARTICULAR PURPOSE.  THE SOFTWARE PROVIDED HEREUNDER IS
ON AN "AS IS" BASIS, AND THE COPYRIGHT HOLDER HAS NO OBLIGATION TO
PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
```

## Unicode Character Database

`crates/sofel-unicode/src/tables.rs` is generated from the Unicode Character Database 18.0.0 (`UnicodeData.txt`, `Scripts.txt`, `PropertyValueAliases.txt`, `BidiMirroring.txt`, `DerivedNormalizationProps.txt`, `emoji/emoji-data.txt`, `VerticalOrientation.txt`; see `ucd.lock`) by `tools/gen-unicode/` ([ADR 0019](docs/adr/0019-unicode-character-data.md)). The crate declares `MIT AND Unicode-3.0`. The Unicode License v3 follows.

```
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2026 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```
