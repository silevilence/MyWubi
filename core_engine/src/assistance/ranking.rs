//! One immutable legal candidate set and one bounded opportunity to reorder it.
//! Hosts must map every selection shortcut through this snapshot, then use the
//! existing query revision / candidate identity commit protocol.
use super::{RequestKey, ScoreRequest, ScoreResponse, ScoringCandidate, Task, Unavailable};
use crate::candidates::CandidateId;
use std::{collections::HashMap, time::Instant};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RankingMode {
    #[default]
    Base,
    BaseThenAssisted {
        pin_first: bool,
    },
    WaitForAssisted,
}

/// Adapter-specific thresholds must be calibrated on development data. These
/// are raw scores within one model, never a universal probability threshold.
#[derive(Debug, Clone, Copy)]
pub struct ScoreThreshold {
    pub minimum: f32,
    pub margin: f32,
}

impl ScoreThreshold {
    pub fn validate(self) -> Result<(), Unavailable> {
        if !self.minimum.is_finite() || !self.margin.is_finite() || self.margin < 0. {
            return Err(Unavailable::NonFinite);
        }
        Ok(())
    }
}

/// A stamp changes when visible order changes. Pointer/touch selection uses the
/// captured stamp so a delayed click cannot select a new item at an old position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionStamp {
    pub key: RequestKey,
    pub order: u64,
}

pub struct RankingRound {
    request: ScoreRequest,
    mode: RankingMode,
    deadline: Instant,
    order: Vec<usize>,
    revision: u64,
    frozen: bool,
    waiting: bool,
}

impl RankingRound {
    /// The caller provides only the current legal candidates, already bounded by
    /// the selected adapter. Base mode never requires scheduling a model request.
    pub fn new(
        request: ScoreRequest,
        mode: RankingMode,
        deadline: Instant,
    ) -> Result<Self, Unavailable> {
        if request.protocol != super::PROTOCOL_VERSION {
            return Err(Unavailable::Protocol);
        }
        if request.candidates.len() > 64 {
            return Err(Unavailable::Limit);
        }
        if request.task != Task::Rank || request.code.is_empty() {
            return Err(Unavailable::Candidates);
        }
        // Reuse protocol/output identity checks without initializing a backend.
        ScoreResponse {
            protocol: super::PROTOCOL_VERSION,
            key: request.key,
            scores: request
                .candidates
                .iter()
                .map(|c| super::CandidateScore {
                    id: c.id,
                    value: 0.,
                })
                .collect(),
        }
        .validate(&request, request.key)?;
        if request.candidates.iter().any(|c| c.text.is_empty()) {
            return Err(Unavailable::Candidates);
        }
        Ok(Self {
            order: (0..request.candidates.len()).collect(),
            frozen: mode == RankingMode::Base,
            waiting: mode == RankingMode::WaitForAssisted,
            request,
            mode,
            deadline,
            revision: 0,
        })
    }

    pub fn needs_inference(&self, now: Instant) -> bool {
        !self.frozen && now < self.deadline
    }

    pub fn stamp(&self) -> SelectionStamp {
        SelectionStamp {
            key: self.request.key,
            order: self.revision,
        }
    }

    /// Expiry permanently settles this round; late results cannot revive it.
    pub fn expire(&mut self, now: Instant) {
        if now >= self.deadline {
            self.freeze();
        }
    }

    /// Call before paging, positioning, pointer-down or selecting. While waiting,
    /// keyboard selection continues to use base identities without replaying keys.
    pub fn freeze(&mut self) {
        self.frozen = true;
        self.waiting = false;
    }

    /// An unavailable model immediately restores the base view in waiting mode.
    pub fn unavailable(&mut self) {
        self.freeze();
    }

    pub fn visible(&mut self, now: Instant) -> Vec<&ScoringCandidate> {
        self.expire(now);
        if self.waiting {
            return vec![];
        }
        self.order
            .iter()
            .map(|&i| &self.request.candidates[i])
            .collect()
    }

    pub fn select(
        &mut self,
        stamp: SelectionStamp,
        index: usize,
    ) -> Result<CandidateId, Unavailable> {
        if stamp != self.stamp() {
            return Err(Unavailable::Stale);
        }
        let selected = self
            .order
            .get(index)
            .copied()
            .ok_or(Unavailable::Candidates)?;
        self.freeze();
        Ok(self.request.candidates[selected].id)
    }

