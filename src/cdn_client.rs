// CDN client — mirrors GameBananaClient but targets the Cloudflare Worker

use crate::models::*;
use crate::filter_data::FilterData;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct CdnClient {
    base_url: String,
    game_id: u32,
}

#[derive(Debug)]
pub enum CdnError {
    Http(String),
    Parse(String),
}

impl std::fmt::Display for CdnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(msg) => write!(f, "CDN HTTP: {}", msg),
            Self::Parse(msg) => write!(f, "CDN Parse: {}", msg),
        }
    }
}

impl From<minreq::Error> for CdnError {
    fn from(e: minreq::Error) -> Self {
        Self::Http(e.to_string())
    }
}

impl From<serde_json::Error> for CdnError {
    fn from(e: serde_json::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

impl CdnClient {
    pub fn new(base_url: &str, game_id: u32) -> Self {
        Self {
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            game_id,
        }
    }

    /// Quick staleness check — returns sync manifest, or None if CDN is down.
    pub fn get_manifest(&self) -> Result<Option<CatalogManifest>, CdnError> {
        let url = format!("{}/api/v1/manifest", self.base_url);
        match minreq::get(&url)
            .with_header("Accept", "application/json")
            .with_timeout(5)
            .send()
        {
            Ok(resp) if resp.status_code == 200 => {
                let manifest: CatalogManifest =
                    serde_json::from_str(resp.as_str()?)?;
                Ok(Some(manifest))
            }
            Ok(_) => Ok(None),
            Err(_) => Ok(None), // CDN down → fall back to direct GB
        }
    }

    /// Get full catalog of ModCards from CDN.
    pub fn get_catalog(&self) -> Result<Vec<ModCard>, CdnError> {
        let url = format!("{}/api/v1/catalog", self.base_url);
        let resp = minreq::get(&url)
            .with_header("Accept", "application/json")
            .with_timeout(30)
            .send()?;
        if resp.status_code != 200 {
            let body = resp.as_str()
                .unwrap_or("(no body)")
                .chars()
                .take(200)
                .collect::<String>();
            return Err(CdnError::Http(format!("HTTP {}: {}", resp.status_code, body)));
        }
        Ok(serde_json::from_str(resp.as_str()?)?)
    }

    /// Incremental catalog: only cards newer than `since_id`.
    pub fn get_catalog_since(&self, since_id: u64) -> Result<Vec<ModCard>, CdnError> {
        let url = format!(
            "{}/api/v1/catalog?since={}",
            self.base_url, since_id
        );
        let resp = minreq::get(&url)
            .with_header("Accept", "application/json")
            .with_timeout(10)
            .send()?;
        if resp.status_code != 200 {
            let body = resp.as_str().unwrap_or("(no body)").chars().take(200).collect::<String>();
            return Err(CdnError::Http(format!("HTTP {}: {}", resp.status_code, body)));
        }
        Ok(serde_json::from_str(resp.as_str()?)?)
    }

    /// Get filter metadata (categories + subcategories).
    pub fn get_metadata(&self) -> Result<FilterData, CdnError> {
        let url = format!("{}/api/v1/{}/catalog/meta", self.base_url, self.game_id);
        let resp = minreq::get(&url)
            .with_header("Accept", "application/json")
            .with_timeout(10)
            .send()?;
        if resp.status_code != 200 {
            return Err(CdnError::Http(format!("HTTP {}", resp.status_code)));
        }
        Ok(serde_json::from_str(resp.as_str()?)?)
    }

    /// Get full mod detail from CDN (returns GB-native format, same as direct GB).
    pub fn get_mod(&self, mod_id: u64) -> Result<ModDetail, CdnError> {
        let url = format!("{}/api/v1/mod/{}", self.base_url, mod_id);
        let resp = minreq::get(&url)
            .with_header("Accept", "application/json")
            .with_timeout(15)
            .send()?;

        let status = resp.status_code;
        let body = resp.as_str().map_err(|e| CdnError::Http(e.to_string()))?;

        if status == 404 {
            return Err(CdnError::Http("mod not found".into()));
        }
        if status != 200 {
            return Err(CdnError::Http(format!("HTTP {}", status)));
        }

        Ok(serde_json::from_str(body)?)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogManifest {
    pub synced_at: u64,
    pub total_mods: usize,
    pub catalog_hash: String,
    #[serde(default)]
    pub first_mod_id: u64,
}
