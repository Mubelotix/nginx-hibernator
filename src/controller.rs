use std::{cmp::max, path::Path, sync::{atomic::{AtomicUsize, Ordering}, Arc, RwLock}, time::Duration};
use chrono::{DateTime, Utc};
use anyhow::anyhow;
use log::*;
use tokio::{fs::read_to_string, spawn, sync::{mpsc::{Receiver, Sender}, broadcast::{Receiver as BroadReceiver, Sender as BroadSender}}, time::sleep};
use crate::{checking_symlink, get_last_started, get_last_stopped, is_healthy, mark_stopped, run_command, try_mark_started, SiteConfig, TopLevelConfig};

pub struct SiteController {
    pub config: &'static SiteConfig,
    state: &'static AtomicUsize,
    starting_since: &'static AtomicUsize,
    start_sender: Sender<()>,
    started_receiver: BroadReceiver<()>
}

impl SiteController {
    pub fn new(config: &'static SiteConfig) -> (Self, Receiver<()>, BroadSender<()>) {
        let (start_sender, start_receiver) = tokio::sync::mpsc::channel(1);
        let (started_sender, started_receiver) = tokio::sync::broadcast::channel(1);
        let state = Box::leak(Box::new(AtomicUsize::new(0)));
        let starting_since = Box::leak(Box::new(AtomicUsize::new(0)));

        (Self {
            config,
            state,
            starting_since,
            start_sender,
            started_receiver
        }, start_receiver, started_sender)
    }

    pub async fn start(&self) {
        let _ = self.start_sender.try_send(()); // We don't care about the error because if this fails, that means the site was already requested to be started
    }

    pub async fn wait_start(&self) {
        self.start().await;
        let mut started_receiver = self.started_receiver.resubscribe();
        let _ = started_receiver.recv().await;
    }

    pub async fn handle(&self, mut start_receiver: Receiver<()>, started_sender: BroadSender<()>) {
        let mut next_check: u64 = 0;
    
        loop {
            let now = Utc::now().timestamp() as u64;
            let to_wait = next_check.saturating_sub(now);
            debug!("Waiting for {to_wait} seconds before checking site {}", self.config.name);
            
            let sleep_task = sleep(Duration::from_secs(to_wait));
            let recv_task = start_receiver.recv();
    
            tokio::select! {
                _ = sleep_task => {
                    let (next_check2, new_state) = check(self.config).await;
                    next_check = next_check2;
                    match new_state {
                        SiteState::Down => self.state.store(0, Ordering::Relaxed),
                        SiteState::Up => self.state.store(1, Ordering::Relaxed),
                        SiteState::Starting { .. } => unreachable!() 
                    };
                },
                _ = recv_task => {
                    // TODO: cooldowns
    
                    // Start the server
                    let r = start_server(self.config).await;
                    if let Err(e) = r {
                        error!("Error while starting site {}: {e}", self.config.name);
                        continue;
                    }
                    self.state.store(1, Ordering::Relaxed);
                    self.starting_since.store(Utc::now().timestamp() as usize, Ordering::Relaxed);
    
                    // Wait until the site is healthy
                    loop {
                        let is_up = is_healthy(self.config.port).await;
                        if is_up {
                            break;
                        }
                        sleep(Duration::from_millis(100)).await;
                    }
                    self.state.store(1, Ordering::Relaxed);
                    let _ = started_sender.send(());
    
                    // Reload nginx
                    info!("Reloading nginx for {}", self.config.name);
                    let should_reload = checking_symlink(&self.config.nginx_available_config(), &self.config.nginx_enabled_config()).await;
                    let should_reload = match should_reload {
                        Ok(should_reload) => should_reload,
                        Err(e) => {
                            error!("Error while checking nginx symlink for {}: {e}", self.config.name);
                            continue;
                        }
                    };
                    if should_reload {
                        let r = run_command("nginx -s reload").await;
                        if let Err(e) = r {
                            error!("Error while reloading nginx for {}: {e}", self.config.name);
                        }
                    }
                }
            }
        }        
    }
}

pub static mut SITE_CONTROLLERS: &[SiteController] = &[];

pub fn get_controller(host: &String) -> Option<&'static SiteController> {
    // SAFETY:
    // Accessing the static mutable is safe because it's only accessed in a read-only way during
    // the server execution. The value is only mutated once, before the server starts.
    unsafe {
        SITE_CONTROLLERS.iter().find(|controller| controller.config.hosts.contains(host))
    }
}

pub enum SiteState {
    Down,
    Up,
    Starting { since: usize }
}

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
    if !try_mark_started(site_config).await {
        trace!("Site {} cannot be started yet (under cooldown)", site_config.name);
        return Ok(());
    }

    info!("Starting service {}", site_config.name);
    run_command(&format!("systemctl start {}", site_config.service_name)).await?;

    Ok(())
}

async fn check(site_config: &'static SiteConfig) -> (u64, SiteState) {
    let now = Utc::now().timestamp() as u64;

    let up = is_healthy(site_config.port).await;
    match up {
        true => {
            let should_shutdown = match should_shutdown(site_config).await {
                Ok(should_shutdown) => should_shutdown,
                Err(err) => {
                    error!("Error while checking site {}: {err}", site_config.name);
                    return (now + site_config.keep_alive, SiteState::Up);
                },
            };
            match should_shutdown {
                ShouldShutdown::Now => {
                    let r = shutdown_server(site_config).await;
                    if let Err(e) = r {
                        error!("Error while shutting down site {}: {e}", site_config.name);
                    }
                    (now + site_config.keep_alive, SiteState::Down)
                },
                ShouldShutdown::NotUntil(next_check) => (next_check, SiteState::Up)
            }
        },
        false => (now + site_config.keep_alive, SiteState::Down)
    }
}

