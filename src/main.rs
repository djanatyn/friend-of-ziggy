use anyhow::Context as AnyhowContext;
use serenity::{
    all::RoleId,
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};

use std::env;

use friend_of_ziggy::poll::MarketPoller;

struct Handler {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let discord_token = required_env("DISCORD_TOKEN")?;
    let dog_online_auth_token = required_env("DOG_ONLINE_AUTH_TOKEN")?;
    let thread_id = ChannelId::new(required_channel_id("DISCORD_THREAD_ID")?);
    let mention_role_id = RoleId::new(required_channel_id("DISCORD_MENTION_ROLE_ID")?);

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&discord_token, intents)
        .event_handler(Handler { thread_id })
        .await
        .context("failed to build Serenity client")?;

    let poller = MarketPoller::new(&dog_online_auth_token, &client, thread_id, mention_role_id);

    tokio::spawn(async move {
        poller.run().await;
    });

    client
        .start()
        .await
        .context("Serenity client exited with an error")?;
    Ok(())
}
