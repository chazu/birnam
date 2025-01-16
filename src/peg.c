#include <stdio.h>

#include "packcc.c"
#include "libtcc.h"

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

int compileString(char *str, char *outputFile) {
  TCCState *s;
  int (*func)(int);

  s = tcc_new();
  if (!s) {
    fprintf(stderr, "Could not create tcc state\n");
    exit(1);
  }

  /*The output type must be set before any compilation*/
  tcc_set_output_type(s, TCC_OUTPUT_DLL);
  if (tcc_compile_string(s, str) == -1)
    return 1;

  tcc_output_file(s, outputFile);
  return 0;
}

int compile_parser() {
  char *code = read_file("testgrammar.so.c");
  compileString(code, "testgrammar");
  return 0;
}

void entrypoint(void *state) {
  printf("PEG entrypoint");
  generate_parser();
  compile_parser();
}

int main() {
  entrypoint(NULL);
  return 0;
}
