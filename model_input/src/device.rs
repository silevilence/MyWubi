//! Device selection and recoverable GPU failures, confined to the inference owner.
use crate::package::{self, ValidatedPackage};
use core_engine::{
    assistance::*,
    config::{ModelDevice, PredictionConfig},
};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub backend: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

/// Enumerate only registered, compiled backends. Never load plugins from model data.
pub fn discover() -> Result<Vec<Device>, Unavailable> {
    #[cfg(all(windows, feature = "llama-cpp-2"))]
    {
        crate::llama::initialize()?;
        Ok(llama_cpp_2::list_llama_ggml_backend_devices()
            .into_iter()
            .filter(|d| matches!(d.backend.as_str(), "Vulkan" | "CUDA"))
            .map(|d| Device {
                index: d.index,
                name: d.name,
                description: d.description,
                backend: d.backend,
                free_bytes: d.memory_free as u64,
                total_bytes: d.memory_total as u64,
            })
            .collect())
    }
    #[cfg(not(all(windows, feature = "llama-cpp-2")))]
    {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Execution {
    pub requested: ModelDevice,
    pub requested_name: String,
    /// None means unloaded; successful native load/warmup is required to publish.
    pub actual: Option<String>,
    pub reason: Option<String>,
}
pub type ExecutionState = Arc<Mutex<Execution>>;

fn choose<'a>(
    config: &PredictionConfig,
    devices: &'a [Device],
    model_bytes: u64,
) -> Result<Option<&'a Device>, String> {
    let backend = match config.device {
        ModelDevice::Cpu => return Ok(None),
        ModelDevice::Auto => return Err("自动模式尚无该模型与设备的净收益证据，保留 CPU".into()),
        ModelDevice::Vulkan => "Vulkan",
        ModelDevice::Cuda => "CUDA",
    };
    let device = devices
        .iter()
        .find(|d| {
            d.backend == backend && (config.device_name.is_empty() || d.name == config.device_name)
        })
        .ok_or_else(|| format!("{backend} 组件、驱动或指定设备不可用"))?;
    // Conservative admission only; native context allocation remains fallible.
    if device.free_bytes < model_bytes.saturating_add(256 * 1024 * 1024) {
        return Err(format!(
            "{} 可用显存不足（需要模型大小另加 256 MiB 余量）",
            device.name
        ));
    }
    Ok(Some(device))
}

pub fn load(
    package: ValidatedPackage,
    config: &PredictionConfig,
    state: ExecutionState,
) -> Result<Box<dyn ModelBackend>, Unavailable> {
    let mut execution = Execution {
        requested: config.device,
        requested_name: config.device_name.clone(),
        ..Default::default()
    };
    let selected = if package.manifest.backend == package::Backend::OnnxRuntime
        && config.device != ModelDevice::Cpu
    {
        Err("当前 ONNX 适配仅启用 CPU；GPU 路径尚未通过产品模型评估".into())
    } else if config.device == ModelDevice::Cpu {
        Ok(None)
    } else {
        let devices = discover().unwrap_or_default();
        let bytes = package.manifest.files.iter().map(|f| f.bytes).sum();
        choose(config, &devices, bytes).map(|d| d.cloned())
    };
    let selected = match selected {
        Ok(device) => device,
        Err(reason) => {
            execution.reason = Some(reason);
            None
        }
    };
    let mut gpu = selected.is_some();
    let backend =
        match package::load_on_device(&package, config.threads, selected.as_ref().map(|d| d.index))
        {
            Ok(backend) => backend,
            Err(error) if gpu => {
                execution.reason = Some(format!("GPU 加载失败：{error}；已回退 CPU"));
                gpu = false;
                package::load_with_threads(&package, config.threads)?
            }
            Err(error) => return Err(error),
        };
    execution.actual = Some(if gpu {
        selected
            .map(|d| format!("{} · {}", d.name, d.description))
            .unwrap_or_default()
    } else {
        "CPU".into()
    });
    *state.lock().map_err(|_| Unavailable::Inference)? = execution;
    Ok(Box::new(Recovering {
        backend: Some(backend),
        package,
        threads: config.threads,
        gpu,
        state,
        cpu_load: package::load_with_threads,
    }))
}

