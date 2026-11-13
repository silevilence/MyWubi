//! Isolated settings preview, built from the draft and its explicitly selected resources.
use crate::{candidates::*, runtime::*, session::*, shortcut_time::*, shortcuts::*, *};
use serde::Serialize;
use std::{
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

#[derive(Clone)]
pub struct FixedClock(pub LocalTime);
impl Clock for FixedClock {
    fn now(&self) -> Result<LocalTime, String> {
        Ok(self.0.clone())
    }
}
pub fn example_time() -> LocalTime {
    LocalTime {
        year: 2026,
        month: 11,
        day: 12,
        hour: 7,
        minute: 8,
        second: 9,
        utc_offset_seconds: 28800,
        timezone: "Asia/Shanghai（固定示例）".into(),
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct PreviewRow {
    pub candidate: Candidate,
    pub source: String,
    pub target: Option<u32>,
    pub actual: usize,
    pub page: usize,
    pub slot: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct ShortcutPreview {
    pub time: LocalTime,
    pub code: String,
    pub rows: Vec<PreviewRow>,
    pub total: usize,
    pub effective_capacity: usize,
    pub states: Vec<RuleAvailability>,
    pub resources: Vec<String>,
    pub warnings: Vec<String>,
}
/// A content identifier for this loaded snapshot, not a cryptographic integrity claim.
fn content_id(value: &impl Serialize) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(value)
        .unwrap_or_default()
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
pub fn build(
    path: PathBuf,
    config: Config,
    code: &str,
    time: LocalTime,
    profile: BehaviorProfile,
) -> Result<ShortcutPreview, String> {
    let config = config.canonicalized().map_err(|e| e.to_string())?;
    let runtime =
        RuntimeBuilder::build(path, config, profile, LoadPolicy::Startup).map_err(|e| e.message)?;
    from_runtime(&runtime, code, time, profile)
}
pub fn from_runtime(
    runtime: &RuntimeSnapshot,
    code: &str,
    time: LocalTime,
    profile: BehaviorProfile,
) -> Result<ShortcutPreview, String> {
    time.validate()?;
    if code.is_empty()
        || code.len() > MAX_CODE_LEN
        || !code.bytes().all(|c| c.is_ascii_alphabetic())
    {
        return Err("预览编码须为 1～32 个英文字母".into());
    }
    let code = code.to_ascii_lowercase();
    let mut session = Session::new_with_clock(
        runtime,
        profile,
        HostContext::default(),
        Arc::new(FixedClock(time.clone())),
    );
    let mut warnings = runtime.temporary.load_errors.clone();
    warnings.extend(runtime.expressions.errors().map(str::to_owned));
    for c in code.chars() {
        let plan = session
            .prepare(Action::InputCode(c))
            .map_err(|e| format!("预览输入失败：{e:?}"))?;
        if matches!(
            plan.op,
            EditSessionOp::CommitAndReplace { .. } | EditSessionOp::CommitThenUpdate { .. }
        ) {
            warnings.push(
                "此编码未命中有效快捷规则，主表自动上屏规则已结束输入轮次；预览不会写入宿主。"
                    .into(),
            );
        }
        session.acknowledge(plan.token, HostOutcome::Applied);
    }
    let sm = session.state_machine();
    let first = session.candidate_window(0, MAX_WINDOW);
    let total = first.total;
    let mut rows = Vec::new();
    // All configured positions are <=10000. Keep the first window plus every dynamic row.
    for offset in (0..total.min(MAX_POSITION as usize)).step_by(MAX_WINDOW) {
        for candidate in session.candidate_window(offset, MAX_WINDOW).candidates {
            let rule_index = candidate.hints.iter().find_map(|h| {
                if let CandidateHint::DateTime(i) = h {
                    Some(*i)
                } else {
                    None
                }
            });
            let actual = candidate.id.0 + 1;
            if actual > MAX_WINDOW && rule_index.is_none() {
                continue;
            }
            let (page, slot) = sm
                .candidate_location(candidate.id.0)
                .ok_or("预览位置映射失败")?;
            rows.push(PreviewRow {
                source: if rule_index.is_some() {
                    "日期时间".into()
                } else {
                    candidate
                        .expression_kind
                        .map(|k| k.label())
                        .unwrap_or("普通候选")
                        .into()
                },
                target: rule_index.map(|i| runtime.config.shortcuts.rules[i].position),
                candidate,
                actual,
                page: page + 1,
                slot: slot + 1,
            });
        }
    }
    let states = runtime
        .config
        .shortcuts
        .rules
        .iter()
        .map(|r| availability(r, runtime.dict.table_config().into()))
        .collect();
    let mut resources = vec![
        format!("主表：{}", runtime.system_table_path.display()),
        format!("用户表：{}", runtime.user_table_path.display()),
        format!(
            "合并码表快照：{} · {} 条",
            content_id(&(runtime.dict.table_config(), runtime.dict.entries())),
            runtime.dict.entries().len()
        ),
    ];
    for kind in expressions::ExpressionKind::ALL {
        let resource = runtime.expressions.resource(kind);
        if let Some(library) = &resource.library {
            resources.push(format!(
                "{}：{} · 版本 {} · 快照 {}",
                kind.label(),
                resource
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "内存资源".into()),
                library.document().version,
                content_id(library.document())
            ));
        }
    }
    Ok(ShortcutPreview {
        time,
        code,
        rows,
        total,
        effective_capacity: sm.candidate_page_capacity(),
        states,
        resources,
        warnings,
    })
}
