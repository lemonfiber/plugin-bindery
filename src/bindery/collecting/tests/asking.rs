use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{BookAsk, BookCurating as _, BookUnit, CandidateState, Failure};
use serde_json::json;

use super::super::super::tests::bindery;
use super::{
    asking, dune, found_author, found_book, found_series, held_book, herbert, in_turn, ok, refused,
    rules, saga, sent, DUNE, HERBERT, SERIES,
};

#[tokio::test]
async fn a_request_naming_what_bindery_does_not_hold_is_refused_and_sends_nothing() {
    let fake = rules(Vec::new());
    let curator = bindery(&fake);
    let ask = asking(BookUnit::Book, &dune(), false);

    let folder = curator
        .request(&BookAsk {
            root_folder: "/elsewhere".to_owned(),
            ..ask.clone()
        })
        .await;
    let profile = curator
        .request(&BookAsk {
            quality_profile: 9,
            ..ask.clone()
        })
        .await;
    let reference = curator
        .request(&BookAsk {
            reference: "conformance".to_owned(),
            ..ask
        })
        .await;

    assert_eq!(refused(folder.err()), "it holds no such root folder");
    assert_eq!(refused(profile.err()), "it holds no such quality profile");
    assert_eq!(
        refused(reference.err()),
        "its catalogue holds no such reference"
    );
    assert!(fake.requests().iter().all(|one| one.method == Method::Get));
}

