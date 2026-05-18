use anyhow::Context;
use humantime::{format_duration, format_rfc3339};
use std::sync::Arc;
use tracing::{info, instrument};

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

/// Poll dogonline.net markets hourly and post Discord messages.
pub struct MarketPoller {
    auth_token: String,
    discord_http: Arc<Http>,
    thread_id: ChannelId,
    mention_role: RoleId,
}

impl MarketPoller {
    /// Markets update every 60 minutes on the hour.
    const MARKET_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

    /// Construct a new MarketPoller to notify a particular thread + role on market changes.
    #[instrument(skip(auth_token, client))]
    pub fn new(auth_token: &str, client: &Client, thread_id: ChannelId, role_id: RoleId) -> Self {
        info!("starting market poller");
        Self {
            auth_token: auth_token.to_owned(),
            discord_http: client.http.clone(),
            thread_id,
            mention_role: role_id,
        }
    }

    /// Markets update every hour on the hour, so wait until the next hour starts.
    fn duration_until_next_hour() -> Duration {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before Unix epoch");
        let remainder = elapsed.as_secs() % Self::MARKET_POLL_INTERVAL.as_secs();
        let seconds_until_next_hour = Self::MARKET_POLL_INTERVAL.as_secs() - remainder;

        Duration::from_secs(seconds_until_next_hour)
    }

    /// Periodically check market status and send Discord messages.
    pub async fn run(self) {
        // run a fetch at startup to test token before scheduling polling
        _ = self.fetch().await;

        // set periodic interval and track ticks
        let mut now = Instant::now();
        let mut time = SystemTime::now();
        let mut next_tick = now + Self::duration_until_next_hour();
        let mut next_poll = next_tick.saturating_duration_since(now);
        let mut interval = interval_at(next_tick, Self::MARKET_POLL_INTERVAL);

        info!(
            period = ?interval.period(),
            sleeping = format_duration(next_poll).to_string(),
            next_poll = format_rfc3339(time + next_poll).to_string(),
            "interval set"
        );

        loop {
            // wait for tick
            let tick = interval.tick().await;
            (time, now) = (SystemTime::now(), Instant::now());
            next_tick = tick + interval.period();
            next_poll = next_tick.saturating_duration_since(now);

            // check market state and send discord message
            info!(
                sleeping = format_duration(next_poll).to_string(),
                next_poll = format_rfc3339(time + next_poll).to_string(),
                "processing tick, scheduled next poll"
            );
            self.check_once().await;
        }
    }

    /// Format message for Discord, adding a mention if the market is HOT HOT HOT!!!.
    fn discord_message(&self, condition: &MarketCondition) -> String {
        if condition.is_hot_hot_hot() {
            format!("{} {}", self.mention_role.mention(), condition.message())
        } else {
            condition.message().to_string()
        }
    }

    /// Fetch `api::Marketstate` and parse `market::MarketCondition`.
    #[instrument(skip(self), err)]
    async fn fetch(&self) -> anyhow::Result<MarketCondition> {
        let auth_token = self.auth_token.to_owned();

        let condition = tokio::task::spawn_blocking(move || -> anyhow::Result<MarketCondition> {
            let payload: MarketState = MarketState::fetch(&auth_token)?;

            let condition = payload
                .market_condition()
                .map_err(anyhow::Error::msg)
                .context("failed to parse market condition")?;
            info!(?condition);
            Ok(condition)
        })
        .await
        .context("market fetch task failed")??;

        Ok(condition)
    }

    /// Run a scheduled check, fetching market status and posting a message to Discord.
    #[instrument(skip(self))]
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
