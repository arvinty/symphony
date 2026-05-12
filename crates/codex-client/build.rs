use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let schema_path = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("docs/codex-protocol/codex_app_server_protocol.v2.schemas.json");

    println!("cargo:rerun-if-changed={}", schema_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    if !schema_path.exists() {
        panic!(
            "codex v2 schema not found at {}; regenerate via `codex app-server generate-json-schema --out docs/codex-protocol/`",
            schema_path.display()
        );
    }

    let schema_json = fs::read_to_string(&schema_path).expect("read codex v2 schema");
    let schema: schemars::schema::RootSchema =
        serde_json::from_str(&schema_json).expect("parse codex v2 schema as RootSchema");

    let mut settings = typify::TypeSpaceSettings::default();
    settings
        .with_derive("Clone".to_string())
        .with_derive("Debug".to_string())
        .with_struct_builder(false);

    let mut type_space = typify::TypeSpace::new(&settings);
    type_space
        .add_root_schema(schema)
        .expect("typify add_root_schema");

    let tokens = type_space.to_stream();
    let file: syn::File = syn::parse2(tokens).expect("typify output should be valid Rust");
    let pretty = prettyplease::unparse(&file);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out = out_dir.join("v2.rs");
    fs::write(&out, pretty).expect("write v2.rs");
}
