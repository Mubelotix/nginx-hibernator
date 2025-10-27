use std::{cmp::max, sync::atomic::{AtomicU64, AtomicUsize, Ordering}, time::Duration};
use chrono::{DateTime, Utc};
use anyhow::anyhow;
use log::*;
use tokio::{fs::read_to_string, sync::{broadcast::{Receiver as BroadReceiver, Sender as BroadSender}, mpsc::{Receiver, Sender}}, time::{sleep, Instant}};
use crate::{checking_symlink, database::DATABASE, get_last_started, get_last_stopped, is_healthy, mark_stopped, run_command, try_mark_started, SiteConfig};

pub struct SiteController {
    pub config: &'static SiteConfig,
    state: &'static AtomicUsize,
    state_last_changed: &'static AtomicU64,
    start_sender: Sender<()>,
    started_receiver: BroadReceiver<()>
}

impl SiteController {
    pub async fn new(config: &'static SiteConfig) -> (Self, Receiver<()>, BroadSender<()>) {
        let (start_sender, start_receiver) = tokio::sync::mpsc::channel(1);
        let (started_sender, started_receiver) = tokio::sync::broadcast::channel(1);

        (Self {
            config,
            state: Box::leak(Box::new(AtomicUsize::new(0))),
            state_last_changed: Box::leak(Box::new(AtomicU64::new(0))),
            start_sender,
            started_receiver
        }, start_receiver, started_sender)
    }

    pub fn trigger_start(&self) {
        let _ = self.start_sender.try_send(()); // We don't care about the error because if this fails, that means the site was already requested to be started
    }

    pub async fn waiting_trigger_start(&self) {
        self.trigger_start();
        let mut started_receiver = self.started_receiver.resubscribe();
        let _ = started_receiver.recv().await;
    }

    async fn on_down(&self) {
        let r = checking_symlink(&self.config.nginx_hibernator_config(), &self.config.nginx_enabled_config()).await;
        let r = match r {
            Ok(true) => run_command("nginx -s reload").await,
            Ok(false) => Ok(()),
            Err(e) => {
                error!("Error while checking nginx symlink for {}: {e}", self.config.name);
                Ok(())
            }
        };

        if let Err(e) = r {
            error!("Error while reloading nginx for {}: {e}", self.config.name);
        }
    }

    async fn on_up(&self) {
        info!("Reloading nginx for {}", self.config.name);
        let should_reload = checking_symlink(&self.config.nginx_available_config(), &self.config.nginx_enabled_config()).await;
        let should_reload = match should_reload {
            Ok(should_reload) => should_reload,
            Err(e) => {
                error!("Error while checking nginx symlink for {}: {e}", self.config.name);
                return;
            }
        };
        if should_reload {
            let r = run_command("nginx -s reload").await;
            if let Err(e) = r {
                error!("Error while reloading nginx for {}: {e}", self.config.name);
            }
        }
    }

    async fn set_state(&self, state: SiteState) {
        let old_state = self.get_state();
        if old_state == state {
            return;
        }
        self.state.store(state as usize, Ordering::Relaxed);
        self.state_last_changed.store(Utc::now().timestamp_millis() as u64, Ordering::Relaxed);

        match state {
            SiteState::Down => self.on_down().await,
            SiteState::Up => self.on_up().await,
            _ => ()
        }
    }

    pub fn get_state(&self) -> SiteState {
        let state = self.state.load(Ordering::Relaxed);
        match state {
            0 => SiteState::Unknown,
            1 => SiteState::Down,
            2 => SiteState::Up,
            3 => SiteState::Starting,
            _ => unreachable!()
        }
    }

    pub fn get_state_with_last_changed_ms(&self) -> (SiteState, u64) {
        let state = self.get_state();
        let last_changed = self.state_last_changed.load(Ordering::Relaxed);
        (state, last_changed)
    }

    pub fn get_state_with_last_changed(&self) -> (SiteState, u64) {
        let (state, last_changed_ms) = self.get_state_with_last_changed_ms();
        (state, last_changed_ms / 1000)
    }

    #[allow(clippy::question_mark)]
    pub async fn get_progress(&self) -> Option<(Duration, Duration)> {
        if self.config.eta_sample_size.0 == 0 {
            trace!("ETA disabled");
            return None;
        }

        let now_ms = Utc::now().timestamp_millis() as u64;
        let (state, mut last_changed_ms) = self.get_state_with_last_changed_ms();
        if state != SiteState::Starting {
            trace!("Site was not starting");
            last_changed_ms = now_ms;
        }

        let done = now_ms - last_changed_ms;
        let done = Duration::from_millis(done);

        let duration_estimate = match DATABASE.get_start_duration_estimate(&self.config.name, self.config.eta_percentile.0) {
            Ok(duration_estimate) => duration_estimate,
            Err(e) => {
                warn!("Couldn't get duration estimate: {e}");
                return None;
            }
        };

        Some((done, duration_estimate))
    }

