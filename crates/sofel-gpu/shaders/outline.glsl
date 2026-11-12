/*
 * Copyright (C) 2026  Behdad Esfahbod
 * Copyright (C) 2017  Eric Lengyel
 *
 * The _hb_gpu_slug body below is based on the Slug algorithm by
 * Eric Lengyel: https://github.com/EricLengyel/Slug
 *
 *  This is part of HarfBuzz, a text shaping library.
 *
 * Permission is hereby granted, without written agreement and without
 * license or royalty fees, to use, copy, modify, and distribute this
 * software and its documentation for any purpose, provided that the
 * above copyright notice and the following two paragraphs appear in
 * all copies of this software.
 *
 * IN NO EVENT SHALL THE COPYRIGHT HOLDER BE LIABLE TO ANY PARTY FOR
 * DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES
 * ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS DOCUMENTATION, EVEN
 * IF THE COPYRIGHT HOLDER HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH
 * DAMAGE.
 *
 * THE COPYRIGHT HOLDER SPECIFICALLY DISCLAIMS ANY WARRANTIES, INCLUDING,
 * BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
 * FITNESS FOR A PARTICULAR PURPOSE.  THE SOFTWARE PROVIDED HEREUNDER IS
 * ON AN "AS IS" BASIS, AND THE COPYRIGHT HOLDER HAS NO OBLIGATION TO
 * PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
 */


/* Shared fragment-shader helpers for the hb-gpu renderers.
 *
 * Requires GLSL 3.30 or GLSL ES 3.00.
 *
 * For GLSL ES 3.00 / WebGL2, define HB_GPU_ATLAS_2D before
 * including this source.  The atlas is then a 2D isampler2D
 * texture of known width (set via hb_gpu_atlas_width uniform)
 * instead of an isamplerBuffer.
 *
 * Exports:
 *   hb_gpu_fetch (offset)                   atlas lookup
 *   _hb_gpu_slug (rc, pixelsPerEm, glyphLoc) MSAA-aware Slug
 *                                            coverage (caller supplies
 *                                            pixelsPerEm so it can be
 *                                            invoked from non-uniform
 *                                            control flow)
 *   hb_gpu_ppem (rc, glyphLoc)              pixels per em at fragment
 *   _hb_gpu_curve_counts (rc, glyphLoc)     debug: per-pixel curve counts
 *   hb_gpu_stem_darken (cov, brightness, ppem) thin-stroke contrast
 *                                            correction
 */


#ifndef HB_GPU_UNITS_PER_EM
#define HB_GPU_UNITS_PER_EM 4
#endif

#define HB_GPU_INV_UNITS float(1.0 / float(HB_GPU_UNITS_PER_EM))


#ifdef HB_GPU_ATLAS_2D
uniform highp isampler2D hb_gpu_atlas;
uniform int hb_gpu_atlas_width;
ivec4 hb_gpu_fetch (int offset)
{
  return texelFetch (hb_gpu_atlas,
		     ivec2 (offset % hb_gpu_atlas_width,
			    offset / hb_gpu_atlas_width), 0);
}
#else
uniform isamplerBuffer hb_gpu_atlas;
ivec4 hb_gpu_fetch (int offset)
{
  return texelFetch (hb_gpu_atlas, offset);
}
#endif


uint _hb_gpu_calc_root_code (float y1, float y2, float y3)
{
  uint i1 = floatBitsToUint (y1) >> 31U;
  uint i2 = floatBitsToUint (y2) >> 30U;
  uint i3 = floatBitsToUint (y3) >> 29U;

  uint shift = (i2 & 2U) | (i1 & ~2U);
  shift = (i3 & 4U) | (shift & ~4U);

  return (0x2E74U >> shift) & 0x0101U;
}

vec2 _hb_gpu_solve_horiz_poly (vec2 a, vec2 b, vec2 p1)
{
  float ra = 1.0 / a.y;
  float rb = 0.5 / b.y;

  float d = sqrt (max (b.y * b.y - a.y * p1.y, 0.0));
  float t1 = (b.y - d) * ra;
  float t2 = (b.y + d) * ra;

  if (a.y == 0.0)
    t1 = t2 = p1.y * rb;

  return vec2 ((a.x * t1 - b.x * 2.0) * t1 + p1.x,
	       (a.x * t2 - b.x * 2.0) * t2 + p1.x);
}

