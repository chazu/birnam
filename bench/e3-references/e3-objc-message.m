// Design 74 E3 Objective-C object/message reference. Per iteration the loop
// object sends -body: to itself, which sends -step: to a receiver object and
// -add: to an accumulator object: three ordinary objc_msgSend dispatches per
// iteration, no manually cached IMPs, no inlining across the message sends.
// Representation differences from Birnam are stated in the record: longs
// stay unboxed here (BOXED=1 builds box every step result in NSNumber), the
// loop is a C for-loop rather than tail sends, and there is no frame machine.
#import <Foundation/Foundation.h>
#include <mach/mach_time.h>
#include <stdio.h>
#include <stdlib.h>

@interface E3Receiver : NSObject
- (long)step:(long)index;
@end
@implementation E3Receiver
- (long)step:(long)index { return 7 + index; }
@end

@interface E3Accumulator : NSObject { long total; }
- (void)add:(long)value;
#if BOXED
- (void)addBoxed:(NSNumber *)value;
#endif
- (long)total;
@end
@implementation E3Accumulator
- (void)add:(long)value { total += value; }
#if BOXED
- (void)addBoxed:(NSNumber *)value { total += [value longValue]; }
#endif
- (long)total { return total; }
@end

@interface E3Loop : NSObject { E3Receiver *receiver; E3Accumulator *acc; }
- (instancetype)initWithReceiver:(E3Receiver *)r accumulator:(E3Accumulator *)a;
- (long)runEmpty:(long)count;
- (long)runStep:(long)count;
- (void)emptyBody:(long)index;
- (void)stepBody:(long)index;
@end
@implementation E3Loop
- (instancetype)initWithReceiver:(E3Receiver *)r accumulator:(E3Accumulator *)a {
  self = [super init]; receiver = r; acc = a; return self;
}
- (void)emptyBody:(long)index { (void)index; }
- (void)stepBody:(long)index {
#if BOXED
  NSNumber *value = [NSNumber numberWithLong:[receiver step:index]];
  [acc addBoxed:value];
#else
  [acc add:[receiver step:index]];
#endif
}
- (long)runEmpty:(long)count { for (long i = 1; i <= count; i++) [self emptyBody:i]; return 7; }
- (long)runStep:(long)count { for (long i = 1; i <= count; i++) [self stepBody:i]; return [acc total]; }
@end

static double micros(uint64_t delta) {
  static mach_timebase_info_data_t info;
  if (info.denom == 0) mach_timebase_info(&info);
  return (double)delta * info.numer / info.denom / 1000.0;
}

int main(int argc, char **argv) {
  long count = argc > 1 ? strtol(argv[1], NULL, 10) : 1024;
  @autoreleasepool {
    E3Receiver *r = [E3Receiver new];
    E3Accumulator *a = [E3Accumulator new];
    E3Loop *loop = [[E3Loop alloc] initWithReceiver:r accumulator:a];
    printf("e3-ready\n");
    uint64_t t0 = mach_absolute_time();
    long emptyAnswer = [loop runEmpty:count];
    uint64_t t1 = mach_absolute_time();
    long checksum = [loop runStep:count];
    uint64_t t2 = mach_absolute_time();
    printf("empty-us %.0f\nstep-us %.0f\nempty-answer %ld\nchecksum %ld\n",
           micros(t1 - t0), micros(t2 - t1), emptyAnswer, checksum);
  }
  return 0;
}
