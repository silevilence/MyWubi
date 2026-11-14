//! Eight-sector bindings shared by the editor and the Android layout resolver.
use super::{keyboard::KeyAction, Error};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
}

pub const ALL: [Direction; 8] = [
    Direction::Up,
    Direction::UpRight,
    Direction::Right,
    Direction::DownRight,
    Direction::Down,
    Direction::DownLeft,
    Direction::Left,
    Direction::UpLeft,
];
pub const SLIDE_DP: f32 = 24.0;
pub const CANCEL_DP: f32 = 12.0;
pub const HYSTERESIS_DEGREES: f32 = 5.0;

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::UpRight => "up_right",
            Self::Right => "right",
            Self::DownRight => "down_right",
            Self::Down => "down",
            Self::DownLeft => "down_left",
            Self::Left => "left",
            Self::UpLeft => "up_left",
        }
    }
    pub fn parse(name: &str) -> Result<Self, Error> {
        ALL.into_iter()
            .find(|d| d.name() == name)
            .ok_or_else(|| Error::Invalid(format!("未知方向：{name}")))
    }
}

/// Coordinates are density-independent, positive y points down. Boundary ties turn clockwise.
pub fn classify(dx: f32, dy: f32, previous: Option<Direction>) -> Option<Direction> {
    if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) < CANCEL_DP {
        return None;
    }
    let angle = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
    if let Some(previous) = previous {
        let center = previous as u8 as f32 * 45.0;
        let distance = ((angle - center + 180.0).rem_euclid(360.0) - 180.0).abs();
        if distance < 22.5 + HYSTERESIS_DEGREES {
            return Some(previous);
        }
    }
    Some(ALL[((angle + 22.5) / 45.0).floor() as usize % 8])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub group: String,
    pub action: KeyAction,
    pub inherited: bool,
}

fn expand(
    groups: &BTreeMap<String, KeyAction>,
    inherited: bool,
) -> Result<BTreeMap<Direction, Binding>, Error> {
    let mut result = BTreeMap::new();
    for (group, action) in groups {
        for name in group.split('+') {
            let direction = Direction::parse(name)?;
            if result
                .insert(
                    direction,
                    Binding {
                        group: group.clone(),
                        action: action.clone(),
                        inherited,
                    },
                )
                .is_some()
            {
                return Err(Error::Invalid(format!(
                    "方向 {name} 重复绑定于动作组 {group}"
                )));
            }
        }
    }
    Ok(result)
}

/// A key replaces inherited sectors individually; disabling and rebinding one sector is an error.
pub fn resolve(
    defaults: &BTreeMap<String, KeyAction>,
    overrides: &BTreeMap<String, KeyAction>,
    disabled: &[String],
) -> Result<BTreeMap<Direction, Binding>, Error> {
    let mut result = expand(defaults, true)?;
    let overrides = expand(overrides, false)?;
    let mut seen = BTreeSet::new();
    for name in disabled {
        let direction = Direction::parse(name)?;
        if !seen.insert(direction) || overrides.contains_key(&direction) {
            return Err(Error::Invalid(format!("方向 {name} 禁用与覆盖冲突")));
        }
        result.remove(&direction);
    }
    result.extend(overrides);
    Ok(result)
}

#[derive(Debug, Clone, Copy)]
pub enum Preset {
    Exact,
    WideUp,
    WideDown,
    WideSides,
}
pub fn preset(kind: Preset, actions: [KeyAction; 8]) -> BTreeMap<String, KeyAction> {
    let groups: &[&str] = match kind {
        Preset::Exact => &[
            "up",
            "up_right",
            "right",
            "down_right",
            "down",
            "down_left",
            "left",
            "up_left",
        ],
        Preset::WideUp => &[
            "up_left+up+up_right",
            "right",
            "down_right",
            "down",
            "down_left",
            "left",
        ],
        Preset::WideDown => &[
            "up",
            "up_right",
            "right",
            "down_right+down+down_left",
            "left",
            "up_left",
        ],
        Preset::WideSides => &[
            "up",
            "up_right+right+down_right",
            "down",
            "down_left+left+up_left",
        ],
    };
    groups
        .iter()
        .zip(actions)
        .map(|(name, action)| (name.to_string(), action))
        .collect()
}

/// Rewrite selected sectors without erasing unselected members of existing merged groups.
pub fn replace(groups:&mut BTreeMap<String,KeyAction>,selected:&BTreeSet<Direction>,action:Option<KeyAction>)->Result<(),Error> {
    let mut next=BTreeMap::new();
    for (name,value) in groups.iter() {
        let remaining=name.split('+').map(Direction::parse).collect::<Result<Vec<_>,_>>()?.into_iter()
            .filter(|d|!selected.contains(d)).map(Direction::name).collect::<Vec<_>>().join("+");
        if !remaining.is_empty(){next.insert(remaining,value.clone());}
    }
    if let Some(action)=action {if !selected.is_empty(){next.insert(selected.iter().map(|d|d.name()).collect::<Vec<_>>().join("+"),action);}}
    expand(&next,false)?;*groups=next;Ok(())
}

#[derive(Clone,Copy,PartialEq,Eq)]
enum Phase {Tracking,Sliding,Holding,Cancelled,Finished}
/// Same release-only contract as Android's DirectionalGesture, in density-independent units.
pub struct Gesture {phase:Phase,pub direction:Option<Direction>,bound:BTreeSet<Direction>,can_hold:bool}
impl Gesture {
    pub fn new(bound:BTreeSet<Direction>,can_hold:bool)->Self {Self{phase:Phase::Tracking,direction:None,bound,can_hold}}
    pub fn cancel(&mut self){self.phase=Phase::Cancelled;self.direction=None;}
    pub fn hold(&mut self){if self.phase==Phase::Tracking && self.can_hold {self.phase=Phase::Holding;}}
    pub fn move_to(&mut self,dx:f32,dy:f32) {
        if matches!(self.phase,Phase::Cancelled|Phase::Finished){return;}
        let distance=dx.hypot(dy);
        if !distance.is_finite() || distance>160. {self.cancel();return;}
        if self.phase==Phase::Holding || (self.phase==Phase::Tracking && distance<SLIDE_DP){return;}
        if distance<CANCEL_DP {self.cancel();return;}
        self.phase=Phase::Sliding;self.direction=classify(dx,dy,self.direction);
    }
    /// -1: click, -2: hold, 0..7: clockwise sector, None: no action.
    pub fn release(&mut self)->Option<i8> {
        let result=match self.phase {Phase::Tracking=>Some(-1),Phase::Holding=>Some(-2),
            Phase::Sliding=>self.direction.filter(|d|self.bound.contains(d)).map(|d|d as i8),_=>None};
        self.phase=Phase::Finished;result
    }
}