#[tokio::test]
async fn a_book_bindery_holds_none_of_is_added_alone_its_new_author_filed_as_asked() {
    let fake = in_turn(vec![
        (
            Method::Get,
            "/api/v1/book?",
            vec![
                ok(r#"{"items":[]}"#),
                ok(&format!(
                    r#"{{"items":[{}]}}"#,
                    held_book(7, "wanted", r#","monitored":true"#)
                )),
            ],
        ),
        (
            Method::Get,
            "/search/book",
            vec![ok(&format!("[{}]", found_book(DUNE, "")))],
        ),
        (Method::Get, "/api/v1/author?", vec![ok(r#"{"items":[]}"#)]),
        (
            Method::Post,
            "/author/book",
            vec![Answer::reply(201, r#"{"id":7,"authorId":4}"#)],
        ),
        (Method::Put, "/api/v1/author/4", vec![ok("{}")]),
    ]);

    let asked = bindery(&fake)
        .request(&asking(BookUnit::Book, &dune(), true))
        .await;

    assert_eq!(asked.ok().map(|one| one.state), Some(CandidateState::Asked));
    assert_eq!(
        sent(&fake, Method::Post, "/author/book"),
        [json!({
            "foreignBookId": DUNE,
            "foreignAuthorId": HERBERT,
            "authorName": "Frank Herbert",
            "searchOnAdd": true,
        })]
    );
    assert_eq!(
        sent(&fake, Method::Put, "/api/v1/author/4"),
        [json!({ "rootFolderId": 1, "qualityProfileId": 2 })]
    );
    assert!(sent(&fake, Method::Put, "/api/v1/book/").is_empty());
}

#[tokio::test]
async fn a_book_whose_author_bindery_follows_is_added_under_them_as_they_are() {
    let fake = in_turn(vec![
        (
            Method::Get,
            "/api/v1/book?",
            vec![ok(r#"{"items":[]}"#), ok(r#"{"items":[]}"#)],
        ),
        (
            Method::Get,
            "/search/book",
            vec![
                ok(&format!("[{}]", found_book(DUNE, ""))),
                ok(&format!("[{}]", found_book(DUNE, r#","libraryBookId":7"#))),
            ],
        ),
        (
            Method::Get,
            "/api/v1/author?",
            vec![ok(&format!(
                r#"{{"items":[{{"id":4,"foreignAuthorId":"{HERBERT}"}}]}}"#
            ))],
        ),
        (
            Method::Post,
            "/author/book",
            vec![Answer::reply(201, r#"{"id":7,"authorId":4}"#)],
        ),
        (
            Method::Get,
            "/api/v1/book/7",
            vec![ok(&held_book(7, "wanted", r#","monitored":true"#))],
        ),
    ]);

    let asked = bindery(&fake)
        .request(&asking(BookUnit::Book, &dune(), false))
        .await;

    assert_eq!(asked.ok().map(|one| one.state), Some(CandidateState::Asked));
    assert!(sent(&fake, Method::Put, "/author/").is_empty());
}

#[tokio::test]
async fn a_book_bindery_holds_is_wanted_again_and_searched_for_where_asked() {
    let fake = rules(vec![
        (
            Method::Get,
            "/api/v1/book?",
            ok(&format!(r#"{{"items":[{}]}}"#, held_book(7, "skipped", ""))),
        ),
        (Method::Put, "/api/v1/book/7", ok("{}")),
        (Method::Post, "/api/v1/book/bulk", ok("{}")),
    ]);
    let curator = bindery(&fake);

    let searched = curator
        .request(&asking(BookUnit::Book, &dune(), true))
        .await;
    let quiet = curator
        .request(&asking(BookUnit::Book, &dune(), false))
        .await;

    assert!(searched.is_ok() && quiet.is_ok());
    let wanted = sent(&fake, Method::Put, "/api/v1/book/7");
    assert_eq!(
        wanted.first(),
        Some(&json!({ "monitored": true, "status": "wanted" }))
    );
    assert_eq!(wanted.len(), 2);
    assert_eq!(
        sent(&fake, Method::Post, "/api/v1/book/bulk"),
        [json!({ "ids": [7], "action": "search" })]
    );
}

#[tokio::test]
async fn a_book_the_catalogue_stamps_as_held_or_bindery_says_it_holds_is_wanted_as_held() {
    let stamped = rules(vec![
        (Method::Get, "/api/v1/book?", ok(r#"{"items":[]}"#)),
        (
            Method::Get,
            "/search/book",
            ok(&format!("[{}]", found_book(DUNE, r#","libraryBookId":7"#))),
        ),
        (
            Method::Get,
            "/api/v1/book/7",
            ok(&held_book(7, "imported", "")),
        ),
        (Method::Put, "/api/v1/book/7", ok("{}")),
    ]);
    let conflicting = in_turn(vec![
        (
            Method::Get,
            "/api/v1/book?",
            vec![
                ok(r#"{"items":[]}"#),
                ok(&format!(
                    r#"{{"items":[{}]}}"#,
                    held_book(7, "imported", "")
                )),
            ],
        ),
        (
            Method::Get,
            "/search/book",
            vec![ok(&format!("[{}]", found_book(DUNE, "")))],
        ),
        (Method::Get, "/api/v1/author?", vec![ok(r#"{"items":[]}"#)]),
        (
            Method::Post,
            "/author/book",
            vec![Answer::reply(409, r#"{"existingBookId":7}"#)],
        ),
        (
            Method::Get,
            "/api/v1/book/7",
            vec![ok(&held_book(7, "imported", ""))],
        ),
        (Method::Put, "/api/v1/book/7", vec![ok("{}")]),
    ]);

    let held = bindery(&stamped)
        .request(&asking(BookUnit::Book, &dune(), false))
        .await;
    let again = bindery(&conflicting)
        .request(&asking(BookUnit::Book, &dune(), false))
        .await;

    assert_eq!(held.ok().map(|one| one.state), Some(CandidateState::Here));
    assert_eq!(again.ok().map(|one| one.state), Some(CandidateState::Here));
    assert_eq!(
        sent(&stamped, Method::Put, "/api/v1/book/7"),
        [json!({ "monitored": true })]
    );
    assert_eq!(sent(&conflicting, Method::Put, "/api/v1/book/7").len(), 1);
}

#[tokio::test]
async fn a_book_bindery_will_not_add_or_cannot_find_is_refused() {
    let turned_down = rules(vec![
        (Method::Get, "/api/v1/book?", ok(r#"{"items":[]}"#)),
        (
            Method::Get,
            "/search/book",
            ok(&format!("[{}]", found_book(DUNE, ""))),
        ),
        (Method::Get, "/api/v1/author?", ok(r#"{"items":[]}"#)),
        (
            Method::Post,
            "/author/book",
            Answer::reply(422, r#"{"error":"no"}"#),
        ),
    ]);
    let unfound = rules(vec![
        (Method::Get, "/api/v1/book?", ok(r#"{"items":[]}"#)),
        (Method::Get, "/search/book", ok("[]")),
    ]);
    let vanished = in_turn(vec![
        (
            Method::Get,
            "/api/v1/book?",
            vec![
                ok(&format!(r#"{{"items":[{}]}}"#, held_book(7, "wanted", ""))),
                ok(r#"{"items":[]}"#),
            ],
        ),
        (Method::Get, "/search/book", vec![ok("[]")]),
        (Method::Put, "/api/v1/book/7", vec![ok("{}")]),
    ]);
    let ask = asking(BookUnit::Book, &dune(), false);

    let refusal = bindery(&turned_down).request(&ask).await;
    let missing = bindery(&unfound).request(&ask).await;
    let gone = bindery(&vanished).request(&ask).await;

    assert!(matches!(refusal, Err(Failure::Refused { .. })));
    for failure in [missing.err(), gone.err()] {
        assert_eq!(refused(failure), "its catalogue holds no such reference");
    }
}

#[tokio::test]
async fn an_author_bindery_follows_is_wanted_with_every_book_and_searched_for_where_asked() {
    let held = format!(
        r#"{{"items":[{{"id":5,"foreignAuthorId":"{HERBERT}","authorName":"Frank Herbert"}}]}}"#
    );
    let fake = rules(vec![
        (Method::Get, "/api/v1/author?", ok(&held)),
        (Method::Put, "/api/v1/author/5", ok("{}")),
        (Method::Post, "/api/v1/author/bulk", ok("{}")),
        (
            Method::Get,
            "/api/v1/author/5",
            ok(r#"{"id":5,"monitored":true,"books":[]}"#),
        ),
    ]);
    let curator = bindery(&fake);

    let searched = curator
        .request(&asking(BookUnit::Author, &herbert(), true))
        .await;
    let quiet = curator
        .request(&asking(BookUnit::Author, &herbert(), false))
        .await;

    assert_eq!(
        searched.ok().map(|one| one.state),
        Some(CandidateState::Asked)
    );
    assert!(quiet.is_ok());
    assert_eq!(
        sent(&fake, Method::Put, "/api/v1/author/5").first(),
        Some(&json!({
            "monitored": true,
            "monitorMode": "all",
            "monitorNewItems": "all",
            "applyMonitorModeToExisting": true,
        }))
    );
    assert_eq!(
        sent(&fake, Method::Post, "/api/v1/author/bulk"),
        [json!({ "ids": [5], "action": "search" })]
    );
}

#[tokio::test]
async fn an_author_bindery_holds_none_of_is_added_with_every_book_filed_as_asked() {
    let fake = in_turn(vec![
        (
            Method::Get,
            "/api/v1/author?",
            vec![
                ok(r#"{"items":[]}"#),
                ok(&format!(
                    r#"{{"items":[{{"id":5,"foreignAuthorId":"{HERBERT}"}}]}}"#
                )),
            ],
        ),
        (
            Method::Get,
            "/search/author",
            vec![ok(&format!("[{}]", found_author("")))],
        ),
        (
            Method::Post,
            "/api/v1/author",
            vec![Answer::reply(201, r#"{"id":5}"#)],
        ),
        (
            Method::Get,
            "/api/v1/author/5",
            vec![ok(r#"{"id":5,"monitored":true}"#)],
        ),
    ]);

    let asked = bindery(&fake)
        .request(&asking(BookUnit::Author, &herbert(), true))
        .await;

    assert_eq!(asked.ok().map(|one| one.state), Some(CandidateState::Asked));
    assert_eq!(
        sent(&fake, Method::Post, "/api/v1/author"),
        [json!({
            "foreignAuthorId": HERBERT,
            "authorName": "Frank Herbert",
            "rootFolderId": 1,
            "qualityProfileId": 2,
            "monitored": true,
            "monitorMode": "all",
            "monitorNewItems": "all",
            "searchOnAdd": true,
        })]
    );
    assert!(sent(&fake, Method::Put, "/author/").is_empty());
}

#[tokio::test]
async fn an_author_the_catalogue_stamps_or_bindery_names_as_held_is_wanted_as_held() {
    let routes = |added: Answer, found: &str| {
        in_turn(vec![
            (
                Method::Get,
                "/api/v1/author?",
                vec![
                    ok(r#"{"items":[]}"#),
                    ok(&format!(
                        r#"{{"items":[{{"id":5,"foreignAuthorId":"{HERBERT}"}}]}}"#
                    )),
                ],
            ),
            (
                Method::Get,
                "/search/author",
                vec![ok(&format!("[{}]", found_author(found)))],
            ),
            (Method::Post, "/api/v1/author", vec![added]),
            (Method::Put, "/api/v1/author/5", vec![ok("{}")]),
            (
                Method::Get,
                "/api/v1/author/5",
                vec![ok(r#"{"id":5,"monitored":true}"#)],
            ),
        ])
    };
    let stamped = routes(Answer::reply(500, "{}"), r#","libraryAuthorId":5"#);
    let conflicting = routes(Answer::reply(409, r#"{"canonicalAuthorId":5}"#), "");
    let refusing = routes(Answer::reply(409, r#"{"error":"no"}"#), "");
    let unfound = rules(vec![
        (Method::Get, "/api/v1/author?", ok(r#"{"items":[]}"#)),
        (Method::Get, "/search/author", ok("[]")),
    ]);
    let ask = asking(BookUnit::Author, &herbert(), false);

    assert!(bindery(&stamped).request(&ask).await.is_ok());
    assert!(bindery(&conflicting).request(&ask).await.is_ok());
    assert!(matches!(
        bindery(&refusing).request(&ask).await,
        Err(Failure::Refused { .. })
    ));
    assert_eq!(
        refused(bindery(&unfound).request(&ask).await.err()),
        "its catalogue holds no such reference"
    );
    for fake in [&stamped, &conflicting] {
        assert_eq!(sent(fake, Method::Put, "/api/v1/author/5").len(), 1);
    }
    assert!(sent(&stamped, Method::Post, "/api/v1/author").is_empty());
}

fn series_routes(held: Vec<Answer>) -> Arc<Fake> {
    let page: String = (1..=500)
        .map(|id| format!(r#"{{"id":{id}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    in_turn(vec![
        (
            Method::Get,
            "/series/hardcover/search",
            vec![ok(&format!("[{}]", found_series()))],
        ),
        (Method::Post, "/api/v1/series/9/fill", vec![ok("{}")]),
        (
            Method::Put,
            "/api/v1/series/9/hardcover-link",
            vec![ok("{}")],
        ),
        (Method::Patch, "/api/v1/series/9", vec![ok("{}")]),
        (
            Method::Get,
            "/api/v1/series/9",
            vec![ok(
                r#"{"id":9,"books":[{"book":{"authorId":501,"status":"wanted","monitored":true}},
                    {"book":{"authorId":3,"status":"wanted"}},{"book":{"authorId":0}},{}]}"#,
            )],
        ),
        (
            Method::Post,
            "/api/v1/series",
            vec![Answer::reply(201, r#"{"id":9}"#)],
        ),
        (Method::Get, "/api/v1/series", held),
        (
            Method::Get,
            "/api/v1/author/501/series",
            vec![ok(&format!(
                r#"[{{"id":9}},{{"id":12,"foreignSeriesId":"{SERIES}"}},{{"id":13,"foreignSeriesId":"ol-series:dune"}}]"#
            ))],
        ),
        (Method::Put, "/api/v1/author/501", vec![ok("{}")]),
        (
            Method::Get,
            "/api/v1/author?limit=500&offset=0",
            vec![ok(&format!(r#"{{"items":[{page}]}}"#))],
        ),
        (
            Method::Get,
            "/api/v1/author?limit=500&offset=500",
            vec![ok(r#"{"items":[]}"#)],
        ),
    ])
}

#[tokio::test]
async fn a_series_is_linked_filled_and_followed_through_each_author_it_adds() {
    let held_after = format!(
        r#"[{{"id":9,"foreignSeriesId":"manual:series:1","monitored":true,
            "hardcoverLink":{{"hardcoverSeriesId":"{SERIES}"}},"books":[]}}]"#
    );
    let fake = series_routes(vec![ok("[]"), ok(&held_after)]);

    let asked = bindery(&fake)
        .request(&asking(BookUnit::Series, &saga(), false))
        .await;

    assert_eq!(asked.ok().map(|one| one.state), Some(CandidateState::Asked));
    assert_eq!(
        sent(&fake, Method::Post, "/api/v1/series"),
        [json!({ "title": "Dune" }), json!({ "mediaType": "ebook" })]
    );
    assert_eq!(
        sent(&fake, Method::Put, "/hardcover-link"),
        [json!({
            "foreignId": SERIES,
            "providerId": "7",
            "slug": "dune",
            "title": "Dune",
            "authorName": "Frank Herbert",
            "bookCount": 6,
        })]
    );
    assert_eq!(
        sent(&fake, Method::Patch, "/api/v1/series/9"),
        [json!({ "monitored": true })]
    );
    assert_eq!(
        sent(&fake, Method::Put, "/api/v1/author/"),
        [json!({
            "monitored": true,
            "monitorMode": "series",
            "monitorNewItems": "all",
            "monitoredSeriesIds": [9, 12],
            "rootFolderId": 1,
            "qualityProfileId": 2,
        })]
    );
}

#[tokio::test]
async fn a_series_the_library_holds_is_filled_without_another_being_made() {
    let held = format!(r#"[{{"id":9,"foreignSeriesId":"{SERIES}","monitored":true,"books":[]}}]"#);
    let fake = series_routes(vec![ok(&held)]);

    let asked = bindery(&fake)
        .request(&asking(BookUnit::Series, &saga(), true))
        .await;

    assert!(asked.is_ok(), "{asked:?}");
    assert!(sent(&fake, Method::Put, "/hardcover-link").is_empty());
    assert_eq!(sent(&fake, Method::Post, "/api/v1/series").len(), 1);
}

#[tokio::test]
async fn a_series_hardcover_does_not_find_is_refused() {
    let fake = rules(vec![
        (Method::Get, "/series/hardcover/search", ok("[]")),
        (Method::Get, "/api/v1/series", ok("[]")),
    ]);

    let asked = bindery(&fake)
        .request(&asking(BookUnit::Series, &saga(), true))
        .await;

    assert_eq!(
        refused(asked.err()),
        "its catalogue holds no such reference"
    );
}
