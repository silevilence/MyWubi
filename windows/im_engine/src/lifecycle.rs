//! 注册关系的所有权与失败重试；COM 调用始终在状态锁之外执行。
use crate::guids::GUID_PRESERVED_SHIFT;
use parking_lot::Mutex;
use windows::core::{IUnknown, Interface, Result};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::TextServices::*;

#[derive(Clone)]
pub(crate) enum Registration<C> {
    KeyEvent(u32),
    SourceKey(u32),
    ThreadEvent(u32),
    ThreadFocus(u32),
    TextEdit(C, u32),
    Compartment(u32),
    LanguageBar,
    PreservedKey(TF_PRESERVEDKEY),
}

impl<C> Registration<C> {
    fn retry_on_failure(&self) -> bool {
        matches!(self, Self::Compartment(_) | Self::LanguageBar)
    }

    fn priority(&self) -> u8 {
        match self {
            Self::Compartment(_) => 0,
            Self::LanguageBar => 1,
            _ => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Active,
    Cleaning,
    Pending,
}

struct State<M, O, C> {
    phase: Phase,
    manager: Option<M>,
    owner: Option<O>,
    tid: u32,
    records: Vec<Registration<C>>,
}

/// 同一生命周期实现供 Windows COM 与失败注入测试使用。
/// M/O/C 只替换不透明的资源句柄，不改变注册或重试策略。
pub(crate) struct Lifecycle<M, O, C> {
    state: Mutex<State<M, O, C>>,
}

impl<M, O, C> Default for Lifecycle<M, O, C> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                phase: Phase::Idle,
                manager: None,
                owner: None,
                tid: 0,
                records: Vec::new(),
            }),
        }
    }
}

impl<M: Clone, O: Clone, C: Clone> Lifecycle<M, O, C> {
    pub fn set_owner(&self, owner: O) {
        let old = self.state.lock().owner.replace(owner);
        drop(old);
    }

    pub fn owner(&self) -> Option<O> {
        self.state.lock().owner.clone()
    }
    pub fn manager(&self) -> Option<M> {
        self.state.lock().manager.clone()
    }
    pub fn tid(&self) -> u32 {
        self.state.lock().tid
    }

    /// 重复激活及清理未完成时，不覆盖任何注册记录。
    pub fn begin(&self, manager: M, tid: u32) -> bool {
        let mut state = self.state.lock();
        if state.phase != Phase::Idle || state.owner.is_none() {
            return false;
        }
        state.manager = Some(manager);
        state.tid = tid;
        state.phase = Phase::Active;
        true
    }

    pub fn record(&self, registration: Registration<C>) {
        let mut state = self.state.lock();
        debug_assert!(state.phase == Phase::Active);
        state.records.push(registration);
    }

    pub fn using_keystroke_mgr(&self) -> bool {
        self.state
            .lock()
            .records
            .iter()
            .any(|item| matches!(item, Registration::KeyEvent(_)))
    }

    pub fn has_language_bar(&self) -> bool {
        self.state
            .lock()
            .records
            .iter()
            .any(|item| matches!(item, Registration::LanguageBar))
    }

    #[cfg(test)]
    fn is_released(&self) -> bool {
        let state = self.state.lock();
        state.phase == Phase::Idle && state.owner.is_none() && state.manager.is_none()
    }

