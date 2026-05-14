use std::{
    env,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use serde::Deserialize;
use serenity::{
    async_trait,
    http::Http,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};
use tokio::time::{Instant, interval_at};

const MARKET_STATE_URL: &str = "https://dogonline.net/auction/api/getstate";
const MARKET_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Deserialize)]
struct MarketStateResponse {
    #[serde(rename = "marketConditions")]
    market_conditions: MarketCondition,
}

#[derive(Debug, Deserialize)]
struct MarketCondition {
    id: u64,
    message: String,
}

struct Handler {
    thread_id: ChannelId,
}

struct MarketPoller {
    auth_token: String,
    discord_http: Arc<Http>,
    thread_id: ChannelId,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _: Context, msg: Message) {
        if msg.channel_id == self.thread_id {
            println!(
                "thread message from {} ({}): {}",
                msg.author.name, msg.author.id, msg.content
            );
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("connected as {}", ready.user.name);
    }
}

impl MarketPoller {
    async fn run(self) {
        self.check_once().await;

        let mut interval = interval_at(
            Instant::now() + duration_until_next_hour(),
            MARKET_POLL_INTERVAL,
        );

        loop {
            interval.tick().await;
            self.check_once().await;
        }
    }

    async fn check_once(&self) {
        match fetch_market_condition(&self.auth_token).await {
            Ok(condition) => {
                let content = format!("Market status: {} ({})", condition.message, condition.id);

                if let Err(error) = self.thread_id.say(&self.discord_http, content).await {
                    eprintln!("failed to send Discord message: {error}");
                }
            }
            Err(error) => {
                eprintln!("failed to fetch market state: {error}");
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let discord_token = required_env("DISCORD_TOKEN")?;
    let dog_online_auth_token = required_env("DOG_ONLINE_AUTH_TOKEN")?;
    let thread_id = ChannelId::new(required_channel_id("DISCORD_THREAD_ID")?);

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&discord_token, intents)
        .event_handler(Handler { thread_id })
        .await
        .context("failed to build Serenity client")?;

    let poller = MarketPoller {
        auth_token: dog_online_auth_token,
        discord_http: client.http.clone(),
        thread_id,
    };

    tokio::spawn(async move {
        poller.run().await;
    });

    client
        .start()
        .await
        .context("Serenity client exited with an error")?;
    Ok(())
}

fn required_env(key: &str) -> Result<String> {
    match env::var(key) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => {
            anyhow::bail!("missing required environment variable {key}")
        }
        Err(env::VarError::NotUnicode(_)) => {
            anyhow::bail!("environment variable {key} is not valid Unicode")
        }
    }
}

fn required_channel_id(key: &str) -> Result<u64> {
    required_env(key)?
        .parse()
        .with_context(|| format!("environment variable {key} is not a valid Discord ID"))
}

async fn fetch_market_condition(auth_token: &str) -> Result<MarketCondition, String> {
    let auth_token = auth_token.to_owned();

    tokio::task::spawn_blocking(move || {
        let cookie = format!("__Secure-better-auth.session_token={auth_token}");

        let mut response = ureq::get(MARKET_STATE_URL)
            .header("Cookie", &cookie)
            .call()
            .map_err(|error| format!("request failed: {error}"))?;

        let payload: MarketStateResponse = response
            .body_mut()
            .read_json()
            .map_err(|error| format!("invalid response body: {error}"))?;

        Ok(payload.market_conditions)
    })
    .await
    .map_err(|error| format!("market fetch task failed: {error}"))?
}

fn duration_until_next_hour() -> Duration {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch");
    let remainder = elapsed.as_secs() % MARKET_POLL_INTERVAL.as_secs();
    let seconds_until_next_hour = MARKET_POLL_INTERVAL.as_secs() - remainder;

    Duration::from_secs(seconds_until_next_hour)
}