vec2 _hb_gpu_solve_vert_poly (vec2 a, vec2 b, vec2 p1)
{
  float ra = 1.0 / a.x;
  float rb = 0.5 / b.x;

  float d = sqrt (max (b.x * b.x - a.x * p1.x, 0.0));
  float t1 = (b.x - d) * ra;
  float t2 = (b.x + d) * ra;

  if (a.x == 0.0)
    t1 = t2 = p1.x * rb;

  return vec2 ((a.y * t1 - b.y * 2.0) * t1 + p1.y,
	       (a.y * t2 - b.y * 2.0) * t2 + p1.y);
}

float _hb_gpu_calc_coverage (float xcov, float ycov, float xwgt, float ywgt)
{
  float coverage = max (abs (xcov * xwgt + ycov * ywgt) /
			max (xwgt + ywgt, 1.0 / 65536.0),
			min (abs (xcov), abs (ycov)));

  return clamp (coverage, 0.0, 1.0);
}

/* Decoded glyph band info for a pixel position. */
struct _hb_gpu_glyph_info
{
  int glyphLoc;
  int bandBase;
  ivec2 bandIndex;
  int numHBands;
  int numVBands;
  vec2 scale;
};

_hb_gpu_glyph_info _hb_gpu_decode_glyph (vec2 renderCoord, uint glyphLoc_)
{
  _hb_gpu_glyph_info gi;
  gi.glyphLoc = int (glyphLoc_);

  ivec4 header0 = hb_gpu_fetch (gi.glyphLoc);
  ivec4 header1 = hb_gpu_fetch (gi.glyphLoc + 1);
  vec4 ext = vec4 (header0) * HB_GPU_INV_UNITS;
  gi.numHBands = header1.r;
  gi.numVBands = header1.g;
  gi.scale = vec2 (float (header1.b), float (header1.a));

  vec2 extSize = ext.zw - ext.xy;
  vec2 bandScale = vec2 (float (gi.numVBands), float (gi.numHBands)) / max (extSize, vec2 (1.0 / 65536.0));
  vec2 bandOffset = -ext.xy * bandScale;

  gi.bandIndex = clamp (ivec2 (renderCoord * bandScale + bandOffset),
			ivec2 (0, 0),
			ivec2 (gi.numVBands - 1, gi.numHBands - 1));

  gi.bandBase = gi.glyphLoc + 2;
  return gi;
}

/* Return pixels per em at this fragment. */
float hb_gpu_ppem (vec2 renderCoord, uint glyphLoc_)
{
  _hb_gpu_glyph_info gi = _hb_gpu_decode_glyph (renderCoord, glyphLoc_);
  vec2 emsPerPixel = fwidth (renderCoord);
  return min (gi.scale.x, gi.scale.y) /
	 max (emsPerPixel.x, emsPerPixel.y);
}

/* Return per-pixel curve counts: (horizontal, vertical). */
ivec2 _hb_gpu_curve_counts (vec2 renderCoord, uint glyphLoc_)
{
  _hb_gpu_glyph_info gi = _hb_gpu_decode_glyph (renderCoord, glyphLoc_);
  int hCount = hb_gpu_fetch (gi.bandBase + gi.bandIndex.y).r;
  int vCount = hb_gpu_fetch (gi.bandBase + gi.numHBands + gi.bandIndex.x).r;
  return ivec2 (hCount, vCount);
}

