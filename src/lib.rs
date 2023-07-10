mod asok;

use anyhow::{Context, Result};
use regex::Regex;
use std::{collections::BTreeMap, path::PathBuf, fmt::Display};
use colored::Colorize;

#[derive(Default, Debug, Clone)]
struct MetricInfo {
    name: String,
    labels: BTreeMap<String, String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MetricUnifiedInfo {
    name: String
}
type Metrics<T> = BTreeMap<MetricUnifiedInfo, T>;

impl From<MetricInfo> for MetricUnifiedInfo {
    fn from(value: MetricInfo) -> Self {
        let mut name = value.name.clone();
        for (key, value) in value.labels.iter() {
            name.push_str(&format!("&{}={}", key, value));
        }
        Self { name }
    }
}

impl Display for MetricUnifiedInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut v = self.name.split('&').collect::<Vec<&str>>();
        write!(f, "{} ", v.first().unwrap().red())?;
        v.remove(0);
        for p in v {
            let v = p.split('=').collect::<Vec<&str>>();
            for q in v.chunks_exact(2) {
                write!(f, "{}: {} ", q[0].cyan(), q[1].italic())?;
            }
        }
        Ok(())
    }
}

impl From<&MetricUnifiedInfo> for MetricInfo {
    fn from(value: &MetricUnifiedInfo) -> Self {
        let v = value.name.split(&['&', '='][..]).collect::<Vec<&str>>();
        assert!(v.len() > 1 && v.len() % 2 == 1);
        let name = v.first().unwrap().to_string();
        let mut labels = BTreeMap::<String, String>::new();
        for p in v.rchunks_exact(2) {
            labels.insert(p[0].to_string(), p[1].to_string());
        }
        Self { name, labels }
    }
}

pub fn get(path: &PathBuf, pattern: &str) -> Result<()> {
    let asok = asok::Asok::from(path);
    asok.check()?;
    let mut metrics = asok.sampling()?;
    let re = Regex::new(pattern).with_context(|| format!("can't build regex from {}", pattern))?;
    for metric in metrics.i_values.iter_mut() {
        if re.is_match(&metric.0.name) {
            println!("{} {}", metric.0, metric.1.to_string().green());
        }
    }
    for metric in metrics.f_values.iter_mut() {
        if re.is_match(&metric.0.name) {
            println!("{} {}", metric.0, metric.1.to_string().green());
        }
    }
    Ok(())
}

pub fn watch(paths: &Vec<PathBuf>, interval: u64) -> Result<()> {
    unimplemented!();
}

pub fn query(file: &PathBuf) -> Result<()> {
    unimplemented!();
}
