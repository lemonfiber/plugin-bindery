use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{BookCurating as _, BookUnit};

use super::super::super::tests::{asked, bindery};
use super::{dune, herbert, ok, refused, saga, DUNE, HERBERT, SERIES};

fn gone() -> Answer {
    Answer::reply(204, "")
}

fn routes(fake: &Fake) -> Vec<(Method, String)> {
    asked(fake)
        .into_iter()
        .map(|(method, path, _)| (method, path))
        .collect()
}

fn shelved(author: i64) -> Answer {
    ok(&format!(
        r#"{{"items":[{{"id":4,"foreignBookId":"{DUNE}","title":"Dune","authorId":{author},"monitored":true}}]}}"#
    ))
}

fn author_of(monitored: bool, mode: &str, series: &str, books: &str) -> Answer {
    ok(&format!(
        r#"{{"id":2,"foreignAuthorId":"{HERBERT}","authorName":"Frank Herbert","monitored":{monitored},
            "monitorMode":"{mode}","monitoredSeriesIds":[{series}],"books":[{books}]}}"#
    ))
}

#[tokio::test]
async fn a_book_goes_with_its_files_and_the_author_its_ask_added_where_nothing_else_is_kept() {
    let fake = Fake::by_route(vec![
        (Method::Delete, "/book/4", gone()),
        (Method::Get, "/book?", shelved(2)),
        (Method::Delete, "/author/2", gone()),
        (
            Method::Get,
            "/author/2",
            author_of(
                false,
                "none",
                "",
                r#"{"id":9,"title":"Dune Messiah","status":"skipped","monitored":false}"#,
            ),
        ),
    ]);
    assert!(bindery(&fake).remove(BookUnit::Book, &dune()).await.is_ok());
    assert_eq!(
        routes(&fake),
        [
            (Method::Get, "/api/v1/book?search=Dune&limit=100".to_owned()),
            (Method::Delete, "/api/v1/book/4?deleteFiles=true".to_owned()),
            (Method::Get, "/api/v1/author/2".to_owned()),
            (
                Method::Delete,
                "/api/v1/author/2?deleteFiles=true".to_owned()
            ),
        ]
    );
}

#[tokio::test]
async fn a_book_s_author_stays_where_it_was_followed_or_keeps_another_book() {
    for author in [
        author_of(true, "all", "", ""),
        author_of(
            false,
            "none",
            "",
            r#"{"id":9,"title":"Dune Messiah","status":"wanted","monitored":true}"#,
        ),
        author_of(
            false,
            "none",
            "",
            r#"{"id":9,"title":"Dune Messiah","status":"imported","monitored":false}"#,
        ),
    ] {
        let fake = Fake::by_route(vec![
            (Method::Delete, "/book/4", gone()),
            (Method::Get, "/book?", shelved(2)),
            (Method::Get, "/author/2", author),
        ]);
        assert!(bindery(&fake).remove(BookUnit::Book, &dune()).await.is_ok());
        assert!(!routes(&fake)
            .iter()
            .any(|(method, path)| *method == Method::Delete && path.contains("/author/")));
    }
    let unattributed = Fake::by_route(vec![
        (Method::Delete, "/book/4", gone()),
        (Method::Get, "/book?", shelved(0)),
    ]);
    assert!(bindery(&unattributed)
        .remove(BookUnit::Book, &dune())
        .await
        .is_ok());
    assert_eq!(routes(&unattributed).len(), 2);
}

#[tokio::test]
async fn an_author_goes_with_every_book_of_theirs_and_their_files() {
    let fake = Fake::by_route(vec![
        (Method::Delete, "/author/2", gone()),
        (
            Method::Get,
            "/author?",
            ok(&format!(
                r#"{{"items":[{{"id":2,"foreignAuthorId":"{HERBERT}","authorName":"Frank Herbert"}}]}}"#
            )),
        ),
    ]);
    assert!(bindery(&fake)
        .remove(BookUnit::Author, &herbert())
        .await
        .is_ok());
    assert_eq!(
        routes(&fake).last(),
        Some(&(
            Method::Delete,
            "/api/v1/author/2?deleteFiles=true".to_owned()
        ))
    );
}

