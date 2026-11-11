use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::{Kind, TitleKind};
use lemonfiber_ports::service::{
    CuratedTitle, DownloadState, Downloading, Failure, Fulfilling as _, Grabbed, TitleRequest,
};

use super::super::queued::fingerprint;
use super::super::tests::{asked, bindery};

const DUNE: &str = r#"{"id":4,"title":"Dune","monitored":true}"#;

const RELEASE: &str = "https://indexer/details/dune-epub";

fn routes(fake: &Fake) -> Vec<(Method, String)> {
    asked(fake)
        .into_iter()
        .map(|(method, path, _)| (method, path))
        .collect()
}

fn refused_with<T>(answer: Result<T, Failure>, words: &str) -> bool {
    matches!(answer.err(), Some(Failure::Refused { detail, .. }) if detail.contains(words))
}

fn queue() -> Answer {
    Answer::reply(
        200,
        serde_json::json!({ "items": [
            {"id": 11, "bookId": 4, "guid": RELEASE, "title": "Dune.EPUB", "size": 2000,
             "percentage": "25", "status": "downloading", "torrentId": "ABC",
             "book": {"id": 4, "title": "Dune"}},
            {"id": 12, "guid": "https://indexer/loose", "title": "Loose.EPUB", "size": 10,
             "status": "grabbed"},
            {"id": 13, "bookId": 5, "guid": "https://indexer/done", "title": "Done.EPUB",
             "status": "imported"},
            {"id": 14, "bookId": 6, "guid": "", "title": "Nameless", "status": "failed"}
        ]})
        .to_string(),
    )
}

fn grabs(guid: &str) -> Answer {
    Answer::reply(
        200,
        serde_json::json!({ "items": [
            {"id": 30, "bookId": 4, "eventType": "grabbed", "data": "not json"},
            {"id": 31, "bookId": 4, "eventType": "grabbed",
             "data": serde_json::json!({"guid": "https://indexer/other"}).to_string()},
            {"id": 32, "bookId": 4, "eventType": "grabbed",
             "data": serde_json::json!({"guid": guid, "size": 2000}).to_string()}
        ]})
        .to_string(),
    )
}

#[tokio::test]
async fn every_book_bindery_holds_is_a_title_under_its_own_id() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"{"items":[
          {"id":4,"title":"Dune","releaseDate":"1965-08-01T00:00:00Z","monitored":true,
           "filePath":"/books/Dune.epub","ebookFilePath":"/books/Dune.epub",
           "audiobookFilePath":"/audiobooks/Dune"},
          {"id":5,"title":"Emma","monitored":false}
        ]}"#,
    ));
    let title = |id: i64, title: &str, year, monitored, files| CuratedTitle {
        reference: id,
        id: Some(id),
        title: title.to_owned(),
        year,
        monitored,
        files,
        seasons: Vec::new(),
    };
    assert_eq!(
        bindery(&fake).titles(TitleKind::Books).await.ok(),
        Some(vec![
            title(4, "Dune", Some(1965), true, 2),
            title(5, "Emma", None, false, 0),
        ])
    );
}

#[tokio::test]
async fn a_book_it_holds_is_searched_for_and_grabbed_by_bindery_s_own_search() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/book/4", Answer::reply(200, DUNE)),
        (
            Method::Post,
            "/wanted/bulk",
            Answer::reply(200, r#"{"results":{"4":{"ok":true,"queued":true}}}"#),
        ),
    ]);
    assert!(bindery(&fake).search(TitleKind::Books, 4).await.is_ok());
    assert_eq!(
        asked(&fake).last(),
        Some(&(
            Method::Post,
            "/api/v1/wanted/bulk".to_owned(),
            Some(serde_json::json!({ "ids": [4], "action": "search" }))
        ))
    );
}

