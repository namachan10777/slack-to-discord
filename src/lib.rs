#![feature(let_chains)]
use anyhow::Context;
use chrono::{FixedOffset, TimeZone};
use discord::ChannelGet;
use futures::StreamExt;
use slack::Message;
use std::time::Duration;
use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    ops::Deref,
};
use tokio::time::sleep;

use tracing::{debug, info, warn};
use zip::ZipArchive;

pub mod discord;
pub mod slack;

pub struct Db {
    pub pool: sqlx::Pool<sqlx::Sqlite>,
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
    #[error("no content-type")]
    NoCntentType,
    #[error("invalid content type")]
    InvalidContentType,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FileRow {
    pub url: String,
    pub inner: Vec<u8>,
    pub mime: String,
}

impl Db {
    pub async fn new(url: &str) -> Result<Self, sqlx::Error> {
        info!("connect db: {}", url);
        let pool = sqlx::sqlite::SqlitePool::connect(url).await?;
        let http_client = reqwest::Client::new();
        Ok(Self { pool, http_client })
    }

    pub async fn fetch_file(&self, url: &str) -> Result<FileRow, DbError> {
        let row = sqlx::query_as!(FileRow, "select * from files where url = ?", url)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::GetSql)?;
        if let Some(row) = row {
            debug!("{} found in db", url);
            Ok(row)
        } else {
            debug!("download {}", url);
            let response = self
                .http_client
                .get(url)
                .send()
                .await
                .map_err(DbError::FetchFromUrl)?;
            let mime = response
                .headers()
                .get("content-type")
                .ok_or(DbError::NoCntentType)?
                .to_str()
                .map_err(|_| DbError::InvalidContentType)?
                .to_owned();
            let bytes = response
                .bytes()
                .await
                .map_err(DbError::FetchFromUrl)?
                .to_vec();
            sqlx::query!(
                r#"insert into files (url, inner, mime) values (?, ?, ?)"#,
                url,
                bytes,
                mime
            )
            .execute(&self.pool)
            .await
            .map_err(DbError::InsertSql)?;
            Ok(FileRow {
                url: url.to_owned(),
                inner: bytes,
                mime,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct File {
    id: u64,
    url: String,
    blob: Vec<u8>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ChannelConfig(HashMap<String, String>);

async fn provision_channel_categories(
    guild: &discord::GuildId,
    token: &discord::BotToken,
    categories: &HashSet<&str>,
) -> Result<HashMap<String, discord::ChannelId>, anyhow::Error> {
    let mut deployed_categories = discord::get_channels(guild, token)
        .await?
        .into_iter()
        .filter(|channel| {
            channel.channel_type == discord::ChannelType::GuildCategory
                && categories.contains(&channel.name.borrow())
        })
        .map(|channel| (channel.name, channel.id))
        .collect::<HashMap<_, _>>();

    for category in categories {
        if !deployed_categories.contains_key(*category) {
            let channel = discord::post_channel(
                guild,
                token,
                &discord::ChannelPost {
                    name: category.deref().to_owned(),
                    channel_type: discord::ChannelType::GuildCategory,
                    parent_id: None,
                },
            )
            .await
            .with_context(|| format!("create category {}", category))?;
            deployed_categories.insert(category.deref().to_owned(), channel.id);
        }
        info!("provisioned category {}", category);
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
        &config
            .0
            .values()
            .map(|category| category.as_str())
            .collect(),
    )
    .await?;

    let categories_reverse = categories
        .iter()
        .map(|(x, y)| (y, x))
        .collect::<HashMap<_, _>>();

    let mut channels_deployed = discord::get_channels(guild, token)
        .await
        .with_context(|| "get discord channels")?
        .into_iter()
        .filter(|channel| {
            channel.channel_type == discord::ChannelType::GuildText
                && channel
                    .parent_id
                    .as_ref()
                    .map(|id| categories_reverse.contains_key(&id))
                    .unwrap_or(false)
        })
        .map(|channel| (channel.name.to_owned(), channel))
        .collect::<HashMap<_, _>>();

    debug!("deployed :{:#?} ", channels_deployed);

    for channel in channels {
        if let Some(_deployed_channel) = channels_deployed.get(&channel.name) {
            continue;
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
            warn!("unconfigured channel {}", channel.name);
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

        serde_json::from_reader(entry).with_context(|| "parse channels.json")?
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
            .with_context(|| "read zip entry name as utf8".to_string())?;
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

#[derive(Clone, PartialEq, Eq, sqlx::FromRow, Debug)]
struct PostRecord {
    id: String,
    slack_channel_id: String,
    discord_channel_id: String,
    slack_ts: String,
    discord_thread_id: Option<String>,
}

fn replace_slack_id_to_real_name(dict: &HashMap<String, String>, src: &str) -> String {
    dict.iter()
        .fold(src.to_owned(), |src, (from, to)| src.replace(from, to))
}

pub async fn post_channel(
    db: &Db,
    token: &discord::BotToken,
    discord_channels: &HashMap<String, ChannelGet>,
    channel: &SlackChannel,
    users: &HashMap<String, slack::User>,
) -> Result<(), anyhow::Error> {
    let discord_channel = discord_channels
        .get(&channel.name)
        .with_context(|| format!("get discord_channel_id of {}", &channel.name))?;
    let discord_channel_id = &discord_channel.id;

    let user_id_to_real_name = users
        .iter()
        .map(|(_, user)| (user.id.clone(), user.readable_name().to_owned()))
        .collect::<HashMap<_, _>>();

    for message in &channel.messages {
        match message {
            slack::Message::Message {
                text,
                files,
                ts,
                reply_count,
                user,
                thread_ts,
                ..
            } => {
                let message_on_db: Option<PostRecord> = sqlx::query_as!(
                    PostRecord,
                    "select * from posts where slack_ts = ? and slack_channel_id = ?",
                    ts,
                    channel.id
                )
                .fetch_optional(&db.pool)
                .with_context(|| format!("ts: {}, channel_id: {}", ts, channel.id))
                .await?;
                if message_on_db.is_none() {
                    let text = format!("**{}** {}\n{}\n", user, ts.jtc_date().to_rfc2822(), text);
                    let message = discord::MessagePost {
                        content: replace_slack_id_to_real_name(&user_id_to_real_name, &text),
                    };
                    let files = files.iter().flatten().collect::<Vec<_>>();
                    let files = futures::stream::iter(files)
                        .filter_map(|file| async move {
                            match file {
                                slack::File::Hosted {
                                    name,
                                    title,
                                    url_private_download,
                                } => {
                                    let file_row = db.fetch_file(url_private_download).await;
                                    Some(file_row.map(|file_row| {
                                        let file = discord::FilePost {
                                            mime: file_row.mime.clone(),
                                            title: title.clone(),
                                            body: file_row.inner,
                                        };
                                        (name.clone(), file)
                                    }))
                                }
                                _ => None,
                            }
                        })
                        .collect::<Vec<_>>()
                        .await
                        .into_iter()
                        .collect::<Result<HashMap<_, _>, _>>()?;
                    if let Some(thread_ts) = thread_ts && reply_count.is_none() {
                            debug!("reply to {}", thread_ts);
                            let thread = sqlx::query_as!(
                                PostRecord,
                                "select * from posts where slack_ts = ? and slack_channel_id = ?",
                                thread_ts,
                                channel.id
                            )
                            .fetch_one(&db.pool)
                            .with_context(|| format!("ts: {}, channel_id: {}", ts, channel.id))
                            .await?;
                            let discord_thread_id =
                                thread.discord_thread_id.with_context(|| {
                                    format!(
                                        "thread {} on {} not found",
                                        thread.slack_ts, channel.name
                                    )
                                })?;
                            let msg = discord::post_message(token, &discord_thread_id.into(), &message, files)
                                .await?;

                            sqlx::query!(
                                "insert into posts values (?, ?, ?, ?, ?);",
                                msg.id,
                                channel.id,
                                discord_channel_id,
                                ts,
                                None::<String>,
                            )
                            .execute(&db.pool)
                            .await?;
                        } else {
                            let msg =
                                discord::post_message(token, discord_channel_id, &message, files).await?;
                            let thread_id = if let Some(count) = reply_count && *count > 0 {
                                    debug!("reply_count: {:?}", count);
                                    Some(
                                        discord::start_thread(
                                            token,
                                            discord_channel_id,
                                            &msg.id,
                                            "slack thread",
                                        )
                                        .await?
                                        .id,
                                    )
                                } else {
                                    None
                                };


                            sqlx::query!(
                                "insert into posts values (?, ?, ?, ?, ?);",
                                msg.id,
                                channel.id,
                                discord_channel_id,
                                ts,
                                thread_id
                            )
                            .execute(&db.pool)
                            .with_context(|| format!("msg.id: {}", msg.id))
                            .await?;
                        }
                    sleep(Duration::from_millis(1000)).await;
                } else {
                    if let Some(message_on_db) = message_on_db {
                        if let Some(thread_id) = message_on_db.discord_thread_id {
                            let thread = discord::get_channel(token, thread_id.into()).await?;
                            info!("{:?}", thread);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
