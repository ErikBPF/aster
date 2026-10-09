use aster_core::{CoreError, Result};
use serde::de::DeserializeOwned;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

// One metadata operation includes its OAuth exchanges and every page/load.
pub(crate) struct Budget {
    bytes: usize,
    reads: usize,
    deadline: Instant,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            bytes: 4 * 1024 * 1024,
            reads: 64,
            deadline: Instant::now() + Duration::from_secs(10),
        }
    }
}
impl Budget {
    async fn response(
        &mut self,
        request: reqwest::RequestBuilder,
        operation: &str,
    ) -> Result<reqwest::Response> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if self.reads == 0 || self.bytes == 0 || remaining.is_zero() {
            return Err(CoreError::Catalog(
                "incomplete catalog observation: read/byte/deadline limit".into(),
            ));
        }
        self.reads -= 1;
        request
            .timeout(remaining)
            .send()
            .await
            .map_err(|_| CoreError::Catalog(format!("{operation} transport/deadline failure")))
    }

    /// Observe headers only, sharing authentication's remaining read/deadline budget.
    pub(crate) async fn status(
        &mut self,
        request: reqwest::RequestBuilder,
        operation: &str,
    ) -> Result<reqwest::StatusCode> {
        Ok(self.response(request, operation).await?.status())
    }

    pub(crate) async fn json<T: DeserializeOwned>(
        &mut self,
        request: reqwest::RequestBuilder,
        operation: &str,
    ) -> Result<T> {
        let mut response = self.response(request, operation).await?;
        if !response.status().is_success() {
            return Err(CoreError::Catalog(format!(
                "{operation} returned HTTP {}",
                response.status().as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|n| n > self.bytes as u64)
        {
            return Err(CoreError::Catalog(
                "incomplete catalog observation: byte limit".into(),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| CoreError::Catalog(format!("{operation} body/deadline failure")))?
        {
            if chunk.len() > self.bytes {
                return Err(CoreError::Catalog(
                    "incomplete catalog observation: byte limit".into(),
                ));
            }
            self.bytes -= chunk.len();
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| CoreError::Catalog(format!("invalid {operation} response")))
    }
}

pub(crate) fn client(certificate: Option<reqwest::Certificate>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10));
    if let Some(certificate) = certificate {
        builder = builder.add_root_certificate(certificate);
    }
    builder.build().expect("catalog TLS client")
}

pub(crate) fn next_token(
    value: Option<&serde_json::Value>,
    seen: &mut HashSet<String>,
) -> Result<Option<String>> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(token))
            if !token.is_empty() && token.len() <= 8192 && seen.insert(token.clone()) =>
        {
            Ok(Some(token.clone()))
        }
        _ => Err(CoreError::Catalog(
            "incomplete catalog observation: invalid/repeated page token".into(),
        )),
    }
}

pub(crate) fn invalid(operation: &str) -> CoreError {
    CoreError::Catalog(format!("invalid {operation} metadata"))
}
