
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{cmp::max, collections::BinaryHeap, fs::{read_link, remove_file, File}, net::TcpStream, os::unix::fs::symlink, process::Command, thread::sleep, time::Duration};
use chrono::{DateTime, Utc};
use rev_lines::RevLines;
use anyhow::anyhow;
use log::*;

mod config;
use config::*;
mod server;
use server::*;
mod cooldown;
use cooldown::*;

#[derive(Debug, Clone, Copy)]
enum ShouldShutdown {
    Now,
    NotUntil(u64),
}

fn should_shutdown(config: &'static SiteConfig) -> anyhow::Result<ShouldShutdown> {
    debug!("Checking if site {} should be shut down", config.name);

    // Find the last line of the file
    let file = File::open(&config.access_log).map_err(|e| anyhow!("could not open access log: {e}"))?;
    let mut rev_lines = RevLines::new(file);
    let last_line = loop {
        let potential_last_line = rev_lines.next().ok_or(anyhow!("no more lines in access log"))??;
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
        let start_position = last_line.find('[').ok_or(anyhow!("no date in last line"))?;
        last_line = &last_line[start_position + 1..];

        let end_position = last_line.find(']').ok_or(anyhow!("no date in last line"))?;
        let date_str = &last_line[..end_position];
        last_line = &last_line[end_position + 1..];

        let Ok(date) = DateTime::parse_from_str(date_str, "%d/%b/%Y:%H:%M:%S %z") else {continue}; // TODO: the format should be configurable

        break date;
    };

    // Calculate the last action timestamp
    let mut last_action = last_request.timestamp() as u64;
    trace!("Last request was at {}", last_action);
    if let Some(last_started) = get_last_started(&config.name) {
        trace!("Last started was at {}", last_started);
        last_action = max(last_action, last_started);
    }
    if let Some(last_stopped) = get_last_stopped(&config.name) {
        trace!("Last stopped was at {}", last_stopped);
        last_action = max(last_action, last_stopped);
    }
    
    // Check if the site should be shut down
    let time_since = (Utc::now().timestamp() as u64).saturating_sub(last_action);
    if time_since > config.keep_alive {
        debug!("Site {} should be shut down now", config.name);
        Ok(ShouldShutdown::Now)
    } else {
        let next_check = last_action + config.keep_alive + 1;
        debug!("Site {} should not be shut down until {next_check}", config.name);
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
    remove_file(link).map_err(|e| anyhow!("could not remove previous symlink: {e}"))?;
    symlink(original, link).map_err(|e| anyhow!("could not create symlink: {e}"))?;
    Ok(true)
}

fn run_command(command: &str) -> anyhow::Result<()> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| anyhow!("could not run command: {e}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("command failed: {command} {stdout} {stderr}"));
    }

    Ok(())
}

fn shutdown_server(config: &TopLevelConfig, site_config: &'static SiteConfig) -> anyhow::Result<()> {
    mark_stopped(&site_config.name);

    info!("Shutting down site {}", site_config.name);

    if checking_symlink(&config.nginx_hibernator_config(), &site_config.nginx_enabled_config())? {
        run_command("nginx -s reload")?;
    }

    run_command(&format!("systemctl stop {}", site_config.service_name))?;

    Ok(())
}

fn start_server(site_config: &'static SiteConfig) -> anyhow::Result<()> {
    if !try_mark_started(site_config) {
        trace!("Site {} cannot be started yet (under cooldown)", site_config.name);
        return Ok(());
    }

    info!("Starting site {}", site_config.name);

    if checking_symlink(&site_config.nginx_available_config(), &site_config.nginx_enabled_config())? {
        run_command("nginx -s reload")?;
    }

    run_command(&format!("systemctl start {}", site_config.service_name))?;

    Ok(())
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
    let mut now = Utc::now().timestamp() as u64;
    for site_index in 0..config.sites.len() {
        check_queue.push(PendingCheck {
            site_index,
            check_at: now,
        });
    }

    while let Some(PendingCheck { site_index, check_at }) = check_queue.pop() {
        now = Utc::now().timestamp() as u64;
        let to_wait = check_at.saturating_sub(now);
        let site_config = &config.sites[site_index];
        debug!("Waiting for {to_wait} seconds before checking site {}", site_config.name);
        sleep(Duration::from_secs(to_wait));
        now = Utc::now().timestamp() as u64;

        let up = is_port_open(site_config.port);
        match up {
            true => {
                let should_shutdown = match should_shutdown(site_config) {
                    Ok(should_shutdown) => should_shutdown,
                    Err(err) => {
                        error!("Error while checking site {}: {err}", site_config.name);
                        continue;
                    },
                };
                match should_shutdown {
                    ShouldShutdown::Now => {
                        shutdown_server(&config.top_level, site_config).unwrap();
                        check_queue.push(PendingCheck {
                            site_index,
                            check_at: now + site_config.keep_alive,
                        });
                    },
                    ShouldShutdown::NotUntil(next_check) => {
                        check_queue.push(PendingCheck {
                            site_index,
                            check_at: next_check,
                        });
                    },
                }
            },
            false => {
                check_queue.push(PendingCheck {
                    site_index,
                    check_at: now + site_config.keep_alive,
                });
            },
        }
    }
}
