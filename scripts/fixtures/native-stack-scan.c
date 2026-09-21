#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/native-stack-v4/native_stack_v4.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

typedef void (*Scan)(const TraceLiveSinkV1 *,uintptr_t,uintptr_t);
static Scan scan;
static uintptr_t values[32],words[]={0,7,7,UINTPTR_MAX,UINTPTR_MAX/2+1,19};
static size_t count;
static int mutate,reentered,reject;
static int emit(void *context,uintptr_t value) {
    assert(context==values);assert(count<32);values[count++]=value;
    if(reject) { printf("candidate %zu\n",(size_t)value);fflush(stdout);return -77; }
    if(mutate && !reentered) {
        reentered=1;words[2]=99;
        uintptr_t nested[]={41,43};TraceLiveSinkV1 sink={values,emit};
        scan(&sink,(uintptr_t)nested,(uintptr_t)nested+sizeof(nested));
    }
    return 0;
}
#ifndef NATIVE_SCAN_BASELINE
static NativeStackCaptureV1 capture;
static uintptr_t captured_start;
static int captured_calls;
static void captured(void *context,uintptr_t start,uintptr_t bottom) {
    assert(start && start<=bottom && bottom-start<=((uintptr_t)128<<20));
    uintptr_t local=0;assert((uintptr_t)&local<start);
    assert(bottom==(uintptr_t)context);
    captured_start=start;captured_calls++;
    /* The first 192 bytes are the actual Darwin arm64 register save area. */
    const uintptr_t *registers=(const uintptr_t *)start;
    int found=0;
    for(size_t i=0;i<24;i++)if(registers[i]==UINT64_C(0x13579bdf2468ace0))found=1;
    assert(found);
}
__attribute__((noinline)) static void capture_register(uintptr_t bottom) {
#if defined(__aarch64__) && defined(__APPLE__)
    register uintptr_t root __asm__("x19")=UINT64_C(0x13579bdf2468ace0);
    __asm__ volatile("" : "+r"(root));
    assert(capture(bottom,(void *)bottom,captured)==1);
    __asm__ volatile("" : "+r"(root));
    assert(root==UINT64_C(0x13579bdf2468ace0));
#else
    (void)bottom;
#endif
}
static void nested(void *context,uintptr_t start,uintptr_t bottom) {
    assert(start<captured_start && bottom==(uintptr_t)context);captured_calls++;
}
static void nesting(void *context,uintptr_t start,uintptr_t bottom) {
    captured_start=start;captured_calls++;
    assert(capture(bottom,context,nested)==1);
}
static void capture_checks(const NativeStackFactoryV4 *api) {
    capture=api->capture;assert(capture);
    Dl_info owner;assert(dladdr((void *)capture,&owner) && strstr(owner.dli_fname,"libbirnam_native_stack.dylib"));
    uintptr_t anchor=0;
    capture_register((uintptr_t)&anchor);assert(captured_calls==1);
    assert(!capture(0,NULL,captured));assert(!capture(1,NULL,captured));
    assert(!capture(UINTPTR_MAX,NULL,captured));assert(!capture((uintptr_t)&anchor,NULL,NULL));
    assert(captured_calls==1);
    assert(capture((uintptr_t)&anchor,&anchor,nesting)==1);assert(captured_calls==3);
    puts("native-stack-capture: native register ownership, shallow frame, interval rejection and nested capture pass");
}
#endif
int main(int argc,char **argv) {
    assert(argc==3);void *image=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(image);
#ifdef NATIVE_SCAN_BASELINE
    scan=(Scan)dlsym(image,"birnam_test_scan");assert(scan);
#else
    NativeStackFactoryProviderV4 provide=(NativeStackFactoryProviderV4)dlsym(image,"birnam_native_stack_factory_v4");assert(provide);
    const NativeStackFactoryV4 *api=provide();assert(api && api->version==4 && api->byte_size==sizeof(*api));scan=api->scan;assert(scan);
    Dl_info owner;assert(dladdr((void *)scan,&owner) && strstr(owner.dli_fname,"libbirnam_native_stack.dylib"));
#endif
    TraceLiveSinkV1 sink={values,emit};
    if(!strcmp(argv[2],"reject")) { reject=1;scan(&sink,(uintptr_t)words,(uintptr_t)words+sizeof(words));assert(0); }
    for(size_t low=0;low<=sizeof(words);low++)for(size_t high=0;high<=sizeof(words);high++) {
        count=0;scan(&sink,(uintptr_t)words+low,(uintptr_t)words+high);
        size_t expected=0;
        for(size_t index=0;index<sizeof(words)/sizeof(*words);index++)
            if(index*sizeof(*words)>=low && (index+1)*sizeof(*words)<=high)assert(expected<count && values[expected++]==words[index]);
        assert(expected==count);
    }
    count=0;scan(&sink,0,0);scan(&sink,UINTPTR_MAX-sizeof(uintptr_t)+1,UINTPTR_MAX);assert(!count);
    mutate=1;scan(&sink,(uintptr_t)words,(uintptr_t)words+sizeof(words));
    const uintptr_t expected[]={0,41,43,7,99,UINTPTR_MAX,UINTPTR_MAX/2+1,19};
    assert(count==sizeof(expected)/sizeof(*expected) && !memcmp(values,expected,sizeof(expected)));
#ifndef NATIVE_SCAN_BASELINE
    capture_checks(api);
#endif
    puts("native-stack-scan: all byte boundaries, exact candidates, empty intervals, incremental reads and emission reentry preserved");
}
