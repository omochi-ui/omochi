use omochi_ui::ResourceRegistry;
use serde_json::{Value, json};
#[test]
fn shared_resource_contract_and_limits() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../../spec/fixtures/resources.json")).unwrap();
    let registry = ResourceRegistry::new(10, 20);
    let uri = registry
        .bytes(fixture["content"].as_str().unwrap().as_bytes(), "video/mp4")
        .unwrap();
    let resource = uri.rsplit('/').next().unwrap();
    for read in fixture["reads"].as_array().unwrap() {
        let response = registry.respond(&json!({"omochi":"resources/1","kind":"request","id":"test","resource":resource,"op":"read","offset":read["offset"],"length":read["length"]}));
        assert_eq!(response["status"], read["status"]);
        assert_eq!(response["data"], read["data"]);
    }
    assert!(registry.bytes(&[0; 11], "video/mp4").is_err());
    registry.release(&uri);
    assert_eq!(
        registry.respond(&json!({"kind":"request","id":"test","op":"stat","resource":resource}))["status"],
        404
    );
    assert_ne!(registry.bytes(b"0123456789", "video/mp4").unwrap(), uri);
    registry.close();
}
