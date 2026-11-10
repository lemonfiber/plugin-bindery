use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    BookAsk, BookCurating as _, BookUnit, Candidate, CandidateState, CuratedUnit, Failure, Profile,
};
use serde_json::Value;

use super::super::tests::bindery;
use super::reference::Reference;

pub(super) const DUNE: &str = "OL893414W";

pub(super) const HERBERT: &str = "OL79034A";

pub(super) const SERIES: &str = "hc-series:7";

pub(super) fn ok(body: &str) -> Answer {
    Answer::reply(200, body.to_owned())
}

pub(super) fn dune() -> String {
    Reference::composed(DUNE, &["Dune", "Frank Herbert"])
}

pub(super) fn herbert() -> String {
    Reference::composed(HERBERT, &["Frank Herbert"])
}

pub(super) fn saga() -> String {
    Reference::composed(SERIES, &["Dune", "Frank Herbert"])
}

pub(super) fn found_book(id: &str, held: &str) -> String {
    format!(
        r#"{{"foreignBookId":"{id}","title":"Dune","releaseDate":"1965-01-01T00:00:00Z",
            "imageUrl":"https://covers.example/dune.jpg",
            "author":{{"foreignAuthorId":"{HERBERT}","authorName":"Frank Herbert"}}{held}}}"#
    )
}

pub(super) fn held_book(id: i64, status: &str, rest: &str) -> String {
    format!(
        r#"{{"id":{id},"foreignBookId":"{DUNE}","title":"Dune","status":"{status}",
            "imageUrl":"/api/v1/images?url=https%3A%2F%2Fcovers.example%2Fheld.jpg",
            "author":{{"authorName":"Frank Herbert"}}{rest}}}"#
    )
}

pub(super) fn found_author(held: &str) -> String {
    format!(r#"{{"foreignAuthorId":"{HERBERT}","authorName":"Frank Herbert"{held}}}"#)
}

pub(super) fn found_series() -> String {
    format!(
        r#"{{"foreignId":"{SERIES}","providerId":"7","slug":"dune","title":"Dune",
            "authorName":"Frank Herbert","bookCount":6}}"#
    )
}

