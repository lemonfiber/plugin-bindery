use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::stage::Stage;
use lemonfiber_ports::service::{DownloadState, Failure, Queue, Queued, Queues as _};

use super::super::tests::{asked, bindery};
use super::{fingerprint, Download};

fn download(status: &str) -> Download {
    serde_json::from_value(serde_json::json!({ "status": status, "size": 1000 }))
        .unwrap_or_default()
}

#[tokio::test]
async fn the_queue_is_answered_in_bindery_s_words_with_what_stopped_marked() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"{"items":[
          {"id":1,"title":"Moving","status":"downloading","errorMessage":""},
          {"id":2,"title":"Broken","status":"importFailed","errorMessage":"permission denied"},
          {"id":3,"title":"Held","status":"importHeld"},
          {"id":4,"title":"Said","status":"downloading","errorMessage":"tracker unreachable"}
        ]}"#,
    ));
    let queued = |title: &str, status: &str, state: &str, message: Option<&str>| Queued {
        title: title.to_owned(),
        status: status.to_owned(),
        state: state.to_owned(),
        message: message.map(str::to_owned),
        download_id: None,
        grabs: 1,
    };
    assert_eq!(
        bindery(&fake).queue().await.ok(),
        Some(Queue {
            total: 4,
            items: vec![
                queued("Moving", "ok", "downloading", None),
                queued("Broken", "error", "importFailed", Some("permission denied")),
                queued("Held", "warning", "importHeld", None),
                queued("Said", "error", "downloading", Some("tracker unreachable")),
            ],
        })
    );
    assert_eq!(
        asked(&fake),
        [(Method::Get, "/api/v1/queue".to_owned(), None)]
    );
    let empty = Fake::always(Answer::reply(200, "{}"));
    assert_eq!(bindery(&empty).queue().await.ok(), Some(Queue::default()));
    let unread = Fake::always(Answer::reply(200, "not json"));
    assert!(matches!(
        bindery(&unread).queue().await,
        Err(Failure::Refused { .. })
    ));
}

#[test]
fn each_state_bindery_tracks_a_download_in_is_a_stage_of_the_pipeline() {
    for (status, stage) in [
        ("grabbed", Stage::Grabbed),
        ("downloading", Stage::Downloading),
        ("failed", Stage::Downloading),
        ("completed", Stage::Downloaded),
        ("importPending", Stage::Downloaded),
        ("importExternal", Stage::Downloaded),
        ("importHeld", Stage::Downloaded),
        ("importFailed", Stage::Downloaded),
        ("importBlocked", Stage::Downloaded),
        ("importing", Stage::Importing),
        ("somethingNew", Stage::Downloading),
    ] {
        assert_eq!(download(status).stage(), stage, "{status}");
    }
}

#[test]
fn each_state_bindery_tracks_a_download_in_is_where_it_stands() {
    for (status, state) in [
        ("grabbed", DownloadState::Queued),
        ("downloading", DownloadState::Downloading),
        ("completed", DownloadState::Completed),
        ("importPending", DownloadState::Completed),
        ("importing", DownloadState::Completed),
        ("importExternal", DownloadState::Completed),
        ("importHeld", DownloadState::Warning),
        ("importFailed", DownloadState::Warning),
        ("failed", DownloadState::Failed),
        ("importBlocked", DownloadState::Failed),
        ("somethingNew", DownloadState::Warning),
    ] {
        assert_eq!(download(status).state(), state, "{status}");
    }
    let mut said = download("downloading");
    said.error_message = "tracker unreachable".to_owned();
    assert_eq!(said.state(), DownloadState::Warning);
    assert!(said.stuck());
    assert!(!download("downloading").stuck());
    assert!(download("downloading").waiting());
    assert!(!download("imported").waiting());
}

#[test]
fn what_is_left_is_read_from_the_share_its_client_says_is_done() {
    let at = |status: &str, percentage: &str| {
        let mut one = download(status);
        one.percentage = percentage.to_owned();
        one.left()
    };
    assert_eq!(at("downloading", "45.3"), 547);
    assert_eq!(at("downloading", "45%"), 550);
    assert_eq!(at("downloading", "0"), 1000);
    assert_eq!(at("downloading", "100.0"), 0);
    assert_eq!(at("downloading", "250"), 0);
    assert_eq!(at("downloading", "-3"), 1000);
    assert_eq!(at("downloading", ""), 1000);
    assert_eq!(at("grabbed", "NaN"), 1000);
    assert_eq!(at("completed", ""), 0);
    assert_eq!(at("importFailed", "unknown"), 0);
    let negative: Download =
        serde_json::from_value(serde_json::json!({ "size": -5 })).unwrap_or_default();
    assert_eq!(negative.bytes(), 0);
}

#[test]
fn the_client_s_id_and_the_release_s_fingerprint_are_named_where_bindery_holds_them() {
    let held = |value: serde_json::Value| -> Download {
        serde_json::from_value(value).unwrap_or_default()
    };
    assert_eq!(
        held(serde_json::json!({ "torrentId": "ABCDEF", "sabnzbdNzoId": "SABnzbd_nzo_1" }))
            .download_id()
            .as_deref(),
        Some("abcdef")
    );
    assert_eq!(
        held(serde_json::json!({ "torrentId": "", "sabnzbdNzoId": "SABnzbd_nzo_1" }))
            .download_id()
            .as_deref(),
        Some("SABnzbd_nzo_1")
    );
    assert_eq!(
        held(serde_json::json!({ "sabnzbdNzoId": "" })).download_id(),
        None
    );
    assert_eq!(
        fingerprint("abc").as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(fingerprint(""), None);
    assert_eq!(
        held(serde_json::json!({ "guid": "abc" })).release(),
        fingerprint("abc")
    );
}
