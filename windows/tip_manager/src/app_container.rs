//! Minimal AppContainer access for TSF code and its active data files.
use std::path::Path;

/// Grant read access to a data file, or read/traverse access to a directory.
/// Existing permissions are preserved; no permissions inherit to children.
pub fn grant_read(path: &Path) -> Result<(), String> {
    grant_access(path, path.is_dir())
}

fn grant_access(path: &Path, executable: bool) -> Result<(), String> {
    #[cfg(windows)]
    return windows_acl::grant_access(path, executable)
        .map_err(|error| format!("无法设置搜索输入权限 {}: {error}", path.display()));
    #[cfg(not(windows))]
    {
        let _ = (path, executable);
        Ok(())
    }
}

/// Prepare the DLL and its immediate directory for AppContainer TSF hosts.
/// Call before registration and after the updater replaces application files.
pub fn prepare_tip(dll_path: &Path) -> Result<(), String> {
    if !dll_path.is_absolute() || !dll_path.is_file() {
        return Err(format!("输入法 DLL 路径无效: {}", dll_path.display()));
    }
    let directory = dll_path
        .parent()
        .ok_or_else(|| "无法定位 DLL 目录".to_string())?;
    grant_access(directory, true)?;
    grant_access(dll_path, true)
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

    pub(super) fn grant_access(path: &Path, executable: bool) -> Result<()> {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // DLL 必须可执行，目录必须可遍历；数据文件仅允许读取。
        let access = FILE_GENERIC_READ.0
            | if executable {
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
        use super::super::grant_read;
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
        fn tip_dll_and_directory_allow_loading_without_exposing_siblings() {
            let dir = tempfile::tempdir().unwrap();
            let dll = dir.path().join("im_engine.dll");
            let private = dir.path().join("private.txt");
            std::fs::write(&dll, b"test DLL placeholder").unwrap();
            std::fs::write(&private, b"private").unwrap();
            let sids = [w!("S-1-15-2-1"), w!("S-1-15-2-2")];
            let private_before = sids.map(|sid| rights(&private, sid));
            let owner_before = rights(&dll, w!("S-1-5-18"));
            super::super::prepare_tip(&dll).unwrap();
            super::super::prepare_tip(&dll).unwrap();
            for (i, sid) in sids.into_iter().enumerate() {
                let required = FILE_GENERIC_READ.0 | FILE_GENERIC_EXECUTE.0;
                for path in [&dll, dir.path()] {
                    let allowed = rights(path, sid);
                    assert_eq!(allowed & required, required, "{} lacks RX", path.display());
                    assert_eq!(allowed & (FILE_GENERIC_WRITE.0 & !required), 0);
                }
                assert_eq!(rights(&private, sid), private_before[i]);
            }
            assert_eq!(rights(&dll, w!("S-1-5-18")), owner_before);
            let new_file = dir.path().join("new-private.txt");
            std::fs::write(&new_file, b"private").unwrap();
            assert_eq!(rights(&new_file, sids[0]), private_before[0]);
        }

        #[test]
        fn replaced_tip_dll_regains_execute_access() {
            let dir = tempfile::tempdir().unwrap();
            let dll = dir.path().join("im_engine.dll");
            std::fs::write(&dll, b"old version").unwrap();
            super::super::prepare_tip(&dll).unwrap();
            let replacement = dir.path().join("replacement.dll");
            std::fs::write(&replacement, b"new version").unwrap();
            std::fs::rename(&replacement, &dll).unwrap();
            for sid in [w!("S-1-15-2-1"), w!("S-1-15-2-2")] {
                assert_eq!(rights(&dll, sid) & 0x20, 0, "replacement has a fresh ACL");
            }
            super::super::prepare_tip(&dll).unwrap();
            for sid in [w!("S-1-15-2-1"), w!("S-1-15-2-2")] {
                let required = FILE_GENERIC_READ.0 | FILE_GENERIC_EXECUTE.0;
                assert_eq!(rights(&dll, sid) & required, required);
            }
        }

        #[test]
        fn invalid_tip_path_does_not_change_directory_permissions() {
            let dir = tempfile::tempdir().unwrap();
            let before = rights(dir.path(), w!("S-1-15-2-1"));
            assert!(super::super::prepare_tip(&dir.path().join("missing.dll")).is_err());
            assert!(super::super::prepare_tip(dir.path()).is_err());
            assert!(super::super::prepare_tip(Path::new("relative.dll")).is_err());
            assert_eq!(rights(dir.path(), w!("S-1-15-2-1")), before);
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
