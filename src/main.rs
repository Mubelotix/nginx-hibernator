
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{collections::{BinaryHeap, HashMap, HashSet, VecDeque}, fmt, fs::{read_link, File}, net::{TcpStream, UdpSocket}, os::unix::fs::symlink, process::Command, sync::{Arc, LazyLock, Mutex}, thread::sleep, time::Duration};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{de::{self, Visitor}, Deserialize, Deserializer};
use rev_lines::RevLines;
use log::*;

mod config;
use config::*;
mod server;
use server::*;

fn can_be_stopped(site_index: usize) -> bool {
    static LAST_STOPPED: LazyLock<Arc<Mutex<HashMap<usize, u64>>>> = LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

    let now = Utc::now().timestamp() as u64;
    let mut last_stopped_table = LAST_STOPPED.lock().unwrap();
    let last_stopped = last_stopped_table.get(&site_index);
    if let Some(last_stopped) = last_stopped {
        if now.saturating_sub(*last_stopped) < 60 {
            return false;
        }
    }

    last_stopped_table.insert(site_index, now);

    true
}

fn can_be_started(site_index: usize) -> bool {
    static LAST_STARTED: LazyLock<Arc<Mutex<HashMap<usize, u64>>>> = LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

    let now = Utc::now().timestamp() as u64;
    let mut last_started_table = LAST_STARTED.lock().unwrap();
    let last_started = last_started_table.get(&site_index);
    if let Some(last_started) = last_started {
        if now.saturating_sub(*last_started) < 60*3 {
            return false;
        }
    }

    last_started_table.insert(site_index, now);

    true
}

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

fn shutdown_server(config: &TopLevelConfig, site_config: &SiteConfig, config_index: usize) -> anyhow::Result<()> {
    if !can_be_stopped(config_index) {
        return Ok(());
    }

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

fn start_server(config: &SiteConfig, config_index: usize) -> anyhow::Result<()> {
    if !can_be_started(config_index) {
        return Ok(());
    }

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
        hosts: vec![String::from("localhost")],
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
    env_logger::init();

    let config_path = std::env::args().nth(1).unwrap_or(String::from("config.toml"));
    let config_data = std::fs::read_to_string(config_path).expect("could not read config file");
    let config: Config = toml::from_str(&config_data).expect("could not parse config file");
    let config = Box::leak(Box::new(config));

    info!("Starting hibernator: managing {} sites", config.sites.len());

    setup_server(config);

    info!("Hibernator started");

    let mut check_queue: BinaryHeap<PendingCheck> = BinaryHeap::new();
    let now = Utc::now().timestamp() as u64;
    for site_index in 0..config.sites.len() {
        check_queue.push(PendingCheck {
            site_index,
            check_at: now,
        });
    }

    while let Some(PendingCheck { site_index, check_at }) = check_queue.pop() {
        let to_wait = check_at.saturating_sub(Utc::now().timestamp() as u64);
        let site_config = &config.sites[site_index];
        debug!("Waiting for {to_wait} seconds before checking site {}", site_config.name);
        sleep(Duration::from_secs(to_wait));

        let up = is_port_open(site_config.port);
        match up {
            true => {
                debug!("Checking if site {} should be shut down", site_config.name);
                let should_shutdown = should_shutdown(site_config).unwrap();
                match should_shutdown {
                    ShouldShutdown::Now => {
                        info!("Shutting down site {}", site_config.name);
                        shutdown_server(&config.top_level, site_config, site_index).unwrap();
                        check_queue.push(PendingCheck {
                            site_index,
                            check_at: now + site_config.keep_alive,
                        });
                    },
                    ShouldShutdown::NotUntil(next_check) => {
                        debug!("Site {} should not be shut down until {}", site_config.name, next_check);
                        check_queue.push(PendingCheck {
                            site_index,
                            check_at: next_check,
                        });
                    },
                }
            },
            false => {
                debug!("Rescheduling check for site {}", site_config.name);

                check_queue.push(PendingCheck {
                    site_index,
                    check_at: now + site_config.keep_alive,
                });
            },
        }
    }
}
