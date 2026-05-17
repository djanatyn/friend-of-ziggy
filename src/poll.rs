use anyhow::Context;
use std::sync::Arc;

use serenity::{
    Client,
    all::{ChannelId, Http, RoleId},
    prelude::Mentionable,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::interval_at;
use tokio::time::{Duration, Instant};

use crate::api::MarketState;
use crate::market::MarketCondition;

pub struct MarketPoller {
    auth_token: String,
    discord_http: Arc<Http>,
    thread_id: ChannelId,
    mention_role: RoleId,
}

impl MarketPoller {
    const MARKET_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

    pub fn new(auth_token: &str, client: &Client, thread_id: ChannelId, role_id: RoleId) -> Self {
        Self {
            auth_token: auth_token.to_owned(),
            discord_http: client.http.clone(),
            thread_id,
            mention_role: role_id,
        }
    }

    fn duration_until_next_hour() -> Duration {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before Unix epoch");
        let remainder = elapsed.as_secs() % Self::MARKET_POLL_INTERVAL.as_secs();
        let seconds_until_next_hour = Self::MARKET_POLL_INTERVAL.as_secs() - remainder;

        Duration::from_secs(seconds_until_next_hour)
    }

    pub async fn run(self) {
        let mut interval = dbg!(interval_at(
            Instant::now() + Self::duration_until_next_hour(),
            Self::MARKET_POLL_INTERVAL,
        ));

        loop {
            interval.tick().await;
            self.check_once().await;
        }
    }

    fn discord_message(&self, condition: &MarketCondition) -> String {
        if condition.is_hot_hot_hot() {
            format!("{} {}", self.mention_role.mention(), condition.message())
        } else {
            condition.message().to_string()
        }
    }

    async fn fetch(&self) -> anyhow::Result<MarketCondition> {
        let auth_token = self.auth_token.to_owned();

        let condition = tokio::task::spawn_blocking(move || -> anyhow::Result<MarketCondition> {
            let payload: MarketState = MarketState::fetch(&auth_token)?;

            payload
                .market_condition()
                .map_err(anyhow::Error::msg)
                .context("failed to parse market condition")
        })
        .await
        .context("market fetch task failed")??;

        Ok(condition)
    }

    async fn check_once(&self) {
        match self.fetch().await {
            Ok(condition) => {
                let text = self.discord_message(&condition);
                if let Err(error) = self.thread_id.say(&self.discord_http, text).await {
                    eprintln!("failed to send Discord message: {error}");
                }
            }
            Err(error) => {
                eprintln!("failed to fetch market state: {error}");
            }
        }
    }
}
