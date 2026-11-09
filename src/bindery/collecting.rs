//! `books.curate` as Bindery answers it: books, authors and series found by words a page
//! at a time, one by the reference this adapter composes for it, asked for at a root
//! folder and quality profile Bindery holds, and those root folders and profiles. Series
//! are found only through Hardcover.

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    AskedWith, BookAsk, BookCurating, BookUnit, Candidate, CandidateState, Candidates, CuratedUnit,
    Failure, Profile as Asked,
};
use serde::de::DeserializeOwned;

use super::curating::{Folder, Profile, FOLDERS, PROFILES, STATUS, THE_FOLDERS, THE_PROFILES};
use super::{unsupported, Bindery};
use reference::Reference;
use wire::{
    published, Author, Book, Disabled, FoundSeries, Offered, Page, Series, Setting, WANTED,
};

mod asking;
mod reference;
mod wire;

const PAGE: usize = 20;

const BOOK_SEARCH: &str = "/search/book";

const AUTHOR_SEARCH: &str = "/search/author";

const SERIES_SEARCH: &str = "/series/hardcover/search";

const SERIES_FOUND: &str = "50";

const BOOKS: &str = "/book";

const AUTHORS: &str = "/author";

const SERIES: &str = "/series";

const HELD_PAGE: usize = 500;

const MEDIA_TYPE: &str = "/setting/default.media_type";

const FORMATS: [&str; 3] = ["ebook", "audiobook", "both"];

const EBOOK: &str = "ebook";

const NOT_FOUND: u16 = 404;

const MISSING_TOKEN: &str = "missing_token";

const ADMIN_DISABLED: &str = "admin_disabled";

const ENV_DISABLED: &str = "env_disabled";

const NO_SUCH_PAGE: &str = "that page begins nowhere a lookup gives";

const NOT_IN_THE_CATALOGUE: &str = "its catalogue holds no such reference";

const THE_BOOKS: &str = "Bindery's books could not be read";

const THE_AUTHORS: &str = "Bindery's authors could not be read";

const THE_SERIES: &str = "Bindery's series could not be read";

impl Bindery {
    fn not_in_the_catalogue(&self) -> Failure {
        self.endpoint.refused(NOT_IN_THE_CATALOGUE)
    }

    async fn searched<T: DeserializeOwned>(
        &self,
        path: &str,
        term: &str,
        what: &str,
    ) -> Result<Vec<T>, Failure> {
        self.read(&format!("{path}?{}", form_encoded(&[("term", term)])), what)
            .await
    }

    async fn household_format(&self) -> Result<String, Failure> {
        let response = self.answered(Method::Get, MEDIA_TYPE, None).await?;
        if response.status == NOT_FOUND {
            return Ok(EBOOK.to_owned());
        }
        let setting: Setting = self
            .endpoint
            .decode(&response, "Bindery's media type could not be read")?;
        Ok(FORMATS
            .iter()
            .find(|format| **format == setting.value)
            .unwrap_or(&EBOOK)
            .to_string())
    }

    async fn series_found(&self, term: &str) -> Result<Vec<FoundSeries>, Failure> {
        let query = form_encoded(&[("term", term), ("limit", SERIES_FOUND)]);
        let response = self
            .answered(Method::Get, &format!("{SERIES_SEARCH}?{query}"), None)
            .await?;
        if response.status == NOT_FOUND {
            if let Ok(disabled) = serde_json::from_str::<Disabled>(&response.body) {
                return Err(unsupported(hardcover_missing(&disabled.reason)));
            }
        }
        self.endpoint.decode(&response, THE_SERIES)
    }

    async fn held_series(&self) -> Result<Vec<Series>, Failure> {
        self.read(SERIES, THE_SERIES).await
    }

    async fn held_book(&self, id: i64) -> Result<Book, Failure> {
        self.read(&format!("{BOOKS}/{id}"), THE_BOOKS).await
    }

    async fn held_author(&self, id: i64) -> Result<Author, Failure> {
        self.read(&format!("{AUTHORS}/{id}"), THE_AUTHORS).await
    }

    async fn library_book(&self, reference: &Reference) -> Result<Option<Book>, Failure> {
        let query = form_encoded(&[("search", reference.word(0)), ("limit", "100")]);
        let page: Page<Book> = self.read(&format!("{BOOKS}?{query}"), THE_BOOKS).await?;
        Ok(page
            .items
            .into_iter()
            .find(|one| one.foreign_book_id == reference.id))
    }

    async fn library_author(&self, id: &str, name: &str) -> Result<Option<Author>, Failure> {
        let query = form_encoded(&[("search", name), ("limit", "100")]);
        let page: Page<Author> = self
            .read(&format!("{AUTHORS}?{query}"), THE_AUTHORS)
            .await?;
        Ok(page
            .items
            .into_iter()
            .find(|one| one.foreign_author_id == id))
    }

