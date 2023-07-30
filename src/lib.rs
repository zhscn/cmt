mod asok;
mod metric;
mod plot;

use anyhow::{bail, Context, Result};
use bincode::{config, Decode, Encode};
use colored::Colorize;
pub use metric::*;
use regex::Regex;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn get(path: &PathBuf, pattern: &str) -> Result<()> {
    let asok = asok::Asok::from(path);
    asok.check()?;
    let mut metrics = asok.sampling()?;
    let re = Regex::new(pattern).with_context(|| format!("can't build regex from {}", pattern))?;

    for metric in metrics.i.iter_mut() {
        if re.is_match(metric.unified_name()) {
            println!("{} {}", metric, metric.value.to_string().green());
        }
    }

    for metric in metrics.f.iter_mut() {
        if re.is_match(metric.unified_name()) {
            println!("{} {}", metric, metric.value.to_string().green());
        }
    }

    Ok(())
}

#[derive(Default, Encode, Decode)]
pub struct WatchResultPerOSD {
    pub index: BTreeMap<String, usize>,
    pub timestamp: Vec<u64>,
    pub i: Columns<i64>,
    pub f: Columns<f64>,
}

impl WatchResultPerOSD {
    fn push(&mut self, t: u64, sample: &mut asok::Sample) {
        self.timestamp.push(t);
        if self.i.is_empty() {
            for (no, i) in sample.i.iter().enumerate() {
                let mut ic = Column::<i64>::from(i);
                ic.value.push(i.value);
                self.i.push(ic);
                self.index
                    .insert(self.i.get_mut(no).unwrap().unified_name().to_string(), no);
            }
            for (no, f) in sample.f.iter().enumerate() {
                let mut fc = Column::<f64>::from(f);
                fc.value.push(f.value);
                self.f.push(fc);
                self.index.insert(
                    self.f.get_mut(no).unwrap().unified_name().to_string(),
                    no + self.i.len(),
                );
            }
        } else {
            for (i, is) in sample.i.iter_mut().zip(self.i.iter_mut()) {
                assert_eq!(i.unified_name(), is.unified_name());
                is.value.push(i.value);
            }
            for (f, fs) in sample.f.iter_mut().zip(self.f.iter_mut()) {
                assert_eq!(f.unified_name(), fs.unified_name());
                fs.value.push(f.value);
            }
        }
    }
    fn select(&self, pattern: &str) -> Result<Columns<f64>> {
        let re = Regex::new(pattern).context("re")?;
        let mut matched_indexs = Vec::<usize>::default();
        for (k, v) in self.index.iter() {
            if re.is_match(&k) {
                matched_indexs.push(*v);
            }
        }
        if matched_indexs.is_empty() {
            bail!("not fount")
        }
        let mut columns = Columns::<f64>::default();
        for idx in matched_indexs {
            if idx < self.i.len() {
                let matched_column = self.i.get(idx).unwrap();
                let mut column = Column::<f64>::from(matched_column);
                column.value = matched_column.value.iter().map(|v| *v as f64).collect();
                columns.push(column);
            } else {
                let idx = idx - self.i.len();
                assert!(idx < self.f.len());
                let matched_column = self.f.get(idx).unwrap();
                let mut column = Column::<f64>::from(matched_column);
                column.value = matched_column.value.clone();
                columns.push(column);
            }
        }
        Ok(columns)
    }
}

type OSDName = String;
type WatchResult = BTreeMap<OSDName, WatchResultPerOSD>;

struct Watcher {
    asoks: Vec<(String, asok::Asok)>,
    result: WatchResult,
}

impl Watcher {
    fn new(paths: &Vec<PathBuf>) -> Self {
        let mut asoks = Vec::<(String, asok::Asok)>::default();
        let mut result = WatchResult::default();
        for path in paths {
            let osd = path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
                .replace(".asok", "");
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
            let mut sample = asok.sampling()?;
            self.result.get_mut(osd).unwrap().push(time, &mut sample);
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

fn list(metrics: &WatchResult) -> Result<()> {
    let osd = metrics.iter().next().unwrap();
    for i in osd.1.i.iter() {
        println!("{}", i);
    }
    for f in osd.1.f.iter() {
        println!("{}", f);
    }
    Ok(())
}

pub fn plot(file: &PathBuf, name: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new().read(true).open(file)?;
    let cfg = config::standard();
    match bincode::decode_from_std_read(&mut file, cfg) {
        Ok(metrics) => {
            if name == "list" {
                list(&metrics)?;
                return Ok(());
            }

            if name == "trans_conflict_ratio" || name == "all" {
                plot::trans_conflict(&metrics, false)?
            }
            if name == "trans_conflict_ratio_detailed" || name == "all" {
                plot::trans_conflict(&metrics, true)?
            }
            if name == "cpu_busy_ratio" || name == "all" {
                plot::cpu_busy_ratio(&metrics)?
            }

            Ok(())
        }
        Err(_) => plot::foo(),
    }
}
