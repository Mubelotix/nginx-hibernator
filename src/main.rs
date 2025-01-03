
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{cmp::max, path::Path, time::Duration};
use chrono::{DateTime, Utc};
use anyhow::anyhow;
use log::*;
use tokio::{fs::read_to_string, spawn, time::sleep};

mod config;
use config::*;
mod server;
use server::*;
mod cooldown;
use cooldown::*;
mod util;
use util::*;

#[derive(Debug, Clone, Copy)]
enum ShouldShutdown {
    Now,
    NotUntil(u64),
}

async fn should_shutdown(config: &'static SiteConfig) -> anyhow::Result<ShouldShutdown> {
    debug!("Checking if site {} should be shut down", config.name);

    // Find the last line of the file
    let content = read_to_string(&config.access_log).await.map_err(|e| anyhow!("could not read access log: {e}"))?;
    let lines = content.lines();
    let mut rev_lines = lines.rev(); // FIXME: It would be more efficient to use rev_lines but it's not async-compatible
    let mut last_line = 'line: loop {
        let potential_last_line = rev_lines.next().ok_or(anyhow!("no more lines in access log"))?;
        if let Some(filter) = &config.access_log_filter {
            if !potential_last_line.contains(filter) {
                continue 'line;
            }
        }

        if let Some(ip_blacklist) = &config.ip_blacklist {
            for ip_blacklist in ip_blacklist {
                if potential_last_line.starts_with(ip_blacklist) {
                    continue 'line;
                }
            }
        }

        if let Some(ip_whitelist) = &config.ip_whitelist {
            let mut found = false;
            for ip_whitelist in ip_whitelist {
                if potential_last_line.starts_with(ip_whitelist) {
                    found = true;
                    break;
                }
            }
            if !found {
                continue 'line;
            }
        }

        if let Some(path_blacklist) = &config.path_blacklist {
            let path = potential_last_line.find('"').ok_or(anyhow!("no path container opening quote in last line"))?;
            let mut potential_path_container = &potential_last_line[path + 1..];
            let end_path = potential_path_container.find('"').ok_or(anyhow!("no path container closing quote in last line"))?;
            potential_path_container = &potential_path_container[..end_path];
            
            let potential_path = potential_path_container.split(' ').nth(1).ok_or(anyhow!("no path in last line"))?;

            for path_blacklist in path_blacklist {
                if path_blacklist.is_match(potential_path) {
                    continue 'line;
                }
            }
        }

        break potential_last_line;
    };
    
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
    if let Some(last_started) = get_last_started(&config.name).await {
        trace!("Last started was at {}", last_started);
        last_action = max(last_action, last_started);
    }
    if let Some(last_stopped) = get_last_stopped(&config.name).await {
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

async fn shutdown_server(site_config: &'static SiteConfig) -> anyhow::Result<()> {
    mark_stopped(&site_config.name).await;

    info!("Shutting down site {}", site_config.name);

    if checking_symlink(&site_config.nginx_hibernator_config(), &site_config.nginx_enabled_config()).await? {
        run_command("nginx -s reload").await?;
    }

    run_command(&format!("systemctl stop {}", site_config.service_name)).await?;

    Ok(())
}

async fn start_server(site_config: &'static SiteConfig) -> anyhow::Result<()> {
    let is_up = is_healthy(site_config.port).await;
    match is_up {
        true => {
            // TODO: cooldown

            info!("Reloading nginx for {}", site_config.name);
            if checking_symlink(&site_config.nginx_available_config(), &site_config.nginx_enabled_config()).await? {
                run_command("nginx -s reload").await?;
            }
        }
        false => {
            if !try_mark_started(site_config).await {
                trace!("Site {} cannot be started yet (under cooldown)", site_config.name);
                return Ok(());
            }

            info!("Starting service {}", site_config.name);
            run_command(&format!("systemctl start {}", site_config.service_name)).await?;
        }
    }

    Ok(())
}

async fn check(site_config: &'static SiteConfig) -> u64 {
    let now = Utc::now().timestamp() as u64;

    let up = is_healthy(site_config.port).await;
    match up {
        true => {
            let should_shutdown = match should_shutdown(site_config).await {
                Ok(should_shutdown) => should_shutdown,
                Err(err) => {
                    error!("Error while checking site {}: {err}", site_config.name);
                    return now + site_config.keep_alive;
                },
            };
            match should_shutdown {
                ShouldShutdown::Now => {
                    let r = shutdown_server(site_config).await;
                    if let Err(e) = r {
                        error!("Error while shutting down site {}: {e}", site_config.name);
                    }
                    now + site_config.keep_alive
                },
                ShouldShutdown::NotUntil(next_check) => next_check,
            }
        },
        false => now + site_config.keep_alive
    }
}

async fn handle_site(_top_level_config: &'static TopLevelConfig, site_config: &'static SiteConfig) {
    let mut next_check: u64 = 0;

    loop {
        let now = Utc::now().timestamp() as u64;
        let to_wait = next_check.saturating_sub(now);
        debug!("Waiting for {to_wait} seconds before checking site {}", site_config.name);
        sleep(Duration::from_secs(to_wait)).await;
        
        next_check = check(site_config).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() { 
    env_logger::init();

    let config_path = std::env::args().nth(1).unwrap_or(String::from("config.toml"));
    let config_data = std::fs::read_to_string(config_path).expect("could not read config file");
    let config: Config = toml::from_str(&config_data).expect("could not parse config file");
    let config = Box::leak(Box::new(config));

    info!("Starting hibernator: managing {} sites", config.sites.len());

    // Make sure every access log exists
    for site_config in &config.sites {
        if !Path::new(&site_config.access_log).exists() {
            panic!("Site {} access log doesn't exist at {}", site_config.name, site_config.access_log);
        }
    }

    // Make sure every hibernator config exists
    for site_config in &config.sites {
        if !Path::new(&site_config.nginx_hibernator_config()).exists() {
            panic!("Site {} hibernator config doesn't exist at {}", site_config.name, site_config.nginx_hibernator_config());
        }
    }

    // Make sure every site has at least one host
    for site_config in &config.sites {
        if site_config.hosts.is_empty() {
            panic!("Site {} must have at least one host", site_config.name);
        }
    }

    // Make sure a site doesn't have blacklist_ips and whitelist_ips at the same time
    for site_config in &config.sites {
        if site_config.ip_blacklist.is_some() && site_config.ip_whitelist.is_some() {
            panic!("Site {} cannot have both blacklist_ips and whitelist_ips", site_config.name);
        }
    }

    // Make sure the whitelists are not empty if they exist
    for site_config in &config.sites {
        if let Some(whitelist_ips) = &site_config.ip_whitelist {
            if whitelist_ips.is_empty() {
                panic!("Site {} whitelist_ips cannot be empty", site_config.name);
            }
        }
    }

    setup_server(config).await;

    info!("Hibernator started");

    // Start all site tasks
    let mut handles = Vec::new();
    for site_config in &config.sites {
        let handle = spawn(handle_site(&config.top_level, site_config));
        handles.push(handle);
    }

    // Join all handles
    for handle in handles {
        let _  = handle.await;
    }
}
