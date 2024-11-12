use std::fmt;
use serde::{de::{self, Visitor}, Deserialize, Deserializer};

fn deserialize_duration<'de, D>(deserializer: D) -> Result<u64, D::Error> where D: Deserializer<'de> {
    struct DurationString;

    impl<'de> Visitor<'de> for DurationString {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string")
        }

        fn visit_str<E>(self, mut value: &str) -> Result<u64, E> where E: de::Error {
            let multiplier = match value.bytes().last() {
                Some(b's') => {
                    value = value.split_at(value.len() - 1).0;
                    1
                },
                Some(b'm') => {
                    value = value.split_at(value.len() - 1).0;
                    60
                },
                Some(b'h') => {
                    value = value.split_at(value.len() - 1).0;
                    60 * 60
                }
                Some(b'd') | Some(b'j') => {
                    value = value.split_at(value.len() - 1).0;
                    60 * 60 * 24
                }
                _ => 1,
            };

            let value = value.parse::<u64>().map_err(de::Error::custom)?;

            Ok(value * multiplier)
        }

        fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }
        
        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v)
        }

        fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }

        fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E> where E: de::Error, {
            Ok(v as u64)
        }
    }

    deserializer.deserialize_any(DurationString)
}

/// The proxy is a feature to reduce friction when your service's APIs are used by other programs.
/// It makes requests wait the upstream server to boot up instead of displaying a waiting page.
/// If the server starts in time, the request will be processed out of the box, as if the server had been running.
/// 
/// Note: If you are relying on nginx to authenticate users, you might want to disable this feature to avoid users bypassing the authentication.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    /// Proxies all requests.
    Always,

    /// Proxies requests only when the upstream server is ready right away.
    WhenReady,

    /// Disables the proxy feature.
    Never,
}

impl ProxyMode {
    fn when_ready() -> Self {
        ProxyMode::WhenReady
    }

    fn always() -> Self {
        ProxyMode::Always
    }
}

#[derive(Deserialize, Debug)]
pub struct ProxyTimeout(pub u64);
impl Default for ProxyTimeout {
    fn default() -> Self {
        ProxyTimeout(28000)
    }
}

#[derive(Deserialize, Debug)]
pub struct ProxyCheckInterval(pub u64);
impl Default for ProxyCheckInterval {
    fn default() -> Self {
        ProxyCheckInterval(500)
    }
}

#[derive(Debug, Deserialize)]
pub struct SiteConfig {
    /// The name of the site. Must be unique.
    pub name: String,

    /// Path to the nginx available config file.
    /// 
    /// Defaults to `/etc/nginx/sites-available/{name}`.
    #[serde(default)]
    pub nginx_available_config: Option<String>,
    
    /// Path to the nginx enabled config file.
    /// 
    /// Defaults to `/etc/nginx/sites-enabled/{name}`.
    #[serde(default)]
    pub nginx_enabled_config: Option<String>,

    /// Where the nginx hibernator config file is located.
    /// 
    /// Defaults to `/etc/nginx/sites-available/nginx-hibernator`.
    #[serde(default)]
    pub nginx_hibernator_config: Option<String>,
    
    /// The port the service listens to.
    /// Used to determine if the service is up.
    pub port: u16,

    /// The path to the access log file.
    /// Your nginx configuration must log the requests to this file.
    pub access_log: String,

    /// Optional filter to match lines in the access log.
    /// Only lines containing this string will be considered.
    #[serde(default)]
    pub access_log_filter: Option<String>,
    
    /// The name of the systemctl service that runs the site.
    /// Commands `systemctl start` and `systemctl stop` will be run with this name.
    pub service_name: String,

    /// The hostnames that the service listens to.
    /// It's used so that the hibernator knows which site to start upon receiving a request.
    pub hosts: Vec<String>,

    /// The proxy mode. See [`ProxyMode`] for more information.
    #[serde(default = "ProxyMode::always")]
    pub proxy_mode: ProxyMode,

    /// The proxy mode for requests isssued by browsers. See [`ProxyMode`] for more information.
    #[serde(default = "ProxyMode::when_ready")]
    pub browser_proxy_mode: ProxyMode,

    /// Maximum time to wait before giving up on the proxy, in milliseconds.
    #[serde(default)]
    pub proxy_timeout_ms: ProxyTimeout,

    /// Interval time to check if the proxy is up, in milliseconds.
    #[serde(default)]
    pub proxy_check_interval_ms: ProxyCheckInterval,

    /// The time in seconds to keep the service running after the last request.
    /// The service will be stopped after this time.
    #[serde(deserialize_with = "deserialize_duration")]
    pub keep_alive: u64,
}

impl SiteConfig {
    pub fn nginx_available_config(&self) -> String {
        match &self.nginx_available_config {
            Some(config) => config.clone(),
            None => format!("/etc/nginx/sites-available/{}", self.name),
        }
    }

    pub fn nginx_enabled_config(&self) -> String {
        match &self.nginx_enabled_config {
            Some(config) => config.clone(),
            None => format!("/etc/nginx/sites-enabled/{}", self.name),
        }
    }

    pub fn nginx_hibernator_config(&self) -> String {
        match &self.nginx_hibernator_config {
            Some(config) => config.clone(),
            None => String::from("/etc/nginx/sites-available/nginx-hibernator"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TopLevelConfig {
    /// The port the hibernator listens to.
    /// This port should never be exposed to the internet.
    /// 
    /// Defaults to `7878`.
    #[serde(default)]
    pub hibernator_port: Option<u16>,
}

impl TopLevelConfig {
    pub fn hibernator_port(&self) -> u16 {
        match &self.hibernator_port {
            Some(port) => *port,
            None => 7878,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub top_level: TopLevelConfig,

    #[serde(default)]
    pub sites: Vec<SiteConfig>,
}
