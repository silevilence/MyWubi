package org.mywubi

import org.junit.Assert.*
import org.junit.Test

class KeyboardLayoutModelTest {
    @Test fun templatesPreserveCharactersAndNeverOverlap() {
        val layouts=listOf(BuiltinKeyboardLayouts.letters(),BuiltinKeyboardLayouts.numbers(false,"回车"),
            BuiltinKeyboardLayouts.numbers(true,"拨号"),BuiltinKeyboardLayouts.symbols(false),BuiltinKeyboardLayouts.symbols(true))
        for(layout in layouts) for((i,key) in layout.keys.withIndex()) {
            assertTrue(key.x>=0 && key.y>=0 && key.x+key.width<=layout.width+.001f && key.y+key.height<=layout.height)
            for(other in layout.keys.take(i)) assertFalse(key.x<other.x+other.width-.001f && other.x<key.x+key.width-.001f && key.y<other.y+other.height && other.y<key.y+key.height)
        }
        assertEquals(('a'..'z').toSet(),layouts[0].keys.filter { it.action.kind=="Character" }.map { it.action.value.single() }.toSet())
        val original=layouts[0].keys.first()
        assertEquals(original.action,original.copy(label="新标签").action)
        assertEquals(.5f,layouts[0].keys.first {it.id=="letter.a"}.x)
        assertEquals(1.5f,layouts[0].keys.first {it.id=="letter.z"}.x)
    }
}
