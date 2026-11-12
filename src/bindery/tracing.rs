//! The trace, the stuck list and what Bindery would grab, read from its own library,
//! history, queue and searches, with the book as the item under Bindery's own id.

use std::collections::BTreeSet;

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_manifest::ApiKind;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::TitleKind;
use lemonfiber_ports::service::stage::{Outcome, HISTORY_HORIZON};
use lemonfiber_ports::service::{
    Failure, FoundItem, ItemPart, Pipeline, QualityReleases, QueueItem, ReleaseProbe, StuckItem,
    TraceEvent,
};
use serde::Deserialize;

use super::collecting::{Book, Page, BOOKS, HELD_PAGE, THE_BOOKS};
use super::curating::{books_alone, THE_WANTED, WANTED};
use super::Bindery;

pub(super) const HISTORY: &str = "/history";

pub(super) const GRABBED: &str = "grabbed";

const THE_HISTORY: &str = "Bindery's history could not be read";

const FIRST: &str = "limit=1&offset=0";

const THE_SEARCH: &str = "Bindery's search for the book could not be read";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Event {
    #[serde(default)]
    pub(super) id: i64,
    #[serde(default)]
    pub(super) book_id: Option<i64>,
    #[serde(default, rename = "eventType")]
    kind: String,
    #[serde(default)]
    pub(super) data: String,
    #[serde(default)]
    created_at: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Found {
    #[serde(default)]
    pub(super) guid: String,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) nzb_url: String,
    #[serde(default)]
    pub(super) size: i64,
    #[serde(default)]
    pub(super) indexer_id: Option<i64>,
    #[serde(default)]
    pub(super) protocol: String,
    #[serde(default)]
    pub(super) media_type: String,
    #[serde(default)]
    approved: bool,
}

#[derive(Deserialize)]
struct Searched {
    #[serde(default)]
    results: Vec<Found>,
}

impl Book {
    fn found(self) -> FoundItem {
        FoundItem {
            year: self.year(),
            id: self.id,
            title: self.title,
            monitored: self.monitored,
        }
    }
}

impl Bindery {
    pub(super) async fn shelf(&self, search: Option<&str>) -> Result<Vec<Book>, Failure> {
        let mut every = Vec::new();
        loop {
            let offset = every.len().to_string();
            let limit = HELD_PAGE.to_string();
            let mut query = vec![("limit", limit.as_str()), ("offset", offset.as_str())];
            query.extend(search.map(|term| ("search", term)));
            let page: Page<Book> = self
                .read(&format!("{BOOKS}?{}", form_encoded(&query)), THE_BOOKS)
                .await?;
            let count = page.items.len();
            every.extend(page.items);
            if count < HELD_PAGE {
                return Ok(every);
            }
        }
    }
    pub(super) async fn history_of(
        &self,
        id: Option<i64>,
        event_type: Option<&str>,
    ) -> Result<Vec<Event>, Failure> {
        let horizon = HISTORY_HORIZON.to_string();
        let book = id.map(|id| id.to_string());
        let mut query = vec![("limit", horizon.as_str())];
        query.extend(book.as_deref().map(|book| ("bookId", book)));
        query.extend(event_type.map(|kind| ("eventType", kind)));
        let page: Page<Event> = self
            .read(&format!("{HISTORY}?{}", form_encoded(&query)), THE_HISTORY)
            .await?;
        Ok(page
            .items
            .into_iter()
            .filter(|event| id.is_none_or(|id| event.book_id == Some(id)))
            .collect())
    }
    pub(super) async fn releases(&self, id: i64) -> Result<Vec<Found>, Failure> {
        let response = self
            .answered(Method::Post, &format!("{BOOKS}/{id}/search"), None)
            .await?;
        let searched: Searched = self.endpoint.decode(&response, THE_SEARCH)?;
        Ok(searched.results)
    }
}

#[async_trait]
impl Pipeline for Bindery {
    async fn library(&self, kind: TitleKind) -> Result<Vec<FoundItem>, Failure> {
        books_alone(kind)?;
        Ok(self
            .shelf(None)
            .await?
            .into_iter()
            .map(Book::found)
            .collect())
    }

    async fn find_items(&self, kind: TitleKind, term: &str) -> Result<Vec<FoundItem>, Failure> {
        books_alone(kind)?;
        let needle = term.trim().to_lowercase();
        Ok(self
            .shelf(Some(term.trim()))
            .await?
            .into_iter()
            .filter(|book| book.title.to_lowercase().contains(&needle))
            .map(Book::found)
            .collect())
    }

    async fn item_history(&self, kind: TitleKind, id: i64) -> Result<Vec<TraceEvent>, Failure> {
        books_alone(kind)?;
        Ok(self
            .history_of(Some(id), None)
            .await?
            .into_iter()
            .filter_map(|event| {
                Outcome::of_event(ApiKind::Bindery, &event.kind).map(|outcome| TraceEvent {
                    outcome,
                    at: event.created_at,
                    part: None,
                })
            })
            .collect())
    }

    async fn item_queue(&self, kind: TitleKind, id: i64) -> Result<Vec<QueueItem>, Failure> {
        books_alone(kind)?;
        Ok(self
            .downloads()
            .await?
            .iter()
            .filter(|download| download.waiting() && download.book_id == Some(id))
            .map(|download| QueueItem {
                part: None,
                stage: download.stage(),
                stuck: download.stuck(),
            })
            .collect())
    }

    async fn item_parts(
        &self,
        kind: TitleKind,
        _id: i64,
        _season: Option<u32>,
    ) -> Result<Vec<ItemPart>, Failure> {
        books_alone(kind)?;
        Ok(Vec::new())
    }

    async fn stuck_items(&self, kind: TitleKind) -> Result<Vec<StuckItem>, Failure> {
        books_alone(kind)?;
        let mut seen = BTreeSet::new();
        Ok(self
            .downloads()
            .await?
            .into_iter()
            .filter(|download| download.waiting() && download.stuck())
            .filter_map(|download| {
                let title = download.book.as_ref()?.title.clone();
                (!title.is_empty() && seen.insert(title.clone())).then(|| StuckItem {
                    title,
                    stage: download.stage(),
                    queued: Some(download.id),
                    title_id: download.book_id,
                    release: download.release(),
                })
            })
            .collect())
    }
}

#[async_trait]
impl QualityReleases for Bindery {
    async fn probe_releases(&self, kind: TitleKind) -> Result<ReleaseProbe, Failure> {
        books_alone(kind)?;
        let wanted: Page<Book> = self.read(&format!("{WANTED}?{FIRST}"), THE_WANTED).await?;
        let Some(first) = wanted.items.first() else {
            return Ok(ReleaseProbe::NothingWanted);
        };
        let found = self.releases(first.id).await?;
        Ok(if found.is_empty() {
            ReleaseProbe::NoneFound
        } else if found.iter().any(|release| release.approved) {
            ReleaseProbe::Matching
        } else {
            ReleaseProbe::NoneMatch
        })
    }
}

#[cfg(test)]
mod tests;
