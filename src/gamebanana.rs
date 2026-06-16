use std::path::Path;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    time::Duration,
};

use crate::models::*;

const BASE_URL: &str = "https://gamebanana.com/apiv11";

#[derive(Debug)]
pub enum GbError {
    Http(String),
    Parse(String),
    NotFound,
}

impl std::fmt::Display for GbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(s) => write!(f, "HTTP error: {s}"),
            Self::Parse(s) => write!(f, "Parse error: {s}"),
            Self::NotFound => write!(f, "Not found"),
        }
    }
}

impl std::error::Error for GbError {}

impl From<minreq::Error> for GbError {
    fn from(e: minreq::Error) -> Self {
        Self::Http(e.to_string())
    }
}

impl From<serde_json::Error> for GbError {
    fn from(e: serde_json::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

// ─── Client ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GameBananaClient {
    game_id: u32,
}

impl GameBananaClient {
    pub fn new(game_id: u32) -> Self {
        Self { game_id }
    }

    pub fn game_id(&self) -> u32 {
        self.game_id
    }

    /// List mods with optional name search
    pub fn list_mods(
        &self,
        page: u32,
        per_page: u32,
        search: Option<&str>,
    ) -> Result<ModListPage, GbError> {
        let mut url = format!(
            "{}/Game/{}/Subfeed?_nPage={}&_nPerpage={}&_sSort=default",
            BASE_URL, self.game_id, page, per_page
        );
        if let Some(q) = search {
            if !q.is_empty() {
                url.push_str(&format!("&_sName={}", url_encode(q)));
            }
        }

        let resp = minreq::get(&url)
            .with_header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .with_header("Accept", "application/json")
            .with_timeout(15)
            .send()?;

        let status = resp.status_code;
        let body = resp.as_str().map_err(|e| GbError::Http(e.to_string()))?;

        if status != 200 {
            return Err(GbError::Http(format!(
                "HTTP {}: {}",
                status,
                &body[..body.len().min(200)]
            )));
        }

        if body.trim().is_empty() {
            return Err(GbError::Http("Empty response body".into()));
        }

        let mut page: ModListPage = serde_json::from_str(body)
            .map_err(|e| GbError::Parse(format!("{e}: {}", &body[..body.len().min(300)])))?;

        // Check if the API itself is exhausted BEFORE our client-side filtering
        let raw_count = page.records.len() as u32;
        page.api_exhausted = raw_count < per_page;

        // Only keep actual mod submissions (exclude Questions, Tools, etc.)
        page.records.retain(|r| {
            r.model_name == "Mod"
                && !matches!(
                    r.root_category.as_ref().map(|c| c.name.as_str()),
                    Some("Mod/Skin Manager") | Some("Tools") | Some("Tutorials")
                )
        });

        Ok(page)
    }

    /// Get full mod details including file list
    pub fn get_mod(&self, mod_id: u64) -> Result<ModDetail, GbError> {
        let url = format!(
            "{}/Mod/{}?_csvProperties=_idRow,_sName,_tsDateModified,_aRootCategory,_aCategory,_aPreviewMedia,_aSubmitter,_aFiles,_sText",
            BASE_URL, mod_id
        );

        let resp = minreq::get(&url)
            .with_header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .with_header("Accept", "application/json")
            .with_timeout(15)
            .send()?;

        if resp.status_code != 200 {
            let body = resp.as_bytes();
            let snippet = String::from_utf8_lossy(&body[..body.len().min(200)]);
            return Err(GbError::Http(format!(
                "HTTP {}: {}",
                resp.status_code, snippet
            )));
        }

        let body = resp.as_str().map_err(|e| GbError::Http(e.to_string()))?;
        Ok(serde_json::from_str(body)?)
    }

