use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
    let script = include_bytes!("rp235x_riscv.x");
    File::create(out.join("rp235x_riscv.x"))
        .expect("write OUT_DIR/rp235x_riscv.x")
        .write_all(script)
        .expect("write rp235x_riscv.x contents");
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=rp235x_riscv.x");
    println!("cargo:rerun-if-changed=build.rs");
}
