//! Asking Bindery for a book alone, an author with every book and those that come later,
//! or a Hardcover series with every book of it, its later books followed through each
//! author it adds; then searching where asked, and answering the candidate as it stands.

use std::collections::BTreeSet;

use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{BookAsk, BookCurating as _, BookUnit, Candidate, Failure};
use serde::Deserialize;
use serde_json::{json, Value};

use super::reference::Reference;
use super::wire::{Added, Author, Book, FoundSeries, Page, Series, SKIPPED, WANTED};
use super::{
    words, Bindery, Folder, Profile, AUTHORS, BOOKS, FOLDERS, HELD_PAGE, PROFILES, SERIES,
    THE_AUTHORS, THE_FOLDERS, THE_PROFILES, THE_SERIES,
};

const ADD_BOOK: &str = "/author/book";

const SEARCH: &str = "search";

const EVERY_BOOK: &str = "all";

const SERIES_ONLY: &str = "series";

const CREATED: u16 = 201;

const CONFLICT: u16 = 409;

#[derive(Deserialize)]
struct Made {
    id: i64,
}

struct Filed {
    folder: i64,
    profile: i64,
}

pub(super) async fn request(bindery: &Bindery, ask: &BookAsk) -> Result<Candidate, Failure> {
    let filed = filed(bindery, ask).await?;
    let reference = Reference::read(&ask.reference, words(ask.unit))
        .ok_or_else(|| bindery.not_in_the_catalogue())?;
    match ask.unit {
        BookUnit::Book => book(bindery, ask, &reference, &filed).await?,
        BookUnit::Author => author(bindery, ask, &reference, &filed).await?,
        BookUnit::Series => series(bindery, &reference, &filed).await?,
    }
    bindery
        .candidate(ask.unit, &ask.reference)
        .await?
        .ok_or_else(|| bindery.not_in_the_catalogue())
}

async fn filed(bindery: &Bindery, ask: &BookAsk) -> Result<Filed, Failure> {
    let folders: Vec<Folder> = bindery.read(FOLDERS, THE_FOLDERS).await?;
    let folder = folders
        .into_iter()
        .find(|one| one.path == ask.root_folder)
        .ok_or_else(|| bindery.endpoint.refused("it holds no such root folder"))?;
    let profiles: Vec<Profile> = bindery.read(PROFILES, THE_PROFILES).await?;
    if !profiles
        .iter()
        .any(|one| i64::from(one.id) == ask.quality_profile)
    {
        return Err(bindery.endpoint.refused("it holds no such quality profile"));
    }
    Ok(Filed {
        folder: folder.id,
        profile: ask.quality_profile,
    })
}

async fn book(
    bindery: &Bindery,
    ask: &BookAsk,
    reference: &Reference,
    filed: &Filed,
) -> Result<(), Failure> {
    let held = if let Some(held) = bindery.library_book(reference).await? {
        Some(held)
    } else {
        added_book(bindery, ask, reference, filed).await?
    };
    let Some(held) = held else {
        return Ok(());
    };
    let wanted = if held.status == SKIPPED {
        json!({ "monitored": true, "status": WANTED })
    } else {
        json!({ "monitored": true })
    };
    bindery
        .done(
            Method::Put,
            &format!("{BOOKS}/{}", held.id),
            Some(wanted.to_string()),
        )
        .await?;
    if ask.search {
        searched(bindery, BOOKS, held.id).await?;
    }
    Ok(())
}

async fn added_book(
    bindery: &Bindery,
    ask: &BookAsk,
    reference: &Reference,
    filed: &Filed,
) -> Result<Option<Book>, Failure> {
    let found = bindery
        .found_book(reference)
        .await?
        .ok_or_else(|| bindery.not_in_the_catalogue())?;
    if let Some(id) = found.library_book_id {
        return Ok(Some(bindery.held_book(id).await?));
    }
    let by = found.author.clone().unwrap_or_default();
    let followed = bindery
        .library_author(&by.foreign_author_id, &by.author_name)
        .await?
        .is_some();
    let body = json!({
        "foreignBookId": found.foreign_book_id,
        "foreignAuthorId": by.foreign_author_id,
        "authorName": by.author_name,
        "searchOnAdd": ask.search,
    });
    let response = bindery
        .answered(Method::Post, ADD_BOOK, Some(body.to_string()))
        .await?;
    let added: Option<Added> = serde_json::from_str(&response.body).ok();
    match (response.status, added) {
        (CREATED, Some(Added { by: Some(by), .. })) => {
            if !followed {
                let filed = json!({
                    "rootFolderId": filed.folder,
                    "qualityProfileId": filed.profile,
                });
                edited(bindery, by, &filed).await?;
            }
            Ok(None)
        }
        (
            CONFLICT,
            Some(Added {
                existing: Some(id), ..
            }),
        ) => Ok(Some(bindery.held_book(id).await?)),
        _ => Err(bindery.endpoint.refusal(&response)),
    }
}

