fn main() {
    // The upstream reader traverses bounded document trees. Windows' 1 MiB
    // executable default is too small for some valid nested groups.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }
}