    async fn found_book(&self, reference: &Reference) -> Result<Option<Book>, Failure> {
        let term = format!("{} {}", reference.word(0), reference.word(1));
        let found: Vec<Book> = self.searched(BOOK_SEARCH, term.trim(), THE_BOOKS).await?;
        Ok(found
            .into_iter()
            .find(|one| one.foreign_book_id == reference.id))
    }

    async fn found_author(&self, reference: &Reference) -> Result<Option<Author>, Failure> {
        let found: Vec<Author> = self
            .searched(AUTHOR_SEARCH, reference.word(0), THE_AUTHORS)
            .await?;
        Ok(found
            .into_iter()
            .find(|one| one.foreign_author_id == reference.id))
    }

    async fn book_standing(&self, found: Book, household: &str) -> Result<Candidate, Failure> {
        let held = match found.library_book_id {
            Some(id) => Some(self.held_book(id).await?),
            None => None,
        };
        let reference = Reference::composed(
            &found.foreign_book_id,
            &[&found.title, found.by().unwrap_or_default()],
        );
        Ok(book(reference, &found, held.as_ref(), household))
    }

    async fn author_standing(&self, found: Author, household: &str) -> Result<Candidate, Failure> {
        let held = match found.library_author_id {
            Some(id) => Some(self.held_author(id).await?),
            None => None,
        };
        let reference = Reference::composed(&found.foreign_author_id, &[&found.name]);
        Ok(author(reference, &found, held.as_ref(), household))
    }

    async fn book_candidate(
        &self,
        reference: &Reference,
        asked: &str,
        household: &str,
    ) -> Result<Option<Candidate>, Failure> {
        if let Some(held) = self.library_book(reference).await? {
            return Ok(Some(book(asked.to_owned(), &held, Some(&held), household)));
        }
        Ok(match self.found_book(reference).await? {
            Some(found) => Some(Candidate {
                reference: asked.to_owned(),
                ..self.book_standing(found, household).await?
            }),
            None => None,
        })
    }

    async fn author_candidate(
        &self,
        reference: &Reference,
        asked: &str,
        household: &str,
    ) -> Result<Option<Candidate>, Failure> {
        if let Some(listed) = self
            .library_author(&reference.id, reference.word(0))
            .await?
        {
            let held = self.held_author(listed.id).await?;
            return Ok(Some(author(
                asked.to_owned(),
                &listed,
                Some(&held),
                household,
            )));
        }
        Ok(match self.found_author(reference).await? {
            Some(found) => Some(Candidate {
                reference: asked.to_owned(),
                ..self.author_standing(found, household).await?
            }),
            None => None,
        })
    }

    async fn series_candidate(
        &self,
        reference: &Reference,
        asked: &str,
        household: &str,
    ) -> Result<Option<Candidate>, Failure> {
        let found = self.series_found(reference.word(0)).await?;
        let held = self.held_series().await?;
        let held = held.iter().find(|one| one.is(&reference.id));
        let found = found.iter().find(|one| one.foreign_id == reference.id);
        Ok(match (found, held) {
            (Some(found), held) => Some(series(asked.to_owned(), found, held, household)),
            (None, Some(held)) => Some(series(
                asked.to_owned(),
                &FoundSeries {
                    foreign_id: reference.id.clone(),
                    title: held.title.clone(),
                    author_name: reference.word(1).to_owned(),
                    ..FoundSeries::default()
                },
                Some(held),
                household,
            )),
            (None, None) => None,
        })
    }
}

#[async_trait]
impl BookCurating for Bindery {
    async fn lookup(
        &self,
        unit: BookUnit,
        term: &str,
        after: Option<&str>,
    ) -> Result<Candidates, Failure> {
        let from = after.map_or(Ok(0), |after| {
            after
                .parse::<usize>()
                .map_err(|_| self.endpoint.refused(NO_SUCH_PAGE))
        })?;
        let household = self.household_format().await?;
        Ok(match unit {
            BookUnit::Book => {
                let (page, next) = paged(self.searched(BOOK_SEARCH, term, THE_BOOKS).await?, from);
                let mut found = Vec::with_capacity(page.len());
                for one in page {
                    found.push(self.book_standing(one, &household).await?);
                }
                Candidates { found, next }
            }
            BookUnit::Author => {
                let (page, next) =
                    paged(self.searched(AUTHOR_SEARCH, term, THE_AUTHORS).await?, from);
                let mut found = Vec::with_capacity(page.len());
                for one in page {
                    found.push(self.author_standing(one, &household).await?);
                }
                Candidates { found, next }
            }
            BookUnit::Series => {
                let (page, next) = paged(self.series_found(term).await?, from);
                let held = self.held_series().await?;
                let found = page
                    .iter()
                    .map(|one| {
                        let reference =
                            Reference::composed(&one.foreign_id, &[&one.title, &one.author_name]);
                        let standing = held.iter().find(|series| series.is(&one.foreign_id));
                        series(reference, one, standing, &household)
                    })
                    .collect();
                Candidates { found, next }
            }
        })
    }

