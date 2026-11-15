//! The Prowlarr instances Bindery pulls its indexers from, telling it about another, and
//! having it read its indexers again from every one it reads from.

use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{AggregatorAt, Failure, KnownAggregator, PullsIndexers};
use serde::Deserialize;

use super::{unsupported, Bindery};

const INSTANCES: &str = "/prowlarr";

const SYNC: &str = "sync";

const THE_INSTANCES: &str = "the Prowlarr instances Bindery pulls from could not be read";

const READS_SERVARR: &str = "servarr";

const ONLY_PROWLARR: &str =
    "Bindery pulls its indexers from Prowlarr alone, which it reads in the servarr API";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Instance {
    id: i64,
    #[serde(default)]
    url: String,
    #[serde(default)]
    api_key_configured: bool,
    #[serde(default)]
    enabled: bool,
}

#[async_trait]
impl PullsIndexers for Bindery {
    async fn aggregators(&self) -> Result<Vec<KnownAggregator>, Failure> {
        let held: Vec<Instance> = self.read(INSTANCES, THE_INSTANCES).await?;
        Ok(held
            .into_iter()
            .map(|instance| KnownAggregator {
                id: instance.id.to_string(),
                url: instance.url,
                keyed: instance.api_key_configured,
            })
            .collect())
    }

    async fn add_aggregator(&self, aggregator: &AggregatorAt) -> Result<(), Failure> {
        if aggregator.protocol.0 != READS_SERVARR {
            return Err(unsupported(ONLY_PROWLARR));
        }
        let body = serde_json::json!({
            "name": aggregator.name,
            "url": aggregator.url,
            "apiKey": aggregator.key,
            "syncOnStartup": true,
            "enabled": true,
        })
        .to_string();
        self.done(Method::Post, INSTANCES, Some(body)).await
    }

    async fn pull(&self) -> Result<(), Failure> {
        let held: Vec<Instance> = self.read(INSTANCES, THE_INSTANCES).await?;
        for instance in held.iter().filter(|instance| instance.enabled) {
            self.done(
                Method::Post,
                &format!("{INSTANCES}/{}/{SYNC}", instance.id),
                None,
            )
            .await?;
        }
        Ok(())
    }
}
