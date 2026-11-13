# Third-party notices

sofel is licensed under the MIT License (see [LICENSE](LICENSE)).

This file lists the third-party code that sofel copies, ports or adapts, with the copyright notices and license texts that their licenses require ([ADR 0002](docs/adr/0002-license-and-reuse.md)).
Each file that contains such code also names its source (repository, version or commit, path) and license in a comment at the top.

## FreeType

Portions of this software are copyright © 2026 The FreeType Project (https://freetype.org). All rights reserved.

sofel ports algorithms and arithmetic from FreeType 2.15.0 (commit `37515cf56ba3349656049edbed89ffc6f6466f2d`), choosing the FreeType License (FTL) of FreeType's dual license. The crates that contain such code declare `MIT AND FTL`.

| Crate | Ported from |
|---|---|
| `sofel-bitmapfont` | `src/winfonts/winfnt.c` (PE32 font resource enumeration) |
| `sofel-geom` | `src/base/ftcalc.c`, `include/freetype/internal/ftcalc.h`, `src/base/fttrigon.c`, `src/base/ftoutln.c` |
| `sofel-read` | `src/pfr/pfrload.c`, `src/pfr/pfrobjs.c`, `src/pfr/pfrcmap.c` (PFR logical fonts, character descriptors and metrics); `src/cff/cffparse.c`, `src/cff/cfftoken.h`, `src/cff/cffload.c` (DICT parsing, INDEX access, charset, encoding, FDSelect, the blend vector and the predefined charsets and encodings), `src/base/ftcalc.c` (64-bit `FT_MulFix`, `FT_DivFix`, `FT_MulDiv`, `FT_RoundFix`, `FT_SqrtFixed`, `FT_Matrix_Check`), and the Adobe engine `src/psaux/psintrp.c`, `src/psaux/pshints.c`, `src/psaux/psstack.c`, `src/psaux/psfont.c`, `src/psaux/psft.c`, `src/psaux/psobjs.c` (`cff_random`), `src/psaux/cffdecode.c` (`cff_lookup_glyph_by_stdcharcode`) ([ADR 0021](docs/adr/0021-cff-outlines.md)); the Adobe files carry the patent license below |
| `sofel-font` | `src/pfr/pfrgload.c`, `src/pfr/pfrsbit.c` (PFR outlines and embedded strikes); `src/sfnt/ttmtx.c`, `src/sfnt/sfobjs.c` (the left side bearing for phantom point 1; the `FT_Face` values), `src/sfnt/ttload.c` (`tt_face_load_name`, `tt_face_load_post`), `src/sfnt/sfdriver.c` (the PostScript name, `sfnt_get_name_index`), `src/sfnt/ttpost.c`, `src/truetype/ttgload.c` (`TT_Get_VMetrics`); `src/cff/cffload.c`, `src/cff/cffobjs.c` (`cff_face_init`), `src/cff/cffdrivr.c` (CID, glyph names, PostScript font information) and `src/base/ftcalc.c` (`FT_Matrix_Multiply_Scaled`, `FT_Vector_Transform_Scaled`) ([ADR 0021](docs/adr/0021-cff-outlines.md)) |
| `sofel-outline` | `src/truetype/ttgload.c`, `src/truetype/ttpload.c`, `src/base/ftoutln.c`; for CFF ([ADR 0021](docs/adr/0021-cff-outlines.md)) `src/cff/cffgload.c` (`cff_slot_load`) and the Adobe engine's builder callbacks `src/psaux/psft.c` and `src/psaux/psobjs.c` (`ps_builder_*`), which carry the patent license below |
| `sofel-raster` | `src/sdf/ftsdf.c`, `src/sdf/ftbsdf.c`, `src/sdf/ftsdfcommon.c`, `src/base/ftcalc.c`, `src/raster/ftraster.c`, `src/smooth/ftgrays.c`, `src/smooth/ftsmooth.c`, `src/base/ftobjs.c`, `src/sfnt/ttcolr.c` (`tt_face_colr_blend_layer`, [ADR 0023](docs/adr/0023-color-glyphs.md)) |
| `sofel-bitmap` | `src/sfnt/ttsbit.c` (`tt_face_load_sbit`, `tt_face_load_strike_metrics`, `tt_sbit_decoder_load_image`, `tt_sbit_decoder_load_bitmap`, `tt_face_load_sbix_image`), `src/sfnt/sfobjs.c` (the `available_sizes`), `src/sfnt/ttmtx.c` (`tt_face_get_metrics`), `src/base/ftobjs.c` (`FT_Match_Size`) ([ADR 0023](docs/adr/0023-color-glyphs.md)) |

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
| `tools/gen-unicode/` | The rules of `src/gen-ucd-table.py` (which decompositions are kept, which pairs compose), `src/gen-arabic-pua.py`, `src/gen-indic-table.py`, `src/gen-use-table.py`, `src/gen-arabic-table.py`, `src/gen-arabic-joining-list.py` and `src/gen-vowel-constraints.py` ([ADR 0019](docs/adr/0019-unicode-character-data.md)) |
| `tools/gen-machine/` | `grammars/*.machine`: the categories and rules of `src/hb-ot-shaper-indic-machine.rl`, `src/hb-ot-shaper-khmer-machine.rl`, `src/hb-ot-shaper-myanmar-machine.rl` and `src/hb-ot-shaper-use-machine.rl`; the generated `crates/sofel-shape/src/tables/machine_*.rs` and the scanner of `crates/sofel-shape/src/machine.rs` (including `find_syllables_use`'s skipping of CGJ and ZWNJ) follow them ([ADR 0024](docs/adr/0024-complex-shapers.md)) |
| `sofel-gpu` and `sofel-geom` | Outline encoding and cubic approximation from `src/hb-gpu-draw.cc` and `src/hb-gpu-cu2qu.hh`, paint records from `src/hb-gpu-paint.cc` (Copyright © 2026 Behdad Esfahbod), the vertex and fragment shader sources (`src/hb-gpu-vertex.*`, `hb-gpu-fragment.*`, `hb-gpu-draw-fragment.*`, `hb-gpu-paint-fragment.*`), and record sorting from `src/hb-algs.hh` ([ADR 0068](docs/adr/0068-vector-and-gpu-output.md)) |
| `tools/gen-ot-tag/` | Reads `src/hb-ot-tag-table.hh` (the output of HarfBuzz's `src/gen-tag-table.py`) ([ADR 0020](docs/adr/0020-shaping-foundation.md)) |
| `tools/hb-dump/` | `--subset`: the option callbacks of `util/hb-subset.cc` and `parse_instancing_spec` of `util/helper-subset.hh` ([ADR 0022](docs/adr/0022-subsetting.md)) |
| `sofel-unicode` | `src/hb-ucd.cc` (Hangul composition and decomposition), `src/hb-common.cc` (`hb_script_from_iso15924_tag`) |
| `sofel-geom` | `src/float.rs`: `_hb_roundf` (HarfBuzz's `roundf` macro) and `hb_clamp_to` from `src/hb-algs.hh` (Copyright © 2017 Google, Inc.; © 2019 Facebook, Inc.) ([ADR 0026](docs/adr/0026-harfbuzz-rounding.md)) |
| `sofel-read` | `src/cff/data.rs`: the 391 CFF standard strings from `src/hb-ot-cff1-std-str.hh` (Copyright © 2019 Adobe, Inc.) ([ADR 0021](docs/adr/0021-cff-outlines.md)); `src/cff/hb.rs`: HarfBuzz's CFF reading (the sanitizer and its budget, `CFFIndex`, charsets, encodings, FDSelects, the CFF2 Item Variation Store, the DICT interpreters and the BCD number parser) from `src/hb-sanitize.hh`, `src/hb-open-type.hh`, `src/hb-ot-cff-common.hh`, `src/hb-ot-cff1-table.hh`, `src/hb-ot-cff2-table.hh`, `src/hb-ot-layout-common.hh`, `src/hb-cff-interp-common.hh`, `src/hb-cff-interp-dict-common.hh` and `src/hb-number-parser.hh` (Copyright © 2018 Adobe Inc.; © 2007-2010 Red Hat, Inc.; © 2010-2018 Google, Inc.; © 2019-2026 Ebrahim Byagowi) ([ADR 0021](docs/adr/0021-cff-outlines.md)) |
| `sofel-read` | `src/layout/`, `src/gdef.rs`, `src/gsub.rs`, `src/gpos.rs` and `src/device.rs` (`Device::delta_pixels`) from `src/hb-ot-layout-common.hh`, `src/hb-ot-layout-gsubgpos.hh`, `src/hb-ot-dual.hh`, `src/OT/Layout/Common/`, `src/OT/Layout/GDEF/GDEF.hh`, `src/OT/Layout/GSUB/`, `src/OT/Layout/GPOS/`, `src/hb-ot-layout.cc` (`GDEF::is_blocklisted`) and `src/hb-sanitize.hh` (the operation budget) ([ADR 0020](docs/adr/0020-shaping-foundation.md)) |
| `sofel-font` | `src/tables/arabic_pua.rs`, generated from HarfBuzz's `src/ArabicPUASimplified.txt` and `src/ArabicPUATraditional.txt`; `src/names.rs` and `src/name_language.rs` from `src/OT/name/name.hh`, `src/hb-ot-name.cc`, `src/hb-ot-name-language-static.hh`, `src/hb-common.cc`, `src/hb-aat-ltag-table.hh`, `src/hb-utf.hh` and `src/hb-ot-post-table.hh`; `src/metrics.rs` and `src/glyf_data.rs` from `src/hb-ot-font.cc`, `src/hb-ot-metrics.cc`, `src/hb-font.hh`, `src/hb-algs.hh` (`_hb_roundf`, `hb_bsearch`, `hb_qsort_inline`), `src/hb-decycler.hh` and `src/OT/glyf/` ([ADR 0016](docs/adr/0016-font-and-vertical-metrics.md)); `src/layout.rs` and `src/base_sanitize.rs` from `src/hb-ot-layout.cc`, `src/hb-ot-layout-base-table.hh`, `src/hb-ot-layout-common.hh` (`Device`, `ItemVariationStore`), `src/hb-ot-tag.cc` (`hb_ot_tag_to_script`) and `src/hb-sanitize.hh` ([ADR 0017](docs/adr/0017-base-baselines.md)); `src/hb_cff.rs` (HarfBuzz's CFF and CFF2 accelerators) and the CFF glyph names of `src/names.rs` from `src/hb-ot-cff1-table.hh`, `src/hb-ot-cff2-table.hh` and `src/hb-face.cc` (Copyright © 2018 Adobe Inc.) ([ADR 0021](docs/adr/0021-cff-outlines.md)) |
| `sofel-shape` | `src/tables/indic.rs`, `src/tables/use_.rs`, `src/tables/arabic.rs` and `src/tables/vowel_constraints.rs`, generated by the rules of HarfBuzz's table generators above, with the numbers of `src/hb-ot-shaper-*-machine.rl` and `src/hb-ot-shaper-indic.hh` ([ADR 0019](docs/adr/0019-unicode-character-data.md)); `src/tag_table.rs`: the language tables and the branches of `hb_ot_tags_from_complex_language` and `hb_ot_ambiguous_tag_to_language`, generated from `src/hb-ot-tag-table.hh`; `src/tag.rs` from `src/hb-ot-tag.cc`; `src/language.rs` from `src/hb-common.cc` (`hb_language_from_string`) (Copyright © 2009 Red Hat, Inc.; © 2011 Google, Inc.) ([ADR 0020](docs/adr/0020-shaping-foundation.md)) |
| `sofel-geom` | `src/paint.rs` and `src/paint_util.rs`: the paint commands of `src/hb-paint.h`, the transforms of `src/hb-geometry.hh` (`hb_transform_t`), and `hb_paint_reduce_linear_anchors`, `hb_paint_normalize_color_line`, `hb_paint_sweep_gradient_tiles` of `src/hb-paint.cc` and `hb_color_lerp` of `src/hb-paint.hh` (Copyright © 2022 Matthias Clasen) ([ADR 0023](docs/adr/0023-color-glyphs.md)) |
| `sofel-read` | `src/colr.rs`, `src/cpal.rs`, `src/cblc.rs`, `src/sbix.rs`, `src/svg.rs` and `src/search.rs`: the lookups and the checks of `src/OT/Color/COLR/COLR.hh`, `src/OT/Color/CPAL/CPAL.hh`, `src/OT/Color/CBDT/CBDT.hh`, `src/OT/Color/sbix/sbix.hh`, `src/OT/Color/svg/svg.hh` and `hb_bsearch_impl` of `src/hb-algs.hh` ([ADR 0023](docs/adr/0023-color-glyphs.md)) |
| `sofel-bitmap` | `choose_strike` and the PNG glyphs and boxes for painting from `src/OT/Color/CBDT/CBDT.hh` and `src/OT/Color/sbix/sbix.hh` ([ADR 0023](docs/adr/0023-color-glyphs.md)) |
| `sofel-color` | `hb_ot_paint_glyph_or_fail` and `hb_ot_get_glyph_extents` of `src/hb-ot-font.cc`, the painting of `src/OT/Color/COLR/COLR.hh`, `src/OT/Color/svg/svg.hh`, `src/hb-ot-color.cc`, `src/hb-paint-extents.hh`, `src/hb-paint-extents.cc`, `src/hb-paint-bounded.hh`, `src/hb-paint-bounded.cc` and `src/hb-decycler.hh` (Copyright © 2022 Matthias Clasen; © 2018 Ebrahim Byagowi; © 2020 Google, Inc.) ([ADR 0023](docs/adr/0023-color-glyphs.md)) |
| `sofel-outline` | `src/hb_cff.rs`: HarfBuzz's CFF and CFF2 glyph boxes from `src/hb-cff-interp-common.hh`, `src/hb-cff-interp-cs-common.hh`, `src/hb-cff1-interp-cs.hh`, `src/hb-cff2-interp-cs.hh`, `src/hb-ot-cff1-table.cc`, `src/hb-ot-cff2-table.cc` and `src/hb-ot-font.cc` (`hb_ot_get_glyph_extents`) (Copyright © 2018 Adobe Inc.; © 2011, 2014 Google, Inc.) ([ADR 0021](docs/adr/0021-cff-outlines.md)) |
| `sofel-shape` | The shaping core ([ADR 0020](docs/adr/0020-shaping-foundation.md)): `src/buffer.rs`, `src/utf.rs`, `src/props.rs` and `src/diff.rs` from `src/hb-buffer.hh`, `src/hb-buffer.cc`, `src/hb-utf.hh`, `src/hb-limits.hh` and `src/hb-common.cc` (`hb_script_get_horizontal_direction`); `src/unicode.rs` from `src/hb-unicode.hh` and `src/hb-unicode.cc`; `src/digest.rs` from `src/hb-set-digest.hh`; `src/plan.rs`, `src/shape.rs`, `src/normalize.rs` and `src/fallback.rs` from `src/hb-ot-shape.cc`, `src/hb-ot-shaper.hh`, `src/hb-ot-shaper-default.cc`, `src/hb-ot-shape-normalize.cc`, `src/hb-ot-shape-fallback.cc` and `src/hb-fallback-shape.cc`; `src/layout/` from `src/hb-ot-map.cc`, `src/hb-ot-map.hh`, `src/hb-ot-layout.cc`, `src/hb-ot-layout.hh`, `src/hb-ot-layout-gsubgpos.hh`, `src/OT/Layout/GDEF/`, `src/OT/Layout/GSUB/` and `src/OT/Layout/GPOS/`; `src/scale.rs` and `src/font.rs` from `src/hb-font.hh` and `src/hb-ot-layout-common.hh` (`HintingDevice::get_delta`) (Copyright © 1998-2004 David Turner and Werner Lemberg; © 2004, 2006-2010 Red Hat, Inc.; © 2007 Chris Wilson; © 2009 Keith Stribley; © 2010-2020 Google, Inc.; © 2019 Facebook, Inc.) ([ADR 0020](docs/adr/0020-shaping-foundation.md)) |
| `sofel-shape` | The script-specific shapers of [ADR 0024](docs/adr/0024-complex-shapers.md) in `src/shapers/`: `hebrew.rs` from `src/hb-ot-shaper-hebrew.cc` (Copyright © 2010, 2012 Google, Inc.), `hangul.rs` from `src/hb-ot-shaper-hangul.cc` (Copyright © 2013 Google, Inc.), `thai.rs` from `src/hb-ot-shaper-thai.cc` (Copyright © 2010, 2012 Google, Inc.), `arabic.rs` and `arabic_fallback.rs` from `src/hb-ot-shaper-arabic.cc`, `src/hb-ot-shaper-arabic-fallback.hh` and `src/hb-ot-shaper-arabic-win1256.hh` (Copyright © 2010, 2012 Google, Inc.; the fallback © 2012 Google, Inc.; the Windows-1256 lookups © 2014 Google, Inc.), `indic.rs` from `src/hb-ot-shaper-indic.cc` and `src/hb-ot-shaper-indic.hh` (Copyright © 2011, 2012 Google, Inc.), `khmer.rs` from `src/hb-ot-shaper-khmer.cc` (Copyright © 2011, 2012 Google, Inc.), `myanmar.rs` from `src/hb-ot-shaper-myanmar.cc` (Copyright © 2011, 2012, 2013 Google, Inc.), `use_.rs` from `src/hb-ot-shaper-use.cc` and the `find_syllables_use` of `src/hb-ot-shaper-use-machine.rl` (Copyright © 2015 Mozilla Foundation; Copyright © 2015 Google, Inc.), `syllabic.rs` from `src/hb-ot-shaper-syllabic.cc` (Copyright © 2021 Behdad Esfahbod), `vowel_constraints.rs` from `src/hb-ot-shaper-vowel-constraints.cc` (generated by HarfBuzz's `gen-vowel-constraints.py`; its header `src/hb-ot-shaper-vowel-constraints.hh` is Copyright © 2018 Google, Inc.); `src/layout/would.rs` from the `would_apply` of `src/hb-ot-layout-gsubgpos.hh` and `src/OT/Layout/GSUB/` and `hb_ot_layout_lookup_would_substitute` of `src/hb-ot-layout.cc`; HarfRust's `src/ot/shaper/` was read alongside |
| `sofel-shape` | `src/feature.rs`: feature strings (`hb_feature_from_string`, `hb_feature_to_string`) from `src/hb-common.cc` (Copyright © 2009, 2010 Red Hat, Inc.; © 2011, 2012 Google, Inc.), and `hb_parse_int`, `hb_parse_uint` from `src/hb-number.cc` (Copyright © 2019-2026 Ebrahim Byagowi); `src/serialize.rs`: buffer serialization from `src/hb-buffer-serialize.cc`, the readers after the Ragel grammars `src/hb-buffer-deserialize-json.rl`, `src/hb-buffer-deserialize-text-glyphs.rl` and `src/hb-buffer-deserialize-text-unicode.rl`, and `glyph_to_string`, `glyph_from_string` from `src/hb-font.hh` (Copyright © 2012, 2013 Google, Inc.; the grammars © 2013 Google, Inc.) ([ADR 0020](docs/adr/0020-shaping-foundation.md)) |
| `sofel-cli` | `src/shape.rs` (`sofel-shape`): the options, the reading of the text, the output and `--verify` from `util/hb-shape.cc`, `util/shape-options.hh`, `util/font-options.hh`, `util/text-options.hh`, `util/shape-format.hh`, `util/shape-output.hh` and `src/hb-buffer-verify.cc` (Copyright © 2010 Behdad Esfahbod; © 2011, 2012 Google, Inc.; © 2022 Behdad Esfahbod) ([ADR 0020](docs/adr/0020-shaping-foundation.md)) |
| `sofel-write` | `src/sfnt.rs`: the font assembly of `src/hb-face-builder.cc` and `src/hb-open-file.hh`; `src/serialize.rs`: the object serializer of `src/hb-serialize.hh` (Copyright © 2007-2008 Red Hat, Inc.; © 2012-2018 Google, Inc.; © 2019 Facebook, Inc.); `src/repack/`: the repacker of `src/hb-repacker.hh`, `src/graph/` (`graph.hh`, `serialize.hh`, `gsubgpos-graph.hh`, `gsubgpos-context.*`, `pairpos-graph.hh`, `markbasepos-graph.hh`, `ligature-graph.hh`, `coverage-graph.hh`, `classdef-graph.hh`, `split-helpers.hh`), `src/hb-subset-serialize.cc`, `src/hb-priority-queue.hh`, `src/hb-map.hh` (the hash table's probing and order) and the Coverage and ClassDef serialization of `src/OT/Layout/Common/` and `src/hb-ot-layout-common.hh`, with its tests from `src/test-repacker.cc` and `src/graph/test-classdef-graph.cc` (Copyright © 2007-2010 Red Hat, Inc.; © 2010-2026 Google, Inc.) ([ADR 0022](docs/adr/0022-subsetting.md)) |
| `sofel-subset` | Variation support rebasing in `src/tables/variation_region.rs` follows `src/hb-subset-instancer-solver.cc` (Copyright © 2023 Behdad Esfahbod). |
| `sofel-subset` | The subsetter: `src/hb-subset.cc`, `src/hb-subset-input.cc` (with `hb_subset_preprocess`), `src/hb-subset-plan.cc`, `src/hb-subset-accelerator.hh` (the preprocessed data), `src/hb-subset-table.hh` (with the growth of `hb_vector_t::alloc` of `src/hb-vector.hh` for the room of the serialization buffer), the cmap reading and subsetting of `src/hb-ot-cmap-table.hh`, `src/OT/glyf/`, `src/hb-ot-hmtx-table.hh`, `src/hb-ot-maxp-table.hh`, `src/hb-ot-os2-table.hh` with the Unicode ranges of `src/hb-ot-os2-unicode-ranges.hh`, `src/hb-ot-post-table.hh` with the standard names of `src/hb-ot-post-macroman.hh`, `src/OT/name/name.hh`, `src/hb-ot-hdmx-table.hh`; the CFF subsetting of `src/hb-subset-cff1.cc`, `src/hb-subset-cff-common.hh`, `src/hb-subset-cff-common.cc` and `src/hb-subset-table-cff.cc` with the charstring interpreter of `src/hb-cff-interp-common.hh`, `src/hb-cff-interp-cs-common.hh`, `src/hb-cff-interp-dict-common.hh` and `src/hb-cff1-interp-cs.hh`, the serialization and `seac` search of `src/hb-ot-cff1-table.hh`, `src/hb-ot-cff1-table.cc` and `src/hb-ot-cff-common.hh`, and `src/hb-ot-vorg-table.hh`; the layout closure and subsetting of `src/hb-subset-plan-layout.cc`, `src/hb-ot-layout-common.hh`, `src/hb-ot-layout-gsubgpos.hh`, `src/hb-ot-layout.cc` (`hb_ot_layout_collect_features`, `hb_ot_layout_lookups_substitute_closure`), `src/OT/Layout/Common/`, `src/OT/Layout/GSUB/`, `src/OT/Layout/GPOS/`, `src/OT/Layout/GDEF/GDEF.hh` and `src/hb-ot-layout-base-table.hh`; the color closure and subsetting of `src/OT/Color/COLR/COLR.hh`, `src/OT/Color/COLR/colrv1-closure.hh`, `src/OT/Color/CPAL/CPAL.hh`, `src/OT/Color/CBDT/CBDT.hh`, `src/OT/Color/sbix/sbix.hh` and `src/hb-subset-table-color.cc`; the MATH closure and subsetting of `src/hb-ot-math-table.hh`; the name id collection of `src/hb-ot-stat-table.hh` and `src/hb-ot-var-fvar-table.hh`; and pinning every axis to its default: `src/hb-subset-plan-var.cc` (`normalize_axes_location`), the axis functions of `src/hb-subset-input.cc`, the axis normalization of `src/hb-ot-var-fvar-table.hh` and `src/hb-ot-var-avar-table.hh` (`SegmentMaps::map_float`, `map_coords_2_14`), the outline recompilation of `src/OT/glyf/Glyph.hh`, `src/OT/glyf/SimpleGlyph.hh`, `src/OT/glyf/CompositeGlyph.hh` and `src/OT/glyf/glyf-helpers.hh` with the cycle check of `src/hb-decycler.hh`, the metrics updates of `src/hb-ot-hmtx-table.hh`, `src/hb-ot-maxp-table.hh`, `src/hb-ot-os2-table.hh` and `src/hb-ot-post-table.hh`, the STAT subsetting of `src/hb-ot-stat-table.hh`, the CFF2 instancing and its conversion to CFF of `src/hb-subset-cff2.cc`, `src/hb-subset-cff2-round.hh`, `src/hb-subset-cff2-to-cff1.hh`, `src/hb-cff-specializer.hh` and `src/hb-cff-width-optimizer.hh` with the CFF2 charstring interpreter of `src/hb-cff2-interp-cs.hh`, the DICTs of `src/hb-ot-cff2-table.hh`, the glyph boxes of `src/hb-ot-cff2-table.cc`, the region scalars of `src/hb-ot-layout-common.hh` (`VarRegionAxis::evaluate`) and `update_instance_metrics_map_from_cff2` of `src/hb-subset-plan-var.cc`, the FeatureVariations evaluation of `src/hb-ot-layout-common.hh` and the instanced value formats of `src/OT/Layout/GPOS/ValueFormat.hh` (Copyright © 2018-2023 Google, Inc.; © 2016 Google, Inc.; © 2020 Google, Inc.; © 2011-2012 Google, Inc.; © 2007-2010 Red Hat, Inc.; © 2010-2012 Google, Inc.; © 2017 Google, Inc.; © 2016 Elie Roux; © 2018-2019 Ebrahim Byagowi; © 2018 Adobe Inc.; © 2016 Igalia S.L.) ([ADR 0022](docs/adr/0022-subsetting.md)) |
| `sofel-cli` | `src/subset.rs`: the options of `util/hb-subset.cc` and `hb_subset_input_to_string_or_fail` of `src/hb-subset-input.cc` ([ADR 0022](docs/adr/0022-subsetting.md)) |

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

## musl

`sofel-geom`'s `src/trigf.rs` ports `src/math/sincosf.c`, `src/math/tanf.c`, `src/math/__sindf.c`, `src/math/__cosdf.c`, `src/math/__tandf.c`, `src/math/__rem_pio2f.c` and the `ipio2` table of `src/math/__rem_pio2_large.c` from musl 1.2.6 (commit `bd54dc3e8049b9c7db092905e743cdebbb8864f3`), licensed under the MIT license ([ADR 0023](docs/adr/0023-color-glyphs.md)). The VARC reference builder also includes copies of those C files and `src/internal/libm.h` under `tools/hb-varc-reference/musl/`. The mathematical functions come from FreeBSD's msun and carry the Sun Microsystems notice below.

```
Copyright © 2005-2020 Rich Felker, et al.

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

```
Copyright (C) 1993, 2004 by Sun Microsystems, Inc. All rights reserved.

Developed at SunPro, a Sun Microsystems, Inc. business.
Permission to use, copy, modify, and distribute this
software is freely granted, provided that this notice
is preserved.
```

## Unicode Character Database

`crates/sofel-unicode/src/tables.rs` is generated from the Unicode Character Database 18.0.0 (`UnicodeData.txt`, `Scripts.txt`, `PropertyValueAliases.txt`, `BidiMirroring.txt`, `DerivedNormalizationProps.txt`, `emoji/emoji-data.txt`, `VerticalOrientation.txt`; see `ucd.lock`) by `tools/gen-unicode/` ([ADR 0019](docs/adr/0019-unicode-character-data.md)). The shaper tables `crates/sofel-shape/src/tables/indic.rs`, `use_.rs`, `arabic.rs` and `vowel_constraints.rs` are generated by the same tool from `IndicSyllabicCategory.txt`, `IndicPositionalCategory.txt`, `ArabicShaping.txt`, `DerivedCoreProperties.txt`, `UnicodeData.txt`, `Blocks.txt`, `Scripts.txt` and `PropertyValueAliases.txt`. Both crates declare `MIT AND Unicode-3.0`. The Unicode License v3 follows.

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

## Microsoft Universal Shaping Engine data

`crates/sofel-shape/src/tables/use_.rs` and `crates/sofel-shape/src/tables/vowel_constraints.rs` are generated in part from Microsoft's `IndicSyllabicCategory-Additional.txt`, `IndicPositionalCategory-Additional.txt` and `IndicShapingInvalidCluster.txt`, distributed in HarfBuzz's `src/ms-use/` under the MIT license ([ADR 0019](docs/adr/0019-unicode-character-data.md)).

```
MIT License

Copyright (c) Microsoft Corporation.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE
```

## HarfRust

`sofel-shape`'s shaping core (`src/buffer.rs`, `src/shape.rs`, `src/plan.rs`, `src/normalize.rs`, `src/fallback.rs` and `src/layout/`) also follows HarfRust 0.15.0 (https://github.com/harfbuzz/harfrust), a Rust port of HarfBuzz, where it matches HarfBuzz 15.1.0. Its license follows.

```
The MIT License (MIT)

Copyright (c) HarfBuzz developers
Copyright (c) 2020 Yevhenii Reizner

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Extra conformance fonts ([ADR 0031](docs/adr/0031-diverse-font-corpus.md))

These fixed binaries are downloaded only for tests and benchmarks. They are not bundled with sofel. Each line identifies the exact source and the license text supplied with the cached font. The same Apache-2.0 license below also covers the already used `test_glyphs-glyf_colr_1_variable.ttf` from the fixed color-fonts source.

| Font ID | Source revision or release | Binary and license |
|---|---|---|
| `arabic` | google/fonts `5bb28733cec313b63fbd415b574b5ae35c8aa8b8` | `fonts/arabic.ttf`; [arabic-license](#arabic-license) |
| `devanagari` | google/fonts `5bb28733cec313b63fbd415b574b5ae35c8aa8b8` | `fonts/devanagari.ttf`; [devanagari-license](#devanagari-license) |
| `khmer` | google/fonts `5bb28733cec313b63fbd415b574b5ae35c8aa8b8` | `fonts/khmer.ttf`; [khmer-license](#khmer-license) |
| `myanmar` | google/fonts `5bb28733cec313b63fbd415b574b5ae35c8aa8b8` | `fonts/myanmar.ttf`; [myanmar-license](#myanmar-license) |
| `balinese` | google/fonts `5bb28733cec313b63fbd415b574b5ae35c8aa8b8` | `fonts/balinese.ttf`; [balinese-license](#balinese-license) |
| `hebrew` | google/fonts `5bb28733cec313b63fbd415b574b5ae35c8aa8b8` | `fonts/hebrew.ttf`; [hebrew-license](#hebrew-license) |
| `source-serif` | adobe-fonts/source-serif `80d3f8894c09c937bebfa9011247d2e1c79fd6f4` | `fonts/source-serif.otf`; [source-serif-license](#source-serif-license) |
| `stix-math` | stipub/stixfonts `bea1dc9a3cc2d8b57e412dfd0f448a84382dfe46` | `fonts/stix-math.otf`; [stix-license](#stix-license) |
| `source-han` | adobe-fonts/source-han-sans release `2.005R`; license commit `6c709ca72d3d7c46ab42ebecc1a26e7d69595a37` | `fonts/source-han.ttc`; [source-han-license](#source-han-license) |
| `amstelvar` | googlefonts/amstelvar-avar2 `8727e9b81b944be8b9b9188a19ba9cf19b867d95` | `fonts/amstelvar.ttf`; [amstelvar-license](#amstelvar-license) |
| `color-cff` | googlefonts/color-fonts `e830e042b0e93fd5b0197a7fe9f0398f6d87f467` | `fonts/color-cff.otf`; [color-license](#color-license) |
| `color-sbix` | googlefonts/color-fonts `e830e042b0e93fd5b0197a7fe9f0398f6d87f467` | `fonts/color-sbix.ttf`; [color-license](#color-license) |
| `color-svg` | googlefonts/color-fonts `e830e042b0e93fd5b0197a7fe9f0398f6d87f467` | `fonts/color-svg.ttf`; [color-license](#color-license) |

### arabic-license

Source: https://raw.githubusercontent.com/google/fonts/5bb28733cec313b63fbd415b574b5ae35c8aa8b8/ofl/notosansarabic/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2022 The Noto Project Authors (https://github.com/notofonts/arabic)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### devanagari-license

Source: https://raw.githubusercontent.com/google/fonts/5bb28733cec313b63fbd415b574b5ae35c8aa8b8/ofl/notosansdevanagari/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2022 The Noto Project Authors (https://github.com/notofonts/devanagari)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### khmer-license

Source: https://raw.githubusercontent.com/google/fonts/5bb28733cec313b63fbd415b574b5ae35c8aa8b8/ofl/notosanskhmer/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2022 The Noto Project Authors (https://github.com/notofonts/khmer)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### myanmar-license

Source: https://raw.githubusercontent.com/google/fonts/5bb28733cec313b63fbd415b574b5ae35c8aa8b8/ofl/notosansmyanmar/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2022 The Noto Project Authors (https://github.com/notofonts/myanmar)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### balinese-license

Source: https://raw.githubusercontent.com/google/fonts/5bb28733cec313b63fbd415b574b5ae35c8aa8b8/ofl/notosansbalinese/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2022 The Noto Project Authors (https://github.com/notofonts/balinese)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### hebrew-license

Source: https://raw.githubusercontent.com/google/fonts/5bb28733cec313b63fbd415b574b5ae35c8aa8b8/ofl/notosanshebrew/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2022 The Noto Project Authors (https://github.com/notofonts/hebrew)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### source-serif-license

Source: https://raw.githubusercontent.com/adobe-fonts/source-serif/80d3f8894c09c937bebfa9011247d2e1c79fd6f4/LICENSE.md

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2014 - 2023 Adobe (http://www.adobe.com/), with Reserved Font Name ‘Source’. All Rights Reserved. Source is a trademark of Adobe in the United States and/or other countries.

This Font Software is licensed under the SIL Open Font License, Version 1.1.

This license is copied below, and is also available with a FAQ at: http://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### stix-license

Source: https://raw.githubusercontent.com/stipub/stixfonts/bea1dc9a3cc2d8b57e412dfd0f448a84382dfe46/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2001-2021 The STIX Fonts Project Authors (https://github.com/stipub/stixfonts), with Reserved Font Name "TM Math". STIX Fonts™ is a trademark of The Institute of Electrical and Electronics Engineers, Inc.

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### source-han-license

Source: https://raw.githubusercontent.com/adobe-fonts/source-han-sans/6c709ca72d3d7c46ab42ebecc1a26e7d69595a37/LICENSE.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2014-2025 Adobe (http://www.adobe.com/), with Reserved Font
Name 'Source'. Source is a trademark of Adobe in the United States
and/or other countries.

This Font Software is licensed under the SIL Open Font License,
Version 1.1.

This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font
creation efforts of academic and linguistic communities, and to
provide a free and open framework in which fonts may be shared and
improved in partnership with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply to
any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software
components as distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to,
deleting, or substituting -- in part or in whole -- any of the
components of the Original Version, by changing formats or by porting
the Font Software to a new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed,
modify, redistribute, and sell modified and unmodified copies of the
Font Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components, in
Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the
corresponding Copyright Holder. This restriction only applies to the
primary font name as presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created using
the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### amstelvar-license

Source: https://raw.githubusercontent.com/googlefonts/amstelvar-avar2/8727e9b81b944be8b9b9188a19ba9cf19b867d95/OFL.txt

Copyright notices and complete license text from this fixed source:

~~~~text
Copyright 2016 The Amstelvar Project Authors (https://github.com/googlefonts/amstelvar-avar2)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:

https://openfontlicense.org


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
~~~~

### color-license

Source: https://raw.githubusercontent.com/googlefonts/color-fonts/e830e042b0e93fd5b0197a7fe9f0398f6d87f467/LICENSE

Copyright notices and complete license text from this fixed source:

~~~~text

                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
~~~~

## bzip2

The legacy block randomisation sequence is from bzip2 (randtable.c).

```text
--------------------------------------------------------------------------

This program, "bzip2", the associated library "libbzip2", and all
documentation, are copyright (C) 1996-2010 Julian R Seward.  All
rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2. The origin of this software must not be misrepresented; you must
   not claim that you wrote the original software.  If you use this
   software in a product, an acknowledgment in the product
   documentation would be appreciated but is not required.

3. Altered source versions must be plainly marked as such, and must
   not be misrepresented as being the original software.

4. The name of the author may not be used to endorse or promote
   products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS
OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE
GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

Julian Seward, jseward@acm.org
bzip2/libbzip2 version 1.1.0 of 6 September 2010

--------------------------------------------------------------------------
```

## Noto Color Emoji gallery fonts (sofel [ADR 0077](docs/adr/0077-noto-color-emoji-gallery.md))

The two fixed v2.047 font binaries are downloaded for the gallery and its tests. They are not bundled with sofel. Both use the license supplied in the same release.

| Font ID | Source file | Cached file |
|---|---|---|
| `color-noto-cbdt` | `fonts/NotoColorEmoji.ttf` | `fonts/color-noto-cbdt.ttf` |
| `color-noto-colrv1` | `fonts/Noto-COLRv1.ttf` | `fonts/color-noto-colrv1.ttf` |

Source: https://raw.githubusercontent.com/googlefonts/noto-emoji/v2.047/fonts/LICENSE

Complete font license text from this fixed release:

~~~~text
This Font Software is licensed under the SIL Open Font License,
Version 1.1.

This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font
creation efforts of academic and linguistic communities, and to
provide a free and open framework in which fonts may be shared and
improved in partnership with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply to
any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software
components as distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to,
deleting, or substituting -- in part or in whole -- any of the
components of the Original Version, by changing formats or by porting
the Font Software to a new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed,
modify, redistribute, and sell modified and unmodified copies of the
Font Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components, in
Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the
corresponding Copyright Holder. This restriction only applies to the
primary font name as presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created using
the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.

~~~~

## Adobe Glyph List

`sofel-type1/src/type42_glyphlist.rs` contains the single-codepoint entries of Adobe Glyph List 2.0 from [adobe-type-tools/agl-aglfn](https://github.com/adobe-type-tools/agl-aglfn), commit `4036a9ca80a62f64f9de4f7321a9a045ad0ecfd6`.

Copyright 2002-2019 Adobe (http://www.adobe.com/).

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of Adobe nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
