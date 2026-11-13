/* ADR 0027: one design coordinate, in the axis order of each face. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static void print_sample_axis(uint32_t *state, float minimum, float maximum)
{
  *state = *state * 1103515245u + 12345u;
  float fraction = (float)((*state >> 16) & 0x7fffu) / 32767.0f;
  volatile float displacement = (maximum - minimum) * fraction;
  float value = minimum + displacement;
  uint32_t bits;
  memcpy(&bits, &value, sizeof(bits));
  printf(" %08x", (unsigned)bits);
}
