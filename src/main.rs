
// name = "example" # The name of the nginx site
// access_log = "/var/log/nginx/example.access.log" # The path to the access log
// access_log_filter = "example.com" # Optional filter to match lines in the access log
// service_name = "webserver" # The name of the service that runs the site
// keep_alive = "5m" # Time to keep the site running after the last access

use std::{fs::metadata, os::unix::fs::MetadataExt, path::Path};
use log::*;
use tokio::spawn;

mod config;
use config::*;
mod server;
use server::*;
mod cooldown;
use cooldown::*;
mod util;
use util::*;
mod controller;
use controller::*;

#[tokio::main(flavor = "current_thread")]
async fn main() { 
    env_logger::init();

    let config_path = std::env::args().nth(1).unwrap_or(String::from("config.toml"));

    #[cfg(target_family = "unix")]
    {
        let metadata = metadata(&config_path).expect("could not read config file metadata");
        let uid = metadata.uid();
        let mode = metadata.mode();
        let current_uid = unsafe { libc::getuid() };

        if uid != current_uid {
            panic!("Config file should be owned by current user");
        }
    
        if mode & 0o002 != 0 {
            panic!("Config file should not be writable by other users");
        }
    }

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
    let mut controllers = Vec::new();
    let mut channels = Vec::new();
    for site_config in &config.sites {
        let (controller, start_receiver, started_sender) = SiteController::new(site_config);

        controllers.push(controller);
        channels.push((start_receiver, started_sender));
    }

    let controllers: &_ = controllers.leak();
    unsafe {
        SITE_CONTROLLERS = controllers;
    }

    let mut handles = Vec::new();
    for ((start_receiver, started_sender), controller) in channels.into_iter().zip(controllers) {
        let handle = controller.handle(start_receiver, started_sender);
        handles.push(spawn(handle));
    }

    // Join all handles
    for handle in handles {
        let _  = handle.await;
    }
}
