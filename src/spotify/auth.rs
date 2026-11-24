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
    let mut builder = OAuthClientBuilder::new(&config.client_id, redirect, vec!["streaming"])
        .with_custom_message(
            "Authorization successful! You can close this tab and return to the terminal.",
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
