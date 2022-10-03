use anyhow::Context;
use discord::{ChannelGet, ChannelId, MessageId, MessagePost};
use slack::Message;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};
use zip::ZipArchive;

pub mod discord;
pub mod slack;

pub struct Db {
    pool: sqlx::Pool<sqlx::Sqlite>,
    http_client: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("get {0}")]
    GetSql(sqlx::Error),
    #[error("insert {0}")]
    InsertSql(sqlx::Error),
    #[error("fetch from url {0}")]
    FetchFromUrl(reqwest::Error),
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
struct FileRow {
    url: String,
    inner: Vec<u8>,
}

impl Db {
    pub async fn new(url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::sqlite::SqlitePool::connect(url).await?;
        let http_client = reqwest::Client::new();
        Ok(Self { pool, http_client })
    }

    pub async fn fetch_file(&self, url: &str) -> Result<Vec<u8>, DbError> {
        let row = sqlx::query_as::<_, FileRow>("select * from files where url = $1")
            .bind(url)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::GetSql)?;
        if let Some(row) = row {
            debug!("{} found in db", url);
            Ok(row.inner)
        } else {
            debug!("download {}", url);
            let response = self
                .http_client
                .get(url)
                .send()
                .await
                .map_err(DbError::FetchFromUrl)?
                .bytes()
                .await
                .map_err(DbError::FetchFromUrl)?;
            sqlx::query("insert into files (url, inner) values ($1, $2)")
                .bind(url)
                .bind(response.as_ref())
                .execute(&self.pool)
                .await
                .map_err(DbError::InsertSql)?;
            Ok(response.as_ref().to_vec())
        }
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct File {
    id: u64,
    url: String,
    blob: Vec<u8>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ChannelConfig(HashMap<String, String>);

async fn provision_channel_categories<S: AsRef<str>>(
    guild: &discord::GuildId,
    token: &discord::BotToken,
    categories: &HashSet<S>,
) -> Result<HashMap<String, discord::ChannelId>, anyhow::Error> {
    let mut deployed_categories = discord::get_channels(guild, token)
        .await?
        .into_iter()
        .filter(|channel| channel.channel_type == discord::ChannelType::GuildCategory)
        .map(|channel| (channel.name, channel.id))
        .collect::<HashMap<_, _>>();

    for category in categories {
        if !deployed_categories.contains_key(category.as_ref()) {
            let channel = discord::post_channel(
                guild,
                token,
                &discord::ChannelPost {
                    name: category.as_ref().to_owned(),
                    channel_type: discord::ChannelType::GuildCategory,
                    parent_id: None,
                },
            )
            .await
            .with_context(|| format!("create category {}", category.as_ref()))?;
            deployed_categories.insert(category.as_ref().to_owned(), channel.id);
        }
        info!("provisioned category {}", category.as_ref());
    }

    Ok(deployed_categories)
}

pub async fn provision_channels(
    guild: &discord::GuildId,
    token: &discord::BotToken,
    channels: Vec<slack::Channel>,
    config: &ChannelConfig,
) -> Result<HashMap<String, ChannelGet>, anyhow::Error> {
    let categories = provision_channel_categories(
        guild,
        token,
        &config.0.iter().map(|(_, category)| category).collect(),
    )
    .await?;

    let categories_reverse = categories
        .iter()
        .map(|(x, y)| (y, x).clone())
        .collect::<HashMap<_, _>>();

    let mut channels_deployed = discord::get_channels(guild, token)
        .await
        .with_context(|| "get discord channels")?
        .into_iter()
        .filter(|channel| channel.channel_type == discord::ChannelType::GuildText)
        .map(|channel| (channel.name.clone(), channel))
        .collect::<HashMap<_, _>>();

    for channel in channels {
        if let Some(deployed_channel) = channels_deployed.get(&channel.name) {
            if let Some(parent_id) = &deployed_channel.parent_id {
                if let Some(parent_channel_name) = categories_reverse.get(&parent_id) {
                    if *parent_channel_name == &channel.name {
                        continue;
                    }
                }
            }
        }

        if let Some(category_name) = config.0.get(&channel.name) {
            let parent_id = categories
                .get(category_name)
                .with_context(|| format!("category {} yet deployed", category_name))?;
            let channel = discord::post_channel(
                guild,
                token,
                &discord::ChannelPost {
                    name: channel.name.clone(),
                    channel_type: discord::ChannelType::GuildText,
                    parent_id: Some(parent_id.clone()),
                },
            )
            .await
            .with_context(|| format!("deploy channel {}", channel.name))?;
            channels_deployed.insert(channel.name.clone(), channel);
        } else {
            warn!("uncofigured channel {}", channel.name);
        }
    }
    Ok(channels_deployed)
}

pub struct SlackChannel {
    pub id: String,
    pub name: String,
    pub messages: Vec<Message>,
}

pub fn get_channels_stream<R: std::io::Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
) -> Result<Vec<SlackChannel>, anyhow::Error> {
    let channels: Vec<slack::Channel> = {
        let entry = zip
            .by_name("channels.json")
            .with_context(|| "read channels.json")?;
        let channels = serde_json::from_reader(entry).with_context(|| "parse channels.json")?;
        channels
    };

    let mut channels = channels
        .into_iter()
        .map(|channel| {
            let channel = SlackChannel {
                id: channel.id.clone(),
                name: channel.name,
                messages: Vec::new(),
            };
            (channel.name.clone(), channel)
        })
        .collect::<HashMap<_, _>>();

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .with_context(|| format!("get zip entry at {}", index))?;
        let entry_name = String::from_utf8(entry.name_raw().to_owned())
            .with_context(|| format!("read zip entry name as utf8"))?;
        if let &[channel_name, file_name] = &entry_name.split('/').collect::<Vec<_>>()[..] {
            if file_name.is_empty() {
                debug!("skip dir {}", entry_name);
                continue;
            }
            let mut messages: Vec<slack::Message> = serde_json::from_reader(&mut entry)
                .with_context(|| format!("parse {}", entry_name))?;
            channels
                .get_mut(channel_name)
                .with_context(|| format!("{} not found in channels.json", entry_name))?
                .messages
                .append(&mut messages);
        } else {
            debug!("skip entry {}", entry.name());
        }
    }