#[tokio::test]
async fn a_series_goes_with_every_book_of_it_and_each_author_followed_for_it_alone() {
    let fake = Fake::by_route(vec![
        (Method::Delete, "/book/", gone()),
        (Method::Delete, "/author/2", gone()),
        (Method::Delete, "/series/7", gone()),
        (
            Method::Get,
            "/series",
            ok(&format!(
                r#"[{{"id":6,"foreignSeriesId":"other","books":[]}},
                   {{"id":7,"foreignSeriesId":"x","hardcoverLink":{{"hardcoverSeriesId":"{SERIES}"}},
                    "books":[{{"book":{{"id":4,"authorId":2}}}},{{"book":{{"id":5,"authorId":3}}}},{{}}]}}]"#
            )),
        ),
        (Method::Get, "/author/2", author_of(true, "series", "7", "")),
        (
            Method::Get,
            "/author/3",
            author_of(true, "series", "7, 8", ""),
        ),
    ]);
    assert!(bindery(&fake)
        .remove(BookUnit::Series, &saga())
        .await
        .is_ok());
    assert_eq!(
        routes(&fake),
        [
            (Method::Get, "/api/v1/series".to_owned()),
            (Method::Delete, "/api/v1/book/4?deleteFiles=true".to_owned()),
            (Method::Delete, "/api/v1/book/5?deleteFiles=true".to_owned()),
            (Method::Get, "/api/v1/author/2".to_owned()),
            (
                Method::Delete,
                "/api/v1/author/2?deleteFiles=true".to_owned()
            ),
            (Method::Get, "/api/v1/author/3".to_owned()),
            (Method::Delete, "/api/v1/series/7".to_owned()),
        ]
    );
}

#[tokio::test]
async fn what_bindery_does_not_hold_is_refused_before_anything_is_removed() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/book?", ok(r#"{"items":[]}"#)),
        (Method::Get, "/author?", ok(r#"{"items":[]}"#)),
        (Method::Get, "/series", ok("[]")),
    ]);
    let curator = bindery(&fake);
    for (unit, reference) in [
        (BookUnit::Book, dune()),
        (BookUnit::Author, herbert()),
        (BookUnit::Series, saga()),
    ] {
        assert_eq!(
            refused(curator.remove(unit, &reference).await.err()),
            "it holds nothing by that reference"
        );
    }
    assert_eq!(
        refused(curator.remove(BookUnit::Book, "conformance").await.err()),
        "its catalogue holds no such reference"
    );
    assert!(routes(&fake)
        .iter()
        .all(|(method, _)| *method == Method::Get));
    assert_eq!(routes(&fake).len(), 3);
}

#[tokio::test]
async fn several_matches_a_near_match_or_an_unassigned_id_are_refused_and_nothing_is_deleted() {
    let book = |id: i64, foreign: &str| {
        format!(r#"{{"id":{id},"foreignBookId":"{foreign}","title":"Dune","authorId":2}}"#)
    };
    let author = |id: i64, foreign: &str| {
        format!(r#"{{"id":{id},"foreignAuthorId":"{foreign}","authorName":"Frank Herbert"}}"#)
    };
    let series = |id: i64, foreign: &str| {
        format!(
            r#"{{"id":{id},"foreignSeriesId":"{foreign}","books":[{{"book":{{"id":4,"authorId":2}}}}]}}"#
        )
    };
    for (books, authors, held_series) in [
        (
            format!("{},{}", book(4, DUNE), book(5, DUNE)),
            format!("{},{}", author(2, HERBERT), author(3, HERBERT)),
            format!("{},{}", series(7, SERIES), series(8, SERIES)),
        ),
        (
            book(4, &format!("{DUNE}X")),
            author(2, &format!("{HERBERT}X")),
            series(7, &format!("{SERIES}0")),
        ),
        (book(0, DUNE), author(0, HERBERT), series(0, SERIES)),
    ] {
        let fake = Fake::by_route(vec![
            (Method::Delete, "/", gone()),
            (
                Method::Get,
                "/book?",
                ok(&format!(r#"{{"items":[{books}]}}"#)),
            ),
            (
                Method::Get,
                "/author?",
                ok(&format!(r#"{{"items":[{authors}]}}"#)),
            ),
            (Method::Get, "/series", ok(&format!("[{held_series}]"))),
        ]);
        let curator = bindery(&fake);
        for (unit, reference) in [
            (BookUnit::Book, dune()),
            (BookUnit::Author, herbert()),
            (BookUnit::Series, saga()),
        ] {
            assert_eq!(
                refused(curator.remove(unit, &reference).await.err()),
                "it holds nothing by that reference",
                "{unit:?}"
            );
        }
        assert!(
            routes(&fake)
                .iter()
                .all(|(method, _)| *method == Method::Get),
            "{:?}",
            routes(&fake)
        );
    }
}
