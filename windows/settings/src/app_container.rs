//! 让搜索等 AppContainer 宿主只读访问输入法正在使用的数据。
//!
//! 不给容器写权限，不向兄弟文件继承；原子替换文件后须重新调用。
use std::path::Path;

pub(crate) fn prepare(config_path: &Path, config: &core_engine::Config) -> Result<(), String> {
    let system = core_engine::config_path::resolve_resource_path(
        config_path,
        &config.dictionary.system_table,
    );
    let user =
        core_engine::config_path::resolve_resource_path(config_path, &config.dictionary.user_table);
    for path in [config_path, system.as_path(), user.as_path()] {
        // 用户词库尚未创建时仍需允许监视其目录。
        for target in path.parent().into_iter().chain(Some(path)) {
            if target.exists() {
                grant_read(target).map_err(|error| {
                    format!("无法设置搜索输入权限 {}: {error}", target.display())
                })?;
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn grant_read(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn grant_read(path: &Path) -> Result<(), String> {
    windows_acl::grant_read(path).map_err(|error| error.to_string())
}

#[cfg(windows)]
mod windows_acl {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::core::{w, Result, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::*;
    use windows::Win32::Security::{ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};
    use windows::Win32::Storage::FileSystem::{FILE_GENERIC_EXECUTE, FILE_GENERIC_READ};

    struct LocalAllocation(*mut std::ffi::c_void);
    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0)));
            }
        }
    }

    pub(super) fn grant_read(path: &Path) -> Result<()> {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // 目录需要列举/遍历以供 notify 监视；文件只有读权限。
        let access = FILE_GENERIC_READ.0
            | if path.is_dir() {
                FILE_GENERIC_EXECUTE.0
            } else {
                0
            };
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            let mut old_acl = std::ptr::null_mut::<ACL>();
            GetNamedSecurityInfoW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(&mut old_acl),
                None,
                &mut descriptor,
            )
            .ok()?;
            let _descriptor = LocalAllocation(descriptor.0);
            // NULL DACL 已允许所有访问，不能将其替换成只有容器 ACE 的 ACL。
            if old_acl.is_null() {
                return Ok(());
            }
            let mut sid = PSID::default();
            let mut restricted = PSID::default();
            ConvertStringSidToSidW(w!("S-1-15-2-1"), &mut sid)?;
            let _sid = LocalAllocation(sid.0);
            ConvertStringSidToSidW(w!("S-1-15-2-2"), &mut restricted)?;
            let _restricted = LocalAllocation(restricted.0);
            let entries = [sid, restricted].map(|sid| EXPLICIT_ACCESS_W {
                grfAccessPermissions: access,
                grfAccessMode: GRANT_ACCESS,
                grfInheritance: Default::default(),
                Trustee: TRUSTEE_W {
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                    ptstrName: PWSTR(sid.0.cast()),
                    ..Default::default()
                },
            });
            // 已有权限时不重写 DACL，避免每次启动触发无意义的热重载。
            let sufficient = entries.iter().all(|entry| {
                let mut rights = 0;
                GetEffectiveRightsFromAclW(old_acl, &entry.Trustee, &mut rights).is_ok()
                    && rights & access == access
            });
            if sufficient {
                return Ok(());
            }
            let mut new_acl = std::ptr::null_mut();
            SetEntriesInAclW(Some(&entries), Some(old_acl), &mut new_acl).ok()?;
            let _acl = LocalAllocation(new_acl.cast());
            SetNamedSecurityInfoW(
                PCWSTR(name.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(new_acl),
                None,
            )
            .ok()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;

        fn rights(path: &Path, sid: PCWSTR) -> u32 {
            let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                let mut descriptor = PSECURITY_DESCRIPTOR::default();
                let mut acl = std::ptr::null_mut();
                GetNamedSecurityInfoW(
                    PCWSTR(name.as_ptr()),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    None,
                    None,
                    Some(&mut acl),
                    None,
                    &mut descriptor,
                )
                .ok()
                .unwrap();
                let _descriptor = LocalAllocation(descriptor.0);
                let mut sid_ptr = PSID::default();
                ConvertStringSidToSidW(sid, &mut sid_ptr).unwrap();
                let _sid = LocalAllocation(sid_ptr.0);
                let trustee = TRUSTEE_W {
                    TrusteeForm: TRUSTEE_IS_SID,
                    ptstrName: PWSTR(sid_ptr.0.cast()),
                    ..Default::default()
                };
                let mut rights = 0;
                GetEffectiveRightsFromAclW(acl, &trustee, &mut rights)
                    .ok()
                    .unwrap();
                rights
            }
        }

        #[test]
        fn grants_read_without_inheritance_or_removing_existing_access() {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("config.toml");
            let sibling = dir.path().join("private.txt");
            std::fs::write(&file, "configuration").unwrap();
            std::fs::write(&sibling, "private").unwrap();
            let original = rights(&file, w!("S-1-5-18")); // SYSTEM
            let sibling_before = rights(&sibling, w!("S-1-15-2-1"));
            grant_read(dir.path()).unwrap();
            grant_read(&file).unwrap();
            grant_read(&file).unwrap();
            for sid in [w!("S-1-15-2-1"), w!("S-1-15-2-2")] {
                let allowed = rights(&file, sid);
                assert_eq!(allowed & FILE_GENERIC_READ.0, FILE_GENERIC_READ.0);
                // WRITE_DATA / APPEND_DATA / WRITE_EA / WRITE_ATTRIBUTES
                assert_eq!(allowed & (FILE_GENERIC_WRITE.0 & !FILE_GENERIC_READ.0), 0);
            }
            assert_eq!(rights(&file, w!("S-1-5-18")), original);
            assert_eq!(rights(&sibling, w!("S-1-15-2-1")), sibling_before);
            let replacement = dir.path().join("config.tmp");
            std::fs::write(&replacement, "replacement").unwrap();
            std::fs::rename(&replacement, &file).unwrap();
            grant_read(&file).unwrap();
            assert_eq!(
                rights(&file, w!("S-1-15-2-1")) & FILE_GENERIC_READ.0,
                FILE_GENERIC_READ.0
            );
        }
    }
}
