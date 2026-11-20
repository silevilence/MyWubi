//! Rebuildable samples preserve the context visible before the accepted edit.
use crate::{Change, EditKind, Event, Evidence, Outcome, Source};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub event: String,
    pub source: Source,
    pub session: String,
    pub sequence: u64,
    pub time_ms: u64,
    pub left: String,
    pub right: String,
    pub target: String,
    pub edit: String,
    pub weight: u32,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Excluded {
    pub event: String,
    pub reason: String,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Samples {
    pub version: u32,
    pub kept: Vec<Sample>,
    pub excluded: Vec<Excluded>,
}
/// Establish a total order without reversing events from a producer whose clock moved back.
pub fn chronological(mut events: Vec<Event>) -> Vec<Event> {
    events.sort_by(|a, b| a.source.cmp(&b.source).then(a.sequence.cmp(&b.sequence)));
    let mut previous = None;
    for event in &mut events {
        if let Some((source, time)) = &previous {
            if source == &event.source {
                event.time_ms = event.time_ms.max(u64::saturating_add(*time, 1));
            }
        }
        previous = Some((event.source.clone(), event.time_ms));
    }
    events.sort_by(|a, b| {
        a.time_ms
            .cmp(&b.time_ms)
            .then_with(|| a.source.cmp(&b.source))
            .then(a.sequence.cmp(&b.sequence))
    });
    events
}

pub fn build(events: impl IntoIterator<Item = Event>) -> Samples {
    let events = chronological(events.into_iter().collect());
    let withdrawn: BTreeSet<_> = events
        .iter()
        .filter_map(|event| match &event.change {
            Change::Edit {
                related: Some(id),
                operation: EditKind::Delete | EditKind::Replace | EditKind::Undo,
            } if event.evidence == Evidence::Observed => Some(id.clone()),
            _ => None,
        })
        .collect();
    let mut result = Samples {
        version: 1,
        ..Default::default()
    };
    let mut seen = BTreeSet::new();
    for event in events {
        let Change::Commit {
            text,
            source,
            outcome,
            ..
        } = &event.change
        else {
            continue;
        };
        let reason = if !seen.insert(event.id.clone()) {
            Some("duplicate")
        } else if *outcome != Outcome::Confirmed {
            Some("unconfirmed")
        } else if *source == Source::ExternalUnknown {
            Some("unknown_authorship")
        } else if withdrawn.contains(&event.id) {
            Some("corrected_or_withdrawn")
        } else if event.before.discontinuity
            || event.before.left.is_none()
            || event.before.right.is_none()
        {
            Some("missing_context")
        } else if text.is_empty() || text.chars().count() > 64 || text.chars().any(char::is_control)
        {
            Some("unsupported_text")
        } else {
            None
        };
        if let Some(reason) = reason {
            result.excluded.push(Excluded {
                event: event.id,
                reason: reason.into(),
            });
            continue;
        }
        let right = event.before.right.clone().unwrap_or_default();
        let replacement = event
            .before
            .selected
            .as_ref()
            .is_some_and(|s| !s.is_empty());
        result.kept.push(Sample {
            event: event.id,
            source: source.clone(),
            session: event.session,
            sequence: event.sequence,
            time_ms: event.time_ms,
            left: event.before.left.unwrap_or_default(),
            right: right.clone(),
            target: text.clone(),
            edit: if replacement {
                "replace"
            } else if !right.is_empty() {
                "insert"
            } else {
                "append"
            }
            .into(),
            weight: if matches!(source, Source::PersonalPrediction | Source::ModelPrediction) {
                1
            } else {
                2
            },
        });
    }
    result
}
