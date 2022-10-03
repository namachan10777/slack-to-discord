use chrono::{DateTime, NaiveDateTime, Utc};
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

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(parse_timestamp(&v).map_err(|e| E::custom(e))?)
    }
}

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<TimeStamp, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(TimeStampVisitor)
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "message")]
    Message {
        text: String,
        files: Option<Vec<File>>,
        #[serde(deserialize_with = "deserialize_timestamp")]
        ts: TimeStamp,
        reply_count: Option<u64>,
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
