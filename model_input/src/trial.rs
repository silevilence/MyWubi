//! Explicit samples only. Never accesses saved configuration or the active input worker.
use core_engine::assistance::*;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub const GUIDE: &str = include_str!("../catalog/custom-model-guide.md");

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub package: PathBuf,
    pub prefix: String,
    pub wait_ms: u64,
    pub threads: u8,
}

#[derive(Serialize)]
pub struct Report {
    pub words: Vec<String>,
    pub load_ms: u128,
    pub predict_ms: u128,
    pub notice: String,
}

pub fn run(request: Request) -> Result<Report, String> {
    if !request.package.is_absolute()
        || request.prefix.trim().is_empty()
        || request.prefix.chars().count() > 256
        || !(50..=1000).contains(&request.wait_ms)
        || !(1..=4).contains(&request.threads)
    {
        return Err(
            "测试前文需为 1～256 字，预算 50～1000 ms，线程 1～4；模型包路径必须绝对定位".into(),
        );
    }
    let start = Instant::now();
    let checked = crate::compatibility::check(&request.package).map_err(|e| e.to_string())?;
    if !crate::service::BackendAvailability::compiled().supports(checked.manifest.backend) {
        return Err("缺少运行组件：请获取含对应 GGUF/ONNX 后端的模型版发行包".into());
    }
    let mut backend =
        crate::package::load_with_threads(&checked, request.threads).map_err(reason)?;
    backend
        .warmup(&ComputeControl {
            cancellation: Cancellation::default(),
            deadline: Instant::now() + Duration::from_secs(30),
        })
        .map_err(reason)?;
    let load_ms = start.elapsed().as_millis();
    let mut session = ModelSession::new(backend);
    let key = RequestKey {
        edit: core_engine::session::EditToken {
            session: 0,
            generation: 0,
            editor: 0,
            epoch: 0,
            runtime: 0,
        },
        query: core_engine::candidates::QueryRevision {
            engine: 0,
            version: 0,
        },
        view: 0,
        context: 0,
        model: 0,
        config: 0,
        request: 0,
    };
    let start = Instant::now();
    let control = ComputeControl {
        cancellation: Cancellation::default(),
        deadline: start + Duration::from_millis(request.wait_ms),
    };
    let result = if session.proposes_words() {
        session
            .propose(
                &ProposalRequest {
                    key,
                    language: "zh".into(),
                    prefix: request.prefix,
                    limit: 1,
                },
                &control,
            )
            .map(|r| r.words)
    } else {
        let candidates = prediction::Lexicon::new(&[])
            .recall(&request.prefix, 16)
            .into_iter()
            .enumerate()
            .map(|(i, c)| ScoringCandidate {
                id: core_engine::candidates::CandidateId(i),
                text: c.text,
            })
            .collect();
        let input = ScoreRequest {
            protocol: 1,
            key,
            task: Task::Predict,
            language: "zh".into(),
            prefix: request.prefix,
            code: String::new(),
            candidates,
        };
        session.score(&input, &control).map(|r| {
            r.prediction_accepted
                .unwrap_or_default()
                .iter()
                .filter_map(|id| {
                    input
                        .candidates
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| c.text.clone())
                })
                .collect()
        })
    };
    let predict_ms = start.elapsed().as_millis();
    session.reset();
    let words = match result {
        Ok(words) => words,
        Err(Unavailable::Empty | Unavailable::LowConfidence) => Vec::new(),
        Err(e) => return Err(reason(e)),
    };
    Ok(Report {words,load_ms,predict_ms,notice:"仅证明此样例兼容运行；空候选表示无召回或未过准入。不是质量认证，未选择模型或更改预测开关。".into()})
}

fn reason(error: Unavailable) -> String {
    match error {
        Unavailable::UnsupportedModel => {
            "不支持的模型架构、算子或分词器；请检查指南中的适配约定".into()
        }
        Unavailable::Timeout => {
            "试运行超过等待预算；保留原配置，可减少前文或使用更小的模型重试".into()
        }
        Unavailable::Capability => "模型声明与运行组件能力不匹配；请核对清单和模型版发行包".into(),
        Unavailable::Load => "模型加载失败；请核对运行组件、内存及模型文件完整性".into(),
        _ => format!("试运行未产生可接受结果：{error}"),
    }
}

pub fn exchange(bytes: &[u8]) -> Vec<u8> {
    let start = Instant::now();
    let result = if bytes.len() > 65536 {
        Err("试运行请求超过限制".into())
    } else {
        serde_json::from_slice(bytes)
            .map_err(|_| "试运行请求格式错误".into())
            .and_then(run)
    };
    serde_json::to_vec(&match result {
        Ok(value) => {
            serde_json::json!({"ok":true,"value":value,"elapsed_ms":start.elapsed().as_millis()})
        }
        Err(error) => {
            serde_json::json!({"ok":false,"error":error,"elapsed_ms":start.elapsed().as_millis()})
        }
    })
    .unwrap_or_default()
}