/* Single-sample coverage in [0, 1]. */
float _hb_gpu_slug_single (vec2 renderCoord, vec2 pixelsPerEm, uint glyphLoc_)
{

  _hb_gpu_glyph_info gi = _hb_gpu_decode_glyph (renderCoord, glyphLoc_);
  int glyphLoc = gi.glyphLoc;
  int bandBase = gi.bandBase;
  int numHBands = gi.numHBands;

  float xcov = 0.0;
  float xwgt = 0.0;

  ivec4 hbandData = hb_gpu_fetch (bandBase + gi.bandIndex.y);
  int hCurveCount = hbandData.r;
  /* Symmetric: choose rightward (desc) or leftward (asc) sort */
  float hSplit = float (hbandData.a) * HB_GPU_INV_UNITS;
  bool hLeftRay = (renderCoord.x < hSplit);
  int hDataOffset = (hLeftRay ? hbandData.b : hbandData.g) + 32768;

  for (int ci = 0; ci < hCurveCount; ci++)
  {
    int curveOffset = hb_gpu_fetch (glyphLoc + hDataOffset + ci).r + 32768;

    ivec4 raw12 = hb_gpu_fetch (glyphLoc + curveOffset);
    ivec4 raw3 = hb_gpu_fetch (glyphLoc + curveOffset + 1);

    vec4 q12 = vec4 (raw12) * HB_GPU_INV_UNITS;
    vec2 q3 = vec2 (raw3.rg) * HB_GPU_INV_UNITS;

    vec4 p12 = q12 - vec4 (renderCoord, renderCoord);
    vec2 p3 = q3 - renderCoord;

    if (hLeftRay) {
      if (min (min (p12.x, p12.z), p3.x) * pixelsPerEm.x > 0.5) break;
    } else {
      if (max (max (p12.x, p12.z), p3.x) * pixelsPerEm.x < -0.5) break;
    }

    uint code = _hb_gpu_calc_root_code (p12.y, p12.w, p3.y);
    if (code != 0U)
    {
      vec2 a = q12.xy - q12.zw * 2.0 + q3;
      vec2 b = q12.xy - q12.zw;
      vec2 r = _hb_gpu_solve_horiz_poly (a, b, p12.xy) * pixelsPerEm.x;
      /* For leftward ray: saturate(0.5 - r) counts coverage from the left */
      vec2 cov = hLeftRay ? clamp (vec2 (0.5) - r, 0.0, 1.0)
			  : clamp (r + vec2 (0.5), 0.0, 1.0);

      if ((code & 1U) != 0U)
      {
	xcov += cov.x;
	xwgt = max (xwgt, clamp (1.0 - abs (r.x) * 2.0, 0.0, 1.0));
      }

      if (code > 1U)
      {
	xcov -= cov.y;
	xwgt = max (xwgt, clamp (1.0 - abs (r.y) * 2.0, 0.0, 1.0));
      }
    }
  }

  /* Crossings over a closed contour sum to zero, so a leftward ray
   * returns the negative of what a rightward ray would; flip it back
   * so that xcov and ycov keep a common sign convention. */
  if (hLeftRay)
    xcov = -xcov;

  float ycov = 0.0;
  float ywgt = 0.0;

  ivec4 vbandData = hb_gpu_fetch (bandBase + numHBands + gi.bandIndex.x);
  int vCurveCount = vbandData.r;
  float vSplit = float (vbandData.a) * HB_GPU_INV_UNITS;
  bool vLeftRay = (renderCoord.y < vSplit);
  int vDataOffset = (vLeftRay ? vbandData.b : vbandData.g) + 32768;

  for (int ci = 0; ci < vCurveCount; ci++)
  {
    int curveOffset = hb_gpu_fetch (glyphLoc + vDataOffset + ci).r + 32768;

    ivec4 raw12 = hb_gpu_fetch (glyphLoc + curveOffset);
    ivec4 raw3 = hb_gpu_fetch (glyphLoc + curveOffset + 1);

    vec4 q12 = vec4 (raw12) * HB_GPU_INV_UNITS;
    vec2 q3 = vec2 (raw3.rg) * HB_GPU_INV_UNITS;

    vec4 p12 = q12 - vec4 (renderCoord, renderCoord);
    vec2 p3 = q3 - renderCoord;

    if (vLeftRay) {
      if (min (min (p12.y, p12.w), p3.y) * pixelsPerEm.y > 0.5) break;
    } else {
      if (max (max (p12.y, p12.w), p3.y) * pixelsPerEm.y < -0.5) break;
    }

    uint code = _hb_gpu_calc_root_code (p12.x, p12.z, p3.x);
    if (code != 0U)
    {
      vec2 a = q12.xy - q12.zw * 2.0 + q3;
      vec2 b = q12.xy - q12.zw;
      vec2 r = _hb_gpu_solve_vert_poly (a, b, p12.xy) * pixelsPerEm.y;
      vec2 cov = vLeftRay ? clamp (vec2 (0.5) - r, 0.0, 1.0)
			  : clamp (r + vec2 (0.5), 0.0, 1.0);

      if ((code & 1U) != 0U)
      {
	ycov -= cov.x;
	ywgt = max (ywgt, clamp (1.0 - abs (r.x) * 2.0, 0.0, 1.0));
      }

      if (code > 1U)
      {
	ycov += cov.y;
	ywgt = max (ywgt, clamp (1.0 - abs (r.y) * 2.0, 0.0, 1.0));
      }
    }
  }

  /* Ditto, for the vertical ray. */
  if (vLeftRay)
    ycov = -ycov;

  return _hb_gpu_calc_coverage (xcov, ycov, xwgt, ywgt);
}

/* MSAA-aware Slug coverage.  Caller supplies pixelsPerEm so this
 * function can be invoked from non-uniform control flow (for
 * example inside an op-stream branch in hb-gpu-paint-fragment.wgsl,
 * where WGSL would otherwise reject an fwidth call). */
