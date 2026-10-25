//! What Bindery files under and fetches with: its download clients, root folders, quality
//! profiles, indexers and queue; and the curator operations it has no equivalent for,
//! answered unsupported.

use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::{Format, Kind};
use lemonfiber_ports::service::{
    AddPlan, Added, Carried, Carrying, Catalogue, CatalogueEntry, Category, ClientProbe,
    Credential, DownloadClient, Failure, FoundItem, Identity, Importing, ItemPart, Maintenance,
    MusicQuality, Pipeline, QualityProfile, QualityReleases, Queue, QueueItem, Queued, Queues,
    Record, RegisteredClient, RegisteredFolder, ReleaseProbe, RootFolder, StuckItem, TraceEvent,
};
use lemonfiber_ports::Client;
use serde::Deserialize;

use super::{Bindery, SERVICE};
use crate::adapter::UPSTREAM;

const STATUS: &str = "/system/status";

const CLIENTS: &str = "/downloadclient";

const FOLDERS: &str = "/rootfolder";

const PROFILES: &str = "/qualityprofile";

const INDEXERS: &str = "/indexer";

const QUEUE: &str = "/queue";

const CATEGORY: &str = "category";

const AUDIOBOOK_CATEGORY: &str = "categoryAudiobook";

const KINDS: [&str; 6] = [
    "sabnzbd",
    "nzbget",
    "qbittorrent",
    "transmission",
    "deluge",
    "rtorrent",
];

const FAILED: [&str; 3] = ["failed", "importFailed", "importBlocked"];

const HELD: &str = "importHeld";

const ERROR: &str = "error";

const WARNING: &str = "warning";

const OK: &str = "ok";

const NO_CATEGORY_FIELD: &str =
    "Bindery files under `category`, and under `categoryAudiobook` for audiobooks, and nothing else";

const NO_KIND: &str =
    "Bindery takes SABnzbd, NZBGet, qBittorrent, Transmission, Deluge and rTorrent";

const BOOKS_ALONE: &str = "Bindery files books, which no kind of media this asks about is";

const NO_HARDLINKS: &str = "Bindery says nothing of whether it imports by hard link";

const NO_RECORDS: &str = "Bindery carries no records to or from another curator";

const NO_MUSIC: &str = "Bindery files no music";

#[derive(Deserialize)]
struct Status {
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeldClient {
    id: i64,
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    category: String,
}

#[derive(Deserialize)]
struct Folder {
    id: i64,
    path: String,
}

#[derive(Deserialize)]
struct Profile {
    id: u32,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Download {
    title: String,
    status: String,
    #[serde(default)]
    error_message: String,
}

#[derive(Deserialize)]
struct Downloads {
    #[serde(default)]
    items: Vec<Download>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Probed {
    #[serde(default)]
    error: Option<String>,
}

fn unsupported(detail: &str) -> Failure {
    Failure::Unsupported {
        service: SERVICE.to_owned(),
        detail: detail.to_owned(),
    }
}

fn tracking(state: &str) -> &'static str {
    if FAILED.contains(&state) {
        ERROR
    } else if state == HELD {
        WARNING
    } else {
        OK
    }
}

impl Bindery {
    fn client_body(&self, client: &DownloadClient) -> Result<String, Failure> {
        if !KINDS.contains(&client.protocol.0.as_str()) {
            return Err(self.endpoint.refused(NO_KIND));
        }
        if ![CATEGORY, AUDIOBOOK_CATEGORY].contains(&client.category.field.as_str()) {
            return Err(self.endpoint.refused(NO_CATEGORY_FIELD));
        }
        let mut body = serde_json::Map::new();
        body.insert("name".to_owned(), client.name.clone().into());
        body.insert("type".to_owned(), client.protocol.0.clone().into());
        body.insert("host".to_owned(), client.host.clone().into());
        body.insert("port".to_owned(), client.port.into());
        body.insert("enabled".to_owned(), true.into());
        body.insert(
            client.category.field.clone(),
            client.category.value.clone().into(),
        );
        match &client.credential {
            Credential::ApiKey(key) => {
                body.insert("apiKey".to_owned(), key.clone().into());
            }
            Credential::UserPass { username, password } => {
                body.insert("username".to_owned(), username.clone().into());
                body.insert("password".to_owned(), password.clone().into());
            }
        }
        Ok(serde_json::Value::Object(body).to_string())
    }
}

#[async_trait]
impl Client for Bindery {
    async fn identity(&self) -> Result<Identity, Failure> {
        let status: Status = self
            .read(STATUS, "Bindery's status could not be read")
            .await?;
        if status.version.is_empty() {
            return Err(self.endpoint.refused("Bindery named no version"));
        }
        Ok(Identity {
            name: UPSTREAM.to_owned(),
            version: status.version,
        })
    }

    async fn register_download_client(&self, client: &DownloadClient) -> Result<(), Failure> {
        let body = self.client_body(client)?;
        self.done(Method::Post, CLIENTS, Some(body)).await
    }

    async fn update_download_client(
        &self,
        id: &str,
        client: &DownloadClient,
    ) -> Result<(), Failure> {
        let numeric = self.assigned(id)?;
        let body = self.client_body(client)?;
        self.done(Method::Put, &format!("{CLIENTS}/{numeric}"), Some(body))
            .await
    }

