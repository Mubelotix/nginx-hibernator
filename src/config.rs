
use std::{fmt, fs::File};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{de::{self, Visitor}, Deserialize, Deserializer};
use rev_lines::RevLines;

fn deserialize_duration<'de, D>(deserializer: D) -> Result<u64, D::Error> where D: Deserializer<'de> {
    struct DurationString;

    impl<'de> Visitor<'de> for DurationString {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string")
        }

        fn visit_str<E>(self, mut value: &str) -> Result<u64, E> where E: de::Error {
            let multiplier = match value.bytes().last() {
                Some(b's') => {
                    value = value.split_at(value.len() - 1).0;
                    1
                },
                Some(b'm') => {
                    value = value.split_at(value.len() - 1).0;
                    60
                },
                Some(b'h') => {
                    value = value.split_at(value.len() - 1).0;
                    60 * 60
                }
                Some(b'd') | Some(b'j') => {
                    value = value.split_at(value.len() - 1).0;
                    60 * 60 * 24
                }
                _ => 1,
            };

            let value = value.parse::<u64>().map_err(de::Error::custom)?;

            Ok(value * multiplier)
        }

        fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }
        
        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v)
        }

        fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }
    }

    deserializer.deserialize_any(DurationString)
}

#[derive(Debug, Deserialize)]
pub struct SiteConfig {
    name: String,

    access_log: String,
    
    #[serde(default)]
    access_log_filter: Option<String>,
    
    service_name: String,

    #[serde(deserialize_with = "deserialize_duration")]
    keep_alive: u64,
}

#[derive(Debug, Deserialize)]
pub struct TopLevelConfig {
    
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    top_level: TopLevelConfig,

    #[serde(default)]
    sites: Vec<SiteConfig>,
}