pub(super) fn rules(routes: Vec<(Method, &'static str, Answer)>) -> Arc<Fake> {
    let mut every: Vec<(Option<Method>, &'static str, Answer)> = routes
        .into_iter()
        .map(|(method, fragment, answer)| (Some(method), fragment, answer))
        .collect();
    every.extend([
        (
            Some(Method::Get),
            "/setting/default.media_type",
            Answer::reply(404, r#"{"error":"setting not found"}"#),
        ),
        (
            Some(Method::Get),
            "/rootfolder",
            ok(r#"[{"id":1,"path":"/books"}]"#),
        ),
        (
            Some(Method::Get),
            "/qualityprofile",
            ok(r#"[{"id":2,"name":"E-Book"}]"#),
        ),
    ]);
    Fake::by_rules(every)
}

pub(super) fn in_turn(routes: Vec<(Method, &'static str, Vec<Answer>)>) -> Arc<Fake> {
    let mut every = routes;
    every.extend([
        (
            Method::Get,
            "/setting/default.media_type",
            vec![Answer::reply(404, "{}")],
        ),
        (
            Method::Get,
            "/rootfolder",
            vec![ok(r#"[{"id":1,"path":"/books"}]"#)],
        ),
        (
            Method::Get,
            "/qualityprofile",
            vec![ok(r#"[{"id":2,"name":"E-Book"}]"#)],
        ),
    ]);
    Fake::by_route_in_turn(every)
}

pub(super) fn asking(unit: BookUnit, reference: &str, search: bool) -> BookAsk {
    BookAsk {
        unit,
        reference: reference.to_owned(),
        root_folder: "/books".to_owned(),
        quality_profile: 2,
        search,
    }
}

pub(super) fn sent(fake: &Fake, method: Method, fragment: &str) -> Vec<Value> {
    fake.requests()
        .into_iter()
        .filter(|one| one.method == method && one.url.contains(fragment))
        .map(|one| {
            one.body
                .and_then(|body| serde_json::from_str(&body).ok())
                .unwrap_or_default()
        })
        .collect()
}

pub(super) fn refused(failure: Option<Failure>) -> String {
    match failure {
        Some(Failure::Refused { detail, .. } | Failure::Unsupported { detail, .. }) => detail,
        other => format!("not a refusal: {other:?}"),
    }
}

fn states(found: &[Candidate]) -> Vec<CandidateState> {
    found.iter().map(|one| one.state).collect()
}

#[test]
fn a_reference_carries_the_foreign_id_and_the_words_that_find_it_again() {
    let read = Reference::read(&dune(), 2);
    assert_eq!(read.as_ref().map(|one| one.id.as_str()), Some(DUNE));
    assert_eq!(read.as_ref().map(|one| one.word(1)), Some("Frank Herbert"));
    assert_eq!(read.as_ref().map(|one| one.word(2)), Some(""));
    let tilde = Reference::composed("odd~id", &["Dune"]);
    assert_eq!(
        Reference::read(&tilde, 1).map(|one| one.id),
        Some("odd~id".to_owned())
    );
    for unreadable in ["conformance", "~RHVuZQ", "OL1W~!!", "OL1W~/w"] {
        assert!(Reference::read(unreadable, 1).is_none(), "{unreadable}");
    }
    assert!(Reference::read(&dune(), 3).is_none());
}

#[tokio::test]
async fn a_book_lookup_answers_a_page_at_a_time_in_the_format_the_household_files() {
    let many = (0..25)
        .map(|n| found_book(&format!("OL{n}W"), ""))
        .collect::<Vec<_>>()
        .join(",");
    let fake = Fake::by_rules(vec![
        (Some(Method::Get), "/search/book", ok(&format!("[{many}]"))),
        (
            Some(Method::Get),
            "/setting/default.media_type",
            ok(r#"{"key":"default.media_type","value":"audiobook"}"#),
        ),
    ]);
    let curator = bindery(&fake);

    let first = curator.lookup(BookUnit::Book, "dune", None).await.ok();
    let second = curator
        .lookup(BookUnit::Book, "dune", Some("20"))
        .await
        .ok();

    assert_eq!(first.as_ref().map(|page| page.found.len()), Some(20));
    assert_eq!(
        first.as_ref().and_then(|page| page.next.clone()).as_deref(),
        Some("20")
    );
    assert_eq!(second.as_ref().map(|page| page.found.len()), Some(5));
    assert_eq!(second.and_then(|page| page.next), None);
    let one = first.and_then(|page| page.found.into_iter().next());
    assert_eq!(
        one,
        Some(Candidate {
            reference: Reference::composed("OL0W", &["Dune", "Frank Herbert"]),
            name: "Dune".to_owned(),
            by: Some("Frank Herbert".to_owned()),
            year: Some(1965),
            cover: Some("https://covers.example/dune.jpg".to_owned()),
            format: Some("audiobook".to_owned()),
            state: CandidateState::MayAsk,
        })
    );
    assert!(fake.asked_for("/api/v1/search/book?term=dune"));
}

#[tokio::test]
async fn a_page_beginning_nowhere_a_lookup_gives_is_refused() {
    let fake = Fake::always(ok("[]"));

    let page = bindery(&fake)
        .lookup(BookUnit::Book, "dune", Some("later"))
        .await;

    assert_eq!(
        refused(page.err()),
        "that page begins nowhere a lookup gives"
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_household_format_bindery_does_not_know_is_read_as_ebook_and_an_unread_one_refused() {
    let odd = rules(vec![
        (
            Method::Get,
            "/setting/default.media_type",
            ok(r#"{"value":"scroll"}"#),
        ),
        (
            Method::Get,
            "/search/book",
            ok(&format!("[{}]", found_book(DUNE, ""))),
        ),
    ]);
    let broken = rules(vec![(
        Method::Get,
        "/setting/default.media_type",
        Answer::reply(500, "{}"),
    )]);

    let found = bindery(&odd).lookup(BookUnit::Book, "dune", None).await;
    let unread = bindery(&broken).lookup(BookUnit::Book, "dune", None).await;

    assert_eq!(
        found
            .ok()
            .and_then(|page| page.found.into_iter().next())
            .and_then(|one| one.format),
        Some("ebook".to_owned())
    );
    assert!(unread.is_err());
}

#[tokio::test]
async fn a_held_book_is_here_asked_or_askable_by_what_the_library_holds_of_it() {
    let found = [
        found_book("OL1W", r#","libraryBookId":1"#),
        found_book("OL2W", r#","libraryBookId":2"#),
        found_book("OL3W", r#","libraryBookId":3"#),
        found_book("OL4W", r#","libraryBookId":4"#),
    ]
    .join(",");
    let fake = rules(vec![
        (Method::Get, "/search/book", ok(&format!("[{found}]"))),
        (
            Method::Get,
            "/api/v1/book/1",
            ok(&held_book(1, "imported", r#","mediaType":"audiobook""#)),
        ),
        (
            Method::Get,
            "/api/v1/book/2",
            ok(&held_book(2, "wanted", r#","monitored":true"#)),
        ),
        (
            Method::Get,
            "/api/v1/book/3",
            ok(&held_book(3, "skipped", r#","monitored":true"#)),
        ),
        (
            Method::Get,
            "/api/v1/book/4",
            ok(&held_book(4, "wanted", r#","monitored":false"#)),
        ),
    ]);

    let found = bindery(&fake)
        .lookup(BookUnit::Book, "dune", None)
        .await
        .map(|page| page.found)
        .unwrap_or_default();

    assert_eq!(
        states(&found),
        [
            CandidateState::Here,
            CandidateState::Asked,
            CandidateState::MayAsk,
            CandidateState::MayAsk
        ]
    );
    assert_eq!(
        found.first().and_then(|one| one.format.clone()).as_deref(),
        Some("audiobook")
    );
    assert_eq!(
        found.get(1).and_then(|one| one.format.clone()).as_deref(),
        Some("ebook")
    );
}

#[tokio::test]
async fn an_author_is_here_partly_here_asked_or_askable_by_the_books_of_theirs_it_holds() {
    let found = (1..=5)
        .map(|n| found_author(&format!(r#","libraryAuthorId":{n}"#)))
        .chain([found_author("")])
        .collect::<Vec<_>>()
        .join(",");
    let author = |id: i64, monitored: bool, statuses: &[&str]| {
        let books = statuses
            .iter()
            .map(|status| format!(r#"{{"status":"{status}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        ok(&format!(
            r#"{{"id":{id},"monitored":{monitored},
                "imageUrl":"/api/v1/images?url=https%3A%2F%2Fcovers.example%2Fa.jpg",
                "books":[{books}]}}"#
        ))
    };
    let fake = rules(vec![
        (Method::Get, "/search/author", ok(&format!("[{found}]"))),
        (
            Method::Get,
            "/api/v1/author/1",
            author(1, true, &["imported", "imported", "skipped"]),
        ),
        (
            Method::Get,
            "/api/v1/author/2",
            author(2, true, &["imported", "wanted"]),
        ),
        (
            Method::Get,
            "/api/v1/author/3",
            author(3, true, &["wanted"]),
        ),
        (
            Method::Get,
            "/api/v1/author/4",
            author(4, false, &["wanted"]),
        ),
        (Method::Get, "/api/v1/author/5", author(5, false, &[])),
    ]);

    let found = bindery(&fake)
        .lookup(BookUnit::Author, "herbert", None)
        .await
        .map(|page| page.found)
        .unwrap_or_default();

    assert_eq!(
        states(&found),
        [
            CandidateState::Here,
            CandidateState::PartlyHere,
            CandidateState::Asked,
            CandidateState::MayAsk,
            CandidateState::MayAsk,
            CandidateState::MayAsk
        ]
    );
    let first = found.first();
    assert_eq!(first.map(|one| one.reference.clone()), Some(herbert()));
    assert_eq!(
        first.and_then(|one| one.cover.clone()).as_deref(),
        Some("https://covers.example/a.jpg")
    );
    assert_eq!(found.last().and_then(|one| one.cover.clone()), None);
}

#[tokio::test]
async fn series_are_found_only_through_hardcover_and_refused_where_bindery_cannot_reach_it() {
    for (reason, said) in [
        ("missing_token", "no Hardcover token is set"),
        ("admin_disabled", "its Hardcover series setting is off"),
        ("env_disabled", "turned off where it runs"),
        ("other", "which it does not offer here"),
    ] {
        let fake = rules(vec![(
            Method::Get,
            "/series/hardcover/search",
            Answer::reply(
                404,
                format!(r#"{{"error":"enhanced hardcover api disabled","reason":"{reason}"}}"#),
            ),
        )]);
        let curator = bindery(&fake);

        let lookup = curator.lookup(BookUnit::Series, "dune", None).await;
        let candidate = curator.candidate(BookUnit::Series, &saga()).await;
        let request = curator
            .request(&asking(BookUnit::Series, &saga(), true))
            .await;

        for failure in [lookup.err(), candidate.err(), request.err()] {
            assert!(
                matches!(&failure, Some(Failure::Unsupported { detail, .. })
                    if detail.starts_with("Bindery finds series only through Hardcover")
                        && detail.ends_with(said)),
                "{reason}: {failure:?}"
            );
        }
    }
    let gone = rules(vec![(
        Method::Get,
        "/series/hardcover/search",
        Answer::reply(404, "not json"),
    )]);
    let unread = bindery(&gone).lookup(BookUnit::Series, "dune", None).await;
    assert!(unread.is_err() && !matches!(unread, Err(Failure::Unsupported { .. })));
}

#[tokio::test]
async fn a_series_is_found_through_hardcover_and_stands_by_the_books_the_library_holds_of_it() {
    let fake = rules(vec![
        (
            Method::Get,
            "/series/hardcover/search",
            ok(&format!(
                r#"[{},{{"foreignId":"hc-series:8","title":"Other"}}]"#,
                found_series()
            )),
        ),
        (
            Method::Get,
            "/api/v1/series",
            ok(&format!(
                r#"[{{"id":3,"foreignSeriesId":"manual:series:1","monitored":true,
                    "hardcoverLink":{{"hardcoverSeriesId":"{SERIES}"}},
                    "books":[{{"book":{{"status":"imported"}}}},{{"book":{{"status":"wanted"}}}}]}}]"#
            )),
        ),
    ]);

    let found = bindery(&fake)
        .lookup(BookUnit::Series, "dune", None)
        .await
        .map(|page| page.found)
        .unwrap_or_default();

    assert_eq!(
        states(&found),
        [CandidateState::PartlyHere, CandidateState::MayAsk]
    );
    assert_eq!(
        found.first(),
        Some(&Candidate {
            reference: saga(),
            name: "Dune".to_owned(),
            by: Some("Frank Herbert".to_owned()),
            year: None,
            cover: None,
            format: Some("ebook".to_owned()),
            state: CandidateState::PartlyHere,
        })
    );
    assert_eq!(found.get(1).and_then(|one| one.by.clone()), None);
    assert!(fake.asked_for("/series/hardcover/search?term=dune&limit=50"));
}

#[tokio::test]
async fn a_reference_this_adapter_did_not_compose_names_nothing_and_asks_nothing() {
    let fake = Fake::always(ok("[]"));
    let curator = bindery(&fake);

    for unit in [BookUnit::Book, BookUnit::Author, BookUnit::Series] {
        assert_eq!(
            curator.candidate(unit, "conformance").await.ok(),
            Some(None)
        );
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_held_book_is_read_from_the_library_before_the_catalogue() {
    let fake = rules(vec![(
        Method::Get,
        "/api/v1/book?",
        ok(&format!(
            r#"{{"items":[{},{}]}}"#,
            r#"{"id":9,"foreignBookId":"OL9W","status":"wanted"}"#,
            held_book(1, "imported", "")
        )),
    )]);

    let found = bindery(&fake).candidate(BookUnit::Book, &dune()).await;

    let one = found.ok().flatten();
    assert_eq!(
        one.as_ref().map(|one| one.state),
        Some(CandidateState::Here)
    );
    assert_eq!(one.as_ref().map(|one| one.reference.clone()), Some(dune()));
    assert_eq!(
        one.and_then(|one| one.cover).as_deref(),
        Some("https://covers.example/held.jpg")
    );
    assert!(fake.asked_for("/api/v1/book?search=Dune&limit=100"));
    assert!(!fake.asked_for("/search/book"));
}

#[tokio::test]
async fn a_book_the_library_does_not_hold_is_found_again_by_its_words_or_is_nothing() {
    let fake = rules(vec![
        (Method::Get, "/api/v1/book?", ok(r#"{"items":[]}"#)),
        (
            Method::Get,
            "/search/book",
            ok(&format!(
                "[{},{}]",
                found_book("OL9W", ""),
                found_book(DUNE, "")
            )),
        ),
    ]);
    let curator = bindery(&fake);

    let found = curator.candidate(BookUnit::Book, &dune()).await;
    let missing = curator
        .candidate(
            BookUnit::Book,
            &Reference::composed("OL0W", &["Dune", "Frank Herbert"]),
        )
        .await;

    assert_eq!(
        found.ok().flatten().map(|one| (one.reference, one.state)),
        Some((dune(), CandidateState::MayAsk))
    );
    assert_eq!(missing.ok(), Some(None));
    assert!(fake.asked_for("/search/book?term=Dune+Frank+Herbert"));
}

#[tokio::test]
async fn an_author_is_read_from_the_library_before_the_catalogue_or_is_nothing() {
    let held = rules(vec![
        (
            Method::Get,
            "/api/v1/author?",
            ok(&format!(
                r#"{{"items":[{{"id":4,"foreignAuthorId":"OL9A"}},{{"id":5,"foreignAuthorId":"{HERBERT}","authorName":"Frank Herbert"}}]}}"#
            )),
        ),
        (
            Method::Get,
            "/api/v1/author/5",
            ok(r#"{"id":5,"monitored":true,"books":[{"status":"wanted"}]}"#),
        ),
    ]);
    let unheld = rules(vec![
        (Method::Get, "/api/v1/author?", ok(r#"{"items":[]}"#)),
        (
            Method::Get,
            "/search/author",
            ok(&format!("[{}]", found_author(""))),
        ),
    ]);

    let listed = bindery(&held).candidate(BookUnit::Author, &herbert()).await;
    let found = bindery(&unheld)
        .candidate(BookUnit::Author, &herbert())
        .await;
    let missing = bindery(&unheld)
        .candidate(BookUnit::Author, &Reference::composed("OL0A", &["Nobody"]))
        .await;

    assert_eq!(
        listed.ok().flatten().map(|one| (one.name, one.state)),
        Some(("Frank Herbert".to_owned(), CandidateState::Asked))
    );
    assert_eq!(
        found.ok().flatten().map(|one| (one.reference, one.state)),
        Some((herbert(), CandidateState::MayAsk))
    );
    assert_eq!(missing.ok(), Some(None));
    assert!(held.asked_for("/api/v1/author?search=Frank+Herbert&limit=100"));
}

#[tokio::test]
async fn a_series_is_read_from_hardcover_or_the_library_or_is_nothing() {
    let held_only = rules(vec![
        (Method::Get, "/series/hardcover/search", ok("[]")),
        (
            Method::Get,
            "/api/v1/series",
            ok(&format!(
                r#"[{{"id":3,"foreignSeriesId":"{SERIES}","title":"Dune Chronicles",
                    "books":[{{"book":{{"status":"wanted","monitored":true}}}}]}}]"#
            )),
        ),
    ]);
    let linked = rules(vec![
        (Method::Get, "/series/hardcover/search", ok("[]")),
        (
            Method::Get,
            "/api/v1/series",
            ok(&format!(
                r#"[{{"id":3,"foreignSeriesId":"manual:series:1","title":"Dune",
                    "hardcoverLink":{{"hardcoverSeriesId":"{SERIES}","hardcoverAuthorName":"F. Herbert"}}}}]"#
            )),
        ),
    ]);
    let nowhere = rules(vec![
        (Method::Get, "/series/hardcover/search", ok("[]")),
        (Method::Get, "/api/v1/series", ok("[]")),
    ]);
    let nameless = Reference::composed(SERIES, &["Dune", ""]);

    let held = bindery(&held_only)
        .candidate(BookUnit::Series, &saga())
        .await;
    let named = bindery(&linked)
        .candidate(BookUnit::Series, &nameless)
        .await;
    let missing = bindery(&nowhere).candidate(BookUnit::Series, &saga()).await;

    assert_eq!(
        held.ok().flatten().map(|one| (one.name, one.by, one.state)),
        Some((
            "Dune Chronicles".to_owned(),
            Some("Frank Herbert".to_owned()),
            CandidateState::Asked
        ))
    );
    assert_eq!(
        named.ok().flatten().map(|one| (one.by, one.state)),
        Some((Some("F. Herbert".to_owned()), CandidateState::MayAsk))
    );
    assert_eq!(missing.ok(), Some(None));
}

#[tokio::test]
async fn the_profiles_are_the_root_folders_quality_profiles_and_units_it_answers() {
    let without = rules(vec![(
        Method::Get,
        "/system/status",
        ok(r#"{"version":"v1.43.1","enhancedHardcoverApi":false}"#),
    )]);
    let with_hardcover = rules(vec![(
        Method::Get,
        "/system/status",
        ok(r#"{"version":"v1.43.1","enhancedHardcoverApi":true}"#),
    )]);

    let with = bindery(&without).profiles().await.ok();
    let series = bindery(&with_hardcover).profiles().await.ok();

    assert_eq!(
        with.as_ref().map(|with| with.root_folders.clone()),
        Some(vec!["/books".to_owned()])
    );
    assert_eq!(
        with.as_ref().map(|with| with.quality.clone()),
        Some(vec![Profile {
            id: 2,
            name: "E-Book".to_owned()
        }])
    );
    assert_eq!(
        with.as_ref().map(|with| with.metadata.clone()),
        Some(Vec::new())
    );
    assert_eq!(
        with.map(|with| with.units),
        Some(vec![CuratedUnit::Book, CuratedUnit::Author])
    );
    assert_eq!(
        series.map(|with| with.units),
        Some(vec![
            CuratedUnit::Book,
            CuratedUnit::Author,
            CuratedUnit::Series
        ])
    );
}

#[tokio::test]
async fn author_and_series_lookups_answer_a_page_at_a_time_too() {
    let authors = (0..21)
        .map(|_| found_author(""))
        .collect::<Vec<_>>()
        .join(",");
    let series = (0..21)
        .map(|_| found_series())
        .collect::<Vec<_>>()
        .join(",");
    let fake = rules(vec![
        (Method::Get, "/search/author", ok(&format!("[{authors}]"))),
        (
            Method::Get,
            "/series/hardcover/search",
            ok(&format!("[{series}]")),
        ),
        (Method::Get, "/api/v1/series", ok("[]")),
    ]);
    let curator = bindery(&fake);

    for unit in [BookUnit::Author, BookUnit::Series] {
        let page = curator.lookup(unit, "dune", None).await.ok();
        assert_eq!(page.as_ref().map(|page| page.found.len()), Some(20));
        assert_eq!(page.and_then(|page| page.next).as_deref(), Some("20"));
    }
}

#[tokio::test]
async fn a_held_book_without_a_cover_of_its_own_is_shown_by_none() {
    let fake = rules(vec![(
        Method::Get,
        "/api/v1/book?",
        ok(&format!(
            r#"{{"items":[{{"id":1,"foreignBookId":"{DUNE}","title":"Dune","status":"wanted"}}]}}"#
        )),
    )]);

    let found = bindery(&fake).candidate(BookUnit::Book, &dune()).await;

    assert_eq!(
        found.ok().flatten().map(|one| (one.cover, one.state)),
        Some((None, CandidateState::MayAsk))
    );
}

mod asking;