float _hb_gpu_slug (vec2 renderCoord, vec2 pixelsPerEm, uint glyphLoc_)
{
  float c = _hb_gpu_slug_single (renderCoord, pixelsPerEm, glyphLoc_);

#ifndef HB_GPU_NO_MSAA
  float ppem = hb_gpu_ppem (renderCoord, glyphLoc_);

  if (ppem < 16.0)
  {
    vec2 emsPerPixel = 1.0 / pixelsPerEm;
    vec2 d = emsPerPixel * (1.0 / 3.0);
    float msaa = 0.25 *
      (_hb_gpu_slug_single (renderCoord + vec2 (-d.x, -d.y), pixelsPerEm, glyphLoc_) +
       _hb_gpu_slug_single (renderCoord + vec2 ( d.x, -d.y), pixelsPerEm, glyphLoc_) +
       _hb_gpu_slug_single (renderCoord + vec2 (-d.x,  d.y), pixelsPerEm, glyphLoc_) +
       _hb_gpu_slug_single (renderCoord + vec2 ( d.x,  d.y), pixelsPerEm, glyphLoc_));

    c = mix (c, msaa, smoothstep (16.0, 8.0, ppem));
  }
#endif

  return c;
}

/* Stem darkening for small sizes.
 *
 * coverage:    output of hb_gpu_draw / _hb_gpu_slug
 * brightness:  foreground brightness in [0, 1]
 * ppem:        pixels per em at this fragment
 */
float hb_gpu_stem_darken (float coverage, float brightness, float ppem)
{
  return pow (coverage,
	      mix (pow (2.0, brightness - 0.5), 1.0,
		   smoothstep (8.0, 48.0, ppem)));
}
/*
 * Copyright (C) 2026  Behdad Esfahbod
 *
 *  This is part of HarfBuzz, a text shaping library.
 *
 * Permission is hereby granted, without written agreement and without
 * license or royalty fees, to use, copy, modify, and distribute this
 * software and its documentation for any purpose, provided that the
 * above copyright notice and the following two paragraphs appear in
 * all copies of this software.
 *
 * IN NO EVENT SHALL THE COPYRIGHT HOLDER BE LIABLE TO ANY PARTY FOR
 * DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES
 * ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS DOCUMENTATION, EVEN
 * IF THE COPYRIGHT HOLDER HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH
 * DAMAGE.
 *
 * THE COPYRIGHT HOLDER SPECIFICALLY DISCLAIMS ANY WARRANTIES, INCLUDING,
 * BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
 * FITNESS FOR A PARTICULAR PURPOSE.  THE SOFTWARE PROVIDED HEREUNDER IS
 * ON AN "AS IS" BASIS, AND THE COPYRIGHT HOLDER HAS NO OBLIGATION TO
 * PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
 */


/* Draw-renderer fragment shader entry.  The heavy lifting (Slug
 * coverage, MSAA, ppem, stem darkening) lives in the shared
 * hb-gpu-fragment.glsl that must be prepended to this source;
 * this file only adds the thin hb_gpu_draw() wrapper that lifts
 * pixelsPerEm out of fwidth() at uniform control flow before
 * calling the shared _hb_gpu_slug(). */


float hb_gpu_draw (vec2 renderCoord, uint glyphLoc_)
{
  vec2 pixelsPerEm = 1.0 / fwidth (renderCoord);
  return _hb_gpu_slug (renderCoord, pixelsPerEm, glyphLoc_);
}

/*
 * Copyright (C) 2026  Behdad Esfahbod
 *
 *  This is part of HarfBuzz, a text shaping library.
 *
 * Permission is hereby granted, without written agreement and without
 * license or royalty fees, to use, copy, modify, and distribute this
 * software and its documentation for any purpose, provided that the
 * above copyright notice and the following two paragraphs appear in
 * all copies of this software.
 *
 * IN NO EVENT SHALL THE COPYRIGHT HOLDER BE LIABLE TO ANY PARTY FOR
 * DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES
 * ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS DOCUMENTATION, EVEN
 * IF THE COPYRIGHT HOLDER HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH
 * DAMAGE.
 *
 * THE COPYRIGHT HOLDER SPECIFICALLY DISCLAIMS ANY WARRANTIES, INCLUDING,
 * BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
 * FITNESS FOR A PARTICULAR PURPOSE.  THE SOFTWARE PROVIDED HEREUNDER IS
 * ON AN "AS IS" BASIS, AND THE COPYRIGHT HOLDER HAS NO OBLIGATION TO
 * PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
 */


