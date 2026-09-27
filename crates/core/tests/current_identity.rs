//! V8b2a RED: a stable, verified user UUID must survive session persistence.

use aster_core::{Principal, SessionRecord};
use serde_json::json;

#[test]
fn stable_user_uuid_survives_session_and_principal_serialization() {
    let source = json!({
        "sid": "session-one",
        "subject": "opaque-oidc-subject",
        "roles": ["editor"],
        "groups": ["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"],
        "user_uuid": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        "created_at": 100,
        "last_seen": 100,
        "user_agent": null
    });
    let stored: SessionRecord = serde_json::from_value(source).unwrap();
    let restarted: SessionRecord =
        serde_json::from_value(serde_json::to_value(&stored).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&restarted).unwrap()["user_uuid"],
        "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
    );

    let principal_source = json!({
        "subject": restarted.subject,
        "roles": restarted.roles,
        "groups": restarted.groups,
        "user_uuid": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
    });
    let principal: Principal = serde_json::from_value(principal_source).unwrap();
    assert_eq!(
        serde_json::to_value(&principal).unwrap()["user_uuid"],
        "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
    );
}
