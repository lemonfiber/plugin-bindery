use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Catalogue as _, Category, ClientProbe, Credential, DownloadClient, Failure, Identity, Protocol,
    QualityProfile, Queue, Queued, Queues as _, RegisteredClient, RegisteredFolder, RootFolder,
};
use lemonfiber_ports::Client as _;

use super::super::tests::{asked, bindery};

fn client(protocol: &str, field: &str, credential: Credential) -> DownloadClient {
    DownloadClient {
        name: "SABnzbd".to_owned(),
        host: "sabnzbd".to_owned(),
        port: 8080,
        protocol: Protocol(protocol.to_owned()),
        credential,
        category: Category {
            field: field.to_owned(),
            value: "books".to_owned(),
        },
    }
}

fn keyed() -> Credential {
    Credential::ApiKey("the-client-key".to_owned())
}

#[tokio::test]
async fn it_is_bindery_at_the_version_it_names() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"{"version":"v1.43.1","commit":"b0b8"}"#,
    ));
    assert_eq!(
        bindery(&fake).identity().await.ok(),
        Some(Identity {
            name: "Bindery".to_owned(),
            version: "v1.43.1".to_owned(),
        })
    );
    assert_eq!(
        asked(&fake),
        [(Method::Get, "/api/v1/system/status".to_owned(), None)]
    );
    let nameless = Fake::always(Answer::reply(200, r#"{"version":""}"#));
    assert!(matches!(
        bindery(&nameless).identity().await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn a_download_client_is_registered_by_its_kind_address_category_and_credential() {
    let fake = Fake::always(Answer::reply(201, "{}"));
    assert!(bindery(&fake)
        .register_download_client(&client("sabnzbd", "category", keyed()))
        .await
        .is_ok());
    let passworded = Credential::UserPass {
        username: "user".to_owned(),
        password: "pass".to_owned(),
    };
    assert!(bindery(&fake)
        .register_download_client(&client("qbittorrent", "categoryAudiobook", passworded))
        .await
        .is_ok());
    assert_eq!(
        asked(&fake),
        [
            (
                Method::Post,
                "/api/v1/downloadclient".to_owned(),
                Some(serde_json::json!({
                    "name": "SABnzbd", "type": "sabnzbd", "host": "sabnzbd", "port": 8080,
                    "enabled": true, "category": "books", "apiKey": "the-client-key",
                }))
            ),
            (
                Method::Post,
                "/api/v1/downloadclient".to_owned(),
                Some(serde_json::json!({
                    "name": "SABnzbd", "type": "qbittorrent", "host": "sabnzbd", "port": 8080,
                    "enabled": true, "categoryAudiobook": "books", "username": "user",
                    "password": "pass",
                }))
            ),
        ]
    );
}

#[tokio::test]
async fn a_client_bindery_cannot_take_or_a_field_it_does_not_file_under_is_refused_before_it_is_asked(
) {
    let fake = Fake::silent();
    for refused in [
        client("aria2", "category", keyed()),
        client("sabnzbd", "tvCategory", keyed()),
    ] {
        assert!(matches!(
            bindery(&fake).register_download_client(&refused).await,
            Err(Failure::Refused { .. })
        ));
        assert!(matches!(
            bindery(&fake).update_download_client("1", &refused).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(matches!(
        bindery(&fake)
            .set_client_field("1", "tvCategory", Some("x"))
            .await,
        Err(Failure::Refused { .. })
    ));
    for id in ["0", "-1", "one", ""] {
        assert!(matches!(
            bindery(&fake)
                .set_client_field(id, "category", Some("x"))
                .await,
            Err(Failure::Refused { .. })
        ));
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_client_it_holds_is_written_again_whole_or_one_field_at_a_time() {
    let fake = Fake::always(Answer::reply(200, "{}"));
    assert!(bindery(&fake)
        .update_download_client("7", &client("sabnzbd", "category", keyed()))
        .await
        .is_ok());
    assert!(bindery(&fake)
        .set_client_field("7", "category", Some("ebooks"))
        .await
        .is_ok());
    assert!(bindery(&fake)
        .set_client_field("7", "categoryAudiobook", None)
        .await
        .is_ok());
    let said = asked(&fake);
    assert_eq!(
        said.iter()
            .map(|(method, path, _)| (*method, path.as_str()))
            .collect::<Vec<_>>(),
        [
            (Method::Put, "/api/v1/downloadclient/7"),
            (Method::Put, "/api/v1/downloadclient/7"),
            (Method::Put, "/api/v1/downloadclient/7"),
        ]
    );
    assert_eq!(
        said.iter()
            .skip(1)
            .map(|(_, _, body)| body.clone())
            .collect::<Vec<_>>(),
        [
            Some(serde_json::json!({ "category": "ebooks" })),
            Some(serde_json::json!({ "categoryAudiobook": "" })),
        ]
    );
}

#[tokio::test]
async fn the_clients_it_holds_are_answered_by_where_they_are_reached_and_what_they_file_under() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"[{"id":1,"host":"sabnzbd","port":8080,"category":"books","apiKey":""},{"id":2,"host":"qbit","port":8081,"category":""}]"#,
    ));
    assert_eq!(
        bindery(&fake).download_clients().await.ok(),
        Some(vec![
            RegisteredClient {
                id: "1".to_owned(),
                host: "sabnzbd".to_owned(),
                port: 8080,
                category: Some(Category {
                    field: "category".to_owned(),
                    value: "books".to_owned(),
                }),
            },
            RegisteredClient {
                id: "2".to_owned(),
                host: "qbit".to_owned(),
                port: 8081,
                category: None,
            },
        ])
    );
}

#[tokio::test]
async fn every_client_is_tested_and_one_bindery_cannot_reach_says_why() {
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/api/v1/downloadclient",
            Answer::reply(200, r#"[{"id":1},{"id":2},{"id":3}]"#),
        ),
        (
            Method::Post,
            "/api/v1/downloadclient/1/test",
            Answer::reply(200, r#"{"message":"Connection verified"}"#),
        ),
        (
            Method::Post,
            "/api/v1/downloadclient/2/test",
            Answer::reply(
                502,
                r#"{"error":"could not reach SABnzbd at sabnzbd:8080"}"#,
            ),
        ),
        (
            Method::Post,
            "/api/v1/downloadclient/3/test",
            Answer::reply(500, ""),
        ),
    ]);
    assert_eq!(
        bindery(&fake).test_download_clients().await.ok(),
        Some(vec![
            ClientProbe {
                id: "1".to_owned(),
                reachable: true,
                detail: None,
            },
            ClientProbe {
                id: "2".to_owned(),
                reachable: false,
                detail: Some("could not reach SABnzbd at sabnzbd:8080".to_owned()),
            },
            ClientProbe {
                id: "3".to_owned(),
                reachable: false,
                detail: None,
            },
        ])
    );
    let unreachable = Fake::by_route(vec![(
        Method::Get,
        "/api/v1/downloadclient",
        Answer::reply(200, r#"[{"id":1}]"#),
    )]);
    assert!(bindery(&unreachable).test_download_clients().await.is_err());
}

#[tokio::test]
async fn root_folders_are_registered_by_path_and_answered_by_id_and_path() {
    let fake = Fake::in_turn(vec![
        Answer::reply(201, "{}"),
        Answer::reply(
            200,
            r#"[{"id":1,"path":"/data/media/books","freeSpace":0}]"#,
        ),
    ]);
    let books = RootFolder {
        path: "/data/media/books".to_owned(),
        media_type: "books".to_owned(),
    };
    assert!(bindery(&fake).register_root_folder(&books).await.is_ok());
    assert_eq!(
        bindery(&fake).root_folders().await.ok(),
        Some(vec![RegisteredFolder {
            id: "1".to_owned(),
            path: "/data/media/books".to_owned(),
        }])
    );
    assert_eq!(
        asked(&fake).first(),
        Some(&(
            Method::Post,
            "/api/v1/rootfolder".to_owned(),
            Some(serde_json::json!({ "path": "/data/media/books" }))
        ))
    );
}

#[tokio::test]
async fn quality_profiles_and_indexers_are_read_from_bindery() {
    let profiles = Fake::always(Answer::reply(
        200,
        r#"[{"id":1,"name":"Any","upgradeAllowed":true},{"id":2,"name":"E-Book"}]"#,
    ));
    assert_eq!(
        bindery(&profiles).quality_profiles().await.ok(),
        Some(vec![
            QualityProfile {
                id: 1,
                name: "Any".to_owned(),
            },
            QualityProfile {
                id: 2,
                name: "E-Book".to_owned(),
            },
        ])
    );
    let indexers = Fake::always(Answer::reply(200, r#"[{"id":1},{"id":2},{"id":3}]"#));
    assert_eq!(bindery(&indexers).indexer_count().await.ok(), Some(3));
}

#[tokio::test]
async fn the_queue_is_answered_in_bindery_s_words_with_what_stopped_marked() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"{"items":[
          {"id":1,"title":"Moving","status":"downloading","errorMessage":""},
          {"id":2,"title":"Broken","status":"importFailed","errorMessage":"permission denied"},
          {"id":3,"title":"Held","status":"importHeld"}
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
            total: 3,
            items: vec![
                queued("Moving", "ok", "downloading", None),
                queued("Broken", "error", "importFailed", Some("permission denied")),
                queued("Held", "warning", "importHeld", None),
            ],
        })
    );
    let empty = Fake::always(Answer::reply(200, "{}"));
    assert_eq!(bindery(&empty).queue().await.ok(), Some(Queue::default()));
}
