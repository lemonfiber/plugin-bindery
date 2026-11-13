use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::TitleKind;
use lemonfiber_ports::service::stage::{Outcome, Stage};
use lemonfiber_ports::service::{
    Failure, FoundItem, Pipeline as _, QualityReleases as _, QueueItem, ReleaseProbe, StuckItem,
    TraceEvent,
};

use super::super::queued::fingerprint;
use super::super::tests::{asked, bindery};

const OTHER_KINDS: [TitleKind; 3] = [TitleKind::Tv, TitleKind::Movies, TitleKind::Music];

fn shelved(books: &serde_json::Value) -> Answer {
    Answer::reply(
        200,
        serde_json::json!({ "items": books, "total": 0 }).to_string(),
    )
}

fn dune() -> serde_json::Value {
    serde_json::json!({
        "id": 4, "title": "Dune", "releaseDate": "1965-08-01T00:00:00Z", "monitored": true,
        "authorId": 2, "author": {"authorName": "Frank Herbert"}
    })
}

fn found(id: i64, title: &str, year: Option<u16>, monitored: bool) -> FoundItem {
    FoundItem {
        id,
        title: title.to_owned(),
        year,
        monitored,
    }
}

fn queue() -> Answer {
    Answer::reply(
        200,
        serde_json::json!({ "items": [
            {"id": 11, "bookId": 4, "guid": "https://indexer/1", "title": "Dune.EPUB",
             "status": "downloading", "book": {"id": 4, "title": "Dune"}},
            {"id": 12, "bookId": 4, "guid": "https://indexer/2", "title": "Dune.AZW3",
             "status": "importFailed", "errorMessage": "permission denied",
             "book": {"id": 4, "title": "Dune"}},
            {"id": 13, "bookId": 5, "guid": "https://indexer/3", "title": "Emma.EPUB",
             "status": "failed", "book": {"id": 5, "title": "Emma"}},
            {"id": 14, "bookId": 4, "guid": "https://indexer/4", "title": "Dune.MOBI",
             "status": "importBlocked", "book": {"id": 4, "title": "Dune"}},
            {"id": 15, "guid": "https://indexer/5", "title": "Loose.EPUB", "status": "failed"},
            {"id": 16, "bookId": 6, "guid": "https://indexer/6", "title": "Done.EPUB",
             "status": "imported", "errorMessage": "kept", "book": {"id": 6, "title": "Done"}},
            {"id": 17, "bookId": 7, "guid": "", "title": "Untitled", "status": "failed",
             "book": {"id": 7, "title": ""}}
        ]})
        .to_string(),
    )
}

#[tokio::test]
async fn the_library_is_every_book_bindery_holds_read_a_page_at_a_time() {
    let full: Vec<serde_json::Value> = (1..=500)
        .map(|id| serde_json::json!({ "id": id, "title": format!("Book {id}") }))
        .collect();
    let fake = Fake::by_path(vec![
        ("offset=500", shelved(&serde_json::json!([dune()]))),
        ("offset=0", shelved(&serde_json::json!(full))),
    ]);
    let library = bindery(&fake)
        .library(TitleKind::Books)
        .await
        .unwrap_or_default();
    assert_eq!(library.len(), 501);
    assert_eq!(library.first(), Some(&found(1, "Book 1", None, false)));
    assert_eq!(library.last(), Some(&found(4, "Dune", Some(1965), true)));
    assert_eq!(
        asked(&fake)
            .into_iter()
            .map(|(_, path, _)| path)
            .collect::<Vec<_>>(),
        [
            "/api/v1/book?limit=500&offset=0",
            "/api/v1/book?limit=500&offset=500"
        ]
    );
}

