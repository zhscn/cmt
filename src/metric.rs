use std::collections::BTreeMap;
use std::fmt::Display;
use bincode::{Encode, Decode};
use colored::Colorize;

#[derive(Default, Debug, Clone, Encode, Decode)]
pub struct Metric<T: Default> {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub unified_name: String,
    pub value: T,
}

pub type Column<T> = Metric<Vec<T>>;
pub type Columns<T> = Vec<Column<T>>;

impl<T: Default> From<&str> for Metric<T> {
    fn from(value: &str) -> Self {
        let v = value.split(&['&', '='][..]).collect::<Vec<&str>>();
        assert!(v.len() > 1 && v.len() % 2 == 1);
        let name = v.first().unwrap().to_string();
        let mut labels = BTreeMap::<String, String>::new();
        for p in v.rchunks_exact(2) {
            labels.insert(p[0].to_string(), p[1].to_string());
        }
        Self {
            name,
            labels,
            unified_name: String::default(),
            value: T::default(),
        }
    }
}

impl<T: Default, U: Default> From<&Metric<U>> for Metric<T> {
    fn from(value: &Metric<U>) -> Self {
        Self {
            name: value.name.clone(),
            labels: value.labels.clone(),
            unified_name: value.unified_name.clone(),
            value: T::default(),
        }
    }
}

impl<T: Default> Display for Metric<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ", self.name.red())?;
        for l in self.labels.iter() {
            write!(f, "{}: {} ", l.0.cyan(), l.1.italic())?;
        }
        Ok(())
    }
}

impl<T: Default> Metric<T> {
    pub fn unified_name(&mut self) -> &str {
        if self.unified_name.is_empty() {
            let mut res = self.name.clone();
            for (k, v) in self.labels.iter() {
                res.push_str(&format!("&{}={}", k, v));
            }
            self.unified_name = res;
        }
        self.unified_name.as_str()
    }
}
