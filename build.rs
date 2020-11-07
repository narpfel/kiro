const SOURCE_FILE_NAME: &str = "kilo.c";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={}", SOURCE_FILE_NAME);
    for entry in std::fs::read_dir("src")? {
        println!("cargo:rerun-if-changed={}", entry?.path().to_str().unwrap());
    }

    let crate_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    cbindgen::generate(crate_dir)?.write_to_file("kiro.h");

    #[cfg(not(windows))]
    cc::Build::new()
        .flag("-std=c11")
        .flag("-Wall")
        .flag("-Wextra")
        .flag("-pedantic")
        .flag("-Werror")
        .file(SOURCE_FILE_NAME)
        .compile("libkilo.a");

    #[cfg(windows)]
    cc::Build::new()
        .file(SOURCE_FILE_NAME)
        .compile("kilo.lib");

    Ok(())
}
