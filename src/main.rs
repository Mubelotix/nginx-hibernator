
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

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
struct SiteConfig {
    name: String,

    access_log: String,
    
    #[serde(default)]
    access_log_filter: Option<String>,
    
    service_name: String,

    #[serde(deserialize_with = "deserialize_duration")]
    keep_alive: u64,
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    sites: Vec<SiteConfig>,
}

#[derive(Debug, Clone, Copy)]
enum CheckResult {
    Shutdown,
    NextCheck(u64),
}

fn check_site(config: &SiteConfig) -> anyhow::Result<CheckResult> {
    // Find the last line of the file
    let file = File::open(&config.access_log)?;
    let mut rev_lines = RevLines::new(file);
    let last_line = loop {
        let potential_last_line = rev_lines.next().ok_or(anyhow::anyhow!("no more lines in access log"))??;
        if let Some(filter) = &config.access_log_filter {
            if potential_last_line.contains(filter) {
                break potential_last_line;
            }
        } else {
            break potential_last_line;
        }
    };
    let mut last_line = last_line.as_str();
    
    // Parse the date of the last request
    let last_request = loop {
        let start_position = last_line.find('[').ok_or(anyhow::anyhow!("no date in last line"))?;
        last_line = &last_line[start_position + 1..];

        let end_position = last_line.find(']').ok_or(anyhow::anyhow!("no date in last line"))?;
        let date_str = &last_line[..end_position];
        last_line = &last_line[end_position + 1..];

        let Ok(date) = DateTime::parse_from_str(date_str, "%d/%b/%Y:%H:%M:%S %z") else {continue}; // TODO: the format should be configurable

        break date;
    };
    
    // Check if the site should be shut down
    let time_since = (Utc::now().timestamp() - last_request.timestamp()) as u64;
    if time_since > config.keep_alive {
        Ok(CheckResult::Shutdown)
    } else {
        let next_check = last_request.timestamp() as u64 + config.keep_alive;
        Ok(CheckResult::NextCheck(next_check))
    }
}

#[test]
fn test() {
    let site_config = SiteConfig {
        name: "example".to_string(),
        access_log: "test.log".to_string(),
        access_log_filter: None,
        service_name: "webserver".to_string(),
        keep_alive: 5 * 60,
    };

    let result = check_site(&site_config).unwrap();

    println!("{:?}", result);
}

fn main() {
    println!("Hello, world!");
}
