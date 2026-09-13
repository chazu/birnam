fn main() {
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/libtcc.c");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/libtcc.h");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/config.h");
    println!("cargo:rerun-if-changed=src/resource_metrics.c");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/tcc.h");
    let mut compiler = cc::Build::new();
    if std::env::var_os("CARGO_FEATURE_RESOURCE_METRICS").is_some() {
        compiler.file("src/resource_metrics.c");
    }
    compiler
        .file("../../vendor/libtcc-src/libtcc.c")
        .include("../../vendor/libtcc-src")
        .flag_if_supported("-std=gnu11")
        .warnings(false)
        .compile("birnam_tcc");
}