type CpuLoader = fn(&ValidatedPackage, u8) -> Result<Box<dyn ModelBackend>, Unavailable>;
struct Recovering {
    backend: Option<Box<dyn ModelBackend>>,
    package: ValidatedPackage,
    threads: u8,
    gpu: bool,
    state: ExecutionState,
    cpu_load: CpuLoader,
}
impl Recovering {
    fn invoke<T>(
        &mut self,
        control: &ComputeControl,
        mut call: impl FnMut(&mut dyn ModelBackend) -> Result<T, Unavailable>,
    ) -> Result<T, Unavailable> {
        control.check()?;
        let result = call(self.backend.as_mut().ok_or(Unavailable::Load)?.as_mut());
        if self.gpu && matches!(result, Err(Unavailable::Inference | Unavailable::Load)) {
            self.gpu = false;
            // Release VRAM before constructing CPU replacement. Do not retry GPU every key.
            self.backend = None;
            if let Ok(mut status) = self.state.lock() {
                status.actual = None;
                status.reason = Some("GPU 执行失败；释放设备并切换 CPU".into());
            }
            self.backend = Some((self.cpu_load)(&self.package, self.threads)?);
            if let Ok(mut status) = self.state.lock() {
                status.actual = Some("CPU".into());
            }
            control.check()?;
            return call(self.backend.as_mut().ok_or(Unavailable::Load)?.as_mut());
        }
        result
    }
}
impl ModelBackend for Recovering {
    fn capabilities(&self) -> &Capabilities {
        &self.package.manifest.capabilities
    }
    fn reset(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.reset();
        }
    }
    fn warmup(&mut self, control: &ComputeControl) -> Result<(), Unavailable> {
        self.invoke(control, |b| b.warmup(control))
    }
    fn score(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        self.invoke(control, |b| b.score(request, control))
    }
    fn propose(
        &mut self,
        request: &ProposalRequest,
        control: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        self.invoke(control, |b| b.propose(request, control))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };

    struct Fault {
        caps: Capabilities,
        error: Option<Unavailable>,
        dropped: Arc<AtomicBool>,
    }
    impl Drop for Fault {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }
    impl ModelBackend for Fault {
        fn capabilities(&self) -> &Capabilities {
            &self.caps
        }
        fn reset(&mut self) {}
        fn score(
            &mut self,
            _: &ScoreRequest,
            _: &ComputeControl,
        ) -> Result<ScoreResponse, Unavailable> {
            Err(Unavailable::Capability)
        }
        fn warmup(&mut self, _: &ComputeControl) -> Result<(), Unavailable> {
            self.error.map_or(Ok(()), Err)
        }
    }
    #[test]
    fn recovery_releases_gpu_and_does_not_retry_deadlines_or_confidence_failures() {
        for error in [
            Unavailable::Inference,
            Unavailable::Load,
            Unavailable::Timeout,
            Unavailable::LowConfidence,
            Unavailable::Cancelled,
        ] {
            let manifest = crate::download::available_models().remove(0).manifest;
            let dropped = Arc::new(AtomicBool::new(false));
            let state = Arc::new(Mutex::new(Execution::default()));
            let mut recovering = Recovering {
                backend: Some(Box::new(Fault {
                    caps: manifest.capabilities.clone(),
                    error: Some(error),
                    dropped: Arc::clone(&dropped),
                })),
                package: ValidatedPackage {
                    root: "unused-fixture".into(),
                    manifest,
                },
                threads: 2,
                gpu: true,
                state: Arc::clone(&state),
                cpu_load: |package, _| {
                    Ok(Box::new(Fault {
                        caps: package.manifest.capabilities.clone(),
                        error: None,
                        dropped: Arc::default(),
                    }))
                },
            };
            let control = ComputeControl {
                cancellation: Cancellation::default(),
                deadline: Instant::now() + Duration::from_secs(1),
            };
            let result = recovering.warmup(&control);
            if matches!(error, Unavailable::Inference | Unavailable::Load) {
                assert!(result.is_ok());
                assert!(dropped.load(Ordering::SeqCst));
                assert_eq!(state.lock().unwrap().actual.as_deref(), Some("CPU"));
                assert!(recovering.warmup(&control).is_ok());
            } else {
                assert_eq!(result, Err(error));
                assert!(!dropped.load(Ordering::SeqCst));
            }
        }
    }
    #[test]
    fn selection_requires_matching_backend_name_and_memory() {
        let mut config = PredictionConfig {
            device: ModelDevice::Vulkan,
            device_name: "Vulkan1".into(),
            ..Default::default()
        };
        let devices = vec![Device {
            index: 3,
            name: "Vulkan1".into(),
            description: "fixture".into(),
            backend: "Vulkan".into(),
            free_bytes: 1024 * 1024 * 1024,
            total_bytes: 1024 * 1024 * 1024,
        }];
        assert_eq!(choose(&config, &devices, 1).unwrap().unwrap().index, 3);
        assert!(choose(&config, &devices, u64::MAX).is_err());
        config.device_name = "Vulkan2".into();
        assert!(choose(&config, &devices, 1).is_err());
        config.device = ModelDevice::Auto;
        assert!(choose(&config, &devices, 1).is_err());
        config.device = ModelDevice::Cpu;
        assert!(choose(&config, &[], 1).unwrap().is_none());
    }
}
