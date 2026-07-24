fn main() {
    println!("cargo:rustc-link-arg=-Wl,-install_name,@rpath/libbirnam_id_tcc_rust.dylib");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/libtcc.c");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/libtcc.h");
    println!("cargo:rerun-if-changed=../../vendor/libtcc-src/config.h");

    cc::Build::new()
        .file("../../vendor/libtcc-src/libtcc.c")
        .include("../../vendor/libtcc-src")
        .flag_if_supported("-std=gnu11")
        .warnings(false)
        .compile("birnam_tcc");
}
