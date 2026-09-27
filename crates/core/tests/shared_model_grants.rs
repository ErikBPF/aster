use aster_core::shared_models::{
    AdminSharedModelSummary, EncryptedSharedModel, InMemorySharedModels,
};
use aster_core::{Role, SharedModelGrant, SharedModelStore};

fn model(url: &str) -> EncryptedSharedModel {
    EncryptedSharedModel {
        summary: AdminSharedModelSummary {
            id: "qwen".into(),
            base_url: url.into(),
            model: "deepseek-v4.1".into(),
        },
        key_version: "v1".into(),
        nonce: vec![1; 24],
        ciphertext: vec![2; 32],
    }
}

#[tokio::test]
async fn ungranted_registration_can_change_destination_before_first_grant() {
    let store = InMemorySharedModels::default();
    store
        .put(model("https://approved.example/v1"))
        .await
        .unwrap();
    let generation = store.grants("qwen").await.unwrap().generation;
    store
        .put(model("https://other-approved.example/v1"))
        .await
        .unwrap();
    let after = store.grants("qwen").await.unwrap();
    assert_eq!(after.generation, generation);
    assert_eq!(after.revision, 0);
    assert_eq!(
        store.list().await.unwrap()[0].summary.base_url,
        "https://other-approved.example/v1"
    );
}

#[tokio::test]
async fn grants_compare_revision_and_preserve_audit_across_delete_recreate() {
    let store = InMemorySharedModels::default();
    store
        .put(model("https://approved.example/v1"))
        .await
        .unwrap();
    let initial = store.grants("qwen").await.unwrap();
    assert_eq!(initial.revision, 0);
    assert!(initial.grants.is_empty());

    let group = SharedModelGrant::Group("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into());
    let role = SharedModelGrant::Role(Role::Editor);
    let first = store
        .replace_grants("qwen", 0, vec![group.clone()], "root", 1000)
        .await
        .unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.grants, vec![group.clone()]);
    assert!(store
        .replace_grants("qwen", 0, vec![role.clone()], "other-admin", 1001)
        .await
        .is_err());
    assert_eq!(store.grants("qwen").await.unwrap().grants, vec![group]);

    let second = store
        .replace_grants("qwen", 1, vec![role], "root", 1002)
        .await
        .unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(store.grant_events("qwen").await.unwrap().len(), 2);
    let mut token_rotation = model("https://approved.example/v1");
    token_rotation.ciphertext = vec![3; 32];
    store.put(token_rotation).await.unwrap();
    let after_token_rotation = store.grants("qwen").await.unwrap();
    assert_eq!(after_token_rotation.generation, initial.generation);
    assert_eq!(after_token_rotation.revision, 2);
    assert_eq!(after_token_rotation.grants, second.grants);
    let mut old = model("https://approved.example/v1");
    old.ciphertext = vec![3; 32];
    assert!(
        store
            .replace_all(vec![(old, model("https://other-approved.example/v1"))])
            .await
            .is_err(),
        "key rotation may not retarget a granted model"
    );
    assert!(
        store
            .put(model("https://other-approved.example/v1"))
            .await
            .is_err(),
        "granted registration cannot silently retarget"
    );

    store.remove("qwen").await.unwrap();
    store
        .put(model("https://other-approved.example/v1"))
        .await
        .unwrap();
    let recreated = store.grants("qwen").await.unwrap();
    assert!(recreated.generation > initial.generation);
    assert_eq!(recreated.revision, 0);
    assert!(recreated.grants.is_empty());
    let events = store.grant_events("qwen").await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|event| event.generation == initial.generation));
    assert_eq!(events[0].actor, "root");
    assert_eq!(events[1].actor, "root");
}
