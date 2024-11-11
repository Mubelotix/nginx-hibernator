
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{collections::{BinaryHeap, HashSet, VecDeque}, fmt, fs::{read_link, File}, net::{TcpStream, UdpSocket}, os::unix::fs::symlink, process::Command, sync::{Arc, Mutex}, thread::sleep, time::Duration};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{de::{self, Visitor}, Deserialize, Deserializer};
use rev_lines::RevLines;

mod config;
use config::*;

#[derive(Debug, Clone, Copy)]
enum ShouldShutdown {
    Now,
    NotUntil(u64),
}

fn should_shutdown(config: &SiteConfig) -> anyhow::Result<ShouldShutdown> {
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
        Ok(ShouldShutdown::Now)
    } else {
        let next_check = last_request.timestamp() as u64 + config.keep_alive;
        Ok(ShouldShutdown::NotUntil(next_check))
    }
}

fn is_port_open(port: u16) -> bool {
    TcpStream::connect(format!("127.0.0.1:{port}")).is_ok()
}

fn checking_symlink(original: &str, link: &str) -> anyhow::Result<bool> {
    let previous_link = read_link(link)?;
    let expected_link = &original;

    if previous_link.to_str() == Some(expected_link) {
        return Ok(false);
    }

    // Replace nginx config with hibernator config
    symlink(original, link)?;
    Ok(true)
}

fn shutdown_server(config: &TopLevelConfig, site_config: &SiteConfig) -> anyhow::Result<()> {
    if checking_symlink(&config.nginx_hibernator_config(), &site_config.nginx_enabled_config())? {
        // Reload nginx
        let status = Command::new("sh")
            .arg("-c")
            .arg("nginx -s reload")
            .status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("nginx reload failed"));
        }
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
    if checking_symlink(&config.nginx_available_config(), &config.nginx_enabled_config())? {
        // Reload nginx
        let status = Command::new("sh")
            .arg("-c")
            .arg("nginx -s reload")
            .status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("nginx reload failed"));
        }
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
        port: 8000,
        access_log: "test.log".to_string(),
        access_log_filter: None,
        service_name: "webserver".to_string(),
        keep_alive: 5 * 60,
    };

    let result = should_shutdown(&site_config).unwrap();

    println!("{:?}", result);
}

#[derive(PartialEq, Eq)]
struct PendingCheck {
    site_index: usize,
    up: bool,
    check_at: u64,
}

impl Ord for PendingCheck {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.check_at.cmp(&other.check_at).reverse()
    }
}

impl PartialOrd for PendingCheck {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn main() {
    let config_path = std::env::args().nth(1).unwrap_or(String::from("config.toml"));
    let config_data = std::fs::read_to_string(config_path).expect("could not read config file");
    let config: Config = toml::from_str(&config_data).expect("could not parse config file");

    let mut check_queue: BinaryHeap<PendingCheck> = BinaryHeap::new();
    let now = Utc::now().timestamp() as u64;
    for site_index in 0..config.sites.len() {
        check_queue.push(PendingCheck {
            site_index,
            up: true, // TODO: check if the site is up
            check_at: now,
        });
    }

    while let Some(PendingCheck { site_index, up, check_at }) = check_queue.pop() {
        let to_wait = check_at.saturating_sub(Utc::now().timestamp() as u64);
        sleep(Duration::from_secs(to_wait));

        let site_config = &config.sites[site_index];
        match up {
            true => {
                let should_shutdown = should_shutdown(site_config).unwrap();
                match should_shutdown {
                    ShouldShutdown::Now => {
                        shutdown_server(&config.top_level, site_config).unwrap();
                        check_queue.push(PendingCheck {
                            site_index,
                            up: false,
                            check_at: now + site_config.keep_alive,
                        });
                    },
                    ShouldShutdown::NotUntil(next_check) => {
                        check_queue.push(PendingCheck {
                            site_index,
                            up: true,
                            check_at: next_check,
                        });
                    },
                }
            },
            false => todo!(),
        }
    }
}
