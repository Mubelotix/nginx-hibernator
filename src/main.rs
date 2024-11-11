
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{cmp::max, fs::File, path::Path, thread::sleep, time::Duration};
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
mod pending_check;
use pending_check::*;
mod util;
use util::*;

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

fn main() { 
    env_logger::init();

    let config_path = std::env::args().nth(1).unwrap_or(String::from("config.toml"));
    let config_data = std::fs::read_to_string(config_path).expect("could not read config file");
    let config: Config = toml::from_str(&config_data).expect("could not parse config file");
    let config = Box::leak(Box::new(config));

    info!("Starting hibernator: managing {} sites", config.sites.len());

    // Make sure the hibernator config file exists
    let hibernator_config = config.top_level.nginx_hibernator_config();
    if !Path::new(&hibernator_config).exists() {
        error!("Hibernator config file doesn't exist at {}", hibernator_config);
        return;
    }

    // Make sure every access log exists
    for site_config in &config.sites {
        if !Path::new(&site_config.access_log).exists() {
            error!("Site {} access log doesn't exist at {}", site_config.name, site_config.access_log);
            return;
        }
    }

    // Make sure every site has at least one host
    for site_config in &config.sites {
        if site_config.hosts.is_empty() {
            error!("Site {} must have at least one host", site_config.name);
            return;
        }
    }

    setup_server(config);

    info!("Hibernator started");

    let mut check_queue = CheckQueue::new();
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
                        let r = shutdown_server(&config.top_level, site_config);
                        if let Err(e) = r {
                            error!("Error while shutting down site {}: {e}", site_config.name);
                        }
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
