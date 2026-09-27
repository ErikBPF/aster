//! GitHub App installation credentials. This adapter is not route-activated.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aster_core::{CoreError, Result, SecretStore};
use base64::Engine;
use chrono::{DateTime, Utc};
use ring::rand::SystemRandom;
use ring::signature::{RsaKeyPair, RSA_PKCS1_SHA256};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;

pub(crate) struct GithubApp {
    origin: reqwest::Url,
    app_id: u64,
    key_name: String,
    secrets: Arc<dyn SecretStore>,
    http: reqwest::Client,
}

pub(crate) struct InstallationToken(String);

impl InstallationToken {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
    expires_at: String,
}

#[derive(Deserialize)]
struct InstallationResponse {
    id: u64,
}

#[derive(Deserialize)]
struct RepositoryResponse {
    id: u64,
    full_name: String,
}

#[derive(Deserialize)]
struct DefaultRefResponse {
    r#ref: String,
    object: RefObject,
}

#[derive(Deserialize)]
struct RefObject {
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

fn unavailable() -> CoreError {
    CoreError::Storage("GitHub App credential unavailable".into())
}

impl GithubApp {
    pub(crate) async fn verified_default_commit(
        &self,
        full_name: &str,
        installation_id: u64,
        repository_id: u64,
        default_branch: &str,
    ) -> Result<String> {
        if !safe_branch(default_branch) {
            return Err(unavailable());
        }
        self.verify_repository(full_name, installation_id, repository_id)
            .await?;
        let token = self
            .installation_token(installation_id, repository_id)
            .await?;
        let url = self
            .origin
            .join(&format!("repos/{full_name}/git/ref/heads/{default_branch}"))
            .map_err(|_| unavailable())?;
        let reference: DefaultRefResponse = read_json(
            self.http
                .get(url)
                .header("User-Agent", "aster-notebooks")
                .header("Accept", "application/vnd.github+json")
                .bearer_auth(token.as_str())
                .send()
                .await
                .map_err(|_| unavailable())?,
        )
        .await?;
        if reference.r#ref != format!("refs/heads/{default_branch}")
            || reference.object.kind != "commit"
            || !valid_oid(&reference.object.sha)
        {
            return Err(unavailable());
        }
        Ok(reference.object.sha)
    }

    pub(crate) async fn verify_repository(
        &self,
        full_name: &str,
        installation_id: u64,
        repository_id: u64,
    ) -> Result<()> {
        let (owner, repo) = full_name.split_once('/').ok_or_else(unavailable)?;
        let safe_segment = |value: &str| {
            !value.is_empty()
                && value != "."
                && value != ".."
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        };
        if !safe_segment(owner) || !safe_segment(repo) || installation_id == 0 || repository_id == 0
        {
            return Err(unavailable());
        }
        let mut repo_url = self.origin.clone();
        repo_url
            .path_segments_mut()
            .map_err(|_| unavailable())?
            .push("repos")
            .push(owner)
            .push(repo);
        let pem = aster_core::require(self.secrets.as_ref(), &self.key_name).await?;
        let jwt = sign_jwt(&pem, self.app_id)?;
        let mut installation_url = repo_url.clone();
        installation_url
            .path_segments_mut()
            .map_err(|_| unavailable())?
            .push("installation");
        let installation: InstallationResponse = read_json(
            self.http
                .get(installation_url)
                .header("User-Agent", "aster-notebooks")
                .header("Accept", "application/vnd.github+json")
                .bearer_auth(jwt)
                .send()
                .await
                .map_err(|_| unavailable())?,
        )
        .await?;
        if installation.id != installation_id {
            return Err(unavailable());
        }
        let token = self
            .installation_token(installation_id, repository_id)
            .await?;
        let repository: RepositoryResponse = read_json(
            self.http
                .get(repo_url)
                .header("User-Agent", "aster-notebooks")
                .header("Accept", "application/vnd.github+json")
                .bearer_auth(token.as_str())
                .send()
                .await
                .map_err(|_| unavailable())?,
        )
        .await?;
        if repository.id != repository_id || repository.full_name != full_name {
            return Err(unavailable());
        }
        Ok(())
    }

