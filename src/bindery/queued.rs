//! Bindery's queue as its own API answers it: every download it tracks, the book each is
//! for, and where each stands, read once for queue health, the trace, the stuck list and
//! what it is downloading.

use async_trait::async_trait;
use lemonfiber_ports::digest::sha256;
use lemonfiber_ports::service::stage::Stage;
use lemonfiber_ports::service::{DownloadState, Failure, Queue, Queued, Queues};
use serde::Deserialize;

use super::Bindery;

pub(super) const QUEUE: &str = "/queue";

const THE_QUEUE: &str = "Bindery's queue could not be read";

const GRABBED: &str = "grabbed";

const DOWNLOADING: &str = "downloading";

const IMPORTING: &str = "importing";

const IMPORTED: &str = "imported";

const COMPLETED: [&str; 4] = ["completed", "importPending", "importing", "importExternal"];

const DOWNLOADED: [&str; 6] = [
    "completed",
    "importPending",
    "importExternal",
    "importHeld",
    "importFailed",
    "importBlocked",
];

const GIVEN_UP: [&str; 2] = ["failed", "importBlocked"];

const FAILED: [&str; 3] = ["failed", "importFailed", "importBlocked"];

const HELD: &str = "importHeld";

const ERROR: &str = "error";

const WARNING: &str = "warning";

const OK: &str = "ok";

const PERCENT: &str = "%";

const PERMILLE: u128 = 1000;
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ForBook {
    #[serde(default)]
    pub(super) title: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Download {
    #[serde(default)]
    pub(super) id: i64,
    #[serde(default)]
    pub(super) guid: String,
    #[serde(default)]
    pub(super) book_id: Option<i64>,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) status: String,
    #[serde(default)]
    pub(super) error_message: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    percentage: String,
    #[serde(default)]
    sabnzbd_nzo_id: Option<String>,
    #[serde(default)]
    torrent_id: Option<String>,
    #[serde(default)]
    pub(super) book: Option<ForBook>,
}

#[derive(Deserialize)]
struct Downloads {
    #[serde(default)]
    items: Vec<Download>,
}
pub(super) fn fingerprint(guid: &str) -> Option<String> {
    (!guid.is_empty()).then(|| sha256(guid))
}

impl Download {
    pub(super) fn waiting(&self) -> bool {
        self.status != IMPORTED
    }
    pub(super) fn tracking(&self) -> &'static str {
        if FAILED.contains(&self.status.as_str()) || !self.error_message.is_empty() {
            ERROR
        } else if self.status == HELD {
            WARNING
        } else {
            OK
        }
    }
    pub(super) fn stuck(&self) -> bool {
        self.tracking() != OK
    }
    pub(super) fn stage(&self) -> Stage {
        let status = self.status.as_str();
        if status == GRABBED {
            Stage::Grabbed
        } else if status == IMPORTING {
            Stage::Importing
        } else if DOWNLOADED.contains(&status) {
            Stage::Downloaded
        } else {
            Stage::Downloading
        }
    }
    pub(super) fn state(&self) -> DownloadState {
        let status = self.status.as_str();
        if GIVEN_UP.contains(&status) {
            DownloadState::Failed
        } else if self.stuck() {
            DownloadState::Warning
        } else if status == GRABBED {
            DownloadState::Queued
        } else if status == DOWNLOADING {
            DownloadState::Downloading
        } else if COMPLETED.contains(&status) {
            DownloadState::Completed
        } else {
            DownloadState::Warning
        }
    }
    pub(super) fn bytes(&self) -> u64 {
        u64::try_from(self.size).unwrap_or_default()
    }
    pub(super) fn left(&self) -> u64 {
        let size = self.bytes();
        let said = self.percentage.trim().trim_end_matches(PERCENT).trim();
        match said.parse::<f64>().ok().filter(|done| done.is_finite()) {
            Some(done) => {
                let done = format!("{:.0}", done.clamp(0.0, 100.0) * 10.0)
                    .parse::<u128>()
                    .unwrap_or_default();
                let left = u128::from(size) * (PERMILLE - done.min(PERMILLE)) / PERMILLE;
                u64::try_from(left).unwrap_or(size)
            }
            None if self.status == GRABBED || self.status == DOWNLOADING => size,
            None => 0,
        }
    }
    pub(super) fn download_id(&self) -> Option<String> {
        self.torrent_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(str::to_lowercase)
            .or_else(|| self.sabnzbd_nzo_id.clone().filter(|id| !id.is_empty()))
    }
    pub(super) fn release(&self) -> Option<String> {
        fingerprint(&self.guid)
    }
}

impl Bindery {
    pub(super) async fn downloads(&self) -> Result<Vec<Download>, Failure> {
        let held: Downloads = self.read(QUEUE, THE_QUEUE).await?;
        Ok(held.items)
    }
}

#[async_trait]
impl Queues for Bindery {
    async fn queue(&self) -> Result<Queue, Failure> {
        let held = self.downloads().await?;
        Ok(Queue {
            total: held.len(),
            items: held
                .into_iter()
                .map(|download| Queued {
                    status: download.tracking().to_owned(),
                    message: (!download.error_message.is_empty()).then_some(download.error_message),
                    title: download.title,
                    state: download.status,
                    download_id: None,
                    grabs: 1,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
