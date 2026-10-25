//! Bindery, the book curator, asked through its own API with the key lemonfiber wrote for
//! this adapter.

use std::sync::Arc;

use lemonfiber_adapter_kit::credential::Holding;
use lemonfiber_adapter_kit::endpoint::Endpoint;
use lemonfiber_ports::http::{Http, Method, Request, JSON};
use lemonfiber_ports::service::Failure;

mod curating;
mod owning;
mod pulling;

/// What every failure is blamed on.
pub const SERVICE: &str = "bindery";

const API: &str = "/api/v1";

const HEALTH: &str = "/api/v1/health";

const KEY_HEADER: &str = "X-Api-Key";

const CONTENT_TYPE: &str = "Content-Type";

/// A client for one Bindery, presenting the key `held` supplies on every call, and
/// claiming it for the administrator `username` and `password` supply.
pub struct Bindery {
    endpoint: Endpoint,
    held: Arc<dyn Holding>,
    username: Arc<dyn Holding>,
    password: Arc<dyn Holding>,
}

impl Bindery {
    /// The Bindery reached at `base` over `http`, asked with the key `held` supplies and
    /// claimed for the administrator `username` and `password` supply.
    #[must_use]
    pub fn new(
        http: Arc<dyn Http>,
        base: impl Into<String>,
        held: Arc<dyn Holding>,
        username: Arc<dyn Holding>,
        password: Arc<dyn Holding>,
    ) -> Self {
        Self {
            endpoint: Endpoint::new(http, base, SERVICE),
            held,
            username,
            password,
        }
    }

    /// Whether Bindery says it is healthy, which it says to anybody.
    pub async fn answers(&self) -> bool {
        let request = Request {
            method: Method::Get,
            url: self.endpoint.url(HEALTH),
            headers: Vec::new(),
            body: None,
            pinned: None,
        };
        self.endpoint
            .send(&request)
            .await
            .is_ok_and(|response| response.is_success())
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> Result<Request, Failure> {
        let key = self.held.key().map_err(|why| {
            self.endpoint
                .refused(&format!("this adapter holds no key for Bindery: {why}"))
        })?;
        let mut headers = vec![(KEY_HEADER.to_owned(), key)];
        if body.is_some() {
            headers.push((CONTENT_TYPE.to_owned(), JSON.to_owned()));
        }
        Ok(Request {
            method,
            url: self.endpoint.url(&format!("{API}{path}")),
            headers,
            body,
            pinned: None,
        })
    }

    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<T, Failure> {
        let response = self
            .endpoint
            .send(&self.request(Method::Get, path, None)?)
            .await?;
        self.endpoint.decode(&response, what)
    }

    async fn done(&self, method: Method, path: &str, body: Option<String>) -> Result<(), Failure> {
        let response = self
            .endpoint
            .send(&self.request(method, path, body)?)
            .await?;
        self.endpoint.expect_success(&response)
    }

    fn assigned(&self, id: &str) -> Result<u64, Failure> {
        id.parse::<u64>()
            .ok()
            .filter(|numeric| *numeric > 0)
            .ok_or_else(|| self.endpoint.refused("that is not an id Bindery assigns"))
    }
}

#[cfg(test)]
mod tests;
