# Sourced by service image tests after compiling their native fixture.
# Only Native Loader is instrumented; its actual constructor runs unchanged.
prefix=$(cd "$prefix" && pwd -P)
mkdir -p "$scratch/lib"
for library in "$prefix"/lib/*; do
  [[ $(basename "$library") == libbirnam_native_loader.dylib ]] && continue
  ln -s "$library" "$scratch/lib/$(basename "$library")"
done
cc -std=c11 -Wall -Wextra -Werror -dynamiclib scripts/fixtures/runtime-services-loader-native.c -o "$scratch/lib/libbirnam_native_loader.dylib"
export BIRNAM_LIB_DIR="$scratch/lib"
export BIRNAM_SERVICE_TEST_ORIGINAL="$prefix/lib/libbirnam_native_loader.dylib"
export BIRNAM_SERVICE_TEST_FIXTURE="$scratch/$service-native.dylib"
export BIRNAM_SERVICE_TEST_KIND="$service"
