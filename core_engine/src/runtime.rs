//! Immutable runtime construction and publication. No filesystem work in input dispatch.
use crate::{BehaviorProfile, Config, Dictionary, StateMachine, TemporaryTables, UserDictionary};
use arc_swap::ArcSwap;
use parking_lot::Mutex;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

/// A consistent resource set. Mutable input buffers belong to sessions.
#[derive(Debug)]
pub struct RuntimeSnapshot {
    pub revision: u64,
    pub dict: Arc<Dictionary>,
    pub temporary: Arc<TemporaryTables>,
    pub config: Config,
    pub config_path: PathBuf,
    pub system_table_path: PathBuf,
    pub user_table_path: PathBuf,
}

impl RuntimeSnapshot {
    pub fn state_machine(&self) -> StateMachine {
        let options = self.config.engine_options();
        let mut sm = StateMachine::with_full_behavior(
            Arc::clone(&self.dict),
            self.config.basic.candidate_count as usize,
            options.auto_commit_unique,
            self.config.basic.punctuation_mode,
            self.dict.table_config().max_code_len as usize,
            options.commit_on_max_code_overflow,
        );
        sm.set_temporary_tables(Arc::clone(&self.temporary));
        sm
    }
    pub fn dependencies(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            self.config_path.clone(),
            self.system_table_path.clone(),
            self.user_table_path.clone(),
        ];
        paths.extend(self.temporary.paths.iter().cloned());
        paths
    }
    /// Legacy construction tolerates auxiliary errors so the watcher can inspect new references.
    pub fn initial_from_config(path: PathBuf, config: Config) -> Result<Self, String> {
        RuntimeBuilder::build(path, config, BehaviorProfile::LegacyV1, LoadPolicy::Startup)
            .map_err(|error| error.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadPolicy {
    Startup,
    Reload,
}

#[derive(Debug)]
pub struct BuildError {
    pub message: String,
    pub dependencies: Vec<PathBuf>,
}

/// Build is separate from publication and session adoption, and can run on a loader thread.
pub struct RuntimeBuilder;
impl RuntimeBuilder {
    pub fn build(
        config_path: PathBuf,
        config: Config,
        profile: BehaviorProfile,
        policy: LoadPolicy,
    ) -> Result<RuntimeSnapshot, BuildError> {
        let system_table_path = crate::config_path::resolve_resource_path(
            &config_path,
            &config.dictionary.system_table,
        );
        let user_table_path =
            crate::config_path::resolve_resource_path(&config_path, &config.dictionary.user_table);
        let mut dependencies = vec![
            config_path.clone(),
            system_table_path.clone(),
            user_table_path.clone(),
        ];
        config
            .validate_for(crate::config::ValidationContext::ActiveResources)
            .map_err(|e| BuildError {
                message: e.to_string(),
                dependencies: dependencies.clone(),
            })?;
        let main = Dictionary::load(&system_table_path).map_err(|e| BuildError {
            message: e.to_string(),
            dependencies: dependencies.clone(),
        })?;
        let temporary = Arc::new(TemporaryTables::load_for_profile(
            &main,
            &system_table_path,
            &config.hotkey,
            profile,
        ));
        dependencies.extend(temporary.paths.iter().cloned());
        if policy == LoadPolicy::Reload && !temporary.load_errors.is_empty() {
            return Err(BuildError {
                message: temporary.load_errors.join("; "),
                dependencies,
            });
        }
        let dict =
            merge_dictionary(main, &user_table_path, &config).map_err(|message| BuildError {
                message,
                dependencies,
            })?;
        Ok(RuntimeSnapshot {
            revision: NEXT_REVISION.fetch_add(1, Ordering::Relaxed),
            dict,
            temporary,
            config,
            config_path,
            system_table_path,
            user_table_path,
        })
    }

    /// Windows startup fallback preserves the old retry-main-then-empty policy.
    pub fn legacy_startup(path: PathBuf) -> RuntimeSnapshot {
        let config = Config::load(&path).unwrap_or_default();
        match Self::build(
            path.clone(),
            config.clone(),
            BehaviorProfile::LegacyV1,
            LoadPolicy::Startup,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                log::error!("{}", error.message);
                let system_table_path = crate::config_path::resolve_resource_path(
                    &path,
                    &config.dictionary.system_table,
                );
                let user_table_path =
                    crate::config_path::resolve_resource_path(&path, &config.dictionary.user_table);
                let dict = load_dictionary(&system_table_path, &user_table_path, &config)
                    .unwrap_or_else(|error| {
                        log::error!("{error}");
                        Dictionary::empty()
                    });
                RuntimeSnapshot {
                    revision: NEXT_REVISION.fetch_add(1, Ordering::Relaxed),
                    dict,
                    temporary: Arc::default(),
                    config,
                    config_path: path,
                    system_table_path,
                    user_table_path,
                }
            }
        }
    }
}

pub fn load_dictionary(
    system: &Path,
    user: &Path,
    config: &Config,
) -> Result<Arc<Dictionary>, String> {
    let main = Dictionary::load(system)
        .map_err(|e| format!("加载码表失败 ({}): {e}", system.display()))?;
    merge_dictionary(main, user, config)
}

/// User (code, word) pairs override weights; reverse lookup is built before this merge.
pub fn merge_dictionary(
    system: Arc<Dictionary>,
    user_path: &Path,
    config: &Config,
) -> Result<Arc<Dictionary>, String> {
    if !config.dictionary.enable_user_dict {
        return Ok(system);
    }
    let user = UserDictionary::load(user_path)
        .map_err(|e| format!("加载用户词库失败 ({}): {e}", user_path.display()))?;
    let keys: HashSet<(&str, &str)> = user
        .entries()
        .iter()
        .map(|e| (e.code.as_str(), e.word.as_str()))
        .collect();
    let mut entries = user.entries().to_vec();
    entries.extend(
        system
            .entries()
            .iter()
            .filter(|e| !keys.contains(&(e.code.as_str(), e.word.as_str())))
            .cloned(),
    );
    system
        .rebuild_with_entries(entries, Default::default())
        .map_err(|e| format!("合并用户词库失败: {e}"))
}

/// Reserve request order before background work. Failed newer requests also invalidate older work.
pub struct RuntimeStore {
    current: ArcSwap<RuntimeSnapshot>,
    request: Mutex<u64>,
}
impl RuntimeStore {
    pub fn new(initial: RuntimeSnapshot) -> Self {
        Self {
            current: ArcSwap::from_pointee(initial),
            request: Mutex::new(0),
        }
    }
    pub fn request(&self) -> u64 {
        let mut n = self.request.lock();
        *n += 1;
        *n
    }
    pub fn snapshot(&self) -> Arc<RuntimeSnapshot> {
        self.current.load_full()
    }
    pub fn publish(&self, request: u64, snapshot: RuntimeSnapshot) -> bool {
        let latest = self.request.lock();
        if request != *latest || !snapshot.temporary.load_errors.is_empty() {
            return false;
        }
        self.current.store(Arc::new(snapshot));
        true
    }
}
