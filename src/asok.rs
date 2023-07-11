use super::*;
use anyhow::{bail, Context, Result};
use std::{
    fs,
    io::{BufReader, Read, Write},
    os::unix::{fs::FileTypeExt, net::UnixStream},
    path::PathBuf,
};

type JsonValue = serde_json::Value;
type JsonMap<T, U> = serde_json::Map<T, U>;

fn read_json(path: &PathBuf) -> Result<Vec<u8>> {
    let mut stream = UnixStream::connect(path)?;
    stream.write_all("{\"prefix\":\"dump_metrics\"}\0".as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut length_buffer = [0u8; 4];
    reader.read_exact(&mut length_buffer)?;

    let length = u32::from_be_bytes(length_buffer) as usize;
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer)?;
    Ok(buffer)
}

#[derive(Default, Encode, Decode)]
pub struct Sample {
    pub(super) i_values: Metrics<i64>,
    pub(super) f_values: Metrics<f64>,
}

impl Sample {
    fn parse_metric_object(&mut self, metric: &JsonMap<String, JsonValue>) -> Result<()> {
        let mut metric_info = MetricInfo::default();
        let (metric_name, metric_labels) = metric
            .iter()
            .next()
            .context("get first field in metric object")?;
        metric_info.name = metric_name.clone();

        let metric_labels = metric_labels
            .as_object()
            .context("convert the value of first field in metric object to object")?;

        for (label_name, label_value) in metric_labels.iter() {
            if label_name == "value" {
                continue;
            }

            let label_value = label_value
                .as_str()
                .context("convert label value to string")?;
            let label_value = if label_name == "device_id" {
                label_value
                    .as_bytes()
                    .first()
                    .context("get first byte of device id")?
                    .to_string()
            } else {
                label_value.to_string()
            };
            metric_info
                .labels
                .insert(label_name.to_string(), label_value);
        }

        match metric_labels
            .get("value")
            .context("get value of metric labels")?
        {
            JsonValue::Number(n) => {
                if n.is_i64() {
                    if metric_name.contains("ratio") {
                        let value = n.as_i64().context("cast to i64")? as f64;
                        self.f_values.map.insert(metric_info.into(), value);
                    } else {
                        let value = n.as_i64().context("cast to i64")?;
                        self.i_values.map.insert(metric_info.into(), value);
                    }
                } else {
                    let value = n.as_f64().context("cast to f64")?;
                    self.f_values.map.insert(metric_info.into(), value);
                }
            }
            JsonValue::Object(o) => {
                {
                    let mut metric_info = metric_info.clone();
                    let value = o
                        .get("count")
                        .context("get bucket count")?
                        .as_i64()
                        .context("convert bucket count to i64")?;
                    metric_info
                        .labels
                        .insert("bucket".to_string(), "count".to_string());
                    self.i_values.map.insert(metric_info.into(), value);
                }
                {
                    let mut metric_info = metric_info.clone();
                    let value = o.get("sum").context("get bucket sum")?;
                    let value = if value.is_i64() {
                        value.as_i64().context("convert bucket sum value to i64")? as f64
                    } else {
                        value.as_f64().context("convert bucket sum value to f64")?
                    };
                    metric_info
                        .labels
                        .insert("bucket".to_string(), "sum".to_string());
                    self.f_values.map.insert(metric_info.into(), value);
                }
                let buckets = o
                    .get("buckets")
                    .context("get buckets object")?
                    .as_array()
                    .context("convert buckets object to array")?;
                for (i, bucket) in buckets.iter().enumerate() {
                    let mut metric_info = metric_info.clone();
                    let bucket = bucket
                        .as_object()
                        .context("convert bucket object to object")?;
                    metric_info
                        .labels
                        .insert("bucket".to_string(), i.to_string());
                    let le = bucket.get("le").context("get le field of bucket")?;
                    let le = if le.is_i64() {
                        le.as_i64().context("convert le to i64")?.to_string()
                    } else if le.is_f64() {
                        le.as_f64().context("convert le to f64")?.to_string()
                    } else {
                        assert!(le.is_string());
                        le.as_str().context("convert le to str")?.to_string()
                    };
                    metric_info.labels.insert("le".to_string(), le);
                    let value = bucket
                        .get("count")
                        .context("get count field of bucket")?
                        .as_i64()
                        .context("convert count to i64")?;
                    self.i_values.map.insert(metric_info.into(), value);
                }
            }
            _ => bail!("unexpect value type"),
        }
        Ok(())
    }
}

pub struct Asok {
    path: PathBuf,
}

impl From<&PathBuf> for Asok {
    fn from(value: &PathBuf) -> Self {
        Self {
            path: value.clone(),
        }
    }
}

impl Asok {
    pub fn check(&self) -> Result<()> {
        let meta = fs::metadata(&self.path)?;
        if !meta.file_type().is_socket() {
            anyhow::bail!(
                "{} is not domain socekt",
                self.path.to_str().context("asok path to str")?
            )
        } else {
            Ok(())
        }
    }

    pub fn sampling(&self) -> Result<Sample> {
        let json = read_json(&self.path)?;
        let mut sample = Sample::default();
        let v: JsonValue = serde_json::from_slice(&json)?;

        let metric_array = v
            .as_object()
            .context("convert asok json to object")?
            .get("metrics")
            .context("get metrics field")?
            .as_array()
            .context("convert metrics field value to array")?;
        for metric in metric_array {
            sample.parse_metric_object(
                metric
                    .as_object()
                    .context("convert each metric element in array to obejct")?,
            )?;
        }
        Ok(sample)
    }
}
