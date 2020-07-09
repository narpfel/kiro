const SOURCE_FILE_NAME: &str = "kilo.c";

fn main() {
    println!("cargo:rerun-if-changed={}", SOURCE_FILE_NAME);

    cc::Build::new()
        .compiler("clang")
        .flag("-std=c18")
        .flag("-flto=thin")
        .flag("-Wall")
        .flag("-Wextra")
        .flag("-pedantic")
        .flag("-Werror")
        .flag("-march=native")
        .file(SOURCE_FILE_NAME)
        .compile("libkilo.a");
}
