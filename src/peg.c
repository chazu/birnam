#include <stdio.h>

#include "packcc.c"

int generate_parser() {
    const char *iname = "test.peg";
    const char *oname = "testgrammar.so";
    bool_t debug = FALSE;
    {
        context_t *ctx = create_context(iname, oname, debug);
        int b = parse(ctx) && generate(ctx);
        destroy_context(ctx);
        if (!b) exit(10);
    }
    return 0;
}

char* read_file(const char* filename) {
    FILE* file = fopen(filename, "r");
    size_t len = 0;
    char* contents = NULL;
    int errno;

    if (!file) {
        // Error: failed to open file
        fprintf(stderr, "Error: could not open %s: %s\n", filename, strerror(errno));
        return NULL;
    }

    // Read file into a string, resizing buffer as necessary
    while ((contents = realloc(contents, len + 1))) {
        int c = fgetc(file);

        if (c == EOF) {
            // End of file reached, add null terminator and exit loop
            contents[len] = 0;
            break;
        }

        contents[len++] = c;
    }

    if (!contents || ferror(file)) {
        // Error: failed to read entire file or encountered an I/O error
        fprintf(stderr, "Error: could not read %s: %s\n", filename, strerror(errno));
        fclose(file);
        free(contents);
        return NULL;
    }

    // Close file and return its contents
    fclose(file);
    return contents;
}

int compile_parser() {
  char *code = read_file("testgrammar.so.c");
  compileString(code, "testgrammar");
  return 0;
}

void entrypoint_peg(void *data) {
  printf("In PEG entrypoint");
  generate_parser();
  compile_parser();
}
