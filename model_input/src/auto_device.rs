//! Conservative, process-local short benchmark; no host text or persisted score cache.
use crate::{
    device::{Device, Execution},
    package::{self, ValidatedPackage},
};
use core_engine::{assistance::*, config::PredictionConfig};
use std::time::{Duration, Instant};

type Backend = Box<dyn ModelBackend>;
type Sample = (Vec<String>, f64);
fn measure(backend: &mut dyn ModelBackend, limit: usize) -> Result<Vec<Sample>, Unavailable> {
    let control = || ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(3),
    };
    backend.warmup(&control())?;
    let template: ScoreRequest = serde_json::from_str(include_str!("../evaluation/request.json"))
        .map_err(|_| Unavailable::Protocol)?;
    let mut samples = Vec::new();
    for prefix in ["北京", "明天去", "我想喝", "北京", "明天去", "我想喝"] {
        let request = ProposalRequest {
            key: template.key,
            language: "zh".into(),
            prefix: prefix.into(),
            limit,
        };
        let start = Instant::now();
        let words = backend.propose(&request, &control())?;
        if words.is_empty() {
            return Err(Unavailable::Empty);
        }
        samples.push((words, start.elapsed().as_secs_f64() * 1000.));
        backend.reset();
    }
    Ok(samples)
}

fn improves(cpu: &[Sample], gpu: &[Sample]) -> bool {
    if cpu.len() != 6
        || cpu.len() != gpu.len()
        || cpu.iter().zip(gpu).any(|(c, g)| {
            c.0 != g.0
                || c.0.is_empty()
                || !c.1.is_finite()
                || !g.1.is_finite()
                || c.1 <= 0.
                || g.1 <= 0.
        })
    {
        return false;
    }
    // Six samples are a screening decision, not a claim about population P95.
    let cpu_worst = cpu.iter().map(|s| s.1).fold(0f64, f64::max);
    let gpu_worst = gpu.iter().map(|s| s.1).fold(0f64, f64::max);
    let cpu_total: f64 = cpu.iter().map(|s| s.1).sum();
    let gpu_total: f64 = gpu.iter().map(|s| s.1).sum();
    gpu_worst < cpu_worst * 0.85 && gpu_total < cpu_total * 0.85
}

/// The caller supplies an admitted, memory-checked device and power-policy reason.
pub(crate) fn select(
    package: &ValidatedPackage,
    config: &PredictionConfig,
    candidate: Option<&Device>,
    restriction: Option<String>,
) -> Result<(Backend, Execution, bool), Unavailable> {
    let mut execution = Execution {
        requested: config.device,
        requested_name: config.device_name.clone(),
        actual: Some("CPU".into()),
        reason: restriction,
    };
    let mut cpu = package::load_with_threads(package, config.threads)?;
    let Some(device) = candidate.filter(|_| execution.reason.is_none()) else {
        return Ok((cpu, execution, false));
    };
    if !cpu.capabilities().proposes_words {
        execution.reason = Some("自动 GPU 尚无该评分协议的短基准；保留 CPU".into());
        return Ok((cpu, execution, false));
    }
    let cpu_samples = match measure(cpu.as_mut(), usize::from(config.max_candidates)) {
        Ok(samples) => samples,
        Err(error) => {
            execution.reason = Some(format!("CPU 短基准未得到完整有效输出：{error}；保留 CPU"));
            return Ok((cpu, execution, false));
        }
    };
    // Avoid overlapping two weight/context allocations on integrated GPUs.
    drop(cpu);
    let mut gpu = package::load_on_device(package, config.threads, Some(device.index));
    let accepted = match &mut gpu {
        Ok(backend) => match measure(backend.as_mut(), usize::from(config.max_candidates)) {
            Ok(samples) => {
                let accepted = improves(&cpu_samples, &samples);
                let cpu_ms: Vec<_> = cpu_samples.iter().map(|s| s.1.round()).collect();
                let gpu_ms: Vec<_> = samples.iter().map(|s| s.1.round()).collect();
                execution.reason = Some(format!(
                    "短基准 CPU {cpu_ms:?} ms / GPU {gpu_ms:?} ms；{}",
                    if accepted {
                        "同输出且总耗时/最慢样本均改善超过 15%"
                    } else {
                        "输出不同或收益不足 15%，保留 CPU"
                    }
                ));
                accepted
            }
            Err(error) => {
                execution.reason = Some(format!("GPU 短基准失败：{error}；保留 CPU"));
                false
            }
        },
        Err(error) => {
            execution.reason = Some(format!("GPU 加载失败：{error}；保留 CPU"));
            false
        }
    };
    if accepted {
        execution.actual = Some(format!("{} · {}", device.name, device.description));
        return Ok((gpu?, execution, true));
    }
    drop(gpu);
    Ok((
        package::load_with_threads(package, config.threads)?,
        execution,
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_requires_matching_outputs_finite_times_and_worst_case_gain() {
        let cpu = vec![(vec!["市".into()], 100.); 6];
        let mut gpu = vec![(vec!["市".into()], 70.); 6];
        assert!(improves(&cpu, &gpu));
        gpu[5].1 = 90.;
        assert!(!improves(&cpu, &gpu));
        gpu[5].1 = 70.;
        gpu[0].0 = vec!["县".into()];
        assert!(!improves(&cpu, &gpu));
        gpu[0].0 = vec!["市".into()];
        gpu[0].1 = f64::NAN;
        assert!(!improves(&cpu, &gpu));
        assert!(!improves(&[], &[]));
    }
}
