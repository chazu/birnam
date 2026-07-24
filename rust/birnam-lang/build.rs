fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-install_name,@rpath/libbirnam_id_lang_rust.dylib");
}