/* Paint-renderer fragment shader.
 *
 * Assumes the shared fragment helpers (hb-gpu-fragment.glsl) and
 * the draw-renderer fragment helpers (hb-gpu-draw-fragment.glsl)
 * are prepended to this source.  The draw helper provides
 * hb_gpu_draw() which this interpreter calls to compute clip-glyph
 * coverage.
 */


/* Fetch the i'th stop of a gradient color line starting at @stops_base
 * (2 texels per stop).  Resolves is_foreground to @foreground. */
vec4 _hb_gpu_stop_color (int stops_base, int i, vec4 foreground, out float offset)
{
  ivec4 a = hb_gpu_fetch (stops_base + i * 2);
  offset = float (a.r) / 32767.0;
  ivec4 b = hb_gpu_fetch (stops_base + i * 2 + 1);
  if ((a.g & 1) != 0)
    return vec4 (foreground.rgb, foreground.a * (float (b.a) / 32767.0));
  return vec4 (b) / 32767.0;
}

/* Apply the color-line extend mode to a projected `t` value. */
float _hb_gpu_extend_t (float t, int extend)
{
  if (extend == 1) {           /* HB_PAINT_EXTEND_REPEAT */
    return t - floor (t);
  } else if (extend == 2) {    /* HB_PAINT_EXTEND_REFLECT */
    float u = t - 2.0 * floor (t * 0.5);
    return u > 1.0 ? 2.0 - u : u;
  }
  return clamp (t, 0.0, 1.0);  /* PAD (default) */
}

/* Walk stops starting at @stops_base and return the sampled color
 * at @t.  Same logic reused by all gradient subtypes. */
vec4 _hb_gpu_eval_stops (int stops_base, int stop_count, float t, vec4 foreground)
{
  float off_prev;
  vec4 col_prev = _hb_gpu_stop_color (stops_base, 0, foreground, off_prev);
  if (t <= off_prev)
    return col_prev;
  for (int i = 1; i < stop_count; i++)
  {
    float off;
    vec4 col = _hb_gpu_stop_color (stops_base, i, foreground, off);
    if (t <= off)
    {
      float span = off - off_prev;
      float f = span > 1e-6 ? (t - off_prev) / span : 0.0;
      /* Interpolate in premultiplied space per OpenType COLR spec. */
      vec4 p0 = vec4 (col_prev.rgb * col_prev.a, col_prev.a);
      vec4 p1 = vec4 (col.rgb * col.a, col.a);
      vec4 pm = mix (p0, p1, f);
      return pm.a > 1e-6 ? vec4 (pm.rgb / pm.a, pm.a) : vec4 (0.0);
    }
    col_prev = col;
    off_prev = off;
  }
  return col_prev;
}

/* Apply the stored 2x2 M^-1 (row-major i16 Q10) to @v.  Scaling
 * renderCoord deltas back into canonical gradient space. */
vec2 _hb_gpu_apply_minv (ivec4 m, vec2 v)
{
  vec4 mf = vec4 (m) * (1.0 / 1024.0);
  return vec2 (mf.x * v.x + mf.y * v.y,
	       mf.z * v.x + mf.w * v.y);
}

/* Sample a linear gradient whose param blob starts at @grad_base:
 *   texel 0: (p0_rendered.x, p0_rendered.y, d_canonical.x, d_canonical.y)
 *   texel 1: L^-1 as i16 Q10 (row-major)
 *   texels 2..: stops (2 texels each)
 * Evaluate t in untransformed space. */
vec4 _hb_gpu_sample_linear (vec2 renderCoord, int grad_base,
			    int stop_count, int extend, vec4 foreground)
{
  ivec4 t0 = hb_gpu_fetch (grad_base);
  ivec4 m  = hb_gpu_fetch (grad_base + 1);
  vec2 p0_r = vec2 (float (t0.r), float (t0.g));
  vec2 d    = vec2 (float (t0.b), float (t0.a));
  float denom = dot (d, d);
  if (denom < 1e-6) return vec4 (0.0);
  vec2 p = _hb_gpu_apply_minv (m, renderCoord - p0_r);
  float t = dot (p, d) / denom;
  t = _hb_gpu_extend_t (t, extend);

  return _hb_gpu_eval_stops (grad_base + 2, stop_count, t, foreground);
}

/* Sample a two-circle radial gradient whose param blob starts at
 * @grad_base:
 *   texel 0: (c0_rendered.x, c0_rendered.y, d_canonical.x, d_canonical.y)
 *     d = c1 - c0 in untransformed space
 *   texel 1: (r0, r1, _, _) in untransformed font units
 *   texel 2: L^-1 as i16 Q10 (row-major)
 *   texels 3..: stops (2 texels each)
 * Solves |p - t*cd|^2 = (r0 + t*(r1-r0))^2 with p in untransformed
 * space, so non-uniform scale / shear on the transform becomes a
 * proper ellipse-in-rendered-space instead of a scalar-fudge. */
