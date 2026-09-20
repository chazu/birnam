#define _DARWIN_C_SOURCE
#include "idapi_v10.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static const NamespacePresenceV1 *original;
static unsigned early_checks;
static int incoming(void *context,const char *name) {
    assert(context==&early_checks);
    /* Tcc checks the incoming ForeignAddress family before selecting its saved
       execution API. Lang selects the saved API before its reload check. */
    assert(!strcmp(name,"ForeignAddress") || !strcmp(name,"ForeignAddressVt"));
    early_checks++;
    return namespace_presence_contains_v1(original,name);
}
void *module_init(IdApi *api,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports) {
    assert(api && !state && presence);original=presence;early_checks=0;
    Dl_info info;assert(dladdr((void *)api->dlsym_fn,&info) && info.dli_fname);
    const char *slash=strrchr(info.dli_fname,'/');assert(slash);
    NamespacePresenceV1 selected={1,sizeof(selected),&early_checks,incoming};
    IdApi copied=*api;NamespaceImportV1 copied_imports=*imports;
    const char *libraries[]={"libbirnam_id_lang.dylib","libbirnam_id_tcc.dylib"};
    for(size_t i=0;i<2;i++) {
        char path[PATH_MAX];int n=snprintf(path,sizeof(path),"%.*s/%s",(int)(slash-info.dli_fname),info.dli_fname,libraries[i]);
        assert(n>0 && (size_t)n<sizeof(path));
        void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
        ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
        assert(!init(&copied,NULL,&selected,&copied_imports));
        assert(early_checks==(i?2u:0u));
    }
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
