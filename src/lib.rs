mod asok;

use anyhow::{Context, Result};
use bincode::{config, Decode, Encode};
use colored::Colorize;
use regex::Regex;
use rustyline::DefaultEditor;
use std::{
    collections::BTreeMap,
    fmt::Display,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Default, Debug, Clone)]
struct MetricInfo {
    name: String,
    labels: BTreeMap<String, String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
struct MetricUnifiedInfo {
    name: String,
}

#[derive(Default, Encode, Decode)]
struct Metrics<T> {
    map: BTreeMap<MetricUnifiedInfo, T>,
}

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

    for metric in metrics.i_values.map.iter_mut() {
        if re.is_match(&metric.0.name) {
            println!("{} {}", metric.0, metric.1.to_string().green());
        }
    }

    for metric in metrics.f_values.map.iter_mut() {
        if re.is_match(&metric.0.name) {
            println!("{} {}", metric.0, metric.1.to_string().green());
        }
    }

    Ok(())
}

#[derive(Default, Encode, Decode)]
struct WatchResultPerOSD {
    i: Metrics<Vec<(u64, i64)>>,
    f: Metrics<Vec<(u64, f64)>>,
}

impl WatchResultPerOSD {
    fn push(&mut self, t: u64, sample: &asok::Sample) {
        for i in sample.i_values.map.iter() {
            let v = self.i.map.entry(i.0.clone()).or_default();
            v.push((t, *i.1));
        }
        for f in sample.f_values.map.iter() {
            let v = self.f.map.entry(f.0.clone()).or_default();
            v.push((t, *f.1));
        }
    }
}

struct Watcher {
    asoks: Vec<(String, asok::Asok)>,
    result: BTreeMap<String, WatchResultPerOSD>,
}

impl Watcher {
    fn new(paths: &Vec<PathBuf>) -> Self {
        let mut asoks = Vec::<(String, asok::Asok)>::default();
        let mut result = BTreeMap::<String, WatchResultPerOSD>::default();
        for path in paths {
            let osd = path.as_os_str().to_str().unwrap().to_string();
            asoks.push((osd.clone(), asok::Asok::from(path)));
            result.insert(osd, WatchResultPerOSD::default());
        }
        Self { asoks, result }
    }

    fn process(&mut self) -> Result<()> {
        for (osd, asok) in self.asoks.iter_mut() {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("get timestamp")?
                .as_millis() as u64;
            let sample = asok.sampling()?;
            self.result.get_mut(osd).unwrap().push(time, &sample);
        }
        Ok(())
    }

    fn store(&self) -> Result<()> {
        let cfg = config::standard();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("data")?;
        bincode::encode_into_std_write(&self.result, &mut file, cfg)?;
        Ok(())
    }
}

pub fn watch(paths: &Vec<PathBuf>, interval: u64) -> Result<()> {
    let exit = Arc::new(AtomicBool::new(false));
    let mut i: u64 = 0;

    {
        let exit = exit.clone();
        ctrlc::set_handler(move || {
            exit.store(true, Ordering::SeqCst);
        })?;
    }

    let mut watcher = Watcher::new(paths);

    while !exit.load(Ordering::SeqCst) {
        if i % interval == 0 {
            watcher.process()?;
        }

        thread::sleep(Duration::from_secs(1));
        i += 1;
    }

    watcher.store()?;
    Ok(())
}

pub fn query(file: &PathBuf) -> Result<()> {
    let mut file = fs::OpenOptions::new().read(true).open(file)?;
    let cfg = config::standard();
    let metrics: BTreeMap<String, WatchResultPerOSD> =
        bincode::decode_from_std_read(&mut file, cfg)?;
    let mut rl = DefaultEditor::new()?;
    loop {
        let line = rl.readline(">>> ");
        if let Ok(line) = line {
            let re =
                Regex::new(&line).with_context(|| format!("can not build regex from: {}", line))?;
            for (_, res) in metrics.iter() {
                for m in res.i.map.iter() {
                    if re.is_match(&m.0.name) {
                        println!("{}", m.0);
                        for v in m.1.iter() {
                            println!("{} {}", v.0, v.1);
                        }
                    }
                }
            }
        } else {
            break;
        }
    }
    Ok(())
}