async fn author(
    bindery: &Bindery,
    ask: &BookAsk,
    reference: &Reference,
    filed: &Filed,
) -> Result<(), Failure> {
    let held = if let Some(held) = bindery
        .library_author(&reference.id, reference.word(0))
        .await?
    {
        Some(held.id)
    } else {
        added_author(bindery, ask, reference, filed).await?
    };
    let Some(id) = held else {
        return Ok(());
    };
    let wanted = json!({
        "monitored": true,
        "monitorMode": EVERY_BOOK,
        "monitorNewItems": EVERY_BOOK,
        "applyMonitorModeToExisting": true,
    });
    edited(bindery, id, &wanted).await?;
    if ask.search {
        searched(bindery, AUTHORS, id).await?;
    }
    Ok(())
}

async fn added_author(
    bindery: &Bindery,
    ask: &BookAsk,
    reference: &Reference,
    filed: &Filed,
) -> Result<Option<i64>, Failure> {
    let found = bindery
        .found_author(reference)
        .await?
        .ok_or_else(|| bindery.not_in_the_catalogue())?;
    if let Some(id) = found.library_author_id {
        return Ok(Some(id));
    }
    let body = json!({
        "foreignAuthorId": found.foreign_author_id,
        "authorName": found.name,
        "rootFolderId": filed.folder,
        "qualityProfileId": filed.profile,
        "monitored": true,
        "monitorMode": EVERY_BOOK,
        "monitorNewItems": EVERY_BOOK,
        "searchOnAdd": ask.search,
    });
    let response = bindery
        .answered(Method::Post, AUTHORS, Some(body.to_string()))
        .await?;
    if response.is_success() {
        return Ok(None);
    }
    match serde_json::from_str::<Added>(&response.body) {
        Ok(Added {
            canonical: Some(id),
            ..
        }) if response.status == CONFLICT => Ok(Some(id)),
        _ => Err(bindery.endpoint.refusal(&response)),
    }
}

async fn series(bindery: &Bindery, reference: &Reference, filed: &Filed) -> Result<(), Failure> {
    let found = bindery.series_found(reference.word(0)).await?;
    let held = bindery.held_series().await?;
    let id = if let Some(held) = held.iter().find(|one| one.is(&reference.id)) {
        held.id
    } else {
        let found = found
            .iter()
            .find(|one| one.foreign_id == reference.id)
            .ok_or_else(|| bindery.not_in_the_catalogue())?;
        linked(bindery, found).await?
    };
    let followed = followed(bindery).await?;
    bindery
        .done(
            Method::Patch,
            &format!("{SERIES}/{id}"),
            Some(json!({ "monitored": true }).to_string()),
        )
        .await?;
    let format = bindery.household_format().await?;
    bindery
        .done(
            Method::Post,
            &format!("{SERIES}/{id}/fill"),
            Some(json!({ "mediaType": format }).to_string()),
        )
        .await?;
    let standing: Series = bindery.read(&format!("{SERIES}/{id}"), THE_SERIES).await?;
    let added: BTreeSet<i64> = standing
        .held()
        .map(|book| book.author_id)
        .filter(|by| *by > 0 && !followed.contains(by))
        .collect();
    for by in added {
        let theirs: Vec<Series> = bindery
            .read(&format!("{AUTHORS}/{by}/series"), THE_SERIES)
            .await?;
        let pinned: Vec<i64> = theirs
            .iter()
            .filter(|one| one.id == id || one.is(&reference.id))
            .map(|one| one.id)
            .collect();
        let following = json!({
            "monitored": true,
            "monitorMode": SERIES_ONLY,
            "monitorNewItems": EVERY_BOOK,
            "monitoredSeriesIds": pinned,
            "rootFolderId": filed.folder,
            "qualityProfileId": filed.profile,
        });
        edited(bindery, by, &following).await?;
    }
    Ok(())
}

async fn linked(bindery: &Bindery, found: &FoundSeries) -> Result<i64, Failure> {
    let made: Made = bindery
        .written(
            Method::Post,
            SERIES,
            &json!({ "title": found.title }),
            THE_SERIES,
        )
        .await?;
    let link = json!({
        "foreignId": found.foreign_id,
        "providerId": found.provider_id,
        "slug": found.slug,
        "title": found.title,
        "authorName": found.author_name,
        "bookCount": found.book_count,
    });
    bindery
        .done(
            Method::Put,
            &format!("{SERIES}/{}/hardcover-link", made.id),
            Some(link.to_string()),
        )
        .await?;
    Ok(made.id)
}

async fn followed(bindery: &Bindery) -> Result<BTreeSet<i64>, Failure> {
    let mut every = BTreeSet::new();
    let mut offset = 0;
    loop {
        let page: Page<Author> = bindery
            .read(
                &format!("{AUTHORS}?limit={HELD_PAGE}&offset={offset}"),
                THE_AUTHORS,
            )
            .await?;
        let count = page.items.len();
        every.extend(page.items.into_iter().map(|one| one.id));
        if count < HELD_PAGE {
            return Ok(every);
        }
        offset += count;
    }
}

async fn edited(bindery: &Bindery, id: i64, body: &Value) -> Result<(), Failure> {
    bindery
        .done(
            Method::Put,
            &format!("{AUTHORS}/{id}"),
            Some(body.to_string()),
        )
        .await
}

async fn searched(bindery: &Bindery, path: &str, id: i64) -> Result<(), Failure> {
    let body = json!({ "ids": [id], "action": SEARCH });
    bindery
        .done(
            Method::Post,
            &format!("{path}/bulk"),
            Some(body.to_string()),
        )
        .await
}
