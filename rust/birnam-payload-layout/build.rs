fn main() {
    println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libbirnam_payload_layout.dylib");
}