    async fn candidate(
        &self,
        unit: BookUnit,
        reference: &str,
    ) -> Result<Option<Candidate>, Failure> {
        let Some(read) = Reference::read(reference, words(unit)) else {
            return Ok(None);
        };
        let household = self.household_format().await?;
        match unit {
            BookUnit::Book => self.book_candidate(&read, reference, &household).await,
            BookUnit::Author => self.author_candidate(&read, reference, &household).await,
            BookUnit::Series => self.series_candidate(&read, reference, &household).await,
        }
    }

    async fn request(&self, ask: &BookAsk) -> Result<Candidate, Failure> {
        asking::request(self, ask).await
    }

    async fn profiles(&self) -> Result<AskedWith, Failure> {
        let folders: Vec<Folder> = self.read(FOLDERS, THE_FOLDERS).await?;
        let quality: Vec<Profile> = self.read(PROFILES, THE_PROFILES).await?;
        let offered: Offered = self
            .read(STATUS, "Bindery's status could not be read")
            .await?;
        Ok(AskedWith {
            root_folders: folders.into_iter().map(|one| one.path).collect(),
            quality: quality
                .into_iter()
                .map(|one| Asked {
                    id: i64::from(one.id),
                    name: one.name,
                })
                .collect(),
            metadata: Vec::new(),
            units: [BookUnit::Book, BookUnit::Author]
                .into_iter()
                .chain(offered.hardcover.then_some(BookUnit::Series))
                .map(CuratedUnit::from)
                .collect(),
        })
    }
}

const fn words(unit: BookUnit) -> usize {
    match unit {
        BookUnit::Author => 1,
        BookUnit::Book | BookUnit::Series => 2,
    }
}

fn hardcover_missing(reason: &str) -> &'static str {
    match reason {
        MISSING_TOKEN => {
            "Bindery finds series only through Hardcover, and no Hardcover token is set"
        }
        ADMIN_DISABLED => {
            "Bindery finds series only through Hardcover, and its Hardcover series setting is off"
        }
        ENV_DISABLED => {
            "Bindery finds series only through Hardcover, and its enhanced Hardcover API is \
             turned off where it runs"
        }
        _ => "Bindery finds series only through Hardcover, which it does not offer here",
    }
}

fn paged<T>(found: Vec<T>, from: usize) -> (Vec<T>, Option<String>) {
    let to = from.saturating_add(PAGE);
    let next = (found.len() > to).then(|| to.to_string());
    (found.into_iter().skip(from).take(PAGE).collect(), next)
}

fn format_of(held: Option<&Book>, household: &str) -> String {
    held.map(|one| one.media_type.as_str())
        .filter(|format| FORMATS.contains(format))
        .unwrap_or(household)
        .to_owned()
}

fn book(reference: String, found: &Book, held: Option<&Book>, household: &str) -> Candidate {
    let state = held.map_or(CandidateState::MayAsk, |one| {
        if one.here() {
            CandidateState::Here
        } else if one.monitored && one.status == WANTED {
            CandidateState::Asked
        } else {
            CandidateState::MayAsk
        }
    });
    Candidate {
        reference,
        name: found.title.clone(),
        by: found.by().map(str::to_owned),
        year: found.year(),
        cover: published(found.image_url.as_deref())
            .or_else(|| held.and_then(|one| published(one.image_url.as_deref()))),
        format: Some(format_of(held, household)),
        state,
    }
}

fn author(reference: String, found: &Author, held: Option<&Author>, household: &str) -> Candidate {
    Candidate {
        reference,
        name: found.name.clone(),
        by: None,
        year: None,
        cover: published(found.image_url.as_deref())
            .or_else(|| held.and_then(|one| published(one.image_url.as_deref()))),
        format: Some(household.to_owned()),
        state: held.map_or(CandidateState::MayAsk, |one| {
            of_many(one.books.iter(), one.monitored)
        }),
    }
}

fn series(
    reference: String,
    found: &FoundSeries,
    held: Option<&Series>,
    household: &str,
) -> Candidate {
    let by = Some(found.author_name.clone())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            held.and_then(|one| one.hardcover_link.as_ref())
                .map(|link| link.hardcover_author_name.clone())
                .filter(|name| !name.is_empty())
        });
    Candidate {
        reference,
        name: found.title.clone(),
        by,
        year: None,
        cover: None,
        format: Some(household.to_owned()),
        state: held.map_or(CandidateState::MayAsk, |one| {
            of_many(
                one.held(),
                one.monitored || one.held().any(|book| book.monitored),
            )
        }),
    }
}

fn of_many<'a>(books: impl Iterator<Item = &'a Book>, wanted: bool) -> CandidateState {
    let counted: Vec<&Book> = books.filter(|one| one.counted()).collect();
    let here = counted.iter().filter(|one| one.here()).count();
    if here > 0 && here == counted.len() {
        CandidateState::Here
    } else if here > 0 {
        CandidateState::PartlyHere
    } else if wanted {
        CandidateState::Asked
    } else {
        CandidateState::MayAsk
    }
}

#[cfg(test)]
mod tests;