    /// 保留现有策略：语言栏/compartment 失败重试，其余订阅 best effort。
    /// 单项失败不短路；清理期间的重入不会重复退订或释放 owner。
    pub fn deactivate<E>(
        &self,
        mut remove: impl FnMut(&M, &O, &Registration<C>) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        let (manager, owner, mut records) = {
            let mut state = self.state.lock();
            if state.phase == Phase::Cleaning {
                return Ok(());
            }
            state.phase = Phase::Cleaning;
            (
                state.manager.clone(),
                state.owner.clone(),
                state.records.clone(),
            )
        };
        records.sort_by_key(Registration::priority);
        let mut retained = Vec::new();
        let mut first_error = None;
        if let (Some(manager), Some(owner)) = (manager.as_ref(), owner.as_ref()) {
            for record in records {
                if let Err(error) = remove(manager, owner, &record) {
                    if record.retry_on_failure() {
                        retained.push(record);
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
            }
        } else {
            // begin/record require the factory owner in production.
            debug_assert!(records.is_empty());
        }
        let released = {
            let mut state = self.state.lock();
            let records = std::mem::replace(&mut state.records, retained);
            if state.records.is_empty() {
                state.phase = Phase::Idle;
                state.tid = 0;
                (records, Some((state.manager.take(), state.owner.take())))
            } else {
                state.phase = Phase::Pending;
                (records, None)
            }
        };
        // Release 也可能重入，不允许在持有状态锁时释放最后一个 COM 引用。
        drop(released);
        first_error.map_or(Ok(()), Err)
    }

    /// 工厂 QueryInterface 失败且尚未激活时解除自身强引用。
    pub fn release_unactivated_owner(&self) {
        let owner = {
            let mut state = self.state.lock();
            if state.phase != Phase::Idle {
                return;
            }
            state.owner.take()
        };
        drop(owner);
    }
}

pub(crate) type TsfLifecycle = Lifecycle<ITfThreadMgr, IUnknown, ITfContext>;

/// Windows adapter：只执行一次具体退订；保留/重试策略在 Lifecycle 中。
pub(crate) fn remove_registration(
    tm: &ITfThreadMgr,
    owner: &IUnknown,
    registration: &Registration<ITfContext>,
) -> Result<()> {
    let result = match registration {
        Registration::KeyEvent(tid) => unsafe {
            tm.cast::<ITfKeystrokeMgr>()?.UnadviseKeyEventSink(*tid)
        },
        Registration::SourceKey(cookie)
        | Registration::ThreadEvent(cookie)
        | Registration::ThreadFocus(cookie) => unsafe {
            tm.cast::<ITfSource>()?.UnadviseSink(*cookie)
        },
        Registration::TextEdit(ctx, cookie) => unsafe {
            ctx.cast::<ITfSource>()?.UnadviseSink(*cookie)
        },
        Registration::Compartment(cookie) => {
            let compartment = unsafe {
                tm.cast::<ITfCompartmentMgr>()?
                    .GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)
            }?;
            unsafe { compartment.cast::<ITfSource>()?.UnadviseSink(*cookie) }
        }
        Registration::LanguageBar => {
            let manager: ITfLangBarItemMgr =
                unsafe { CoCreateInstance(&CLSID_TF_LangBarItemMgr, None, CLSCTX_INPROC_SERVER) }?;
            unsafe { manager.RemoveItem(&owner.cast::<ITfLangBarItem>()?) }
        }
        Registration::PreservedKey(key) => unsafe {
            tm.cast::<ITfKeystrokeMgr>()?
                .UnpreserveKey(&GUID_PRESERVED_SHIFT, key)
        },
    };
    if let Err(error) = &result {
        log::warn!("[TSF] 注销记录失败: {error}");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn active() -> Lifecycle<u32, Arc<()>, ()> {
        let lifecycle = Lifecycle::default();
        lifecycle.set_owner(Arc::new(()));
        assert!(lifecycle.begin(7, 9));
        lifecycle
    }

    #[test]
    fn partial_registration_cleans_only_successful_records() {
        let lifecycle = active();
        lifecycle.record(Registration::ThreadEvent(12));
        let mut removed = Vec::new();
        lifecycle
            .deactivate::<()>(|tm, _, record| {
                assert_eq!(*tm, 7);
                if let Registration::ThreadEvent(cookie) = record {
                    removed.push(*cookie);
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(removed, [12]);
        assert!(lifecycle.is_released());
    }

    #[test]
    fn failure_retains_owner_continues_cleanup_and_retries_only_pending() {
        let lifecycle = active();
        let owner = lifecycle.owner().unwrap();
        let weak = Arc::downgrade(&owner);
        drop(owner);
        lifecycle.record(Registration::ThreadEvent(12));
        lifecycle.record(Registration::LanguageBar);
        lifecycle.record(Registration::Compartment(0)); // cookie 0 is valid.
        let mut calls = Vec::new();
        let failure = lifecycle.deactivate(|_, _, record| {
            calls.push(record.priority());
            match record {
                Registration::Compartment(_) => Err("compartment"),
                Registration::LanguageBar => Err("language"),
                _ => Ok(()),
            }
        });
        assert_eq!(failure, Err("compartment"));
        assert_eq!(calls, [0, 1, 2]);
        assert!(weak.upgrade().is_some());
        assert!(!lifecycle.begin(99, 99));
        assert_eq!(lifecycle.tid(), 9);
        let mut retries = 0;
        lifecycle
            .deactivate::<()>(|_, _, _| {
                retries += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(retries, 2);
        assert!(lifecycle.is_released());
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn duplicate_activation_preserves_original_manager_and_records() {
        let lifecycle = active();
        lifecycle.record(Registration::KeyEvent(9));
        assert!(!lifecycle.begin(99, 99));
        assert_eq!(lifecycle.manager(), Some(7));
        assert!(lifecycle.using_keystroke_mgr());
    }

    #[test]
    fn cleanup_calls_can_reenter_without_deadlocking_or_dropping_owner() {
        let lifecycle = active();
        lifecycle.record(Registration::LanguageBar);
        lifecycle
            .deactivate::<()>(|_, _, _| {
                assert!(lifecycle.owner().is_some());
                assert!(!lifecycle.begin(2, 3));
                lifecycle
                    .deactivate::<()>(|_, _, _| panic!("duplicate cleanup"))
                    .unwrap();
                Ok(())
            })
            .unwrap();
        assert!(lifecycle.is_released());
    }

    #[test]
    fn best_effort_failure_does_not_prevent_final_release() {
        let lifecycle = active();
        lifecycle.record(Registration::ThreadFocus(1));
        lifecycle.deactivate(|_, _, _| Err("failure")).unwrap();
        assert!(lifecycle.is_released());
    }

    #[test]
    fn unactivated_owner_can_be_released_but_pending_owner_cannot() {
        let lifecycle: Lifecycle<u32, Arc<()>, ()> = Lifecycle::default();
        assert!(!lifecycle.begin(7, 9));
        lifecycle.set_owner(Arc::new(()));
        lifecycle.release_unactivated_owner();
        assert!(lifecycle.is_released());
        lifecycle.set_owner(Arc::new(()));
        assert!(lifecycle.begin(7, 9));
        lifecycle.record(Registration::LanguageBar);
        assert!(lifecycle.deactivate(|_, _, _| Err(())).is_err());
        lifecycle.release_unactivated_owner();
        assert!(lifecycle.owner().is_some());
        lifecycle.deactivate::<()>(|_, _, _| Ok(())).unwrap();
        assert!(lifecycle.is_released());
    }
}
