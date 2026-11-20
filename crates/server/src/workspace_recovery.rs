//! Immutable, bounded recovery objects shared by the S3 and fixture adapters.
use aster_core::{CoreError, Result, WorkspaceRecovery};
use object_store::ObjectStoreExt;
use std::sync::Arc;

pub(crate) struct ObjectRecovery {
    pub(crate) store: Arc<dyn object_store::ObjectStore>,
}

fn key_path(key: &str) -> Result<object_store::path::Path> {
    if key.len() > 1024
        || key.contains('\\')
        || key.chars().any(char::is_control)
        || key
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(CoreError::Invalid("invalid recovery object key".into()));
    }
    object_store::path::Path::parse(key)
        .map_err(|_| CoreError::Invalid("invalid recovery object key".into()))
}

fn unavailable(_: object_store::Error) -> CoreError {
    CoreError::Storage("workspace recovery storage unavailable".into())
}

async fn bounded<T>(operation: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(std::time::Duration::from_secs(30), operation)
        .await
        .map_err(|_| CoreError::Storage("workspace recovery storage timed out".into()))?
}

#[async_trait::async_trait]
impl WorkspaceRecovery for ObjectRecovery {
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        use futures::TryStreamExt;
        let path = key_path(prefix)?;
        let boundary = format!("{prefix}/");
        bounded(async {
            let mut objects = self.store.list(Some(&path));
            let mut keys = Vec::new();
            while let Some(object) = objects.try_next().await.map_err(unavailable)? {
                let key = object.location.to_string();
                if key.starts_with(&boundary) {
                    keys.push(key);
                }
                if keys.len() > 10_000 {
                    return Err(CoreError::Storage(
                        "workspace recovery object quota exceeded".into(),
                    ));
                }
            }
            keys.sort();
            Ok(keys)
        })
        .await
    }
    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
        if bytes.len() > super::workspace_snapshot::MAX_ARCHIVE_BYTES {
            return Err(CoreError::Invalid(
                "workspace archive quota exceeded".into(),
            ));
        }
        let path = key_path(key)?;
        bounded(async {
            self.store
                .put_opts(&path, bytes.into(), object_store::PutMode::Create.into())
                .await
                .map_err(unavailable)?;
            Ok(())
        })
        .await
    }
    async fn get(&self, key: &str, max_bytes: usize) -> Result<Vec<u8>> {
        use futures::TryStreamExt;
        let path = key_path(key)?;
        let limit = max_bytes.min(super::workspace_snapshot::MAX_ARCHIVE_BYTES);
        bounded(async {
            let object = self.store.get(&path).await.map_err(unavailable)?;
            if object.meta.size > limit as u64 {
                return Err(CoreError::Invalid(
                    "workspace archive quota exceeded".into(),
                ));
            }
            let mut stream = object.into_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.try_next().await.map_err(unavailable)? {
                if chunk.len() > limit.saturating_sub(bytes.len()) {
                    return Err(CoreError::Invalid(
                        "workspace archive quota exceeded".into(),
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        })
        .await
    }
    async fn delete(&self, key: &str) -> Result<()> {
        let path = key_path(key)?;
        bounded(async { self.store.delete(&path).await.map_err(unavailable) }).await
    }
}

#[cfg(test)]
pub(crate) fn fixture_store() -> Arc<dyn object_store::ObjectStore> {
    match std::env::var("ASTER_TEST_WORKSPACE_S3_ENDPOINT") {
        Err(_) => Arc::new(object_store::memory::InMemory::new()),
        Ok(endpoint) => {
            let url = reqwest::Url::parse(&endpoint).unwrap();
            assert_eq!(url.scheme(), "http");
            assert_eq!(url.host_str(), Some("127.0.0.1"));
            Arc::new(
                object_store::aws::AmazonS3Builder::new()
                    .with_endpoint(endpoint)
                    .with_allow_http(true)
                    .with_region("us-west-2")
                    .with_bucket_name(std::env::var("ASTER_TEST_WORKSPACE_S3_BUCKET").unwrap())
                    .with_access_key_id(std::env::var("ASTER_TEST_WORKSPACE_S3_KEY").unwrap())
                    .with_secret_access_key(
                        std::env::var("ASTER_TEST_WORKSPACE_S3_SECRET").unwrap(),
                    )
                    .build()
                    .unwrap(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn immutable_objects_round_trip_with_read_limits() {
        let recovery = ObjectRecovery {
            store: fixture_store(),
        };
        let key = "workspaces/0123456789abcdef/checkpoints/abc";
        recovery
            .put(key, b"draft and git objects".to_vec())
            .await
            .unwrap();
        assert_eq!(
            recovery.get(key, 100).await.unwrap(),
            b"draft and git objects"
        );
        assert!(recovery.get(key, 2).await.is_err());
        assert!(recovery.put(key, b"replacement".to_vec()).await.is_err());
        assert_eq!(
            recovery.get(key, 100).await.unwrap(),
            b"draft and git objects"
        );
        assert_eq!(
            recovery.list("workspaces/0123456789abcdef").await.unwrap(),
            vec![key]
        );
        assert!(recovery
            .list("workspaces/another-owner")
            .await
            .unwrap()
            .is_empty());
        assert!(recovery.list("../escape").await.is_err());
        recovery.delete(key).await.unwrap();
        assert!(recovery.get(key, 100).await.is_err());
        for key in ["../escape", "/absolute", "a//b", "a/../b", "a\\b", ""] {
            assert!(
                recovery.put(key, vec![1]).await.is_err(),
                "unsafe key {key}"
            );
        }
    }
}
