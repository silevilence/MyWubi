//! Inspect actual shaped glyph runs, including system font fallback.
use crate::TextLayout;
use std::{cell::RefCell, collections::BTreeSet, ffi::c_void};
use windows::{
    core::*,
    Win32::{Foundation::E_POINTER, Graphics::DirectWrite::*},
};

#[derive(Debug, Clone, Default)]
pub struct Coverage {
    pub missing_glyphs: usize,
    pub visible_glyphs: usize,
    pub families: BTreeSet<String>,
}

impl Coverage {
    /// Each Emoji entry is one sequence. Multiple ink-bearing glyphs indicate that
    /// the system decomposed it. This conservative check is not a visual-quality claim.
    pub fn needs_fallback(&self, emoji_sequence: bool) -> bool {
        self.missing_glyphs > 0 || (emoji_sequence && self.visible_glyphs != 1)
    }
}

pub fn inspect(layout: &TextLayout) -> Result<Coverage> {
    let probe = ComObject::new(Probe {
        result: RefCell::default(),
    });
    unsafe {
        layout
            .raw
            .Draw(None, &probe.to_interface::<IDWriteTextRenderer>(), 0.0, 0.0)?;
    }
    let result = probe.result.borrow().clone();
    Ok(result)
}

#[implement(IDWriteTextRenderer, IDWritePixelSnapping)]
struct Probe {
    result: RefCell<Coverage>,
}

impl IDWritePixelSnapping_Impl for Probe_Impl {
    fn IsPixelSnappingDisabled(&self, _: *const c_void) -> Result<BOOL> {
        Ok(true.into())
    }
    fn GetPixelsPerDip(&self, _: *const c_void) -> Result<f32> {
        Ok(1.0)
    }
    fn GetCurrentTransform(&self, _: *const c_void, transform: *mut DWRITE_MATRIX) -> Result<()> {
        if transform.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe {
            *transform = DWRITE_MATRIX {
                m11: 1.0,
                m22: 1.0,
                ..Default::default()
            };
        }
        Ok(())
    }
}

impl IDWriteTextRenderer_Impl for Probe_Impl {
    fn DrawGlyphRun(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: DWRITE_MEASURING_MODE,
        run: *const DWRITE_GLYPH_RUN,
        _: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        if run.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        // DirectWrite owns these buffers for the duration of this callback.
        let run = unsafe { &*run };
        if run.glyphCount == 0 {
            return Ok(());
        }
        if run.glyphIndices.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        let glyphs =
            unsafe { std::slice::from_raw_parts(run.glyphIndices, run.glyphCount as usize) };
        let mut result = self.result.borrow_mut();
        result.missing_glyphs += glyphs.iter().filter(|g| **g == 0).count();
        if let Some(face) = run.fontFace.as_ref() {
            let mut metrics = vec![DWRITE_GLYPH_METRICS::default(); glyphs.len()];
            unsafe {
                face.GetDesignGlyphMetrics(
                    glyphs.as_ptr(),
                    run.glyphCount,
                    metrics.as_mut_ptr(),
                    run.isSideways.as_bool(),
                )?;
            }
            result.visible_glyphs += metrics
                .iter()
                .filter(|m| {
                    i64::from(m.advanceWidth)
                        - i64::from(m.leftSideBearing)
                        - i64::from(m.rightSideBearing)
                        > 0
                        && i64::from(m.advanceHeight)
                            - i64::from(m.topSideBearing)
                            - i64::from(m.bottomSideBearing)
                            > 0
                })
                .count();
            if let Ok(face) = face.cast::<IDWriteFontFace3>() {
                unsafe {
                    if let Ok(names) = face.GetFamilyNames() {
                        if names.GetCount() > 0 {
                            let mut name = vec![0; names.GetStringLength(0)? as usize + 1];
                            names.GetString(0, &mut name)?;
                            name.pop();
                            result.families.insert(String::from_utf16_lossy(&name));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn DrawUnderline(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_UNDERLINE,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
    fn DrawStrikethrough(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_STRIKETHROUGH,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
    fn DrawInlineObject(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: Ref<IDWriteInlineObject>,
        _: BOOL,
        _: BOOL,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
}
