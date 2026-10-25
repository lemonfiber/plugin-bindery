//! Who holds Bindery's first account, and claiming it: the administrator lemonfiber
//! minted, made by Bindery's first-run setup while nobody has made one.

use std::sync::Arc;

use async_trait::async_trait;
use lemonfiber_adapter_kit::credential::Holding;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, Owner, Ownership};
use serde::Deserialize;

use super::Bindery;

const STATUS: &str = "/auth/status";

const SETUP: &str = "/auth/setup";

const LOGIN: &str = "/auth/login";

const SETUP_COMPLETE: u16 = 409;

const NO_ADMINISTRATOR: &str = "this adapter holds no administrator for Bindery";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    setup_required: bool,
}

impl Bindery {
    fn administrator(&self) -> Result<String, Failure> {
        let held = |holding: &Arc<dyn Holding>| {
            holding
                .key()
                .map_err(|why| self.endpoint.refused(&format!("{NO_ADMINISTRATOR}: {why}")))
        };
        Ok(serde_json::json!({
            "username": held(&self.username)?,
            "password": held(&self.password)?,
        })
        .to_string())
    }
}

#[async_trait]
impl Ownership for Bindery {
    async fn owner(&self) -> Result<Owner, Failure> {
        let status: Status = self
            .read(
                STATUS,
                "whether Bindery has its first account could not be read",
            )
            .await?;
        if status.setup_required {
            return Ok(Owner::Open);
        }
        let response = self
            .endpoint
            .send(&self.request(Method::Post, LOGIN, Some(self.administrator()?))?)
            .await?;
        match response.status {
            401 | 403 => Ok(Owner::Theirs),
            _ if response.is_success() => Ok(Owner::Ours),
            _ => Err(self.endpoint.refusal(&response)),
        }
    }

    async fn claim(&self) -> Result<Owner, Failure> {
        let held = self.owner().await?;
        if held != Owner::Open {
            return Ok(held);
        }
        let response = self
            .endpoint
            .send(&self.request(Method::Post, SETUP, Some(self.administrator()?))?)
            .await?;
        match response.status {
            SETUP_COMPLETE => self.owner().await,
            _ if response.is_success() => Ok(Owner::Ours),
            _ => Err(self.endpoint.refusal(&response)),
        }
    }
}
