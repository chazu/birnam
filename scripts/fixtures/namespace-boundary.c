/* Existing presence/import/publication admission transcripts through production
 * constructors. Each process receives the actual selected capability. */
#include "reflection-fixture.h"
#include <pthread.h>
static void *capability;
static const NamespaceImportV1 *imports;
static const char *operation;
static unsigned root_calls;
static int check(const char *name) {
    if(!strcmp(operation,"presence"))return namespace_presence_contains_v1(capability,name);
    if(!strcmp(operation,"import"))return namespace_import_v1(capability,name)!=NULL;
    return namespace_export_v1(capability,name,api->nil_obj)==api->nil_obj;
}
static void *worker(void *name) { (void)check(name);return NULL; }
static void roots(MarkFn mark) {
    (void)mark;root_calls++;
    assert(check(!strcmp(operation,"publication")?"namespace-root-publication":"Object"));
}
static int inspect(void *context,const StorageViewV1 *view) { (void)view;return check(context); }
static BirnamOutcome exercise(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    (void)id;assert(count==8 || count==9);reflection_start(args,count);capability=fixture_word(args[4]);
    char *kind=fixture_text(args[7]);assert(!strncmp(kind,"namespace-",10));operation=kind+10;
    if(!strcmp(operation,"publication")) { assert(count==9);imports=fixture_word(args[8]); }
    else assert(count==8 && (!strcmp(operation,"presence") || !strcmp(operation,"import")));
    if(!strcmp(mode,"null")) { (void)check(NULL);assert(!"null name accepted"); }
    if(!strcmp(mode,"owner") || !strcmp(mode,"null-owner")) {
        pthread_t thread;void *name=!strcmp(mode,"owner")?(void *)"Object":NULL;
        assert(!pthread_create(&thread,NULL,worker,name));assert(!pthread_join(thread,NULL));assert(!"foreign owner accepted");
    }
    if(!strcmp(mode,"observation") || !strcmp(mode,"null-observation")) {
        StorageRequestV1 request={1,sizeof(request),!strcmp(mode,"observation")?(void *)"Object":NULL,inspect};
        (void)observe(&request);assert(!"observation did not invoke visitor");
    }
    assert(!strcmp(mode,"collection"));
    if(imports)assert(check("namespace-root-publication"));
    uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_register_root(roots);api->gc_collect();assert(root_calls);
    if(imports)assert(namespace_import_v1(imports,"namespace-root-publication")==api->nil_obj);
    printf("%s remains callable during root collection\n",operation);
    return (BirnamOutcome){0};
}
FIXTURE_NATIVE_ENTRY(exercise)
