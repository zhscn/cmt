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

#[derive(Default, Debug, Clone)]
pub struct Sample {
    pub i: Vec<Metric<i64>>,
    pub f: Vec<Metric<f64>>,
}

impl Sample {
    /// json example:
    ///
    /// {
    ///   "LBA_alloc_extents": {
    ///     "shard": "0",
    ///     "value": 86
    ///   }
    /// }
    fn parse_json(&mut self, json: &JsonMap<String, JsonValue>) -> Result<()> {
        let mut i_m = Metric::<i64>::default();

        // metric_name: "LBA_alloc_extents"
        // metrics_labes: {...}
        let (metric_name, metric_labels) = json
            .iter()
            .next()
            .context("get first field in metric object")?;

        i_m.name = metric_name.clone();

        // label_name: "shard"
        // label_value: "0"
        for (label_name, label_value) in metric_labels
            .as_object()
            .context("convert metrics_labes to object")?
            .iter()
        {
            if label_name == "value" {
                continue;
            }

            let label_value = label_value.as_str().context("convert label_value to str")?;
            // TODO: remove it
            let label_value = if label_name == "device_id" {
                label_value
                    .as_bytes()
                    .first()
                    .context("get first byte of device id")?
                    .to_string()
            } else {
                label_value.to_string()
            };

            i_m.labels.insert(label_name.to_string(), label_value);
        }

        // process value
        match metric_labels
            .get("value")
            .context("get value from metic_labels")?
        {
            JsonValue::Number(n) => {
                if n.is_i64() {
                    if metric_name.contains("ratio") {
                        let mut f_m = Metric::<f64>::from(&i_m);
                        f_m.value = n.as_i64().context("cast to i64")? as f64;
                        self.f.push(f_m);
                    } else {
                        i_m.value = n.as_i64().context("cast to i64")?;
                        self.i.push(i_m);
                    }
                } else {
                    let mut f_m = Metric::<f64>::from(&i_m);
                    f_m.value = n.as_f64().context("cast to i64")?;
                    self.f.push(f_m);
                }
            }
            // {
            //   "sum": 0,
            //   "count": 0,
            //   "buckets": [
            //     {
            //       "le": "1",
            //       "count": 0
            //     },
            //     {
            //       "le": "+Inf",
            //       "count": 0
            //     }
            //   ]
            // }
            JsonValue::Object(o) => {
                // sum f64
                {
                    let mut f_m = Metric::<f64>::from(&i_m);
                    f_m.value = o
                        .get("sum")
                        .map(|v| {
                            if v.is_i64() {
                                v.as_i64().unwrap() as f64
                            } else {
                                v.as_f64().unwrap()
                            }
                        })
                        .context("get sum")?;
                    self.f.push(f_m);
                }
                // count i64
                {
                    let mut i_m = i_m.clone();
                    i_m.value = o.get("count").map(|v| v.as_i64().unwrap()).unwrap();
                    self.i.push(i_m);
                }
                // buckets
                for (i, bucket) in o
                    .get("buckets")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                {
                    // bucket index label
                    let mut i_m = i_m.clone();
                    i_m.labels.insert("bucket".to_string(), i.to_string());

                    let bucket = bucket.as_object().unwrap();

                    // bucket le label
                    let le = bucket.get("le").unwrap();
                    let le = if le.is_i64() {
                        le.as_i64().context("convert le to i64")?.to_string()
                    } else if le.is_f64() {
                        le.as_f64().context("convert le to f64")?.to_string()
                    } else {
                        assert!(le.is_string());
                        le.as_str().context("convert le to str")?.to_string()
                    };
                    i_m.labels.insert("le".to_string(), le);

                    // bucket count i64 as metric value
                    i_m.value = bucket.get("count").unwrap().as_i64().unwrap();

                    self.i.push(i_m);
                }
            }
            _ => bail!("unexpected value type"),
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
            sample.parse_json(
                metric
                    .as_object()
                    .context("convert each metric element in array to obejct")?,
            )?;
        }
        Ok(sample)
    }
}
