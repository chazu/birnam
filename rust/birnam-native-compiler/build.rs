fn main() {
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/libtcc.c");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/tccrun.c");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/libtcc.h");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/config.h");
    println!("cargo:rerun-if-changed=src/resource_metrics.c");
    println!("cargo:rerun-if-changed=src/allocation_metrics.c");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/tcc.h");
    let mut compiler = cc::Build::new();
    // Mapping disposition is part of the production TCC ownership protocol.
    compiler.file("src/resource_metrics.c");
    if std::env::var_os("CARGO_FEATURE_ALLOCATION_METRICS").is_some() {
        compiler.file("src/allocation_metrics.c");
    }
    compiler
        .file("../../vendor/libtcc-src/libtcc.c")
        .include("../../vendor/libtcc-src")
        .flag_if_supported("-std=gnu11")
        .warnings(false)
        .compile("birnam_tcc");
}
