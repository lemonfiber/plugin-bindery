use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    AggregatorAt, Failure, KnownAggregator, Protocol, PullsIndexers as _,
};

use super::Bindery;

pub(super) const KEY: &str = "abcdefghijklmnop";

pub(super) const USERNAME: &str = "qrstuvwx";

pub(super) const PASSWORD: &str = "ABCDEFGHIJKLMNOP";

struct Missing;

impl lemonfiber_adapter_kit::credential::Holding for Missing {
    fn key(&self) -> Result<String, String> {
        Err("upstream.json could not be read: not found".to_owned())
    }
}

pub(super) fn bindery(fake: &Arc<Fake>) -> Bindery {
    Bindery::new(
        fake.clone(),
        "http://bindery:8787",
        Arc::new(KEY.to_owned()),
        Arc::new(USERNAME.to_owned()),
        Arc::new(PASSWORD.to_owned()),
    )
}

pub(super) fn asked(fake: &Fake) -> Vec<(Method, String, Option<serde_json::Value>)> {
    fake.requests()
        .into_iter()
        .map(|one| {
            (
                one.method,
                one.url.trim_start_matches("http://bindery:8787").to_owned(),
                one.body
                    .as_deref()
                    .and_then(|body| serde_json::from_str(body).ok()),
            )
        })
        .collect()
}

fn aggregator(protocol: &str) -> AggregatorAt {
    AggregatorAt {
        name: "conformance".to_owned(),
        url: "http://prowlarr:9696".to_owned(),
        protocol: Protocol(protocol.to_owned()),
        key: "the-aggregator-key".to_owned(),
    }
}

#[tokio::test]
async fn every_call_carries_the_key_and_a_body_says_it_is_json() {
    let fake = Fake::always(Answer::reply(201, "{}"));
    assert!(bindery(&fake)
        .add_aggregator(&aggregator("servarr"))
        .await
        .is_ok());
    assert!(fake.requests().iter().all(|one| {
        one.headers
            .iter()
            .any(|(name, value)| name == "X-Api-Key" && value == KEY)
            && one
                .headers
                .iter()
                .any(|(name, value)| name == "Content-Type" && value == "application/json")
    }));
}

#[tokio::test]
async fn the_prowlarr_instances_bindery_pulls_from_are_answered_by_id_address_and_whether_a_key_is_held(
) {
    let fake = Fake::always(Answer::reply(
        200,
        r#"[{"id":3,"url":"http://prowlarr:9696","apiKey":"","apiKeyConfigured":true},{"id":4}]"#,
    ));
    assert_eq!(
        bindery(&fake).aggregators().await.ok(),
        Some(vec![
            KnownAggregator {
                id: "3".to_owned(),
                url: "http://prowlarr:9696".to_owned(),
                keyed: true,
            },
            KnownAggregator {
                id: "4".to_owned(),
                url: String::new(),
                keyed: false,
            },
        ])
    );
    assert_eq!(
        asked(&fake),
        [(Method::Get, "/api/v1/prowlarr".to_owned(), None)]
    );
}

