#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/native-stack-v2/native_stack_v2.h"
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
int main(int argc,char **argv) {
    assert(argc==3);void *image=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(image);
#ifdef NATIVE_SCAN_BASELINE
    scan=(Scan)dlsym(image,"birnam_test_scan");assert(scan);
#else
    NativeStackFactoryProviderV2 provide=(NativeStackFactoryProviderV2)dlsym(image,"birnam_native_stack_factory_v2");assert(provide);
    const NativeStackFactoryV2 *api=provide();assert(api && api->version==2 && api->byte_size==sizeof(*api));scan=api->scan;assert(scan);
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
    puts("native-stack-scan: all byte boundaries, exact candidates, empty intervals, incremental reads and emission reentry preserved");
}
