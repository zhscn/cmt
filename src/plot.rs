use anyhow::{Context, Result};

use crate::{WatchResult, WatchResultPerOSD};

pub fn trans_conflict(metrics: &WatchResult, detailed: bool) -> Result<()> {
    for (osd_name, metrics) in metrics {
        let invalidated = metrics.select("cache_trans_invalidated")?;
        let committed = metrics.select("cache_trans_committed")?;
        for (inva, comm) in invalidated.iter().zip(committed.iter()) {
            let mut last_invalidate = 0.0;
            let mut last_committed = 0.0;
            let mut ratio = Vec::<f64>::default();
            for (i, c) in inva.value.iter().zip(comm.value.iter()) {
                ratio.push((i - last_invalidate) / (c - last_committed));
                last_invalidate = *i;
                last_committed = *c;
            }
        }
    }
    Ok(())
}

pub fn cpu_busy_ratio(metrics: &WatchResult) -> Result<()> {
    unimplemented!()
}

pub fn foo() -> Result<()> {
    unimplemented!()
}
