//! What a curator answers about the titles it holds, as Bindery answers it for books: each
//! book under Bindery's own id, a search for one, what it is downloading, a download
//! dropped with its release blocklisted, and that release grabbed again.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::{Kind, TitleKind};
use lemonfiber_ports::service::{
    CuratedTitle, Downloading, Failure, Fulfilling, Grabbed, TitleRequest,
};
use serde::Deserialize;
use serde_json::json;

use super::collecting::{Book, BOOKS, NOT_FOUND, THE_BOOKS};
use super::curating::{books_alone, BOOKS_ALONE};
use super::queued::{fingerprint, Download, QUEUE};
use super::tracing::{Event, Found, GRABBED, HISTORY};
use super::{unsupported, Bindery};

const SEARCH: &str = "/wanted/bulk";

const SEARCHED: &str = "search";

const GRAB: &str = "/queue/grab";

const BLOCKLIST: &str = "/blocklist";

const THE_BLOCKLIST: &str = "Bindery's blocklist could not be read";

const THE_ANSWER: &str = "Bindery's answer to the search could not be read";

const NOT_SEARCHED: &str = "Bindery did not say whether it searched for the book";

const NO_GRAB: &str =
    "Bindery's history holds no grab of that release, so it has nothing to blocklist it from";

#[derive(Deserialize)]
struct Released {
    #[serde(default)]
    guid: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Blocked {
    id: i64,
    #[serde(default)]
    book_id: Option<i64>,
    #[serde(default)]
    guid: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    error: String,
}

#[derive(Deserialize)]
struct Answers {
    #[serde(default)]
    results: BTreeMap<String, Answer>,
}

impl Book {
    fn files(&self) -> u32 {
        let held: BTreeSet<&str> = [
            &self.file_path,
            &self.ebook_file_path,
            &self.audiobook_file_path,
        ]
        .into_iter()
        .map(String::as_str)
        .filter(|path| !path.is_empty())
        .collect();
        u32::try_from(held.len()).unwrap_or(u32::MAX)
    }

    fn curated(self) -> CuratedTitle {
        CuratedTitle {
            reference: self.id,
            id: Some(self.id),
            year: self.year().map(u32::from),
            monitored: self.monitored,
            files: self.files(),
            title: self.title,
            seasons: Vec::new(),
        }
    }
}

impl Event {
    fn released(&self) -> Option<String> {
        serde_json::from_str::<Released>(&self.data)
            .ok()
            .map(|data| data.guid)
            .filter(|guid| !guid.is_empty())
    }
}

impl Download {
    fn downloading(self) -> Option<Downloading> {
        Some(Downloading {
            title_id: self.book_id?,
            bytes: self.bytes(),
            left: self.left(),
            expected: None,
            state: self.state(),
            download_id: self.download_id(),
            queued: Some(self.id),
            episode: None,
            release: self.title,
        })
    }
}

impl Bindery {
    async fn holding(&self, id: i64) -> Result<Book, Failure> {
        let response = self
            .answered(Method::Get, &format!("{BOOKS}/{id}"), None)
            .await?;
        if response.status == NOT_FOUND {
            return Err(self.endpoint.refused(&format!("it holds no book as {id}")));
        }
        self.endpoint.decode(&response, THE_BOOKS)
    }

    async fn blocklisted(&self, download: &Download) -> Result<(), Failure> {
        let grab = self
            .history_of(download.book_id, Some(GRABBED))
            .await?
            .into_iter()
            .find(|event| event.released().as_deref() == Some(download.guid.as_str()))
            .ok_or_else(|| self.endpoint.refused(NO_GRAB))?;
        self.done(
            Method::Post,
            &format!("{HISTORY}/{}{BLOCKLIST}", grab.id),
            None,
        )
        .await
    }

    async fn lifted(&self, id: i64, release: &str, found: Option<&Found>) -> Result<(), Failure> {
        let blocked: Vec<Blocked> = self.read(BLOCKLIST, THE_BLOCKLIST).await?;
        for entry in blocked.iter().filter(|entry| {
            entry.book_id == Some(id)
                && (fingerprint(&entry.guid).as_deref() == Some(release)
                    || found.is_some_and(|found| found.title == entry.title))
        }) {
            self.done(Method::Delete, &format!("{BLOCKLIST}/{}", entry.id), None)
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Fulfilling for Bindery {
    async fn catalogued(
        &self,
        _kind: Kind,
        _reference: i64,
    ) -> Result<Option<CuratedTitle>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn titles(&self, kind: TitleKind) -> Result<Vec<CuratedTitle>, Failure> {
        books_alone(kind)?;
        Ok(self
            .shelf(None)
            .await?
            .into_iter()
            .map(Book::curated)
            .collect())
    }

    async fn held_title(&self, _kind: Kind, _id: i64) -> Result<Option<CuratedTitle>, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn request(&self, _kind: Kind, _request: &TitleRequest) -> Result<CuratedTitle, Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn search(&self, kind: TitleKind, id: i64) -> Result<(), Failure> {
        books_alone(kind)?;
        self.holding(id).await?;
        let asked = json!({ "ids": [id], "action": SEARCHED });
        let mut answers: Answers = self
            .written(Method::Post, SEARCH, &asked, THE_ANSWER)
            .await?;
        match answers.results.remove(&id.to_string()) {
            Some(answer) if answer.ok => Ok(()),
            Some(answer) if !answer.error.is_empty() => Err(self.endpoint.refused(&answer.error)),
            _ => Err(self.endpoint.refused(NOT_SEARCHED)),
        }
    }

    async fn remove(&self, _kind: Kind, _id: i64) -> Result<(), Failure> {
        Err(unsupported(BOOKS_ALONE))
    }

    async fn downloading(&self, kind: TitleKind) -> Result<Vec<Downloading>, Failure> {
        books_alone(kind)?;
        Ok(self
            .downloads()
            .await?
            .into_iter()
            .filter(Download::waiting)
            .filter_map(Download::downloading)
            .collect())
    }

    async fn unqueue(&self, kind: TitleKind, id: i64, blocklist: bool) -> Result<String, Failure> {
        books_alone(kind)?;
        let download = self
            .downloads()
            .await?
            .into_iter()
            .find(|download| download.id == id)
            .ok_or_else(|| {
                self.endpoint
                    .refused(&format!("it holds no queue entry {id}"))
            })?;
        let release = download.release().ok_or_else(|| {
            self.endpoint
                .refused("that queue entry names no release Bindery could fingerprint")
        })?;
        if blocklist {
            self.blocklisted(&download).await?;
        }
        self.done(Method::Delete, &format!("{QUEUE}/{id}"), None)
            .await?;
        Ok(release)
    }

    async fn grab(&self, kind: TitleKind, id: i64, release: &str) -> Result<Grabbed, Failure> {
        books_alone(kind)?;
        self.holding(id).await?;
        let found = self
            .releases(id)
            .await?
            .into_iter()
            .find(|found| fingerprint(&found.guid).as_deref() == Some(release));
        self.lifted(id, release, found.as_ref()).await?;
        let Some(found) = found else {
            return Ok(Grabbed::Gone);
        };
        let asked = json!({
            "guid": found.guid,
            "title": found.title,
            "nzbUrl": found.nzb_url,
            "size": found.size,
            "bookId": id,
            "indexerId": found.indexer_id,
            "protocol": found.protocol,
            "mediaType": found.media_type,
        });
        self.done(Method::Post, GRAB, Some(asked.to_string()))
            .await?;
        Ok(Grabbed::Grabbed)
    }
}

#[cfg(test)]
mod tests;