    async fn should_shutdown(&self) -> anyhow::Result<ShouldShutdown> {
        debug!("Checking if site {} should be shut down", self.config.name);
        let now = Utc::now().timestamp() as u64;

        // Read the file and get the last line
        let content = read_to_string(&self.config.access_log).await.map_err(|e| anyhow!("could not read access log: {e}"))?;
        let lines = content.lines();
        let mut rev_lines = lines.rev(); // FIXME: It would be more efficient to use rev_lines but it's not async-compatible
        let mut last_line = 'line: loop {
            let potential_last_line = match rev_lines.next() {
                Some(potential_last_line) => potential_last_line,
                None => {
                    // No more lines in access log.
                    // That means no-one has been accessing the site since it's up.
                    let (state, last_changed) = self.get_state_with_last_changed();
    
                    // That shouldn't happen often given this method only gets called when the site is up
                    if !state.is_up() {
                        return Ok(ShouldShutdown::NotUntil(now + self.config.keep_alive)); // Not sure keep_alive is the right value to use
                    }
                    
                    if now - last_changed >= self.config.keep_alive {
                        return Ok(ShouldShutdown::Now);
                    } else {
                        return Ok(ShouldShutdown::NotUntil(last_changed + self.config.keep_alive));
                    }
                }
            };

            if let Some(filter) = &self.config.access_log_filter {
                if !potential_last_line.contains(filter) {
                    continue 'line;
                }
            }
    
            if let Some(ip_blacklist) = &self.config.ip_blacklist {
                for ip_blacklist in ip_blacklist {
                    if potential_last_line.starts_with(ip_blacklist) {
                        continue 'line;
                    }
                }
            }
    
            if let Some(ip_whitelist) = &self.config.ip_whitelist {
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
    
            if let Some(path_blacklist) = &self.config.path_blacklist {
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
        if let Some(last_started) = get_last_started(&self.config.name).await {
            trace!("Last started was at {}", last_started);
            last_action = max(last_action, last_started);
        }
        if let Some(last_stopped) = get_last_stopped(&self.config.name).await {
            trace!("Last stopped was at {}", last_stopped);
            last_action = max(last_action, last_stopped);
        }
        
        // Check if the site should be shut down
        let time_since = now.saturating_sub(last_action);
        if time_since > self.config.keep_alive {
            debug!("Site {} should be shut down now", self.config.name);
            Ok(ShouldShutdown::Now)
        } else {
            let next_check = last_action + self.config.keep_alive + 1;
            debug!("Site {} should not be shut down until {next_check}", self.config.name);
            Ok(ShouldShutdown::NotUntil(next_check))
        }
    }    

    async fn check(&self) -> u64 {
        let now = Utc::now().timestamp() as u64;

        let up = is_healthy(self.config.port).await;
        match up {
            true => {
                let should_shutdown = match self.should_shutdown().await {
                    Ok(should_shutdown) => should_shutdown,
                    Err(err) => {
                        error!("Error while checking site {}: {err}", self.config.name);
                        self.set_state(SiteState::Up).await;
                        return now + self.config.keep_alive;
                    },
                };
                match should_shutdown {
                    ShouldShutdown::Now => {
                        mark_stopped(&self.config.name).await;

                        info!("Shutting down site {}", self.config.name);

                        self.set_state(SiteState::Down).await;
                        let r = run_command(&format!("systemctl stop {}", self.config.service_name)).await;
                        if let Err(e) = r {
                            error!("Error while shutting down site {}: {e}", self.config.name);
                            self.set_state(SiteState::Unknown).await;
                        }
                        
                        now + self.config.keep_alive
                    },
                    ShouldShutdown::NotUntil(next_check) => {
                        self.set_state(SiteState::Up).await;
                        next_check
                    }
                }
            },
            false => {
                self.set_state(SiteState::Down).await;
                now + self.config.keep_alive
            }
        }
    }

    async fn start(&self, started_sender: &BroadSender<()>) {    
        // Start the server
        if !try_mark_started(self.config).await {
            trace!("Site {} cannot be started yet (under cooldown)", self.config.name);
            return;
        }
        info!("Starting service {}", self.config.name);
        let r = run_command(&format!("systemctl start {}", self.config.service_name)).await;
        if let Err(e) = r {
            error!("Error while starting site {}: {e}", self.config.name);
            return;
        }
        self.set_state(SiteState::Starting).await;

        // Wait until the site is healthy
        let start = Instant::now();
        let state = loop {
            if start.elapsed() > Duration::from_millis(self.config.start_timeout_ms.0) {
                error!("Site {} did not start in time", self.config.name);
                break SiteState::Unknown;
            }

            let is_up = is_healthy(self.config.port).await;
            if is_up {
                break SiteState::Up;
            }
            sleep(Duration::from_millis(self.config.start_check_interval_ms.0)).await;
        };
        let end = Instant::now();
        self.set_state(state).await;
        let _ = started_sender.send(());

        // Log the start duration
        if state == SiteState::Up {
            if let Err(e) = DATABASE.put_start_duration(&self.config.name, end - start, self.config.eta_sample_size.0) {
                error!("Couldn't store start duration: {e}");
            }
        }
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
                _ = sleep_task => next_check = self.check().await,
                _ = recv_task => self.start(&started_sender).await,
            }
        }        
    }
}

pub static mut SITE_CONTROLLERS: &[SiteController] = &[];

pub fn get_controller(host: &String) -> Option<&'static SiteController> {
    // SAFETY:
    // Accessing the static mutable is safe because it's only accessed in a read-only way during
    // the server execution. The value is only mutated once, before the server starts.
    #[allow(static_mut_refs)]
    unsafe {
        SITE_CONTROLLERS.iter().find(|controller| controller.config.hosts.contains(host))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteState {
    Unknown,
    Down,
    Up,
    Starting
}

impl SiteState {
    pub fn is_up(&self) -> bool {
        matches!(self, SiteState::Up)
    }
}

#[derive(Debug, Clone, Copy)]
enum ShouldShutdown {
    Now,
    NotUntil(u64),
}

