package org.mywubi

/** Geometry and actions are independent of labels and decoration. Coordinates are logical units. */
data class LayoutAction(val kind: String, val value: String = "")
data class LayoutKey(val id: String, val label: String, val x: Float, val y: Float,
    val width: Float, val height: Float = 1f, val action: LayoutAction,
    val role: String = "key", val alternate: String? = null)
data class KeyboardLayout(val id: String, val width: Float, val height: Float, val keys: List<LayoutKey>) {
    companion object {
        /** Call on the loader thread. Only the shared validator can admit package geometry. */
        fun validated(id: String, source: org.json.JSONObject, mapping: org.json.JSONObject?, landscape: Boolean): KeyboardLayout {
            val checked=NativeBridge.objectCall("ValidateKeyboardLayout","layout" to source,"mapping" to mapping).getJSONObject("layout")
            val layout=if(landscape) checked.optJSONObject("landscape") ?: checked else checked
            val keys=layout.getJSONArray("keys")
            return KeyboardLayout(id,layout.getDouble("width").toFloat(),layout.getDouble("height").toFloat(),
                (0 until keys.length()).map { index ->
                    val key=keys.getJSONObject(index);val action=key.getJSONObject("action")
                    LayoutKey(key.getString("id"),key.getString("label"),key.getDouble("x").toFloat(),key.getDouble("y").toFloat(),
                        key.getDouble("width").toFloat(),key.getDouble("height").toFloat(),
                        LayoutAction(action.getString("kind"),action.optString("value")),key.getString("role"))
                })
        }
    }
}

/** These templates preserve the original 26-key geometry, including half-column staggering. */
object BuiltinKeyboardLayouts {
    fun letters(): KeyboardLayout {
        val keys = mutableListOf<LayoutKey>()
        listOf("qwertyuiop", "asdfghjkl", "zxcvbnm").forEachIndexed { row, letters ->
            val start = when(row) { 1 -> .5f; 2 -> 1.5f; else -> 0f }
            letters.forEachIndexed { column, c ->
                keys += LayoutKey("letter.$c", c.toString(), start+column, row.toFloat(),1f,
                    action=LayoutAction("Character",c.toString()))
            }
        }
        keys += LayoutKey("shift","⇧",0f,2f,1.5f,action=LayoutAction("Shift"),role="function")
        keys += LayoutKey("backspace","⌫",8.5f,2f,1.5f,action=LayoutAction("Backspace"),role="function")
        return KeyboardLayout("builtin.26",10f,3f,keys)
    }
    fun numbers(phone: Boolean, enterLabel: String): KeyboardLayout {
        val keys=mutableListOf<LayoutKey>()
        (if(phone) listOf("+","*","#",",") else listOf("+","-","/","=")).forEachIndexed { row,label ->
            keys += LayoutKey("left.$row",label,0f,row.toFloat(),1f,action=LayoutAction("Character",label),role="function")
        }
        listOf("123","456","789",".0,").forEachIndexed { row,values ->
            values.forEachIndexed { column,c -> keys += LayoutKey("number.$row.$column",c.toString(),1f+column*1.5f,row.toFloat(),1.5f,action=LayoutAction("Character",c.toString())) }
        }
        listOf("⌫" to "Backspace","ABC" to "Letters","符号" to "Symbols",enterLabel to "Enter").forEachIndexed { row,(label,action) ->
            keys += LayoutKey("right.$row",label,5.5f,row.toFloat(),1.5f,action=LayoutAction(action),role=if(action=="Enter") "selected" else "function")
        }
        return KeyboardLayout(if(phone) "builtin.phone" else "builtin.numbers",7f,4f,keys)
    }
    fun symbols(ascii: Boolean): KeyboardLayout {
        val rows=if(ascii) listOf("!@#$%^&*", "()[]{}<>", "-_=+\\|/~", "`'\":;?,.")
            else listOf("，。！？、：；", "（）【】《》“”", "@#￥%&*+", "-_=/'\"~")
        return KeyboardLayout("builtin.symbols.$ascii",8f,4f,rows.flatMapIndexed { row,values ->
            values.mapIndexed { column,c -> LayoutKey("symbol.$row.$column",c.toString(),column*8f/values.length,row.toFloat(),8f/values.length,
                action=LayoutAction("Character",c.toString())) }
        })
    }
}
