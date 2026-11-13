//! Replay a bounded settings-only query through the real engine. No host adapter is involved.
use crate::{candidates::*, expressions::ExpressionKind, runtime::*, session::*, *};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrialAction {
    Input(String),
    Select(usize),
    Find,
    Edit,
    Backspace,
}
#[derive(Debug, Clone, Serialize)]
pub struct TrialResult {
    pub view: SessionView,
    pub window: CandidateWindow,
    pub selected: Option<String>,
    pub resources: Vec<String>,
}
pub fn build(
    path: PathBuf,
    config: Config,
    kind: ExpressionKind,
    input: ExpressionInput,
    actions: &[TrialAction],
) -> Result<TrialResult, String> {
    let config = config.canonicalized().map_err(|e| e.to_string())?;
    let runtime =
        RuntimeBuilder::build(path, config, BehaviorProfile::TouchV1, LoadPolicy::Startup)
            .map_err(|e| e.message)?;
    run(&runtime, kind, input, actions)
}
pub fn run(
    runtime: &RuntimeSnapshot,
    kind: ExpressionKind,
    input: ExpressionInput,
    actions: &[TrialAction],
) -> Result<TrialResult, String> {
    if actions.len() > 256
        || actions
            .iter()
            .map(|a| {
                if let TrialAction::Input(s) = a {
                    s.chars().count()
                } else {
                    0
                }
            })
            .sum::<usize>()
            > 4096
    {
        return Err("本次试用操作较多，请清空后重新试用".into());
    }
    let mode = match kind {
        ExpressionKind::Emoji => TemporaryMode::Emoji,
        ExpressionKind::Kaomoji => TemporaryMode::Kaomoji,
        ExpressionKind::Kana => TemporaryMode::Kana,
        ExpressionKind::Math => TemporaryMode::Math,
        ExpressionKind::Punctuation => TemporaryMode::Punctuation,
        ExpressionKind::Symbols => TemporaryMode::Symbols,
    };
    let mut session = Session::new(runtime, BehaviorProfile::TouchV1, HostContext::default());
    let mut selected = None;
    fn dispatch(
        session: &mut Session,
        action: Action,
        selected: &mut Option<String>,
    ) -> Result<(), String> {
        let plan = session
            .prepare(action)
            .map_err(|e| format!("试用失败：{e:?}"))?;
        if let EditSessionOp::CommitAndReplace { text, .. } = &plan.op {
            *selected = Some(text.clone());
        }
        session.acknowledge(plan.token, HostOutcome::Applied);
        Ok(())
    }
    dispatch(&mut session, Action::BeginTemporary(mode), &mut selected)?;
    dispatch(
        &mut session,
        Action::SetExpressionInput(input),
        &mut selected,
    )?;
    for action in actions {
        if selected.is_some() {
            return Err("结果已选中，请清空后开始新查询".into());
        }
        match action {
            TrialAction::Input(text) => {
                for c in text.chars() {
                    dispatch(&mut session, Action::InputCode(c), &mut selected)?;
                }
            }
            TrialAction::Select(index) => {
                let window = session.candidate_window(*index, 1);
                let candidate = window.candidates.first().ok_or("候选不存在，请重新查找")?;
                dispatch(
                    &mut session,
                    Action::SelectCandidate(window.query_revision, candidate.id),
                    &mut selected,
                )?;
            }
            TrialAction::Find => dispatch(&mut session, Action::FindExpressions, &mut selected)?,
            TrialAction::Edit => {
                dispatch(&mut session, Action::EditExpressionQuery, &mut selected)?
            }
            TrialAction::Backspace => dispatch(&mut session, Action::Backspace, &mut selected)?,
        }
    }
    let resource = runtime.expressions.resource(kind);
    let resources = vec![
        format!(
            "主表：{}；用户表：{}",
            runtime.system_table_path.display(),
            runtime.user_table_path.display()
        ),
        format!(
            "{}：{} · 版本 {}",
            kind.label(),
            resource
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "内存资源".into()),
            resource
                .library
                .as_ref()
                .map(|l| l.document().version.as_str())
                .unwrap_or("不可用")
        ),
    ];
    Ok(TrialResult {
        view: session.snapshot().clone(),
        window: session.candidate_window(0, MAX_WINDOW),
        selected,
        resources,
    })
}