vec4 _hb_gpu_sample_radial (vec2 renderCoord, int grad_base,
			    int stop_count, int extend, vec4 foreground)
{
  ivec4 t0 = hb_gpu_fetch (grad_base);
  ivec4 t1 = hb_gpu_fetch (grad_base + 1);
  ivec4 m  = hb_gpu_fetch (grad_base + 2);
  vec2 c0_r = vec2 (float (t0.r), float (t0.g));
  vec2 cd   = vec2 (float (t0.b), float (t0.a));
  float r0 = float (t1.r);
  float r1 = float (t1.g);

  float dr = r1 - r0;
  vec2 p  = _hb_gpu_apply_minv (m, renderCoord - c0_r);

  float A = dot (cd, cd) - dr * dr;
  float B = -2.0 * (dot (p, cd) + r0 * dr);
  float C = dot (p, p) - r0 * r0;

  float t;
  if (abs (A) > 1e-6)
  {
    float disc = B * B - 4.0 * A * C;
    if (disc < 0.0) return vec4 (0.0);
    float sq = sqrt (disc);
    /* Prefer the larger root; fall back to the smaller if the
     * larger gives a negative interpolated radius. */
    float t1 = (-B + sq) / (2.0 * A);
    float t2 = (-B - sq) / (2.0 * A);
    t = (r0 + t1 * dr >= 0.0) ? t1 : t2;
  }
  else
  {
    if (abs (B) < 1e-6) return vec4 (0.0);
    t = -C / B;
  }

  t = _hb_gpu_extend_t (t, extend);
  return _hb_gpu_eval_stops (grad_base + 3, stop_count, t, foreground);
}

/* Sample a sweep gradient whose param blob starts at @grad_base:
 *   texel 0: (center_rendered.x, center_rendered.y, start_q14, end_q14)
 *            start/end are Q14 fractions of pi in untransformed space
 *   texel 1: L^-1 as i16 Q10 (row-major)
 *   texels 2..: stops (2 texels each) */
vec4 _hb_gpu_sample_sweep (vec2 renderCoord, int grad_base,
			   int stop_count, int extend, vec4 foreground)
{
  ivec4 t0 = hb_gpu_fetch (grad_base);
  ivec4 m  = hb_gpu_fetch (grad_base + 1);
  vec2 c_r = vec2 (float (t0.r), float (t0.g));
  float a0 = float (t0.b) / 16384.0;  /* fraction of pi */
  float a1 = float (t0.a) / 16384.0;
  float span = a1 - a0;
  if (abs (span) < 1e-6) return vec4 (0.0);

  vec2 p = _hb_gpu_apply_minv (m, renderCoord - c_r);
  /* atan2 returns (-pi, pi]; normalize to [0, 2) fractions of pi. */
  float ang = atan (p.y, p.x) / 3.14159265358979;
  if (ang < 0.0) ang += 2.0;
  float t = (ang - a0) / span;
  t = _hb_gpu_extend_t (t, extend);
  return _hb_gpu_eval_stops (grad_base + 2, stop_count, t, foreground);
}

/* Composite two premultiplied RGBA layers using one of the COLRv1
 * compositing modes.  Unsupported modes fall back to SRC_OVER.
 * Values match hb_paint_composite_mode_t. */
