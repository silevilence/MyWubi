//! MSAA bridge for the focusless raster window. Windows UI Automation uses its
//! legacy-accessibility proxy; actions carry the same rendered token as mouse clicks.
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::{Com::*, Ole::*, Variant::*},
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
};

pub(crate) struct Item {
    pub rect: RECT,
    pub token: u64,
    pub id: usize,
    pub name: String,
    pub selected: bool,
}

#[implement(IAccessible, IOleWindow)]
pub(crate) struct Accessible {
    pub hwnd: HWND,
    pub target: HWND,
    pub items: Vec<Item>,
    pub name: String,
}

fn integer(value: i32) -> VARIANT {
    let mut result = VARIANT::default();
    // This variant owns no allocated storage; the union arm is selected by VT_I4.
    unsafe {
        (*result.Anonymous.Anonymous).vt = VT_I4;
        (*result.Anonymous.Anonymous).Anonymous.lVal = value;
    }
    result
}
fn child(value: &VARIANT) -> Result<i32> {
    unsafe {
        if value.Anonymous.Anonymous.vt == VT_I4 {
            Ok(value.Anonymous.Anonymous.Anonymous.lVal)
        } else {
            Err(E_INVALIDARG.into())
        }
    }
}
impl Accessible_Impl {
    fn item(&self, value: &VARIANT) -> Result<&Item> {
        let index = child(value)?
            .checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| Error::from(E_INVALIDARG))?;
        self.items.get(index).ok_or_else(|| E_INVALIDARG.into())
    }
    fn bounds(&self, value: &VARIANT) -> Result<RECT> {
        let mut window = RECT::default();
        unsafe {
            GetWindowRect(self.hwnd, &mut window)?;
        }
        if child(value)? == 0 {
            return Ok(window);
        }
        let rect = self.item(value)?.rect;
        Ok(RECT {
            left: window.left + rect.left,
            top: window.top + rect.top,
            right: window.left + rect.right,
            bottom: window.top + rect.bottom,
        })
    }
}
impl IOleWindow_Impl for Accessible_Impl {
    fn GetWindow(&self) -> Result<HWND> {
        Ok(self.hwnd)
    }
    fn ContextSensitiveHelp(&self, _: BOOL) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
}
impl IDispatch_Impl for Accessible_Impl {
    fn GetTypeInfoCount(&self) -> Result<u32> {
        Ok(0)
    }
    fn GetTypeInfo(&self, _: u32, _: u32) -> Result<ITypeInfo> {
        Err(E_NOTIMPL.into())
    }
    fn GetIDsOfNames(
        &self,
        _: *const GUID,
        _: *const PCWSTR,
        _: u32,
        _: u32,
        _: *mut i32,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn Invoke(
        &self,
        _: i32,
        _: *const GUID,
        _: u32,
        _: DISPATCH_FLAGS,
        _: *const DISPPARAMS,
        _: *mut VARIANT,
        _: *mut EXCEPINFO,
        _: *mut u32,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
}
impl IAccessible_Impl for Accessible_Impl {
    fn accParent(&self) -> Result<IDispatch> {
        Err(E_NOTIMPL.into())
    }
    fn accChildCount(&self) -> Result<i32> {
        Ok(self.items.len() as i32)
    }
    fn get_accChild(&self, _: &VARIANT) -> Result<IDispatch> {
        Err(S_FALSE.into())
    }
    fn get_accName(&self, v: &VARIANT) -> Result<BSTR> {
        Ok(if child(v)? == 0 {
            self.name.as_str()
        } else {
            self.item(v)?.name.as_str()
        }
        .into())
    }
    fn get_accValue(&self, _: &VARIANT) -> Result<BSTR> {
        Ok(BSTR::new())
    }
    fn get_accDescription(&self, v: &VARIANT) -> Result<BSTR> {
        self.get_accName(v)
    }
    fn get_accRole(&self, v: &VARIANT) -> Result<VARIANT> {
        Ok(integer(if child(v)? == 0 {
            ROLE_SYSTEM_PANE
        } else {
            self.item(v)?;
            ROLE_SYSTEM_PUSHBUTTON
        } as i32))
    }
    fn get_accState(&self, v: &VARIANT) -> Result<VARIANT> {
        Ok(integer(if child(v)? != 0 && self.item(v)?.selected {
            STATE_SYSTEM_SELECTED as i32
        } else {
            0
        }))
    }
    fn get_accHelp(&self, _: &VARIANT) -> Result<BSTR> {
        Ok("方向键选择，回车上屏，Tab 搜索，Esc 关闭，PageUp/PageDown 翻页".into())
    }
    fn get_accHelpTopic(&self, _: *mut BSTR, _: &VARIANT) -> Result<i32> {
        Err(E_NOTIMPL.into())
    }
    fn get_accKeyboardShortcut(&self, _: &VARIANT) -> Result<BSTR> {
        Ok(BSTR::new())
    }
    fn accFocus(&self) -> Result<VARIANT> {
        Ok(integer(0))
    }
    fn accSelection(&self) -> Result<VARIANT> {
        Ok(integer(
            self.items
                .iter()
                .position(|i| i.selected)
                .map_or(0, |i| i as i32 + 1),
        ))
    }
    fn get_accDefaultAction(&self, v: &VARIANT) -> Result<BSTR> {
        if child(v)? == 0 {
            Ok(BSTR::new())
        } else {
            self.item(v)?;
            Ok("选择".into())
        }
    }
    fn accSelect(&self, _: i32, _: &VARIANT) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn accLocation(
        &self,
        x: *mut i32,
        y: *mut i32,
        w: *mut i32,
        h: *mut i32,
        v: &VARIANT,
    ) -> Result<()> {
        if x.is_null() || y.is_null() || w.is_null() || h.is_null() {
            return Err(E_POINTER.into());
        }
        let r = self.bounds(v)?;
        // COM supplies four writable out parameters; reject null before writing.
        unsafe {
            *x = r.left;
            *y = r.top;
            *w = r.right - r.left;
            *h = r.bottom - r.top;
        }
        Ok(())
    }
    fn accNavigate(&self, direction: i32, start: &VARIANT) -> Result<VARIANT> {
        let start = child(start)?;
        let next = match direction as u32 {
            NAVDIR_FIRSTCHILD => 1,
            NAVDIR_LASTCHILD => self.items.len() as i32,
            NAVDIR_NEXT => start + 1,
            NAVDIR_PREVIOUS => start - 1,
            _ => 0,
        };
        Ok(if next > 0 && next <= self.items.len() as i32 {
            integer(next)
        } else {
            VARIANT::default()
        })
    }
    fn accHitTest(&self, x: i32, y: i32) -> Result<VARIANT> {
        for index in 1..=self.items.len() {
            let r = self.bounds(&integer(index as i32))?;
            if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                return Ok(integer(index as i32));
            }
        }
        Ok(integer(0))
    }
    fn accDoDefaultAction(&self, v: &VARIANT) -> Result<()> {
        let item = self.item(v)?;
        if self.target.is_invalid() {
            return Err(E_FAIL.into());
        }
        unsafe {
            PostMessageW(
                Some(self.target),
                crate::candidate_dispatch::SELECT,
                WPARAM(item.token as usize),
                LPARAM(item.id as isize),
            )
        }
    }
    fn put_accName(&self, _: &VARIANT, _: &BSTR) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn put_accValue(&self, _: &VARIANT, _: &BSTR) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
}