    pub(crate) fn new(
        origin: &str,
        app_id: u64,
        key_name: &str,
        secrets: Arc<dyn SecretStore>,
    ) -> Result<Self> {
        if origin != "https://api.github.com/" {
            return Err(unavailable());
        }
        Self::build(origin, app_id, key_name, secrets, false)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(
        origin: &str,
        app_id: u64,
        key_name: &str,
        secrets: Arc<dyn SecretStore>,
    ) -> Result<Self> {
        Self::build(origin, app_id, key_name, secrets, true)
    }

    fn build(
        origin: &str,
        app_id: u64,
        key_name: &str,
        secrets: Arc<dyn SecretStore>,
        allow_test_loopback: bool,
    ) -> Result<Self> {
        let url = reqwest::Url::parse(origin).map_err(|_| unavailable())?;
        let test_loopback = allow_test_loopback
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
        if !(url.scheme() == "https" || test_loopback)
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
            || app_id == 0
            || key_name.is_empty()
        {
            return Err(unavailable());
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self {
            origin: url,
            app_id,
            key_name: key_name.into(),
            secrets,
            http,
        })
    }

    pub(crate) async fn installation_token(
        &self,
        installation_id: u64,
        repository_id: u64,
    ) -> Result<InstallationToken> {
        if installation_id == 0 || repository_id == 0 {
            return Err(unavailable());
        }
        let pem = aster_core::require(self.secrets.as_ref(), &self.key_name).await?;
        let jwt = sign_jwt(&pem, self.app_id)?;
        let url = self
            .origin
            .join(&format!(
                "app/installations/{installation_id}/access_tokens"
            ))
            .map_err(|_| unavailable())?;
        let response = self
            .http
            .post(url)
            .header("User-Agent", "aster-notebooks")
            .header("Accept", "application/vnd.github+json")
            .bearer_auth(jwt)
            .json(&json!({
                "repository_ids": [repository_id],
                "permissions": {"contents": "write"}
            }))
            .send()
            .await
            .map_err(|_| unavailable())?;
        let token: TokenResponse = read_json(response).await?;
        if token.token.is_empty()
            || token.token.len() > 8192
            || token.token.chars().any(char::is_control)
        {
            return Err(unavailable());
        }
        let expires = DateTime::parse_from_rfc3339(&token.expires_at).map_err(|_| unavailable())?;
        if expires <= Utc::now() + chrono::Duration::seconds(30) {
            return Err(unavailable());
        }
        Ok(InstallationToken(token.token))
    }
}

pub(crate) fn safe_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 128
        && !branch.starts_with('-')
        && branch.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with('.')
                && !part.ends_with(".lock")
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

fn valid_oid(oid: &str) -> bool {
    (oid.len() == 40 || oid.len() == 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn read_json<T: DeserializeOwned>(mut response: reqwest::Response) -> Result<T> {
    if !response.status().is_success() {
        return Err(unavailable());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if body.len() + chunk.len() > 64 * 1024 {
            return Err(unavailable());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| unavailable())
}

fn sign_jwt(pem: &str, app_id: u64) -> Result<String> {
    let pem = pem.trim();
    let (body, pkcs8) = if let Some(body) = pem
        .strip_prefix("-----BEGIN PRIVATE KEY-----")
        .and_then(|value| value.strip_suffix("-----END PRIVATE KEY-----"))
    {
        (body, true)
    } else if let Some(body) = pem
        .strip_prefix("-----BEGIN RSA PRIVATE KEY-----")
        .and_then(|value| value.strip_suffix("-----END RSA PRIVATE KEY-----"))
    {
        (body, false)
    } else {
        return Err(unavailable());
    };
    let encoded: String = body
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| unavailable())?;
    let key = if pkcs8 {
        RsaKeyPair::from_pkcs8(&der)
    } else {
        RsaKeyPair::from_der(&der)
    }
    .map_err(|_| unavailable())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| unavailable())?
        .as_secs();
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(
            &json!({"iss": app_id, "iat": now.saturating_sub(60), "exp": now + 540}),
        )
        .map_err(|_| unavailable())?,
    );
    let message = format!("{header}.{claims}");
    let mut signature = vec![0; key.public().modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        message.as_bytes(),
        &mut signature,
    )
    .map_err(|_| unavailable())?;
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature);
    Ok(format!("{message}.{signature}"))
}
