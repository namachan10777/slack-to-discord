use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use sqlx::{Decode, Encode};

const DISCORD_ENDPOINT_COMMON: &str = "https://discord.com/api/v10";

pub struct BotToken(String);
pub struct GuildId(String);

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct ChannelId(String);

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
}

#[derive(Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelType {
    GuildText = 0,
    GuildVoice = 2,
    GuildCategory = 4,
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
    #[error("request {0}")]
    Request(reqwest::Error),
    #[error("schema {0}")]
    Schema(reqwest::Error),
}

async fn get_method<T: DeserializeOwned>(token: &BotToken, url: &str) -> Result<T, Error> {
    let response = Client::new()
        .get(url)
        .header("Authorization", format!("Bot {}", token.as_str()))
        .send()
        .await
        .map_err(Error::Request)?
        .json::<T>()
        .await
        .map_err(Error::Schema)?;
    Ok(response)
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
        .json::<R>()
        .await
        .map_err(Error::Schema)?;
    Ok(response)
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
) -> Result<MessageGet, Error> {
    post_method_json(
        token,
        &format!(
            "{}/chanels/{}/messages/",
            DISCORD_ENDPOINT_COMMON, channel.0
        ),
        message,
    )
    .await
}
