//! What this adapter says it is, and the kit that serves its contracts.

use std::sync::Arc;

use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::{indexer, library, owner};

use crate::bindery::Bindery;

/// Where Bindery answers, by its service id and the port it listens on.
pub const ADDRESS: &str = "http://bindery:8787";

/// The part of `upstream.json` holding the key Bindery answers its API to.
pub const API_KEY: &str = "api_key";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Bindery";

/// The Bindery release this adapter's recordings were taken from.
pub const RELEASE: &str = "v1.43.1";

/// The digest of the image that release's recordings were taken from.
pub const DIGEST: &str = "sha256:27109c963f91a27f08d8825b406b2b5a5c59aed080bd9f3a7e585e235673d489";

/// What this adapter says it is: the contracts it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![
            lemonfiber_contract::spoken(owner::claim::CAPABILITY, owner::claim::MAJOR),
            lemonfiber_contract::spoken(indexer::pull::CAPABILITY, indexer::pull::MAJOR),
            lemonfiber_contract::spoken(library::curate::CAPABILITY, library::curate::MAJOR),
        ],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The contracts, served from `bindery` under `key`, ready while Bindery answers.
#[must_use]
pub fn kit(key: String, bindery: Arc<Bindery>) -> Kit {
    let asked = Arc::clone(&bindery);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .serving(owner::claim::served(Arc::clone(&bindery)))
        .serving(indexer::pull::served(Arc::clone(&bindery)))
        .serving(library::curate::served(bindery))
}

#[cfg(test)]
mod tests;
