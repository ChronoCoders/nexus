use std::path::PathBuf;

fn main() {
    let out = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dict = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nexus-fix-codegen/tests/fixtures/FIX_sample.xml");
    println!("cargo:rerun-if-changed={}", dict.display());
    nexus_fix_codegen::generate()
        .dictionary(&dict)
        .out_dir(&out)
        .rustfmt(false)
        .run()
        .expect("codegen failed");
}
