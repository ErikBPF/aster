//! Generates the ConnectRPC service, client and codecs from `proto/aster.proto`.
//!
//! proto3 optional fields, the google.api.http bindings and the well-known
//! types all come from the vendored `proto/` tree plus protoc's own include
//! directory (see `PROTOC`/`PROTOC_INCLUDE` in devenv.nix).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto");
    connectrpc_build::Config::new()
        .files(&["../../proto/aster.proto"])
        .includes(&["../../proto"])
        .include_file("_connectrpc.rs")
        .emit_descriptor_set("aster_descriptor.bin")
        .compile()?;
    Ok(())
}
