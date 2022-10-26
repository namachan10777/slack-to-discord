use std::collections::HashMap;

use reqwest::{multipart, Client};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use serde_repr::{Deserialize_repr, Serialize_repr};
use sqlx::{Decode, Encode};
use tracing::{info, trace};

const DISCORD_ENDPOINT_COMMON: &str = "https://discord.com/api/v10";

pub struct BotToken(String);
pub struct GuildId(String);

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct ChannelId(String);

impl From<String> for ChannelId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl<'r, DB: sqlx::Database> sqlx::Decode<'r, DB> for ChannelId
where
    &'r str: Decode<'r, DB>,
{
    fn decode(
        value: <DB as sqlx::database::HasValueRef<'r>>::ValueRef,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<DB>>::decode(value)?;
        Ok(ChannelId(s.to_owned()))
    }
}

impl<'q, DB: sqlx::Database> sqlx::Encode<'q, DB> for ChannelId
where
    String: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        <String as Encode<'q, DB>>::encode(self.0.clone(), buf)
    }
}

impl<DB: sqlx::Database> sqlx::Type<DB> for ChannelId
where
    String: sqlx::Type<DB>,
{
    fn type_info() -> <DB as sqlx::Database>::TypeInfo {
        <String as sqlx::Type<DB>>::type_info()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretLoadError {
    #[error("not present by env-var")]
    NotPresentByEnvVar,
    #[error("not unicode")]
    NotUnicode,
}

impl BotToken {
    pub fn from_env(env_name: &str) -> Result<Self, SecretLoadError> {
        match std::env::var(env_name) {
            Ok(var) => Ok(Self(var)),
            Err(std::env::VarError::NotPresent) => Err(SecretLoadError::NotPresentByEnvVar),
            Err(std::env::VarError::NotUnicode(_)) => Err(SecretLoadError::NotUnicode),
        }
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl GuildId {
    pub fn from_env(env_name: &str) -> Result<Self, SecretLoadError> {
        match std::env::var(env_name) {
            Ok(var) => Ok(Self(var)),
            Err(std::env::VarError::NotPresent) => Err(SecretLoadError::NotPresentByEnvVar),
            Err(std::env::VarError::NotUnicode(_)) => Err(SecretLoadError::NotUnicode),
        }
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Deserialize, Debug)]
pub struct ChannelGet {
    pub name: String,
    pub id: ChannelId,
    #[serde(rename = "type")]
    pub channel_type: ChannelType,
    pub parent_id: Option<ChannelId>,
    pub message_count: Option<u64>,
}

#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelType {
    GuildText = 0,
    GuildVoice = 2,
    GuildCategory = 4,
    PublicThread = 11,
}

#[derive(Serialize)]
pub struct ChannelPost {
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: ChannelType,
    pub parent_id: Option<ChannelId>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request :: {0}")]
    Request(reqwest::Error),
    #[error("schema :: {0}")]
    Schema(serde_json::Error),
    #[error("invalid mime :: {0}")]
    InvalidMimeType(reqwest::Error),
}

async fn get_method<T: DeserializeOwned>(token: &BotToken, url: &str) -> Result<T, Error> {
    let response = Client::new()
        .get(url)
        .header("Authorization", format!("Bot {}", token.as_str()))
        .send()
        .await
        .map_err(Error::Request)?
        .text()
        .await
        .map_err(Error::Request)?;
    trace!("response: {}", response);
    serde_json::from_str(&response).map_err(Error::Schema)
}

async fn post_method_json<R: DeserializeOwned, P: Serialize>(
    token: &BotToken,
    url: &str,
    payload: P,
) -> Result<R, Error> {
    let response = Client::new()
        .post(url)
        .header("Authorization", format!("Bot {}", token.as_str()))
        .json(&payload)
        .send()
        .await
        .map_err(Error::Request)?
        .text()
        .await
        .map_err(Error::Request)?;
    trace!("response: {}", response);
    serde_json::from_str(&response).map_err(Error::Schema)
}

async fn patch_method_json<R: DeserializeOwned, P: Serialize>(
    token: &BotToken,
    url: &str,
    payload: P,
) -> Result<R, Error> {
    let response = Client::new()
        .patch(url)
        .header("Authorization", format!("Bot {}", token.as_str()))
        .json(&payload)
        .send()
        .await
        .map_err(Error::Request)?
        .text()
        .await
        .map_err(Error::Request)?;
    trace!("response: {}", response);
    serde_json::from_str(&response).map_err(Error::Schema)
}

pub async fn get_channels(guild: &GuildId, token: &BotToken) -> Result<Vec<ChannelGet>, Error> {
    get_method(
        token,
        &format!(
            "{}/guilds/{}/channels",
            DISCORD_ENDPOINT_COMMON,
            guild.as_str()
        ),
    )
    .await
}

pub async fn post_channel(
    guild: &GuildId,
    token: &BotToken,
    channel: &ChannelPost,
) -> Result<ChannelGet, Error> {
    post_method_json(
        token,
        &format!(
            "{}/guilds/{}/channels",
            DISCORD_ENDPOINT_COMMON,
            guild.as_str()
        ),
        channel,
    )
    .await
}

#[derive(Debug, Clone)]
pub struct FilePost {
    pub mime: String,
    pub title: String,
    pub body: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MessagePost {
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MessageId(String);

impl<'r, DB: sqlx::Database> sqlx::Decode<'r, DB> for MessageId
where
    &'r str: Decode<'r, DB>,
{
    fn decode(
        value: <DB as sqlx::database::HasValueRef<'r>>::ValueRef,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<DB>>::decode(value)?;
        Ok(MessageId(s.to_owned()))
    }
}

impl<'q, DB: sqlx::Database> sqlx::Encode<'q, DB> for MessageId
where
    String: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        <String as Encode<'q, DB>>::encode(self.0.clone(), buf)
    }
}

impl<DB: sqlx::Database> sqlx::Type<DB> for MessageId
where
    String: sqlx::Type<DB>,
{
    fn type_info() -> <DB as sqlx::Database>::TypeInfo {
        <String as sqlx::Type<DB>>::type_info()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MessageGet {
    pub id: MessageId,
    pub channel_id: ChannelId,
}

pub async fn post_message(
    token: &BotToken,
    channel: &ChannelId,
    message: &MessagePost,
    attached_files: HashMap<String, FilePost>,
) -> Result<MessageGet, Error> {
    if attached_files.is_empty() {
        post_method_json(
            token,
            &format!(
                "{}/channels/{}/messages",
                DISCORD_ENDPOINT_COMMON, channel.0
            ),
            message,
        )
        .await
    } else {
        let attachments = attached_files
            .iter()
            .enumerate()
            .map(|(index, (filename, file))| {
                json!({
                    "id": index,
                    "filename": filename,
                    "description": file.title.clone(),
                })
            })
            .collect::<Vec<_>>();
        let parts = attached_files
            .into_iter()
            .map(|(filename, file)| {
                multipart::Part::bytes(file.body)
                    .file_name(filename)
                    .mime_str(&file.mime)
                    .map_err(Error::InvalidMimeType)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let form = parts
            .into_iter()
            .enumerate()
            .fold(multipart::Form::new(), |form, (index, part)| {
                form.part(format!("files[{}]", index), part)
            });
        let payload_json = json!({
            "content": message.content,
            "attachments": attachments,
        });
        let form = form.part(
            "payload_json",
            multipart::Part::text(serde_json::to_string(&payload_json).unwrap())
                .mime_str("application/json")
                .unwrap(),
        );
        info!("post files");
        let response = reqwest::Client::new()
            .post(format!(
                "{}/channels/{}/messages",
                DISCORD_ENDPOINT_COMMON, channel.0
            ))
            .multipart(form)
            .header("Authorization", format!("Bot {}", token.0))
            .send()
            .await
            .map_err(Error::Request)?
            .text()
            .await
            .map_err(Error::Request)?;
        trace!("response: {}", response);
        serde_json::from_str(&response).map_err(Error::Schema)
    }
}

pub async fn get_channel(token: &BotToken, channel: &ChannelId) -> Result<ChannelGet, Error> {
    get_method(
        token,
        &format!("{}/channels/{}", DISCORD_ENDPOINT_COMMON, channel.0),
    )
    .await
}

pub async fn start_thread(
    token: &BotToken,
    channel: &ChannelId,
    message_id: &MessageId,
    name: &str,
) -> Result<ChannelGet, Error> {
    post_method_json(
        token,
        &format!(
            "{}/channels/{}/messages/{}/threads",
            DISCORD_ENDPOINT_COMMON, channel.0, message_id.0
        ),
        json!({
            "name": name,
        }),
    )
    .await
}

pub async fn get_channel(token: &BotToken, channel: &ChannelId) -> Result<ChannelGet, Error> {
    get_method(
        token,
        &format!("{}/channels/{}", DISCORD_ENDPOINT_COMMON, channel.0),
    )
    .await
}

pub async fn archive_channel(token: &BotToken, channel: &ChannelId) -> Result<ChannelGet, Error> {
    patch_method_json(
        token,
        &format!("{}/channels/{}", DISCORD_ENDPOINT_COMMON, channel.0),
        &json!({"archived": true}),
    )
    .await
}
