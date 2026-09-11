fn main() {
    println!("cargo:rustc-link-arg=-Wl,-install_name,@rpath/libbirnam_id_tcc_rust.dylib");
}