#[tokio::test]
async fn a_book_is_found_by_its_own_title_and_never_by_its_author_s_name() {
    let fake = Fake::always(shelved(&serde_json::json!([
        dune(),
        {"id": 5, "title": "Children of Dune", "monitored": false},
        {"id": 6, "title": "The Dosadi Experiment", "author": {"authorName": "Frank Herbert"}}
    ])));
    assert_eq!(
        bindery(&fake)
            .find_items(TitleKind::Books, " dune ")
            .await
            .ok(),
        Some(vec![
            found(4, "Dune", Some(1965), true),
            found(5, "Children of Dune", None, false)
        ])
    );
    let herbert = bindery(&fake)
        .find_items(TitleKind::Books, "Herbert")
        .await
        .ok();
    assert_eq!(herbert, Some(Vec::new()));
    assert_eq!(
        asked(&fake).first().map(|(_, path, _)| path.as_str()),
        Some("/api/v1/book?limit=500&offset=0&search=dune")
    );
}

#[tokio::test]
async fn a_book_s_history_is_what_bindery_recorded_of_it_alone() {
    let fake = Fake::always(Answer::reply(
        200,
        serde_json::json!({ "items": [
            {"id": 9, "bookId": 4, "eventType": "bookImported", "createdAt": "2026-11-02T10:00:00Z"},
            {"id": 8, "bookId": 4, "eventType": "bookRenamed", "createdAt": "2026-11-02T09:30:00Z"},
            {"id": 7, "bookId": 4, "eventType": "downloadFolderImported", "createdAt": "2026-11-02T09:00:00Z"},
            {"id": 6, "bookId": 4, "eventType": "downloadStalled", "createdAt": "2026-11-01T12:00:00Z"},
            {"id": 5, "bookId": 4, "eventType": "downloadFailed", "createdAt": "2026-11-01T11:00:00Z"},
            {"id": 4, "bookId": 4, "eventType": "grabbed", "createdAt": "2026-11-01T10:00:00Z"},
            {"id": 3, "bookId": 5, "eventType": "grabbed", "createdAt": "2026-11-01T09:00:00Z"},
            {"id": 2, "eventType": "grabbed", "createdAt": "2026-11-01T08:00:00Z"},
            {"id": 1, "bookId": 4, "eventType": "bookFileDeleted", "createdAt": "2026-10-01T08:00:00Z"}
        ]})
        .to_string(),
    ));
    let event = |outcome, at: &str| TraceEvent {
        outcome,
        at: at.to_owned(),
        part: None,
    };
    assert_eq!(
        bindery(&fake).item_history(TitleKind::Books, 4).await.ok(),
        Some(vec![
            event(Outcome::Imported, "2026-11-02T10:00:00Z"),
            event(Outcome::DownloadFailed, "2026-11-01T12:00:00Z"),
            event(Outcome::DownloadFailed, "2026-11-01T11:00:00Z"),
            event(Outcome::Grabbed, "2026-11-01T10:00:00Z"),
            event(Outcome::Removed, "2026-10-01T08:00:00Z"),
        ])
    );
    assert_eq!(
        bindery(&fake).item_history(TitleKind::Books, 0).await.ok(),
        Some(Vec::new())
    );
    assert_eq!(
        asked(&fake).first().map(|(_, path, _)| path.as_str()),
        Some("/api/v1/history?limit=100&bookId=4")
    );
}