    pub fn apply(
        &mut self,
        response: &ScoreResponse,
        current: RequestKey,
        now: Instant,
        threshold: ScoreThreshold,
    ) -> Result<bool, Unavailable> {
        response.validate(&self.request, current)?;
        threshold.validate()?;
        self.expire(now);
        if self.frozen {
            return Ok(false);
        }
        let scores: HashMap<_, _> = response.scores.iter().map(|s| (s.id.0, s.value)).collect();
        let score = |index: usize| scores[&self.request.candidates[index].id.0];
        let start = usize::from(matches!(
            self.mode,
            RankingMode::BaseThenAssisted { pin_first: true }
        ));
        let mut order = self.order.clone();
        // Stable sort preserves original ordering for tied scores.
        order[start..].sort_by(|&a, &b| score(b).total_cmp(&score(a)));
        let ranked = &order[start..];
        let confident = ranked.first().is_some_and(|&best| {
            score(best) >= threshold.minimum
                && ranked
                    .get(1)
                    .is_none_or(|&next| score(best) - score(next) >= threshold.margin)
        });
        let changed = confident && self.order != order;
        if changed {
            self.order = order;
            self.revision = self.revision.wrapping_add(1);
        }
        self.freeze();
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assistance::CandidateScore, candidates::QueryRevision, session::EditToken};
    use std::time::Duration;
    fn request() -> ScoreRequest {
        ScoreRequest {
            protocol: 1,
            key: RequestKey {
                edit: EditToken {
                    session: 1,
                    generation: 2,
                    editor: 3,
                    epoch: 4,
                    runtime: 5,
                },
                query: QueryRevision {
                    engine: 1,
                    version: 2,
                },
                view: 1,
                context: 1,
                model: 1,
                config: 1,
                request: 1,
            },
            task: Task::Rank,
            language: "zh".into(),
            prefix: "下周去".into(),
            code: "xian".into(),
            candidates: [(9, "先"), (4, "西安"), (20, "仙")]
                .into_iter()
                .map(|(id, text)| ScoringCandidate {
                    id: CandidateId(id),
                    text: text.into(),
                })
                .collect(),
        }
    }
    fn response(r: &ScoreRequest) -> ScoreResponse {
        ScoreResponse {
            protocol: 1,
            key: r.key,
            scores: vec![
                CandidateScore {
                    id: CandidateId(20),
                    value: 3.,
                },
                CandidateScore {
                    id: CandidateId(4),
                    value: 2.,
                },
                CandidateScore {
                    id: CandidateId(9),
                    value: 1.,
                },
            ],
        }
    }
    const THRESHOLD: ScoreThreshold = ScoreThreshold {
        minimum: 0.,
        margin: 0.5,
    };
    #[test]
    fn all_modes_preserve_identity_and_waiting_selection_uses_base_without_replay() {
        let now = Instant::now();
        for (mode, ids) in [
            (RankingMode::Base, vec![9, 4, 20]),
            (
                RankingMode::BaseThenAssisted { pin_first: false },
                vec![20, 4, 9],
            ),
            (
                RankingMode::BaseThenAssisted { pin_first: true },
                vec![9, 20, 4],
            ),
            (RankingMode::WaitForAssisted, vec![20, 4, 9]),
        ] {
            let r = request();
            let result = response(&r);
            let mut round = RankingRound::new(r, mode, now + Duration::from_millis(100)).unwrap();
            assert_eq!(
                round.visible(now).is_empty(),
                mode == RankingMode::WaitForAssisted
            );
            round.apply(&result, result.key, now, THRESHOLD).unwrap();
            assert_eq!(
                round
                    .visible(now)
                    .iter()
                    .map(|c| c.id.0)
                    .collect::<Vec<_>>(),
                ids
            );
            assert_eq!(round.select(round.stamp(), 1).unwrap().0, ids[1]);
        }
        let r = request();
        let result = response(&r);
        let mut round = RankingRound::new(
            r,
            RankingMode::WaitForAssisted,
            now + Duration::from_millis(100),
        )
        .unwrap();
        assert_eq!(round.select(round.stamp(), 0).unwrap(), CandidateId(9));
        assert!(!round.apply(&result, result.key, now, THRESHOLD).unwrap());
    }
    #[test]
    fn timeout_failure_and_started_selection_permanently_freeze_order() {
        let now = Instant::now();
        for event in 0..3 {
            let r = request();
            let result = response(&r);
            let mut round = RankingRound::new(
                r,
                RankingMode::WaitForAssisted,
                now + Duration::from_millis(100),
            )
            .unwrap();
            match event {
                0 => round.expire(now + Duration::from_millis(100)),
                1 => round.unavailable(),
                _ => round.freeze(),
            }
            assert!(!round.apply(&result, result.key, now, THRESHOLD).unwrap());
            assert_eq!(round.visible(now)[0].id, CandidateId(9));
        }
    }
    #[test]
    fn stale_clicks_versions_bad_scores_and_low_confidence_cannot_change_selection() {
        let now = Instant::now();
        let r = request();
        let mut result = response(&r);
        let mut round = RankingRound::new(
            r,
            RankingMode::BaseThenAssisted { pin_first: false },
            now + Duration::from_secs(1),
        )
        .unwrap();
        let old = round.stamp();
        let mut wrong = result.key;
        wrong.edit.editor += 1;
        assert_eq!(
            round.apply(&result, wrong, now, THRESHOLD),
            Err(Unavailable::Stale)
        );
        result.scores[0].value = f32::NAN;
        assert_eq!(
            round.apply(&result, result.key, now, THRESHOLD),
            Err(Unavailable::NonFinite)
        );
        result.scores[0].value = 3.;
        assert!(round.apply(&result, result.key, now, THRESHOLD).unwrap());
        assert_eq!(round.select(old, 0), Err(Unavailable::Stale));
        assert_eq!(round.select(round.stamp(), 0).unwrap(), CandidateId(20));
        let r = request();
        let result = response(&r);
        let mut round = RankingRound::new(
            r,
            RankingMode::WaitForAssisted,
            now + Duration::from_secs(1),
        )
        .unwrap();
        assert!(!round
            .apply(
                &result,
                result.key,
                now,
                ScoreThreshold {
                    minimum: 10.,
                    margin: 0.
                }
            )
            .unwrap());
        assert_eq!(round.visible(now)[0].id, CandidateId(9));
    }
}
