use std::{
    env,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serenity::{
    all::RoleId,
    async_trait,
    http::Http,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};

use tokio::time::{Instant, interval_at};

// avoid shadowing serenity::prelude::Context
use anyhow::Context as AnyhowContext;

const MARKET_STATE_URL: &str = "https://dogonline.net/auction/api/getstate";
const MARKET_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub mod api {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct MarketStateResponse {
        #[serde(rename = "marketConditions")]
        pub market_conditions: MarketConditionResponse,
    }

    #[derive(Debug, Deserialize)]
    pub struct MarketConditionResponse {
        pub id: u64,
        pub message: String,
    }
}

#[derive(Debug)]
enum MarketCondition {
    InASlump,
    ChuggingAlong,
    LookingHot,
    HotHotHot,
}

impl MarketCondition {
    fn is_hot_hot_hot(&self) -> bool {
        matches!(self, Self::HotHotHot)
    }

    fn message(&self) -> String {
        match self {
            Self::InASlump => "in a slump. 😴".to_string(),
            Self::ChuggingAlong => "just chugging along... 🚂".to_string(),
            Self::LookingHot => "looking HOT! 🔥".to_string(),
            Self::HotHotHot => "HOT HOT HOT!!! 🔥🥵🔥".to_string(),
        }
    }
}

impl TryFrom<api::MarketStateResponse> for MarketCondition {
    type Error = String;

    fn try_from(value: api::MarketStateResponse) -> Result<Self, Self::Error> {
        match value.market_conditions.id {
            1 => Ok(MarketCondition::InASlump),
            2 => Ok(MarketCondition::ChuggingAlong),
            3 => Ok(MarketCondition::LookingHot),
            4 => Ok(MarketCondition::HotHotHot),
            id => Err(format!(
                "failed to parse condition: {} ({})",
                id, value.market_conditions.message
            )),
        }
    }
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
    const MENTION_ROLE: u64 = 1505602669694160986;

    async fn run(self) {
        self.check_once().await;

        let mut interval = dbg!(interval_at(
            Instant::now() + duration_until_next_hour(),
            MARKET_POLL_INTERVAL,
        ));

        loop {
            interval.tick().await;
            self.check_once().await;
        }
    }

    fn discord_message(condition: &MarketCondition) -> String {
        if condition.is_hot_hot_hot() {
            format!(
                "{} {}",
                RoleId::new(Self::MENTION_ROLE).mention(),
                condition.message()
            )
        } else {
            condition.message()
        }
    }

    async fn check_once(&self) {
        match dbg!(fetch_market_condition(&self.auth_token).await) {
            Ok(condition) => {
                let text = Self::discord_message(&condition);
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

fn required_env(key: &str) -> anyhow::Result<String> {
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

fn required_channel_id(key: &str) -> anyhow::Result<u64> {
    required_env(key)?
        .parse()
        .with_context(|| format!("environment variable {key} is not a valid Discord ID"))
}

async fn fetch_market_condition(auth_token: &str) -> anyhow::Result<MarketCondition> {
    let auth_token = auth_token.to_owned();

    let condition = tokio::task::spawn_blocking(move || -> anyhow::Result<MarketCondition> {
        let cookie = format!("__Secure-better-auth.session_token={auth_token}");

        let mut response = ureq::get(MARKET_STATE_URL)
            .header("Cookie", &cookie)
            .call()
            .context("request to market API failed")?;

        let payload: api::MarketStateResponse = response
            .body_mut()
            .read_json()
            .context("failed to decode market API response")?;

        MarketCondition::try_from(payload)
            .map_err(anyhow::Error::msg)
            .context("failed to parse market condition")
    })
    .await
    .context("market fetch task failed")??;

    Ok(condition)
}

fn duration_until_next_hour() -> Duration {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch");
    let remainder = elapsed.as_secs() % MARKET_POLL_INTERVAL.as_secs();
    let seconds_until_next_hour = MARKET_POLL_INTERVAL.as_secs() - remainder;

    Duration::from_secs(seconds_until_next_hour)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
