fn main() {
    println!("cargo:rerun-if-changed=assets/wts.rc");
    println!("cargo:rerun-if-changed=assets/wts.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for("assets/wts.rc", ["wts", "wts-ui"], embed_resource::NONE)
            .manifest_required().unwrap();
    }
}
