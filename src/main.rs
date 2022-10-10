use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use slack_to_discord::{slack, ChannelConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::{fs, io};
use tracing::info;

#[derive(clap::Parser, Debug)]
struct Opts {
    #[clap(short, long)]
    msg: PathBuf,
    #[clap(short, long)]
    db: String,
    #[clap(short, long)]
    config: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Config {
    channel: ChannelConfig,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();

    let db = slack_to_discord::Db::new(&opts.db).await?;

    let archive = fs::File::open(opts.msg).with_context(|| "Reading msg archive")?;
    let archive = io::BufReader::new(archive);
    let mut archive = zip::ZipArchive::new(archive).with_context(|| "Open msg archive")?;

    let channels_json = archive
        .by_name("channels.json")
        .with_context(|| "read channels.json")?;
    let channels = serde_json::from_reader(channels_json).with_context(|| "parse channels.json")?;

    let users = archive
        .by_name("users.json")
        .with_context(|| "read users.json")?;
    let users = serde_json::from_reader::<_, Vec<slack::User>>(users)
        .with_context(|| "parse users.json")?
        .into_iter()
        .map(|user| (user.id.clone(), user))
        .collect::<HashMap<_, _>>();

    let guild = slack_to_discord::discord::GuildId::from_env("GUILD_ID")?;
    let token = slack_to_discord::discord::BotToken::from_env("BOT_TOKEN")?;

    let config = tokio::fs::read(opts.config)
        .await
        .with_context(|| "read channel config")?;
    let config: Config = serde_json::from_slice(&config).with_context(|| "parse channel config")?;

    let discord_channels =
        slack_to_discord::provision_channels(&guild, &token, channels, &config.channel).await?;
    let slack_messages =
        slack_to_discord::get_channels_stream(&mut archive).with_context(|| "load messages")?;

    for channel in slack_messages {
        if !discord_channels.contains_key(&channel.name) {
            continue;
        }
        info!(
            "channel {} has {} messages",
            channel.name,
            channel.messages.len()
        );
        slack_to_discord::post_channel(&db, &token, &discord_channels, &channel, &users).await?;
    }
    Ok(())
}
