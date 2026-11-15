//! Taking an ask back off Bindery: a book alone, an author with every book of theirs, or a
//! series with every book of it, each with its files and nothing excluded from a later
//! import; with each author an ask added only for what is removed, where nothing else of
//! theirs is kept.

use std::collections::BTreeSet;

use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{BookUnit, Failure};

use super::asking::SERIES_ONLY;
use super::reference::Reference;
use super::wire::Book;
use super::{words, Bindery, AUTHORS, BOOKS, SERIES};

const WITH_FILES: &str = "deleteFiles=true";

const NOT_HELD: &str = "it holds nothing by that reference";

pub(super) async fn remove(bindery: &Bindery, unit: BookUnit, asked: &str) -> Result<(), Failure> {
    let reference =
        Reference::read(asked, words(unit)).ok_or_else(|| bindery.not_in_the_catalogue())?;
    match unit {
        BookUnit::Book => book(bindery, &reference).await,
        BookUnit::Author => author(bindery, &reference).await,
        BookUnit::Series => series(bindery, &reference).await,
    }
}

async fn book(bindery: &Bindery, reference: &Reference) -> Result<(), Failure> {
    let held = only(bindery, bindery.library_books(reference).await?, |one| {
        one.id
    })?;
    deleted(bindery, BOOKS, held.id).await?;
    gone_with(bindery, held.author_id, None).await
}

async fn author(bindery: &Bindery, reference: &Reference) -> Result<(), Failure> {
    let held = only(
        bindery,
        bindery
            .library_authors(&reference.id, reference.word(0))
            .await?,
        |one| one.id,
    )?;
    deleted(bindery, AUTHORS, held.id).await
}

async fn series(bindery: &Bindery, reference: &Reference) -> Result<(), Failure> {
    let matching: Vec<_> = bindery
        .held_series()
        .await?
        .into_iter()
        .filter(|one| one.is(&reference.id))
        .collect();
    let held = only(bindery, matching, |one| one.id)?;
    let mut by = BTreeSet::new();
    for book in held.held().filter(|book| book.id > 0) {
        deleted(bindery, BOOKS, book.id).await?;
        by.insert(book.author_id);
    }
    for author in by {
        gone_with(bindery, author, Some(held.id)).await?;
    }
    bindery
        .done(Method::Delete, &format!("{SERIES}/{}", held.id), None)
        .await
}

async fn deleted(bindery: &Bindery, path: &str, id: i64) -> Result<(), Failure> {
    bindery
        .done(Method::Delete, &format!("{path}/{id}?{WITH_FILES}"), None)
        .await
}

async fn gone_with(bindery: &Bindery, by: i64, series: Option<i64>) -> Result<(), Failure> {
    if by <= 0 {
        return Ok(());
    }
    let author = bindery.held_author(by).await?;
    let added_for_it = match series {
        None => !author.monitored,
        Some(id) => {
            author.monitored
                && author.monitor_mode == SERIES_ONLY
                && author.monitored_series_ids.iter().all(|one| *one == id)
        }
    };
    if added_for_it && !author.books.iter().any(kept) {
        deleted(bindery, AUTHORS, by).await?;
    }
    Ok(())
}

fn only<T>(bindery: &Bindery, matching: Vec<T>, id: impl Fn(&T) -> i64) -> Result<T, Failure> {
    let mut matching = matching.into_iter().filter(|one| id(one) > 0);
    match (matching.next(), matching.next()) {
        (Some(held), None) => Ok(held),
        _ => Err(bindery.endpoint.refused(NOT_HELD)),
    }
}

fn kept(book: &Book) -> bool {
    book.monitored || book.here()
}