    async fn set_client_field(
        &self,
        id: &str,
        field: &str,
        value: Option<&str>,
    ) -> Result<(), Failure> {
        let numeric = self.assigned(id)?;
        if ![CATEGORY, AUDIOBOOK_CATEGORY].contains(&field) {
            return Err(self.endpoint.refused(NO_CATEGORY_FIELD));
        }
        let body = serde_json::json!({ field: value.unwrap_or_default() }).to_string();
        self.done(Method::Put, &format!("{CLIENTS}/{numeric}"), Some(body))
            .await
    }

    async fn test_download_clients(&self) -> Result<Vec<ClientProbe>, Failure> {
        let held: Vec<HeldClient> = self
            .read(CLIENTS, "Bindery's download clients could not be read")
            .await?;
        let mut probes = Vec::with_capacity(held.len());
        for client in held {
            let response = self
                .endpoint
                .send(&self.request(
                    Method::Post,
                    &format!("{CLIENTS}/{}/test", client.id),
                    None,
                )?)
                .await?;
            let said: Option<Probed> = serde_json::from_str(&response.body).ok();
            probes.push(ClientProbe {
                id: client.id.to_string(),
                reachable: response.is_success(),
                detail: (!response.is_success())
                    .then(|| said.and_then(|said| said.error))
                    .flatten(),
            });
        }
        Ok(probes)
    }

    async fn register_root_folder(&self, folder: &RootFolder) -> Result<(), Failure> {
        let body = serde_json::json!({ "path": folder.path }).to_string();
        self.done(Method::Post, FOLDERS, Some(body)).await
    }

    async fn root_folders(&self) -> Result<Vec<RegisteredFolder>, Failure> {
        let held: Vec<Folder> = self
            .read(FOLDERS, "Bindery's root folders could not be read")
            .await?;
        Ok(held
            .into_iter()
            .map(|folder| RegisteredFolder {
                id: folder.id.to_string(),
                path: folder.path,
            })
            .collect())
    }

    async fn download_clients(&self) -> Result<Vec<RegisteredClient>, Failure> {
        let held: Vec<HeldClient> = self
            .read(CLIENTS, "Bindery's download clients could not be read")
            .await?;
        Ok(held
            .into_iter()
            .map(|client| RegisteredClient {
                id: client.id.to_string(),
                host: client.host,
                port: client.port,
                category: (!client.category.is_empty()).then(|| Category {
                    field: CATEGORY.to_owned(),
                    value: client.category,
                }),
            })
            .collect())
    }

    async fn quality_profiles(&self) -> Result<Vec<QualityProfile>, Failure> {
        let held: Vec<Profile> = self
            .read(PROFILES, "Bindery's quality profiles could not be read")
            .await?;
        Ok(held
            .into_iter()
            .map(|profile| QualityProfile {
                id: profile.id,
                name: profile.name,
            })
            .collect())
    }
}

#[async_trait]
impl Queues for Bindery {
    async fn queue(&self) -> Result<Queue, Failure> {
        let held: Downloads = self
            .read(QUEUE, "Bindery's queue could not be read")
            .await?;
        Ok(Queue {
            total: held.items.len(),
            items: held
                .items
                .into_iter()
                .map(|download| Queued {
                    status: tracking(&download.status).to_owned(),
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

#[async_trait]
impl Catalogue for Bindery {
    async fn lookup(&self, _kind: Kind, _term: &str) -> Result<Vec<CatalogueEntry>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn add_plan(&self, _kind: Kind) -> Result<AddPlan, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn add(
        &self,
        _kind: Kind,
        _entry: &CatalogueEntry,
        _plan: &AddPlan,
    ) -> Result<Added, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn indexer_count(&self) -> Result<usize, Failure> {
        let held: Vec<serde_json::Value> = self
            .read(INDEXERS, "Bindery's indexers could not be read")
            .await?;
        Ok(held.len())
    }
}

#[async_trait]
impl Maintenance for Bindery {
    async fn search_upgrades(&self, _kind: Kind) -> Result<(), Failure> {
        Err(unsupported(BOOKS_ALONE))
    }
}

#[async_trait]
impl Importing for Bindery {
    async fn hardlinks(&self) -> Result<bool, Failure> {
        Err(unsupported(NO_HARDLINKS))
    }

    async fn set_hardlinks(&self, _hardlink: bool) -> Result<(), Failure> {
        Err(unsupported(NO_HARDLINKS))
    }
}

#[async_trait]
impl Carrying for Bindery {
    async fn records(&self, _kind: Record) -> Result<Vec<Carried>, Failure> {
        Err(unsupported(NO_RECORDS))
    }

    async fn carry(&self, _kind: Record, _item: &Carried) -> Result<(), Failure> {
        Err(unsupported(NO_RECORDS))
    }
}

#[async_trait]
impl QualityReleases for Bindery {
    async fn probe_releases(&self, _kind: Kind) -> Result<ReleaseProbe, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }
}

#[async_trait]
impl MusicQuality for Bindery {
    async fn apply_music_format(&self, _format: Format) -> Result<(), Failure> {
        Err(unsupported(NO_MUSIC))
    }
}

#[async_trait]
impl Pipeline for Bindery {
    async fn library(&self, _kind: Kind) -> Result<Vec<FoundItem>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn find_items(&self, _kind: Kind, _term: &str) -> Result<Vec<FoundItem>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn item_history(&self, _kind: Kind, _id: i64) -> Result<Vec<TraceEvent>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn item_queue(&self, _kind: Kind, _id: i64) -> Result<Vec<QueueItem>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn item_parts(
        &self,
        _kind: Kind,
        _id: i64,
        _season: Option<u32>,
    ) -> Result<Vec<ItemPart>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn stuck_items(&self, _kind: Kind) -> Result<Vec<StuckItem>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }
}

#[cfg(test)]
mod tests;