    /// Download a mod file. Follows CDN redirect chain.
    /// Returns bytes written.
    pub fn download_file(
        &self,
        file: &ModFile,
        dest: impl AsRef<Path>,
        on_progress: &dyn Fn(u64, u64),
        should_abort: &dyn Fn() -> bool,
    ) -> Result<u64, GbError> {
        let dest = dest.as_ref();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let mut effective_total = file.size;
        const MAX_RECOVERY_ATTEMPTS: usize = 6;
        const MAX_REDIRECTS: usize = 5;
        const RANGE_CHUNK_BYTES: u64 = 1024 * 1024;
        const STREAM_REQUEST_TIMEOUT_SECS: u64 = 10;
        const LEGACY_STREAM_TIMEOUT_SECS: u64 = 300;
        #[allow(unused_assignments)]
        let mut last_error: Option<String> = None;
        let mut recovery_failures = 0usize;

        'recovery: loop {
            if should_abort() {
                return Err(GbError::Http("download cancelled".into()));
            }
            let existing_len = std::fs::metadata(dest).map(|meta| meta.len()).unwrap_or(0);

            let resume_from = existing_len;
            let mut response = None;
            let requested_range_end =
                resume_from.saturating_add(RANGE_CHUNK_BYTES.saturating_sub(1));
            let should_request_range = true;
            let request_timeout = if effective_total > 0 {
                STREAM_REQUEST_TIMEOUT_SECS
            } else {
                LEGACY_STREAM_TIMEOUT_SECS
            };
            let mut request_url = file.download_url.clone();
            for _ in 0..MAX_REDIRECTS {
                if should_abort() {
                    return Err(GbError::Http("download cancelled".into()));
                }
                let mut request = minreq::get(&request_url)
                    .with_header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                    .with_timeout(request_timeout);
                if should_request_range {
                    request = request.with_header(
                        "Range",
                        format!("bytes={resume_from}-{requested_range_end}"),
                    );
                }
                let resp = request.send_lazy()?;

                match resp.status_code {
                    301 | 302 | 307 | 308 => {
                        request_url = resp
                            .headers
                            .get("location")
                            .ok_or_else(|| GbError::Http("Redirect without Location".into()))?
                            .to_string();
                    }
                    200 | 206 => {
                        response = Some(resp);
                        break;
                    }
                    416 => {
                        if let Some(actual_total) = resp
                            .headers
                            .get("content-range")
                            .and_then(|value| parse_total_from_content_range(value))
                        {
                            if existing_len == actual_total && actual_total > 0 {
                                on_progress(actual_total, actual_total);
                                return Ok(actual_total);
                            }
                            if existing_len > actual_total && actual_total > 0 {
                                let _ = std::fs::remove_file(dest);
                                effective_total = actual_total;
                                recovery_failures = 0;
                                continue 'recovery;
                            }
                            return Err(GbError::Http(format!(
                                "Unexpected status: 416 (local={existing_len}, remote={actual_total})"
                            )));
                        }
                        return Err(GbError::Http(format!(
                            "Unexpected status: 416 (local={existing_len})"
                        )));
                    }
                    code => {
                        return Err(GbError::Http(format!("Unexpected status: {code}")));
                    }
                }
            }

            let Some(mut body) = response else {
                return Err(GbError::Http("Too many redirects".into()));
            };

            if should_abort() {
                return Err(GbError::Http("download cancelled".into()));
            }
            if should_request_range && body.status_code != 206 {
                return Err(GbError::Http("server ignored range resume request".into()));
            }
            let append = resume_from > 0 && body.status_code == 206;
            if let Some(actual_total) = body
                .headers
                .get("content-range")
                .and_then(|value| parse_total_from_content_range(value))
            {
                effective_total = actual_total;
            } else if body.status_code == 200 && effective_total == 0 {
                if let Some(actual_total) = body
                    .headers
                    .get("content-length")
                    .and_then(|value| value.parse::<u64>().ok())
                {
                    effective_total = actual_total;
                }
            }
            let mut out = if append {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dest)
                    .map_err(|e| GbError::Http(e.to_string()))?
            } else {
                std::fs::File::create(dest).map_err(|e| GbError::Http(e.to_string()))?
            };
            let mut downloaded = if append { resume_from } else { 0 };
            let request_started_from = downloaded;
            let expected_downloaded_after_request = if effective_total > 0 {
                match body.status_code {
                    206 => Some(requested_range_end.saturating_add(1).min(effective_total)),
                    200 => Some(effective_total),
                    _ => None,
                }
            } else {
                None
            };
            on_progress(downloaded, effective_total);

            let mut buffer = [0u8; 64 * 1024];
            loop {
                if should_abort() {
                    return Err(GbError::Http("download cancelled".into()));
                }
                match body.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        out.write_all(&buffer[..read])
                            .map_err(|e| GbError::Http(e.to_string()))?;
                        downloaded += read as u64;
                        on_progress(downloaded, effective_total);
                    }
                    Err(err) => {
                        last_error = Some(err.to_string());
                        let made_progress = downloaded > request_started_from;
                        if effective_total > 0 && is_timeout_io_error(&err) && made_progress {
                            recovery_failures = 0;
                            continue 'recovery;
                        }
                        if made_progress {
                            recovery_failures = 0;
                        } else {
                            recovery_failures = recovery_failures.saturating_add(1);
                        }
                        if recovery_failures >= MAX_RECOVERY_ATTEMPTS {
                            break 'recovery;
                        }
                        if !made_progress {
                            let mut waited = 0u64;
                            while waited < 250 {
                                if should_abort() {
                                    return Err(GbError::Http("download cancelled".into()));
                                }
                                let step = 50u64.min(250 - waited);
                                std::thread::sleep(Duration::from_millis(step));
                                waited += step;
                            }
                        }
                        continue 'recovery;
                    }
                }
            }

            if effective_total == 0 || downloaded >= effective_total {
                return Ok(downloaded);
            }
            if expected_downloaded_after_request.is_some_and(|expected| downloaded >= expected) {
                recovery_failures = 0;
                continue 'recovery;
            }

            last_error = Some(format!(
                "stream ended early ({downloaded}/{effective_total} bytes)"
            ));
            if downloaded > request_started_from {
                recovery_failures = 0;
                continue 'recovery;
            }
            recovery_failures = recovery_failures.saturating_add(1);
            if recovery_failures >= MAX_RECOVERY_ATTEMPTS {
                break;
            }
            let mut waited = 0u64;
            while waited < 250 {
                if should_abort() {
                    return Err(GbError::Http("download cancelled".into()));
                }
                let step = 50u64.min(250 - waited);
                std::thread::sleep(Duration::from_millis(step));
                waited += step;
            }
        }

        let final_error = last_error.unwrap_or_else(|| "unexpected end of file".to_string());
        Err(GbError::Http(final_error))
    }
}

fn is_timeout_io_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}

fn parse_total_from_content_range(value: &str) -> Option<u64> {
    let (_, total) = value.split_once('/')?;
    total.trim().parse::<u64>().ok()
}

fn url_encode(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                r.push(b as char);
            }
            b' ' => r.push('+'),
            _ => {
                r.push('%');
                r.push(HEX[(b >> 4) as usize]);
                r.push(HEX[(b & 0x0F) as usize]);
            }
        }
    }
    r
}

const HEX: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
];
