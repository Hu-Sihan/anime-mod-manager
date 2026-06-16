use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE, USER_AGENT};
use reqwest::{Client, StatusCode};

use crate::models::ModFile;

const USER_AGENT_VALUE: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const REQUEST_TIMEOUT_SECS: u64 = 30;
const READ_TIMEOUT_SECS: u64 = 15;
const MAX_RECOVERY_ATTEMPTS: usize = 6;
const RETRY_DELAY_MS: u64 = 250;
const ABORT_POLL_MS: u64 = 10;

#[derive(Debug)]
pub enum ModFileDownloadError {
    Http(String),
    Io(String),
    Runtime(String),
}

impl std::fmt::Display for ModFileDownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(message) => write!(f, "HTTP error: {message}"),
            Self::Io(message) => write!(f, "IO error: {message}"),
            Self::Runtime(message) => write!(f, "Runtime error: {message}"),
        }
    }
}

impl std::error::Error for ModFileDownloadError {}

impl From<reqwest::Error> for ModFileDownloadError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value.to_string())
    }
}

pub struct ModFileDownloader {
    client: Client,
}

impl ModFileDownloader {
    pub fn new() -> Self {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("failed to create mod file downloader");
        Self { client }
    }

    pub fn download_file(
        &self,
        file: &ModFile,
        dest: impl AsRef<Path>,
        on_progress: &dyn Fn(u64, u64),
        should_abort: &dyn Fn() -> bool,
    ) -> Result<u64, ModFileDownloadError> {
        let dest = dest.as_ref().to_path_buf();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| ModFileDownloadError::Io(err.to_string()))?;
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| ModFileDownloadError::Runtime(err.to_string()))?;

        runtime.block_on(self.download_file_inner(file, dest.as_path(), on_progress, should_abort))
    }

    async fn download_file_inner(
        &self,
        file: &ModFile,
        dest: &Path,
        on_progress: &dyn Fn(u64, u64),
        should_abort: &dyn Fn() -> bool,
    ) -> Result<u64, ModFileDownloadError> {
        let mut effective_total = file.size;
        #[allow(unused_assignments)]
        let mut last_error: Option<String> = None;
        let mut recovery_failures = 0usize;

        'recovery: loop {
            if should_abort() {
                return Err(cancelled_error());
            }

            let existing_len = std::fs::metadata(dest).map(|meta| meta.len()).unwrap_or(0);
            let resume_from = existing_len;

            let response = tokio::select! {
                biased;
                _ = wait_for_abort(should_abort) => {
                    return Err(cancelled_error());
                }
                result = tokio::time::timeout(
                    Duration::from_secs(REQUEST_TIMEOUT_SECS),
                    self.client
                        .get(&file.download_url)
                        .header(USER_AGENT, USER_AGENT_VALUE)
                        .header(RANGE, format!("bytes={resume_from}-"))
                        .send()
                ) => {
                    match result {
                        Ok(Ok(response)) => response,
                        Ok(Err(err)) => return Err(ModFileDownloadError::Http(err.to_string())),
                        Err(_) => return Err(ModFileDownloadError::Http("request timeout".to_string())),
                    }
                }
            };

            match response.status() {
                StatusCode::PARTIAL_CONTENT => {}
                StatusCode::OK if resume_from == 0 => {}
                StatusCode::RANGE_NOT_SATISFIABLE => {
                    if let Some(actual_total) = response
                        .headers()
                        .get(CONTENT_RANGE)
                        .and_then(|value| value.to_str().ok())
                        .and_then(parse_total_from_content_range)
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
                        return Err(ModFileDownloadError::Http(format!(
                            "Unexpected status: 416 (local={existing_len}, remote={actual_total})"
                        )));
                    }

                    return Err(ModFileDownloadError::Http(format!(
                        "Unexpected status: 416 (local={existing_len})"
                    )));
                }
                status => {
                    if resume_from > 0 && status == StatusCode::OK {
                        return Err(ModFileDownloadError::Http(
                            "server ignored range resume request".to_string(),
                        ));
                    }
                    return Err(ModFileDownloadError::Http(format!(
                        "Unexpected status: {}",
                        status.as_u16()
                    )));
                }
            }

            if let Some(actual_total) = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_total_from_content_range)
            {
                effective_total = actual_total;
            } else if let Some(content_length) = response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
            {
                effective_total = match response.status() {
                    StatusCode::PARTIAL_CONTENT => resume_from.saturating_add(content_length),
                    StatusCode::OK => content_length,
                    _ => effective_total,
                };
            }

            let append = resume_from > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
            let mut out = if append {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dest)
                    .map_err(|err| ModFileDownloadError::Io(err.to_string()))?
            } else {
                std::fs::File::create(dest)
                    .map_err(|err| ModFileDownloadError::Io(err.to_string()))?
            };

            let mut response = response;
            let mut downloaded = if append { resume_from } else { 0 };
            let request_started_from = downloaded;

            on_progress(downloaded, effective_total);

            loop {
                if should_abort() {
                    return Err(cancelled_error());
                }
                let next_chunk = tokio::select! {
                    biased;
                    _ = wait_for_abort(should_abort) => {
                        return Err(cancelled_error());
                    }
                    result = tokio::time::timeout(Duration::from_secs(READ_TIMEOUT_SECS), response.chunk()) => result,
                };

                match next_chunk {
                    Ok(Ok(Some(chunk))) => {
                        if should_abort() {
                            return Err(cancelled_error());
                        }
                        out.write_all(&chunk)
                            .map_err(|err| ModFileDownloadError::Io(err.to_string()))?;
                        downloaded = downloaded.saturating_add(chunk.len() as u64);
                        if should_abort() {
                            return Err(cancelled_error());
                        }
                        on_progress(downloaded, effective_total);
                    }
                    Ok(Ok(None)) => break,
                    Ok(Err(err)) => {
                        last_error = Some(err.to_string());
                        if downloaded > request_started_from {
                            recovery_failures = 0;
                            continue 'recovery;
                        }
                        recovery_failures = recovery_failures.saturating_add(1);
                        if recovery_failures >= MAX_RECOVERY_ATTEMPTS {
                            return Err(ModFileDownloadError::Http(
                                last_error
                                    .clone()
                                    .unwrap_or_else(|| "unexpected end of file".to_string()),
                            ));
                        }
                        wait_before_retry(should_abort).await?;
                        continue 'recovery;
                    }
                    Err(_) => {
                        last_error = Some("stream chunk timeout".to_string());
                        if downloaded > request_started_from {
                            recovery_failures = 0;
                            continue 'recovery;
                        }
                        recovery_failures = recovery_failures.saturating_add(1);
                        if recovery_failures >= MAX_RECOVERY_ATTEMPTS {
                            return Err(ModFileDownloadError::Http(
                                last_error
                                    .clone()
                                    .unwrap_or_else(|| "unexpected end of file".to_string()),
                            ));
                        }
                        wait_before_retry(should_abort).await?;
                        continue 'recovery;
                    }
                }
            }

            if effective_total == 0 || downloaded >= effective_total {
                return Ok(downloaded);
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
            wait_before_retry(should_abort).await?;
        }

        Err(ModFileDownloadError::Http(
            last_error.unwrap_or_else(|| "unexpected end of file".to_string()),
        ))
    }
}

fn cancelled_error() -> ModFileDownloadError {
    ModFileDownloadError::Http("download cancelled".to_string())
}

async fn wait_for_abort(should_abort: &dyn Fn() -> bool) {
    while !should_abort() {
        tokio::time::sleep(Duration::from_millis(ABORT_POLL_MS)).await;
    }
}

async fn wait_before_retry(should_abort: &dyn Fn() -> bool) -> Result<(), ModFileDownloadError> {
    tokio::select! {
        biased;
        _ = wait_for_abort(should_abort) => Err(cancelled_error()),
        _ = tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)) => Ok(()),
    }
}

fn parse_total_from_content_range(value: &str) -> Option<u64> {
    let (_, total) = value.split_once('/')?;
    total.trim().parse::<u64>().ok()
}