    let sorted_channels = channels
        .into_values()
        .into_iter()
        .map(|mut channel| {
            channel.messages.sort_by_key(|message| match message {
                slack::Message::Message { ts, .. } => ts.clone(),
            });
            channel
        })
        .collect::<Vec<_>>();
    Ok(sorted_channels)
}

#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
struct PostRecord {
    id: MessageId,
    slack_channel_id: String,
    discord_channel_id: ChannelId,
    slack_ts: slack::TimeStamp,
    discord_thread_id: ChannelId,
}

pub async fn post_channel(
    db: &Db,
    token: &discord::BotToken,
    discord_channels: &HashMap<String, ChannelGet>,
    channel: &SlackChannel,
) -> Result<(), anyhow::Error> {
    let discord_channel = discord_channels
        .get(&channel.name)
        .with_context(|| format!("get discord_channel_id of {}", &channel.name))?;
    let discord_channel_id = &discord_channel.id;
    for message in &channel.messages {
        match message {
            slack::Message::Message {
                text,
                files,
                ts,
                ..
            } => {
                let message_on_db = sqlx::query_as::<_, PostRecord>(
                    "select * from posts where slack_ts == $1 and slack_channel_id == $2",
                )
                .bind(ts)
                .bind(&channel.id)
                .fetch_optional(&db.pool)
                .await
                .with_context(|| format!("read message on {} at {}", channel.name, ts.date()))?;

                if message_on_db.is_some() {
                    continue;
                }

                if let Some(_files) = files {
                    unimplemented!();
                } else {
                    let discord_message = discord::post_message(
                        token,
                        &discord_channel_id,
                        &MessagePost {
                            content: text.clone(),
                        },
                    )
                    .await?;
                    sqlx::query(
                        "insert into posts (id, slack_channel_id, discord_channel_id, slack_ts, discord_thread_id) values ($1, $2, $3, $4, null)"
                    )
                    .bind(discord_message.id)
                    .bind(&channel.id)
                    .bind(discord_message.channel_id)
                    .bind(ts)
                    .execute(&db.pool).await?;
                }
            }
        }
    }
    Ok(())
}
