#ifndef FIXTURE_ALLOCATION_BUDGET_SUPPORT_H
#define FIXTURE_ALLOCATION_BUDGET_SUPPORT_H
#include "../../abi/fixtures/allocation-budget-v1/allocation_budget_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_allocation_budget(void *floor,const char *floor_path) {
    AllocationBudgetDependencyV1 attach=(AllocationBudgetDependencyV1)dlsym(floor,"birnam_allocation_budget_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_allocation_budget.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    AllocationBudgetFactoryProviderV1 provide=(AllocationBudgetFactoryProviderV1)dlsym(image,"birnam_allocation_budget_factory_v1");assert(provide);
    const AllocationBudgetFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
