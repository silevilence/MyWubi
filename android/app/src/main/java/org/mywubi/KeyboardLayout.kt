package org.mywubi

/** Geometry and actions are independent of labels and decoration. Coordinates are logical units. */
data class LayoutAction(val kind: String, val value: String = "")
internal val LayoutAction.isSymbol: Boolean
    get() = kind == "InputSymbol" || (kind == "Character" && value.codePoints().noneMatch(Character::isLetter))
data class LayoutKey(val id: String, val label: String, val x: Float, val y: Float,
    val width: Float, val height: Float = 1f, val action: LayoutAction,
    val role: String = "key", val alternate: String? = null,
    val directions: Map<Int,LayoutAction> = emptyMap(),val hold: LayoutAction? = null,
    val groupCharacters: List<String> = emptyList())

/** Default first. A group tap remains ambiguous, while each picker entry is a precise input. */
internal fun LayoutKey.holdOptions(symbol: String?): List<LayoutAction> {
    val characters = buildList {
        if (action.kind == "Character") add(action)
        groupCharacters.forEach { add(LayoutAction("Character", it)) }
        addAll(directions.values.filter { it.kind == "Character" })
    }.distinct()
    val literal = symbol?.let { LayoutAction("InputSymbol", it) }
    val inputs = listOfNotNull(hold, literal, directions[0]?.takeIf { it.kind == "Character" }) + characters
    val default = inputs.firstOrNull { it.isSymbol } ?: inputs.firstOrNull()
    if (default == null || (hold == null && literal == null && characters == listOf(action))) return emptyList()
    return (listOf(default) + inputs).distinctBy {
        if (it.kind == "InputSymbol") LayoutAction("Character", it.value) else it
    }
}

data class KeyboardLayout(val id: String, val width: Float, val height: Float, val keys: List<LayoutKey>) {
    val isNineKeyGrid: Boolean
        get() = width == 3f && height == 3f && keys.size == 9 &&
            keys.all { it.width == 1f && it.height == 1f && it.x % 1f == 0f && it.y % 1f == 0f }

    companion object {
        /** Call on the loader thread. Only the shared validator can admit package geometry. */
        fun validated(id: String, source: org.json.JSONObject, mapping: org.json.JSONObject?, landscape: Boolean): KeyboardLayout {
            val response=NativeBridge.objectCall("ValidateKeyboardLayout","layout" to source,"mapping" to mapping)
            val checked=response.getJSONObject("layout")
            val layout=if(landscape) checked.optJSONObject("landscape") ?: checked else checked
            val bindings=if(landscape && checked.optJSONObject("landscape")!=null) response.getJSONObject("landscape_bindings") else response.getJSONObject("bindings")
            val keys=layout.getJSONArray("keys")
            return KeyboardLayout(id,layout.getDouble("width").toFloat(),layout.getDouble("height").toFloat(),
                (0 until keys.length()).map { index ->
                    val key=keys.getJSONObject(index);val action=key.getJSONObject("action")
                    fun parseAction(value: org.json.JSONObject)=LayoutAction(value.getString("kind"),value.optString("value"))
                    val directions=bindings.getJSONObject(key.getString("id"))
                    val primary = parseAction(action)
                    val hold = key.optJSONObject("long_press")?.let(::parseAction)
                    val swipes = directionIds.mapIndexedNotNull { direction,name ->
                        directions.optJSONObject(name)?.getJSONObject("action")?.let { direction to parseAction(it) }
                    }.toMap()
                    val groupCharacters = (listOfNotNull(primary, hold) + swipes.values)
                        .filter { it.kind == "Group" }.distinct().flatMap {
                            mapping!!.getJSONObject("groups").getString(it.value).codePoints().toArray()
                                .map { codePoint -> String(Character.toChars(codePoint)) }
                        }.distinct()
                    LayoutKey(key.getString("id"),key.getString("label"),key.getDouble("x").toFloat(),key.getDouble("y").toFloat(),
                        key.getDouble("width").toFloat(),key.getDouble("height").toFloat(),
                        primary,key.getString("role"),directions=swipes,hold=hold,groupCharacters=groupCharacters)
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