vec4 _hb_gpu_composite (vec4 src, vec4 dst, int mode)
{
  vec4 r = src + dst * (1.0 - src.a);  /* SRC_OVER default */

  /* Approximate unsupported COLRv1 modes with the nearest Porter-Duff
   * mode we do implement.  Better a recognizable rendering than a
   * silent SRC_OVER fallback.  DIFFERENCE / EXCLUSION / HSL_* are
   * not similar enough to anything we have, so they still fall
   * through to SRC_OVER below. */
  if      (mode == 14 || mode == 18 || mode == 19) mode = 23; /* OVERLAY / COLOR_BURN / HARD_LIGHT -> MULTIPLY */
  else if (mode == 17 || mode == 20)               mode = 13; /* COLOR_DODGE / SOFT_LIGHT -> SCREEN */

  if      (mode == 0)  r = vec4 (0.0);                       /* CLEAR */
  else if (mode == 1)  r = src;                              /* SRC */
  else if (mode == 2)  r = dst;                              /* DST */
  else if (mode == 4)  r = dst + src * (1.0 - dst.a);        /* DST_OVER */
  else if (mode == 5)  r = src * dst.a;                      /* SRC_IN */
  else if (mode == 6)  r = dst * src.a;                      /* DST_IN */
  else if (mode == 7)  r = src * (1.0 - dst.a);              /* SRC_OUT */
  else if (mode == 8)  r = dst * (1.0 - src.a);              /* DST_OUT */
  else if (mode == 9)                                        /* SRC_ATOP */
    r = src * dst.a + dst * (1.0 - src.a);
  else if (mode == 10)                                       /* DST_ATOP */
    r = dst * src.a + src * (1.0 - dst.a);
  else if (mode == 11)                                       /* XOR */
    r = src * (1.0 - dst.a) + dst * (1.0 - src.a);
  else if (mode == 12)                                       /* PLUS */
    r = min (src + dst, vec4 (1.0));
  else if (mode == 13) {                                     /* SCREEN (premul) */
    r.rgb = src.rgb + dst.rgb - src.rgb * dst.rgb;
    r.a = src.a + dst.a - src.a * dst.a;
  }
  else if (mode == 15) {                                     /* DARKEN */
    r.rgb = min (src.rgb * dst.a, dst.rgb * src.a)
          + src.rgb * (1.0 - dst.a) + dst.rgb * (1.0 - src.a);
    r.a = src.a + dst.a - src.a * dst.a;
  }
  else if (mode == 16) {                                     /* LIGHTEN */
    r.rgb = max (src.rgb * dst.a, dst.rgb * src.a)
          + src.rgb * (1.0 - dst.a) + dst.rgb * (1.0 - src.a);
    r.a = src.a + dst.a - src.a * dst.a;
  }
  else if (mode == 23) {                                     /* MULTIPLY (premul) */
    r.rgb = src.rgb * (1.0 - dst.a) + dst.rgb * (1.0 - src.a)
          + src.rgb * dst.rgb;
    r.a = src.a + dst.a - src.a * dst.a;
  }
  /* SRC_OVER (3) and DIFFERENCE / EXCLUSION / HSL_* (21, 22, 24-27)
   * fall through to the SRC_OVER default. */

  return r;
}

/* Wrap _hb_gpu_slug with a sub-glyph extents bail-out.  Many
 * paint layers cover a small region of the outer glyph quad; for
 * fragments outside the layer's bbox (with an AA + MSAA-spread
 * margin) the slug coverage is exactly 0, so we can skip the
 * band/curve walk entirely. */
float _hb_gpu_slug_clipped (vec2 renderCoord, vec2 pixelsPerEm, uint glyphLoc_)
{
  ivec4 header0 = hb_gpu_fetch (int (glyphLoc_));
  vec4 ext = vec4 (header0) * HB_GPU_INV_UNITS;
  vec2 margin = 2.0 / pixelsPerEm;
  if (any (lessThan    (renderCoord, ext.xy - margin)) ||
      any (greaterThan (renderCoord, ext.zw + margin)))
    return 0.0;
  return _hb_gpu_slug (renderCoord, pixelsPerEm, glyphLoc_);
}

/* Combine slug coverages from all clip outlines on the current
 * layer.  Factored out of LAYER_SOLID and LAYER_GRADIENT so the
 * shader has one set of inlined slug walks instead of two.  flags
 * bits: 0x100 = HAS_CLIP2; 0x200 = HAS_CLIP3 (HAS_CLIP3 implies
 * HAS_CLIP2). */
float _hb_gpu_layer_coverage (vec2 renderCoord, vec2 pixelsPerEm,
			      int base, int flags,
			      int clip1_payload, int clip2_payload, int clip3_payload)
{
  float cov = _hb_gpu_slug_clipped (renderCoord, pixelsPerEm,
				    uint (base + clip1_payload));
  if ((flags & 0x100) != 0)
  {
    cov *= _hb_gpu_slug_clipped (renderCoord, pixelsPerEm,
				 uint (base + clip2_payload));
    if ((flags & 0x200) != 0)
      cov *= _hb_gpu_slug_clipped (renderCoord, pixelsPerEm,
				   uint (base + clip3_payload));
  }
  return cov;
}

/* Walks the paint blob's flat op stream and returns a
 * premultiplied RGBA coverage value for the current fragment.
 *
 * glyphLoc: atlas texel offset of the paint-blob header.
 * foreground: caller-supplied foreground color, used when an op
 *             sets the is_foreground flag.
 */
#define HB_GPU_PAINT_GROUP_DEPTH 4

