use anyhow::Context as AnyhowContext;
use clap::Parser;
use serenity::{
    all::RoleId,
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use friend_of_ziggy::poll::MarketPoller;

#[derive(Parser)]
struct Cli {
    /// Token for Discord bot user.
    #[arg(env = "DISCORD_TOKEN", hide_env_values = true)]
    discord_token: String,
    /// Session token for dogonline.net.
    #[arg(env = "DOG_ONLINE_AUTH_TOKEN", hide_env_values = true)]
    dog_online_auth_token: String,
    /// Discord Thread ID for market notifications.
    #[arg(env = "DISCORD_THREAD_ID")]
    thread_id: ChannelId,
    /// Discord Role ID for HOT HOT HOT!!! notifications.
    #[arg(env = "DISCORD_MENTION_ROLE_ID")]
    mention_role_id: RoleId,
}

#[derive(Debug)]
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    let args = Cli::parse();

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(args.discord_token, intents)
        .event_handler(Handler {
            thread_id: args.thread_id,
        })
        .await
        .context("failed to build Serenity client")?;

    let poller = MarketPoller::new(
        &args.dog_online_auth_token,
        &client,
        args.thread_id,
        args.mention_role_id,
    );

    tokio::spawn(async move {
        poller.run().await;
    });

    client
        .start()
        .await
        .context("Serenity client exited with an error")?;
    Ok(())
}
