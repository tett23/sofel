/*
 * FreeType's cff_random (src/psaux/psobjs.c), which psaux.c compiles as a
 * static function, made callable by ft-dump --calc (ADR 0082). The file
 * compiles psobjs.c again in its own translation unit, the way psaux.c
 * does.
 */

#define FT_MAKE_OPTION_SINGLE_OBJECT
#include "psconv.c"
#include "psobjs.c"

FT_UInt32 ft_dump_cff_random(FT_UInt32 r)
{
  return cff_random(r);
}
