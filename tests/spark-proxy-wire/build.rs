use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("ASTER_SPARK_PROTO_ROOT")?);
    let connect = root.join("spark/connect");
    let protos: Vec<_> = [
        "base",
        "catalog",
        "commands",
        "common",
        "expressions",
        "ml",
        "ml_common",
        "pipelines",
        "relations",
        "types",
    ]
    .iter()
    .map(|name| connect.join(format!("{name}.proto")))
    .collect();
    tonic_prost_build::configure()
        .build_client(false)
        .build_server(true)
        .bytes(".")
        .compile_protos(&protos, &[root])?;
    Ok(())
}
