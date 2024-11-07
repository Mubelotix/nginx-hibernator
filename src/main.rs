
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{fmt, fs::File};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{de::{self, Visitor}, Deserialize, Deserializer};
use rev_lines::RevLines;

mod config;
use config::*;

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
