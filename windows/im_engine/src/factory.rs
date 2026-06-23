//! TSF 文本服务的 `IClassFactory` 实现。
//!
//! 当系统通过 `regsvr32` 把本 DLL 当作进程内 COM 服务器加载并调用
//! `CoCreateInstance(CLSID_TEXT_SERVICE, ...)` 时，会先调用本 DLL 导出的
//! `DllGetClassObject`，由其中返回本工厂；随后系统调用
//! `IClassFactory::CreateInstance` 申请 `ITfTextInputProcessor` 实例。
//!
//! 本工厂在创建实例时同步把一个对自身的 `IUnknown` 强引用回灌进 [`TextService`]
//! 的 `self_unknown` 字段，作为后续 `Activate` 中 `AdviseSink` 的 self 指针。

use windows::core::{implement, ComObject, Interface, Ref, Result, GUID, HRESULT};
use windows::Win32::Foundation::CLASS_E_NOAGGREGATION;
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};

use crate::text_service::TextService;

/// 全局 COM 工厂。本类型实现 `IClassFactory`，由 `DllGetClassObject` 单例返回。
#[implement(IClassFactory)]
pub struct TextServiceFactory;

impl IClassFactory_Impl for TextServiceFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<'_, windows::core::IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        // TSF TIP 不支持聚合：拒绝非空 punkOuter。
        if punkouter.as_ref().is_some() {
            return Err(HRESULT(CLASS_E_NOAGGREGATION.0).into());
        }

        let obj = ComObject::new({
            // 占位：在 DLL 验证环境下使用空码表 + 默认参数；真实激活流程由
            // global engine bubble 接管 StateMachine/Dictionary 的初始化，
            // 后续阶段会从 im_engine_init 处替换为加载过的字典。
            // 空码表条目从不触发解析错误，使用 expect 而非 ? 以避免错误类型差异。
            let dict = core_engine::Dictionary::from_entries(
                Vec::new(),
                None,
                Default::default(),
            )
            .expect("空码表构建不应失败");
            TextService::new(dict, 5, true)
        });

        // 在交还外部接口之前，把 box 自己的 IUnknown 副本写回 TextService，
        // 供 AdviseSink 使用 `IID_ITfKeyEventSink` 进行自注册。
        // 避免循环引用：TextService::Deactivate 中会 release_self_unknown()。
        let unk_outer: windows::core::IUnknown = obj.to_interface();
        {
            // Deref to &TextService and stash the IUnknown back-pointer clone.
            let srv: &TextService = &obj;
            srv.set_self_unknown(unk_outer.clone());
        }
        // 此处 refcount：1（obj 自身）+ 1（self_unknown 字段）= 2。
        drop(unk_outer);

        // 调用 `Interface::cast` 对请求的 riid 进行 QueryInterface；
        // 这种“按 IID 动态查询”契合 DllGetClassObject 流程。
        let iid = unsafe { &*riid };
        let unk_for_qi: windows::core::IUnknown = obj.to_interface();
        let mut raw: *mut core::ffi::c_void = std::ptr::null_mut();
        let hr = unsafe { Interface::query(&unk_for_qi, iid, &mut raw) };
        if hr.is_ok() && !raw.is_null() {
            // QueryInterface 已 AddRef；调用方负责 Release。
            unsafe { *ppvobject = raw; }
            Ok(())
        } else {
            log::error!("[TSF] CreateInstance: QueryInterface({iid:?}) failed hr={hr:?}");
            unsafe { *ppvobject = std::ptr::null_mut(); }
            Err(HRESULT(-1).into())
        }
    }

    fn LockServer(&self, flock: windows::core::BOOL) -> Result<()> {
        // TSF 进程内服务器由系统按引用计数自动管理；
        // 此处仅为兼容性占位，不做实际磁盘锁定。
        let _ = flock;
        Ok(())
    }
}