#[tokio::test]
async fn a_search_bindery_refuses_or_does_not_answer_for_the_book_is_refused_in_its_words() {
    for (said, words) in [
        (
            r#"{"results":{"4":{"ok":false,"code":"auto_grab_disabled","error":"automatic grabbing is disabled"}}}"#,
            "automatic grabbing is disabled",
        ),
        (r#"{"results":{"4":{"ok":false}}}"#, "did not say"),
        (r#"{"results":{}}"#, "did not say"),
    ] {
        let fake = Fake::by_route(vec![
            (Method::Get, "/book/4", Answer::reply(200, DUNE)),
            (Method::Post, "/wanted/bulk", Answer::reply(200, said)),
        ]);
        assert!(
            refused_with(bindery(&fake).search(TitleKind::Books, 4).await, words),
            "{said}"
        );
    }
}

#[tokio::test]
async fn a_book_bindery_does_not_hold_is_refused_before_anything_is_searched_or_grabbed() {
    let fake = Fake::always(Answer::reply(404, r#"{"error":"book not found"}"#));
    assert!(refused_with(
        bindery(&fake).search(TitleKind::Books, 0).await,
        "it holds no book as 0"
    ));
    assert!(refused_with(
        bindery(&fake).grab(TitleKind::Books, 0, RELEASE).await,
        "it holds no book as 0"
    ));
    assert_eq!(
        routes(&fake),
        [
            (Method::Get, "/api/v1/book/0".to_owned()),
            (Method::Get, "/api/v1/book/0".to_owned())
        ]
    );
}

#[tokio::test]
async fn what_it_is_downloading_is_each_download_for_a_book_not_yet_imported() {
    let fake = Fake::always(queue());
    assert_eq!(
        bindery(&fake).downloading(TitleKind::Books).await.ok(),
        Some(vec![
            Downloading {
                title_id: 4,
                release: "Dune.EPUB".to_owned(),
                bytes: 2000,
                left: 1500,
                expected: None,
                state: DownloadState::Downloading,
                download_id: Some("abc".to_owned()),
                queued: Some(11),
                episode: None,
            },
            Downloading {
                title_id: 6,
                release: "Nameless".to_owned(),
                bytes: 0,
                left: 0,
                expected: None,
                state: DownloadState::Failed,
                download_id: None,
                queued: Some(14),
                episode: None,
            },
        ])
    );
}

#[tokio::test]
async fn a_download_is_dropped_with_its_release_blocklisted_from_its_grab_and_fingerprinted() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/queue", queue()),
        (Method::Get, "/history", grabs(RELEASE)),
        (
            Method::Post,
            "/history/32/blocklist",
            Answer::reply(201, "{}"),
        ),
        (Method::Delete, "/queue/11", Answer::reply(204, "")),
    ]);
    assert_eq!(
        bindery(&fake)
            .unqueue(TitleKind::Books, 11, true)
            .await
            .ok(),
        fingerprint(RELEASE)
    );
    assert_eq!(
        routes(&fake),
        [
            (Method::Get, "/api/v1/queue".to_owned()),
            (
                Method::Get,
                "/api/v1/history?limit=100&bookId=4&eventType=grabbed".to_owned()
            ),
            (Method::Post, "/api/v1/history/32/blocklist".to_owned()),
            (Method::Delete, "/api/v1/queue/11".to_owned()),
        ]
    );
    let kept = Fake::by_route(vec![
        (Method::Get, "/queue", queue()),
        (Method::Delete, "/queue/12", Answer::reply(204, "")),
    ]);
    assert_eq!(
        bindery(&kept)
            .unqueue(TitleKind::Books, 12, false)
            .await
            .ok(),
        fingerprint("https://indexer/loose")
    );
    assert_eq!(
        routes(&kept),
        [
            (Method::Get, "/api/v1/queue".to_owned()),
            (Method::Delete, "/api/v1/queue/12".to_owned()),
        ]
    );
}

#[tokio::test]
async fn a_download_that_cannot_be_dropped_as_asked_is_refused_before_anything_is_removed() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/queue", queue()),
        (Method::Get, "/history", grabs("https://indexer/elsewhere")),
    ]);
    assert!(refused_with(
        bindery(&fake).unqueue(TitleKind::Books, 0, true).await,
        "it holds no queue entry 0"
    ));
    assert!(refused_with(
        bindery(&fake).unqueue(TitleKind::Books, 14, true).await,
        "names no release"
    ));
    assert!(refused_with(
        bindery(&fake).unqueue(TitleKind::Books, 11, true).await,
        "nothing to blocklist it from"
    ));
    assert!(routes(&fake)
        .iter()
        .all(|(method, _)| *method == Method::Get));
}

