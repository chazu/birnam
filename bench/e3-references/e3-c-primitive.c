// Design 74 E3 optimized primitive-integer reference: the same arithmetic in a
// plain C loop compiled -O2. The result is consumed (printed) so the loop
// cannot be eliminated; the step function is opaque to the optimizer.
#include <mach/mach_time.h>
#include <stdio.h>
#include <stdlib.h>
__attribute__((noinline)) long e3_step(long index) { return 7 + index; }
static double micros(uint64_t delta) {
  static mach_timebase_info_data_t info;
  if (info.denom == 0) mach_timebase_info(&info);
  return (double)delta * info.numer / info.denom / 1000.0;
}
int main(int argc, char **argv) {
  long count = argc > 1 ? strtol(argv[1], NULL, 10) : 1024;
  volatile long sink = 0;
  printf("e3-ready\n");
  uint64_t t0 = mach_absolute_time();
  for (long i = 1; i <= count; i++) sink += 0;
  uint64_t t1 = mach_absolute_time();
  long checksum = 0;
  for (long i = 1; i <= count; i++) checksum += e3_step(i);
  uint64_t t2 = mach_absolute_time();
  printf("empty-us %.0f\nstep-us %.0f\nempty-answer %d\nchecksum %ld\n", micros(t1 - t0), micros(t2 - t1), 7, checksum + (long)sink);
  return 0;
}