vec4 hb_gpu_paint (vec2 renderCoord, uint glyphLoc, vec4 foreground,
		   out float coverage)
{
  /* fwidth once, at uniform control flow: every per-layer
   * coverage sample below uses this pre-computed pixelsPerEm via
   * _hb_gpu_slug. */
  vec2 pixelsPerEm = 1.0 / fwidth (renderCoord);

  int base    = int (glyphLoc);
  ivec4 h0    = hb_gpu_fetch (base);      /* (num_ops, _, _, _) */
  ivec4 h2    = hb_gpu_fetch (base + 2);  /* (ops_offset, _, _, _) */
  int num_ops = h0.r;
  int cursor  = base + h2.r;

  vec4 acc = vec4 (0.0);
  vec4 group_stack[HB_GPU_PAINT_GROUP_DEPTH];
  int sp = 0;
  coverage = 0.0;

  for (int i = 0; i < num_ops; i++)
  {
    ivec4 op    = hb_gpu_fetch (cursor);
    int op_type = op.r;
    int aux     = op.g;
    int payload = (op.b << 16) | (op.a & 0xffff);

    if (op_type == 0)  /* LAYER_SOLID */
    {
      /* texel 1: (clip2_hi, clip2_lo, clip3_hi, clip3_lo) -- valid
       *           per HAS_CLIP2 / HAS_CLIP3 flag bits.
       * texel 2: RGBA as signed Q15. */
      ivec4 op2 = hb_gpu_fetch (cursor + 1);
      int clip2_payload = (op2.r << 16) | (op2.g & 0xffff);
      int clip3_payload = (op2.b << 16) | (op2.a & 0xffff);
      ivec4 ct = hb_gpu_fetch (cursor + 2);
      vec4 col = ((aux & 1) != 0)
	       ? vec4 (foreground.rgb, foreground.a * (float (ct.a) / 32767.0))
	       : vec4 (ct) / 32767.0;

      float cov = _hb_gpu_layer_coverage (renderCoord, pixelsPerEm,
					  base, aux,
					  payload, clip2_payload, clip3_payload);
      coverage = max (coverage, cov);
      vec4 src = vec4 (col.rgb * col.a, col.a) * cov;
      acc = src + acc * (1.0 - src.a);

      cursor += 3;
    }
    else if (op_type == 1)  /* LAYER_GRADIENT */
    {
      /* texel 1: (clip2_hi, clip2_lo, clip3_hi, clip3_lo) -- valid
       *           per HAS_CLIP2 / HAS_CLIP3 flag bits.
       * texel 2: (grad_payload_hi, grad_payload_lo, extend, stop_count) */
      ivec4 op2 = hb_gpu_fetch (cursor + 1);
      int clip2_payload = (op2.r << 16) | (op2.g & 0xffff);
      int clip3_payload = (op2.b << 16) | (op2.a & 0xffff);
      ivec4 op3 = hb_gpu_fetch (cursor + 2);
      int grad_payload = (op3.r << 16) | (op3.g & 0xffff);
      int extend       = op3.b;
      int stop_count   = op3.a;
      int subtype      = aux & 0xff;

      vec4 col = vec4 (0.0);
      if (subtype == 0)       /* linear */
        col = _hb_gpu_sample_linear (renderCoord,
                                     base + grad_payload,
                                     stop_count, extend, foreground);
      else if (subtype == 1)  /* radial */
        col = _hb_gpu_sample_radial (renderCoord,
                                     base + grad_payload,
                                     stop_count, extend, foreground);
      else if (subtype == 2)  /* sweep */
        col = _hb_gpu_sample_sweep  (renderCoord,
                                     base + grad_payload,
                                     stop_count, extend, foreground);

      float cov = _hb_gpu_layer_coverage (renderCoord, pixelsPerEm,
					  base, aux,
					  payload, clip2_payload, clip3_payload);
      coverage = max (coverage, cov);
      vec4 src = vec4 (col.rgb * col.a, col.a) * cov;
      acc = src + acc * (1.0 - src.a);

      cursor += 3;
    }
    else if (op_type == 2)  /* PUSH_GROUP */
    {
      if (sp < HB_GPU_PAINT_GROUP_DEPTH) {
        group_stack[sp] = acc;
        sp++;
      }
      acc = vec4 (0.0);
      cursor += 1;
    }
    else if (op_type == 3)  /* POP_GROUP */
    {
      if (sp > 0) {
        sp--;
        vec4 src = acc;
        vec4 dst = group_stack[sp];
        acc = _hb_gpu_composite (src, dst, aux);
      }
      cursor += 1;
    }
    else
    {
      break;
    }
  }

  return acc;
}
