use aster_core::odcs::OdcsDocument;
use serde_json::json;

#[test]
fn multiobject_roundtrip_preserves_identity() {
    let raw = json!({
        "apiVersion": "v3.2.0", "kind": "DataContract",
        "id": "sales-contract", "version": "1.4.0",
        "schema": [
            {"id":"orders-id", "name":"orders", "physicalName":"order_rows",
             "properties":[{"id":"order-amount", "name":"amount", "logicalType":"number"}]},
            {"id":"refunds-id", "name":"refunds", "physicalName":"refund_rows",
             "properties":[{"id":"refund-amount", "name":"amount", "logicalType":"number"}]}
        ]
    });
    let text = serde_json::to_string_pretty(&raw).unwrap();
    let document = OdcsDocument::parse(text.as_bytes()).unwrap();
    assert_eq!(
        document.projection.id, "sales-contract",
        "document identity is not its artifact path"
    );
    assert_eq!(document.source, text.as_bytes());
    assert_eq!(document.raw, raw);
    let retained = document.raw;
    assert_eq!(retained["version"], "1.4.0");
    assert_eq!(retained["schema"], raw["schema"]);
}

#[test]
fn invalid_version_and_legacy_are_distinct() {
    let supported = "apiVersion: v3.2.0\nkind: DataContract\nid: sales\nversion: 1.4.0\n";
    assert!(OdcsDocument::parse(supported.as_bytes()).is_ok());
    assert!(
        OdcsDocument::parse(supported.replace("v3.2.0", "v3.1.0").as_bytes()).is_err(),
        "schema-compatible v3.1 must fail the explicit intake support gate"
    );
    assert!(OdcsDocument::parse(b"name: sales\n").is_err());
    assert!(OdcsDocument::parse(b"\xff").is_err());
    assert!(OdcsDocument::parse(format!("{supported}id: duplicate\n").as_bytes()).is_err());
}
