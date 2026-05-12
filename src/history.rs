use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::fs::read_to_string;
use tokio::sync::RwLock;

pub static SERVICE_HISTORY: LazyLock<ServiceHistory> = LazyLock::new(|| ServiceHistory(Arc::new(RwLock::new(HashMap::new()))));

pub struct ServiceHistory(Arc<RwLock<HashMap<String, Vec<usize>>>>);

async fn read_file(file: &str) -> Vec<usize> {
    match read_to_string(file).await {
        Ok(content) => content.lines().filter_map(|line| line.parse().ok()).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            elog!("Failed to read file {file}: {e}");
            Vec::new()
        }
    }
}

async fn write_file(file: &str, values: &[usize]) {
    let content = values.iter().map(|v| v.to_string()).collect::<Vec<String>>().join("\n");
    if let Err(e) = tokio::fs::write(file, content).await {
        elog!("Failed to write {file}: {e}");
    };
}

fn compute_eta(mut values: &[usize], percentile: usize, count: usize) -> Option<usize> {
    if values.len() > count {
        values = &values[values.len() - count..];
    }
    let mut values = values.to_owned();
    values.sort();
    let idx = (values.len() * percentile) / 100;
    values.get(idx).copied()
}

impl ServiceHistory {
    pub async fn put_history(&self, file: &str, new_val: usize) {
        let mut cache = self.0.write().await;
        let mut values = match cache.get(file) {
            Some(values) => values.clone(),
            None => read_file(file).await, // TODO: Should not read while we lock for write
        };
        values.push(new_val);
        cache.insert(file.to_string(), values.clone());
        drop(cache);
        write_file(file, &values).await;
    }

    pub async fn get_eta(&self, file: &str, percentile: usize, count: usize) -> Option<usize> {
        let cache = self.0.read().await;
        if let Some(values) = cache.get(file) {
            return compute_eta(values, percentile, count);
        }
        drop(cache);

        let mut cache = self.0.write().await;
        let values = read_file(file).await;
        let eta = compute_eta(&values, percentile, count);
        cache.insert(file.to_string(), values);
        eta
    }
}
