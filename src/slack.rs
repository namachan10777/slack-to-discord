use std::fmt::Display;

use chrono::{DateTime, NaiveDateTime, Utc};
use chrono_tz::Tz;
use serde::{de::Visitor, Deserialize, Deserializer};
use sqlx::{Database, Decode, Encode};
pub type MessagePerDay = Vec<Message>;

pub fn hello() -> String {
    "Hello World!".into()
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct TimeStamp(DateTime<Utc>);

impl TimeStamp {
    pub fn date(&self) -> &DateTime<Utc> {
        &self.0
    }

    pub fn jtc_date(&self) -> DateTime<Tz> {
        self.0.with_timezone(&chrono_tz::Asia::Tokyo)
    }
}

impl Display for TimeStamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0))
    }
}

impl<'r, DB: sqlx::Database> sqlx::Decode<'r, DB> for TimeStamp
where
    &'r str: Decode<'r, DB>,
{
    fn decode(
        value: <DB as sqlx::database::HasValueRef<'r>>::ValueRef,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<'r, DB>>::decode(value)?;
        let timestamp = parse_timestamp(s)?;
        Ok(timestamp)
    }
}

impl<'q, DB: Database> sqlx::Encode<'q, DB> for TimeStamp
where
    String: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        let secs = self.0.timestamp();
        let nsecs = self.0.timestamp_subsec_nanos();
        let s = format!("{}.{}", secs, nsecs);
        <String as Encode<'q, DB>>::encode_by_ref(&s, buf)
    }
}

impl<'a, DB: sqlx::Database> sqlx::Type<DB> for TimeStamp
where
    &'a str: sqlx::Type<DB>,
{
    fn type_info() -> <DB as sqlx::Database>::TypeInfo {
        <&str as sqlx::Type<DB>>::type_info()
    }
}

struct TimeStampVisitor;

fn parse_timestamp(src: &str) -> anyhow::Result<TimeStamp> {
    let mut splited = src.split('.');

    let secs: i64 = splited
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing secs"))?
        .parse()
        .map_err(|e| anyhow::anyhow!("parse secs due to {}", e))?;

    let nsecs: u32 = splited
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing nsecs"))?
        .parse()
        .map_err(|e| anyhow::anyhow!("parse nsecs due to {}", e))?;

    let native = NaiveDateTime::from_timestamp(secs, nsecs);
    let utc = DateTime::from_utc(native, Utc);
    Ok(TimeStamp(utc))
}

impl<'de> Visitor<'de> for TimeStampVisitor {
    type Value = TimeStamp;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("expect <secs>.<msecs>")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        parse_timestamp(v).map_err(|e| E::custom(e))
    }
}

impl<'de> Deserialize<'de> for TimeStamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(TimeStampVisitor)
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum MessageSubType {
    #[serde(rename = "channel_join")]
    Join,
    #[serde(rename = "channel_purpose")]
    Purpose,
    #[serde(rename = "thread_broadcast")]
    ThreadBroadcast,
    #[serde(rename = "tombstone")]
    Tombstone,
    #[serde(rename = "channel_topic")]
    Topic,
    #[serde(rename = "reminder_add")]
    ReminderAdd,
    #[serde(rename = "channel_name")]
    ChannelName,
    #[serde(rename = "channel_archive")]
    Archive,
    #[serde(rename = "channel_unarchive")]
    Unarchive,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "message")]
    Message {
        text: String,
        files: Option<Vec<File>>,
        user: String,
        subtype: Option<MessageSubType>,
        ts: TimeStamp,
        reply_count: Option<u64>,
        thread_ts: Option<TimeStamp>,
    },
}

#[derive(Deserialize, Debug)]
#[serde(tag = "mode")]
pub enum File {
    #[serde(rename = "hosted")]
    Hosted {
        name: String,
        title: String,
        url_private_download: String,
    },
    #[serde(rename = "tombstone")]
    Tombstone,
    #[serde(rename = "external")]
    External { name: String, title: String },
    #[serde(rename = "snippet")]
    Snippet,
}

#[derive(Deserialize)]
pub struct Channel {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub real_name: Option<String>,
    pub name: String,
}

impl User {
    pub fn readable_name(&self) -> &str {
        if let Some(real_name) = &self.real_name {
            real_name
        } else {
            &self.name
        }
    }
}
