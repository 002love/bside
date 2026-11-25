use std::path::Path;

use anyhow::{Result, anyhow};
use librespot_core::{
    authentication::Credentials, cache::Cache, config::SessionConfig, session::Session,
};
use librespot_oauth::OAuthClientBuilder;

use super::DEFAULT_REDIRECT_URI;

pub fn open_cache(cache_dir: &Path) -> Result<Cache> {
    Cache::new(Some(cache_dir), None, None, None)
        .map_err(|e| anyhow!("Failed to open credentials cache at {}: {e}", cache_dir.display()))
}

pub async fn login_oauth(
    cache_dir: &Path,
    open_browser: bool,
    redirect_uri: Option<&str>,
) -> Result<Session> {
    let config = SessionConfig::default();
    let cache = open_cache(cache_dir)?;

    let redirect = redirect_uri.unwrap_or(DEFAULT_REDIRECT_URI);
    let mut builder = OAuthClientBuilder::new(
        &config.client_id,
        redirect,
        vec![
            "streaming",
            "user-read-currently-playing",
            "user-read-playback-state",
            "user-read-recently-played",
        ],
    )
    .with_custom_message(
        "Authorization successful! You can close this tab and return to bside.",
    );
    if open_browser {
        builder = builder.open_in_browser();
    }

    let oauth_client = builder
        .build()
        .map_err(|e| anyhow!("Failed to initialize OAuth client: {e}"))?;

    let token = oauth_client
        .get_access_token_async()
        .await
        .map_err(|e| anyhow!("OAuth login failed: {e}"))?;

    let credentials = Credentials::with_access_token(token.access_token);
    let session = Session::new(config, Some(cache));
    session
        .connect(credentials, true)
        .await
        .map_err(|e| anyhow!("Failed to connect Spotify session: {e}"))?;

    Ok(session)
}

pub async fn connect_cached(cache_dir: &Path) -> Result<Session> {
    let config = SessionConfig::default();
    let cache = open_cache(cache_dir)?;
    let credentials = cache.credentials().ok_or_else(|| {
        anyhow!(
            "No cached Spotify credentials found in {}. Run `bside login` first.",
            cache_dir.display()
        )
    })?;

    let session = Session::new(config, Some(cache));
    session
        .connect(credentials, true)
        .await
        .map_err(|e| anyhow!("Failed to connect Spotify session using cached credentials: {e}"))?;

    Ok(session)
}

#[allow(dead_code)]
pub fn clear_cached_credentials(cache_dir: &Path) -> Result<()> {
    let cred_path = cache_dir.join("credentials.json");
    match std::fs::remove_file(&cred_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(anyhow!(
            "Failed to remove cached credentials at {}: {err}",
            cred_path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::clear_cached_credentials;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn clear_cached_credentials_removes_file_and_ignores_missing() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "bside-auth-test-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        // Missing file succeeds cleanly
        clear_cached_credentials(&dir).expect("clear missing");

        // Existing credentials.json is removed
        let cred = dir.join("credentials.json");
        std::fs::write(&cred, b"{}").expect("write cred");
        assert!(cred.exists());
        clear_cached_credentials(&dir).expect("clear existing");
        assert!(!cred.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