#[tokio::test]
async fn a_book_s_queue_is_each_download_still_on_its_way_for_it() {
    let fake = Fake::always(queue());
    assert_eq!(
        bindery(&fake).item_queue(TitleKind::Books, 4).await.ok(),
        Some(vec![
            QueueItem {
                part: None,
                stage: Stage::Downloading,
                stuck: false,
            },
            QueueItem {
                part: None,
                stage: Stage::Downloaded,
                stuck: true,
            },
            QueueItem {
                part: None,
                stage: Stage::Downloaded,
                stuck: true,
            },
        ])
    );
    assert_eq!(
        bindery(&fake).item_queue(TitleKind::Books, 6).await.ok(),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn a_book_has_no_parts_and_bindery_is_not_asked_for_any() {
    let fake = Fake::silent();
    assert_eq!(
        bindery(&fake)
            .item_parts(TitleKind::Books, 4, Some(1))
            .await
            .ok(),
        Some(Vec::new())
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn each_stuck_book_is_named_once_by_its_own_title_with_its_entry_and_release() {
    let fake = Fake::always(queue());
    assert_eq!(
        bindery(&fake).stuck_items(TitleKind::Books).await.ok(),
        Some(vec![
            StuckItem {
                title: "Dune".to_owned(),
                stage: Stage::Downloaded,
                queued: Some(12),
                title_id: Some(4),
                release: fingerprint("https://indexer/2"),
            },
            StuckItem {
                title: "Emma".to_owned(),
                stage: Stage::Downloading,
                queued: Some(13),
                title_id: Some(5),
                release: fingerprint("https://indexer/3"),
            },
        ])
    );
}

#[tokio::test]
async fn what_it_would_grab_is_read_from_bindery_s_search_for_the_first_book_it_wants() {
    let wanted = || {
        Answer::reply(
            200,
            r#"{"items":[{"id":4,"title":"Dune"}],"total":3,"limit":1,"offset":0}"#,
        )
    };
    let searched = |results: serde_json::Value| {
        Answer::reply(
            200,
            serde_json::json!({ "results": results, "debug": null }).to_string(),
        )
    };
    for (results, probe) in [
        (serde_json::json!([]), ReleaseProbe::NoneFound),
        (
            serde_json::json!([{"guid": "a", "approved": false, "rejection": "release is blocklisted"}, {"guid": "b", "approved": true}]),
            ReleaseProbe::Matching,
        ),
        (
            serde_json::json!([{"guid": "a", "approved": false}]),
            ReleaseProbe::NoneMatch,
        ),
    ] {
        let fake = Fake::by_route(vec![
            (Method::Get, "/wanted/missing", wanted()),
            (Method::Post, "/book/4/search", searched(results)),
        ]);
        assert_eq!(
            bindery(&fake).probe_releases(TitleKind::Books).await.ok(),
            Some(probe)
        );
        assert_eq!(
            asked(&fake)
                .into_iter()
                .map(|(method, path, _)| (method, path))
                .collect::<Vec<_>>(),
            [
                (
                    Method::Get,
                    "/api/v1/wanted/missing?limit=1&offset=0".to_owned()
                ),
                (Method::Post, "/api/v1/book/4/search".to_owned()),
            ]
        );
    }
    let nothing = Fake::always(Answer::reply(200, r#"{"items":[],"total":0}"#));
    assert_eq!(
        bindery(&nothing)
            .probe_releases(TitleKind::Books)
            .await
            .ok(),
        Some(ReleaseProbe::NothingWanted)
    );
    let failing = Fake::by_route(vec![
        (Method::Get, "/wanted/missing", wanted()),
        (Method::Post, "/book/4/search", Answer::reply(500, "{}")),
    ]);
    assert!(matches!(
        bindery(&failing).probe_releases(TitleKind::Books).await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn every_kind_but_books_is_unsupported_and_bindery_is_not_asked() {
    let fake = Fake::silent();
    let asking = bindery(&fake);
    for kind in OTHER_KINDS {
        let answers = [
            asking.library(kind).await.err(),
            asking.find_items(kind, "Dune").await.err(),
            asking.item_history(kind, 4).await.err(),
            asking.item_queue(kind, 4).await.err(),
            asking.item_parts(kind, 4, None).await.err(),
            asking.stuck_items(kind).await.err(),
            asking.probe_releases(kind).await.err(),
        ];
        assert!(
            answers
                .iter()
                .all(|answer| matches!(answer, Some(Failure::Unsupported { .. }))),
            "{kind:?}: {answers:?}"
        );
    }
    assert!(fake.requests().is_empty());
}
