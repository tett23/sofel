void sincosf(float x, float *s, float *c);
float sinf(float x) { float s, c; sincosf(x, &s, &c); return s; }
float cosf(float x) { float s, c; sincosf(x, &s, &c); return c; }
