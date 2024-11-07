
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{fmt, fs::File, os::unix::fs::symlink, process::Command};

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

fn shutdown_server(config: &TopLevelConfig, site_config: &SiteConfig) -> anyhow::Result<()> {
    // Replace nginx config with hibernator config
    symlink(config.nginx_hibernator_config(), site_config.nginx_enabled_config())?;

    // Reload nginx
    let status = Command::new("sh")
        .arg("-c")
        .arg("nginx -s reload")
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("nginx reload failed"));
    }

    // Shutdown the service
    let status = Command::new("systemctl")
        .arg("stop")
        .arg(&site_config.service_name)
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("service stop failed"));
    }

    Ok(())
}

fn start_server(config: &SiteConfig) -> anyhow::Result<()> {
    // Replace hibernator config with nginx config
    symlink(config.nginx_available_config(), config.nginx_enabled_config())?;

    // Reload nginx
    let status = Command::new("sh")
        .arg("-c")
        .arg("nginx -s reload")
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("nginx reload failed"));
    }

    // Start the service
    let status = Command::new("systemctl")
        .arg("start")
        .arg(&config.service_name)
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("service start failed"));
    }

    Ok(())
}

#[test]
fn test() {
    let site_config = SiteConfig {
        name: "example".to_string(),
        nginx_available_config: None,
        nginx_enabled_config: None,
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