#[tokio::test]
async fn a_release_is_grabbed_again_by_its_fingerprint_with_its_blocklist_entries_lifted() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/book/4", Answer::reply(200, DUNE)),
        (
            Method::Post,
            "/book/4/search",
            Answer::reply(
                200,
                serde_json::json!({ "results": [
                    {"guid": "https://indexer/details/other", "title": "Dune.MOBI"},
                    {"guid": RELEASE, "title": "Dune.EPUB", "nzbUrl": "https://indexer/get/1",
                     "size": 2000, "indexerId": 3, "protocol": "usenet", "mediaType": "ebook",
                     "approved": false, "rejection": "release is blocklisted"}
                ]})
                .to_string(),
            ),
        ),
        (
            Method::Get,
            "/blocklist",
            Answer::reply(
                200,
                serde_json::json!([
                    {"id": 1, "bookId": 4, "guid": RELEASE, "title": "anything"},
                    {"id": 2, "bookId": 4, "guid": "https://indexer/details/dune-epub?passkey=s", "title": "Dune.EPUB"},
                    {"id": 3, "bookId": 5, "guid": RELEASE, "title": "Dune.EPUB"},
                    {"id": 4, "bookId": 4, "guid": "https://indexer/details/other", "title": "Dune.MOBI"}
                ])
                .to_string(),
            ),
        ),
        (Method::Delete, "/blocklist/", Answer::reply(204, "")),
        (Method::Post, "/queue/grab", Answer::reply(202, "{}")),
    ]);
    let release = fingerprint(RELEASE).unwrap_or_default();
    assert_eq!(
        bindery(&fake)
            .grab(TitleKind::Books, 4, &release)
            .await
            .ok(),
        Some(Grabbed::Grabbed)
    );
    let said = asked(&fake);
    assert_eq!(
        said.iter()
            .map(|(method, path, _)| (*method, path.as_str()))
            .collect::<Vec<_>>(),
        [
            (Method::Get, "/api/v1/book/4"),
            (Method::Post, "/api/v1/book/4/search"),
            (Method::Get, "/api/v1/blocklist"),
            (Method::Delete, "/api/v1/blocklist/1"),
            (Method::Delete, "/api/v1/blocklist/2"),
            (Method::Post, "/api/v1/queue/grab"),
        ]
    );
    assert_eq!(
        said.last().and_then(|(_, _, body)| body.clone()),
        Some(serde_json::json!({
            "guid": RELEASE, "title": "Dune.EPUB", "nzbUrl": "https://indexer/get/1",
            "size": 2000, "bookId": 4, "indexerId": 3, "protocol": "usenet",
            "mediaType": "ebook",
        }))
    );
}

#[tokio::test]
async fn a_release_no_indexer_offers_any_more_is_gone_with_its_own_blocklist_entry_lifted() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/book/4", Answer::reply(200, DUNE)),
        (
            Method::Post,
            "/book/4/search",
            Answer::reply(200, r#"{"results":[{"guid":"https://indexer/details/other","title":"Dune.EPUB"}]}"#),
        ),
        (
            Method::Get,
            "/blocklist",
            Answer::reply(
                200,
                serde_json::json!([
                    {"id": 1, "bookId": 4, "guid": RELEASE, "title": "Dune.EPUB"},
                    {"id": 2, "bookId": 4, "guid": "https://indexer/details/other", "title": "Dune.EPUB"}
                ])
                .to_string(),
            ),
        ),
        (Method::Delete, "/blocklist/1", Answer::reply(204, "")),
    ]);
    let release = fingerprint(RELEASE).unwrap_or_default();
    assert_eq!(
        bindery(&fake)
            .grab(TitleKind::Books, 4, &release)
            .await
            .ok(),
        Some(Grabbed::Gone)
    );
    assert_eq!(
        routes(&fake).last(),
        Some(&(Method::Delete, "/api/v1/blocklist/1".to_owned()))
    );
}

#[tokio::test]
async fn every_kind_but_books_and_every_video_only_operation_is_unsupported_unasked() {
    let fake = Fake::silent();
    let asking = bindery(&fake);
    let request = TitleRequest {
        reference: 1,
        root_folder: "/books".to_owned(),
        quality_profile: 1,
        seasons: Vec::new(),
        anime: false,
        later_seasons: false,
        search: true,
    };
    let unsupported = |answer: Option<Failure>| matches!(answer, Some(Failure::Unsupported { .. }));
    for kind in [TitleKind::Tv, TitleKind::Movies, TitleKind::Music] {
        assert!(unsupported(asking.titles(kind).await.err()));
        assert!(unsupported(asking.search(kind, 4).await.err()));
        assert!(unsupported(asking.downloading(kind).await.err()));
        assert!(unsupported(asking.unqueue(kind, 11, true).await.err()));
        assert!(unsupported(asking.grab(kind, 4, RELEASE).await.err()));
    }
    for kind in Kind::ALL {
        assert!(unsupported(asking.catalogued(kind, 1).await.err()));
        assert!(unsupported(asking.held_title(kind, 1).await.err()));
        assert!(unsupported(asking.request(kind, &request).await.err()));
        assert!(unsupported(asking.remove(kind, 1).await.err()));
    }
    assert!(fake.requests().is_empty());
}
