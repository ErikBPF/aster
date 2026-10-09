use aster_server::odcs_intake;
use std::path::Path;

#[test]
fn benchmark_contracts_pass_the_runtime_intake() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/benchmark-catalog");
    let manifest = std::fs::read(root.join("manifest.json")).unwrap();
    let bundle =
        odcs_intake::load_bundle(&root, &odcs_intake::sha256(&manifest), "benchmark-r24").unwrap();
    assert_eq!(bundle.documents.len(), 33);
    assert_eq!(
        bundle.physical_bindings.as_ref().unwrap().bindings.len(),
        33
    );
    for artifact in bundle.documents {
        assert_eq!(artifact.document.raw["schema"].as_array().unwrap().len(), 1);
        assert_eq!(
            artifact.document.source,
            std::fs::read(root.join(artifact.selection.path)).unwrap()
        );
    }
}
