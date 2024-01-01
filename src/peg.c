#include <stdio.h>

#include "packcc.c"

int generate_parser() {
    const char *iname = "test.peg";
    const char *oname = "testgrammar.so";
    bool_t debug = FALSE;
    {
    }
    {
        context_t *ctx = create_context(iname, oname, debug);
        int b = parse(ctx) && generate(ctx);
        destroy_context(ctx);
        if (!b) exit(10);
    }
    return 0;
}

int entrypoint_peg() {
  printf("In PEG entrypoint");
  generate_parser();
  return 0;
}
