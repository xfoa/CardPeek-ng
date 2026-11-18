#![allow(missing_docs)]

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("resources/icon.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("embed the application icon");
    }
}