#[tokio::test]
async fn a_prowlarr_is_added_with_its_key_to_be_read_on_every_start() {
    let fake = Fake::always(Answer::reply(201, "{}"));
    assert!(bindery(&fake)
        .add_aggregator(&aggregator("servarr"))
        .await
        .is_ok());
    assert_eq!(
        asked(&fake),
        [(
            Method::Post,
            "/api/v1/prowlarr".to_owned(),
            Some(serde_json::json!({
                "name": "conformance",
                "url": "http://prowlarr:9696",
                "apiKey": "the-aggregator-key",
                "syncOnStartup": true,
                "enabled": true,
            }))
        )]
    );
    let refused = Fake::always(Answer::reply(400, r#"{"error":"url is required"}"#));
    assert!(matches!(
        bindery(&refused)
            .add_aggregator(&aggregator("servarr"))
            .await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn an_aggregator_in_any_other_api_than_prowlarr_s_is_unsupported_and_bindery_is_not_asked() {
    let fake = Fake::silent();
    assert!(matches!(
        bindery(&fake)
            .add_aggregator(&aggregator("nzbhydra2"))
            .await,
        Err(Failure::Unsupported { service, .. }) if service == "bindery"
    ));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn without_a_key_nothing_is_asked_and_the_refusal_names_no_part_of_one() {
    let fake = Fake::silent();
    let keyless = Bindery::new(
        fake.clone(),
        "http://bindery:8787",
        Arc::new(Missing),
        Arc::new(USERNAME.to_owned()),
        Arc::new(PASSWORD.to_owned()),
    );
    assert!(matches!(
        keyless.aggregators().await,
        Err(Failure::Refused { detail, .. }) if !detail.contains(KEY)
    ));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn an_unreadable_or_refused_answer_is_a_failure() {
    let unread = Fake::always(Answer::reply(200, "nope"));
    assert!(matches!(
        bindery(&unread).aggregators().await,
        Err(Failure::Refused { .. })
    ));
    let refused = Fake::always(Answer::reply(401, ""));
    assert!(matches!(
        bindery(&refused).aggregators().await,
        Err(Failure::Unauthorised { .. })
    ));
    assert!(matches!(
        bindery(&Fake::silent()).aggregators().await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn it_answers_while_bindery_says_it_is_healthy() {
    assert!(
        bindery(&Fake::always(Answer::reply(200, "{}")))
            .answers()
            .await
    );
    assert!(
        !bindery(&Fake::always(Answer::reply(503, "")))
            .answers()
            .await
    );
    assert!(!bindery(&Fake::silent()).answers().await);
}

const FRESH: &str = r#"{"authenticated":false,"setupRequired":true,"mode":"enabled"}"#;

const SET_UP: &str = r#"{"authenticated":false,"setupRequired":false,"mode":"enabled"}"#;

fn administrator() -> serde_json::Value {
    serde_json::json!({ "username": USERNAME, "password": PASSWORD })
}

#[tokio::test]
async fn a_bindery_nobody_has_set_up_is_open_and_one_set_up_is_ours_only_where_this_administrator_signs_in(
) {
    use lemonfiber_ports::service::{Owner, Ownership as _};
    let fresh = Fake::in_turn(vec![Answer::reply(200, FRESH)]);
    assert!(matches!(bindery(&fresh).owner().await, Ok(Owner::Open)));
    assert_eq!(
        asked(&fresh),
        [(Method::Get, "/api/v1/auth/status".to_owned(), None)]
    );
    let ours = Fake::in_turn(vec![
        Answer::reply(200, SET_UP),
        Answer::reply(200, r#"{"ok":true}"#),
    ]);
    assert!(matches!(bindery(&ours).owner().await, Ok(Owner::Ours)));
    assert_eq!(
        asked(&ours).get(1),
        Some(&(
            Method::Post,
            "/api/v1/auth/login".to_owned(),
            Some(administrator())
        ))
    );
    for refused in [401, 403] {
        let theirs = Fake::in_turn(vec![Answer::reply(200, SET_UP), Answer::reply(refused, "")]);
        assert!(matches!(bindery(&theirs).owner().await, Ok(Owner::Theirs)));
    }
    let limited = Fake::in_turn(vec![Answer::reply(200, SET_UP), Answer::reply(429, "")]);
    assert!(matches!(
        bindery(&limited).owner().await,
        Err(Failure::Refused { .. })
    ));
    let unread = Fake::in_turn(vec![Answer::reply(200, "nope")]);
    assert!(matches!(
        bindery(&unread).owner().await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn a_bindery_nobody_has_set_up_is_claimed_for_this_administrator_and_one_set_up_is_left_as_it_is(
) {
    use lemonfiber_ports::service::{Owner, Ownership as _};
    let fresh = Fake::in_turn(vec![
        Answer::reply(200, FRESH),
        Answer::reply(200, r#"{"ok":true}"#),
    ]);
    assert!(matches!(bindery(&fresh).claim().await, Ok(Owner::Ours)));
    assert_eq!(
        asked(&fresh),
        [
            (Method::Get, "/api/v1/auth/status".to_owned(), None),
            (
                Method::Post,
                "/api/v1/auth/setup".to_owned(),
                Some(administrator())
            ),
        ]
    );
    let theirs = Fake::in_turn(vec![Answer::reply(200, SET_UP), Answer::reply(401, "")]);
    assert!(matches!(bindery(&theirs).claim().await, Ok(Owner::Theirs)));
    assert!(asked(&theirs)
        .iter()
        .all(|(_, path, _)| path != "/api/v1/auth/setup"));
    let raced = Fake::in_turn(vec![
        Answer::reply(200, FRESH),
        Answer::reply(409, r#"{"error":"setup already complete"}"#),
        Answer::reply(200, SET_UP),
        Answer::reply(401, ""),
    ]);
    assert!(matches!(bindery(&raced).claim().await, Ok(Owner::Theirs)));
    let refused = Fake::in_turn(vec![Answer::reply(200, FRESH), Answer::reply(400, "")]);
    assert!(matches!(
        bindery(&refused).claim().await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn without_the_administrator_nothing_is_set_up_and_the_refusal_names_no_part_of_one() {
    use lemonfiber_ports::service::Ownership as _;
    let fake = Fake::in_turn(vec![Answer::reply(200, FRESH)]);
    let nameless = Bindery::new(
        fake.clone(),
        "http://bindery:8787",
        Arc::new(KEY.to_owned()),
        Arc::new(USERNAME.to_owned()),
        Arc::new(Missing),
    );
    assert!(matches!(
        nameless.claim().await,
        Err(Failure::Refused { detail, .. }) if !detail.contains(USERNAME) && !detail.contains(KEY)
    ));
    assert_eq!(fake.requests().len(), 1);
}
