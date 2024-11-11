use std::fmt;
use serde::{de::{self, Visitor}, Deserialize, Deserializer};

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
    pub name: String,

    #[serde(default)]
    pub nginx_available_config: Option<String>,
    
    #[serde(default)]
    pub nginx_enabled_config: Option<String>,
    
    pub port: u16,

    pub access_log: String,
    
    #[serde(default)]
    pub access_log_filter: Option<String>,
    
    pub service_name: String,

    pub hosts: Vec<String>,

    #[serde(deserialize_with = "deserialize_duration")]
    pub keep_alive: u64,
}

impl SiteConfig {
    pub fn nginx_available_config(&self) -> String {
        match &self.nginx_available_config {
            Some(config) => config.clone(),
            None => format!("/etc/nginx/sites-available/{}", self.name),
        }
    }

    pub fn nginx_enabled_config(&self) -> String {
        match &self.nginx_enabled_config {
            Some(config) => config.clone(),
            None => format!("/etc/nginx/sites-enabled/{}", self.name),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TopLevelConfig {
    #[serde(default)]
    pub nginx_hibernator_config: Option<String>,

    #[serde(default)]
    pub hibernator_port: Option<u16>,
}

impl TopLevelConfig {
    pub fn nginx_hibernator_config(&self) -> String {
        match &self.nginx_hibernator_config {
            Some(config) => config.clone(),
            None => String::from("/etc/nginx/sites-available/hibernator"),
        }
    }

    pub fn hibernator_port(&self) -> u16 {
        match &self.hibernator_port {
            Some(port) => *port,
            None => 7878,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub top_level: TopLevelConfig,

    #[serde(default)]
    pub sites: Vec<SiteConfig>,
}
