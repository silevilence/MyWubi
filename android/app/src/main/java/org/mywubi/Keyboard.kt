package org.mywubi

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.BorderStroke
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.platform.ViewConfiguration
import androidx.compose.ui.unit.DpSize
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import org.json.JSONArray
import org.json.JSONObject

data class CandidateItem(val id: Long, val text: String, val hint: String, val expressionKind: String? = null, val dateTime: Boolean = false) {
    val spokenHint: String get() = if(expressionKind == null && !dateTime && hint.isNotEmpty()) "编码提示 $hint" else hint
}
data class ExpressionQueryUi(val kind: String, val phase: String, val text: String, val message: String?, val input: String = "Words")

private val LocalKeyboardKeyHeight = staticCompositionLocalOf { 54.dp }
data class KeyboardModel(
    val expression: ExpressionQueryUi? = null,
    val emojiAvailable: Boolean = false, val kaomojiAvailable: Boolean = false,
    val emojiReason: String? = null, val kaomojiReason: String? = null,
    val specialCapabilities: Map<String, Pair<Boolean, String?>> = emptyMap(),
    val clipboard: String? = null,
    val prediction: Boolean = false,
    val epoch: Long = 0, val enabled: Boolean = false, val spelling: String = "",
    val status: String = "正在加载码表…", val statusError: Boolean = false,
    val base: String = "Main", val temporary: String? = null,
    val pinyinAvailable: Boolean = false, val modesAvailable: Boolean = false, val candidates: List<CandidateItem> = emptyList(),
    val query: String = "{}", val total: Int = 0, val offset: Int = 0,
    val layout: String = "letters", val enterLabel: String = "回车", val privateInput: Boolean = false,
    val idle: Boolean = false, val updateAvailable: Boolean = false,
    val pinyinMode: KeyMode = KeyMode.Off
)

/** The IME owns this lifecycle; no Activity lifecycle is assumed by Compose. */
class ImeViewOwner : LifecycleOwner, SavedStateRegistryOwner {
    private val registry = LifecycleRegistry(this)
    private val saved = SavedStateRegistryController.create(this)
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry: SavedStateRegistry get() = saved.savedStateRegistry
    init { saved.performAttach(); saved.performRestore(null); registry.currentState = Lifecycle.State.CREATED }
    fun visible(value: Boolean) { registry.currentState = if(value) Lifecycle.State.RESUMED else Lifecycle.State.CREATED }
    fun destroy() { registry.currentState = Lifecycle.State.DESTROYED }
}

@Composable
@OptIn(ExperimentalLayoutApi::class)
fun Keyboard(model: KeyboardModel, dispatch: (Any) -> Unit, window: (Int) -> Unit,
             openSettings: () -> Unit = {}, onTouch: (Boolean) -> Unit = {},
             hideKeyboard: () -> Unit = {}, switchIme: () -> Unit) {
    val currentTouch by rememberUpdatedState(onTouch)
    val landscape = LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val keyHeight = if (landscape) 48.dp else 54.dp
    var shift by remember(model.epoch) { mutableStateOf(KeyMode.Off) }
    val pinyinLabel = when (model.pinyinMode) {
        KeyMode.Locked -> "拼音 · 锁定"
        KeyMode.Once -> "拼音 · 本次"
        KeyMode.Off -> if (model.temporary == "Pinyin") "拼音 · 本次" else "拼音"
    }
    val cyclePinyin = { dispatch("CyclePinyin") }
    val pinyinMode = if (model.pinyinMode == KeyMode.Off && model.temporary == "Pinyin") KeyMode.Once else model.pinyinMode
    val preview = remember(model.epoch) { KeyPreviewState() }
    var origin by remember { mutableStateOf(Offset.Zero) }
    var keyboardWidth by remember { mutableFloatStateOf(0f) }
    var panel by remember(model.epoch, model.layout) {
        mutableStateOf(if (model.layout in listOf("number", "phone")) "numbers" else "letters")
    }
    var expanded by remember(model.epoch) { mutableStateOf(false) }
    var specialMenu by remember(model.epoch) { mutableStateOf(false) }
    val showExpanded = expanded && model.candidates.isNotEmpty() && !model.privateInput
    var asciiSymbols by remember(model.epoch, model.base, model.privateInput, model.layout) {
        mutableStateOf(model.privateInput || model.base == "English" || model.layout != "letters")
    }
    // Composition and predictions own the toolbar slot until the input is cleared.
    val showCandidateBar = !model.privateInput && (model.spelling.isNotEmpty() || model.candidates.isNotEmpty())
    val showToolbar = !showCandidateBar && model.expression == null
    // Reserve scaled text space in both states, including an empty candidate list.
    val barHeight = maxOf(48.dp, (28 * LocalDensity.current.fontScale + 16).dp)
    LaunchedEffect(model.spelling) { if (model.spelling.isEmpty()) expanded = false }
    val colors = myWubiColors()
    val baseEnabled = model.enabled && model.modesAvailable && !model.privateInput && model.expression == null
    val baseLabel = if (model.base == "Main") "中 → 英" else "英 → 中"
    val toggleBase = { dispatch(JSONObject().put("SetBaseMode", if (model.base == "Main") "English" else "Main")) }
    fun input(label: String) { dispatch(JSONObject().put("InputText", JSONArray().put(label).put("Normal"))) }
    fun choose(item: CandidateItem) {
        dispatch(JSONObject().put("SelectCandidate", JSONArray().put(JSONObject(model.query)).put(item.id)))
        expanded = false
    }
    MaterialTheme(colorScheme=colors) {
        val pinyinControl: @Composable () -> Unit = {
            PinyinButton(pinyinMode, pinyinLabel, baseEnabled && model.pinyinAvailable, cyclePinyin)
        }
        val controls: @Composable () -> Unit = {
            model.expression?.let { query ->
                val results = query.phase == "Results"
                val name = when(query.kind) { "emoji" -> "Emoji"; "kaomoji" -> "颜文字"; "kana" -> "假名"; "math" -> "数学符号"; "punctuation" -> "标点"; "symbols" -> "其他符号"; else -> "特殊输入" }
                FlowRow(Modifier.fillMaxWidth().heightIn(min=48.dp).padding(horizontal=10.dp), horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                    Text("$name · ${if(results) "选择结果" else if(query.input == "Literal") "字面别名" else "主码表组词"}", Modifier.padding(vertical=14.dp), fontSize=14.sp)
                    TextButton(onClick={expanded=false; dispatch(if(results) "EditExpressionQuery" else "FindExpressions")}, enabled=model.enabled) {
                        Text(if(results) "修改关键词" else "查找")
                    }
                    TextButton(onClick={expanded=false; dispatch("ClearExpressionQuery")}, enabled=model.enabled) { Text("清空") }
                    if(!results) TextButton(onClick={
                        dispatch(JSONObject().put("SetExpressionInput", if(query.input == "Literal") "Words" else "Literal"))
                    }, enabled=model.enabled && query.text.isEmpty() && model.spelling.isEmpty()) {
                        Text(if(query.input == "Literal") "切换为主码表组词" else "切换为字面别名")
                    }
                    TextButton(onClick={expanded=false; dispatch("EndTemporary")}, enabled=model.enabled) { Text("取消") }
                }
                if(!results && (query.text.isNotEmpty() || model.spelling.isNotEmpty())) Text("切换输入方式前请先清空查询", Modifier.padding(horizontal=10.dp), fontSize=12.sp)
                Text("关键词：${query.text.ifEmpty { "尚未输入" }}" + if(model.spelling.isEmpty()) "" else "　编码：${model.spelling}",
                    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal=10.dp, vertical=4.dp),
                    color=colors.primary, fontSize=16.sp, maxLines=1)
                val notice = if(model.statusError) model.status else query.message
                if(!notice.isNullOrBlank()) Text(notice, Modifier.fillMaxWidth().padding(horizontal=10.dp), fontSize=12.sp, color=colors.onSurfaceVariant)
            }
            // Reserve the same single text line while idle, composing or showing predictions.
            // Keep actions in the toolbar so their 48 dp touch targets do not enlarge this line.
            val showSpelling = model.spelling.isNotEmpty() && !model.privateInput
            val showClipboard = model.clipboard != null && model.expression == null && model.spelling.isEmpty() && model.candidates.isEmpty() && !model.privateInput
            if (showExpanded) {
                Row(Modifier.fillMaxWidth().heightIn(min=48.dp), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    val heading = if (showSpelling) model.spelling else if (model.prediction) "字词预测" else "全部候选"
                    if (landscape) {
                        Text("$heading · ${model.total} 个候选", Modifier.weight(1f).padding(horizontal=12.dp),
                            color=colors.primary, fontSize=16.sp, maxLines=1,
                            overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                    } else Column(Modifier.weight(1f).padding(horizontal=12.dp)) {
                        Text(heading,
                            color=colors.primary, fontSize=16.sp, maxLines=1,
                            overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                        Text("${model.total} 个候选", color=colors.onSurfaceVariant, fontSize=12.sp, maxLines=1)
                    }
                    ToolbarIconButton(R.drawable.ic_collapse_candidates, "收起") { expanded=false }
                    IconButton(onClick=hideKeyboard,
                        modifier=Modifier.size(48.dp).imeAction("收起键盘", true, hideKeyboard)) {
                        Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_hide_keyboard), null, Modifier.size(24.dp), tint=colors.onSurface)
                    }
                }
            } else {
                if (model.expression == null) Text(if (showSpelling) model.spelling else "",
                    Modifier.fillMaxWidth().heightIn(min=28.dp).padding(horizontal=12.dp, vertical=4.dp)
                        .then(if (showSpelling) Modifier else Modifier.clearAndSetSemantics {}),
                    color=colors.primary, fontSize=16.sp, lineHeight=20.sp, maxLines=1,
                    overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                Row(Modifier.fillMaxWidth().heightIn(min=barHeight), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    Row(Modifier.weight(1f), horizontalArrangement=Arrangement.SpaceEvenly,
                        verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                        if (showClipboard) {
                            TextButton(onClick = { dispatch("PasteClipboard") }, enabled = model.enabled,
                                modifier = Modifier.weight(1f).heightIn(min = 48.dp).imeAction("粘贴剪贴板内容", model.enabled) { dispatch("PasteClipboard") }) {
                                Text("粘贴：${model.clipboard}", maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                            }
                            ToolbarIconButton(R.drawable.ic_close, "关闭剪贴板建议") { dispatch("DismissClipboard") }
                        } else if (model.candidates.isNotEmpty() && !model.privateInput) {
                            if (model.prediction) Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_prediction),
                                "字词预测", Modifier.padding(start=8.dp).size(18.dp), tint=colors.primary)
                            LazyRow(Modifier.weight(1f).heightIn(min=48.dp)) {
                                items(model.candidates, key={"${model.query}:${it.id}"}) { item -> CandidateButton(item, model.enabled) {choose(item)} }
                            }
                            ToolbarIconButton(R.drawable.ic_expand_candidates, "展开") { expanded=true }
                        } else {
                            if (model.privateInput || model.statusError || !model.enabled) Box(Modifier.width(24.dp),
                                contentAlignment=androidx.compose.ui.Alignment.Center) {
                                when {
                                    model.privateInput -> Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_mode_lock),
                                        "安全输入", Modifier.size(18.dp), tint=colors.onSurface)
                                    model.statusError -> Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_input_error),
                                        model.status, Modifier.size(20.dp), tint=MaterialTheme.colorScheme.error)
                                    !model.enabled -> CircularProgressIndicator(Modifier.size(18.dp)
                                        .clearAndSetSemantics { contentDescription = model.status }, strokeWidth=2.dp)
                                }
                            }
                            if (showToolbar) {
                                val showUpdate = model.idle && model.updateAvailable
                                val label = if (showUpdate) "设置，有可用更新" else "设置"
                                ToolbarIconButton(R.drawable.ic_settings, label,
                                    badge={ if (showUpdate) Badge() }, action=openSettings)
                                Box {
                                    ToolbarIconButton(R.drawable.ic_emoji, "特殊输入", enabled=baseEnabled) { specialMenu=true }
                                    DropdownMenu(expanded=specialMenu && baseEnabled, onDismissRequest={specialMenu=false}) {
                                        for((mode, title) in listOf("Emoji" to "Emoji", "Kaomoji" to "颜文字", "Kana" to "假名", "Math" to "数学符号", "Punctuation" to "标点", "Symbols" to "其他符号")) {
                                            val capability = model.specialCapabilities[mode] ?: when(mode) {
                                                "Emoji" -> model.emojiAvailable to model.emojiReason
                                                "Kaomoji" -> model.kaomojiAvailable to model.kaomojiReason
                                                else -> false to "词库未加载"
                                            }
                                            DropdownMenuItem(text={Column {
                                                Text(title)
                                                if(!capability.first) Text(capability.second ?: "不可用", fontSize=12.sp)
                                            }}, enabled=capability.first, onClick={
                                                specialMenu=false
                                                dispatch(JSONObject().put("BeginTemporary", mode))
                                            })
                                        }
                                    }
                                }
                                ToolbarIconButton(R.drawable.ic_switch_ime, "切换输入法", action=switchIme)
                                pinyinControl()
                            } else {
                                Spacer(Modifier.weight(1f))
                            }
                        }
                    }
                    IconButton(onClick=hideKeyboard,
                        modifier=Modifier.size(48.dp).imeAction("收起键盘", true, hideKeyboard)) {
                        Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_hide_keyboard), null,
                            Modifier.size(24.dp), tint=colors.onSurface)
                    }
                }
            }
        }
        val keys: @Composable () -> Unit = {
            if (showExpanded) {
                val hasPages = model.offset > 0 || model.offset + model.candidates.size < model.total
                val showHints = model.candidates.any { it.hint.isNotEmpty() }
                // Spend the former spelling-strip space on the grid; retain the typing area's height.
                val gridHeight = keyHeight * 3 + 28.dp - (if (hasPages) 48.dp else 0.dp)
                key(model.query, model.offset) {
                    val columnWidth = (112 * LocalDensity.current.fontScale.coerceIn(1f, 1.5f)).dp
                    LazyVerticalGrid(columns=GridCells.Adaptive(columnWidth),
                        modifier=Modifier.fillMaxWidth().height(gridHeight)
                            .semantics { contentDescription = "全部候选" }) {
                        items(model.candidates, key={it.id}, span={item ->
                            GridItemSpan(if (item.expressionKind != null || item.text.codePointCount(0, item.text.length) > 4) maxLineSpan else 1)
                        }) { item -> ExpandedCandidateButton(item,model.enabled,showHints) {choose(item)} }
                    }
                }
                if (hasPages) Row(Modifier.fillMaxWidth().heightIn(min=48.dp), horizontalArrangement=Arrangement.SpaceBetween,
                    verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    val previous = { window((model.offset-256).coerceAtLeast(0)) }
                    val next = { window(model.offset+256) }
                    val canNext = model.offset+model.candidates.size<model.total
                    TextButton(onClick=previous,enabled=model.offset>0,
                        modifier=Modifier.imeAction("上一组",model.offset>0,previous)) {Text("上一组", maxLines=1)}
                    Text("${model.offset+1}–${model.offset+model.candidates.size} / ${model.total}",
                        Modifier.weight(1f), fontSize=12.sp, maxLines=1, textAlign=androidx.compose.ui.text.style.TextAlign.Center)
                    TextButton(onClick=next,enabled=canNext,
                        modifier=Modifier.imeAction("下一组",canNext,next)) {Text("下一组", maxLines=1)}
                }
            } else when (panel) {
                "letters" -> {
                    BoxWithConstraints(Modifier.fillMaxWidth()) {
                        val unit = maxWidth / 10
                        Column {
                            for ((index, row) in listOf("qwertyuiop", "asdfghjkl", "zxcvbnm").withIndex()) {
                                Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.Center) {
                                    if (index == 2) Key(if(shift == KeyMode.Locked) "⇪" else "⇧",model.enabled,model.epoch,Modifier.width(unit*1.5f),
                                        description=when(shift) { KeyMode.Off -> "小写"; KeyMode.Once -> "大写 · 本次"; KeyMode.Locked -> "大写 · 锁定" },
                                        function=true, selected=shift != KeyMode.Off) {shift=shift.next()}
                                    for (character in row) {
                                        val value = if(shift != KeyMode.Off) character.uppercaseChar().toString() else character.toString()
                                        val symbol = letterSymbol(character, !model.privateInput && model.layout == "letters" &&
                                            (model.base == "Main" || model.temporary == "Pinyin"))
                                        Key(value,model.enabled,model.epoch,Modifier.width(unit), alternate=symbol,
                                            previewEnabled=!model.privateInput,
                                            alternateAction={dispatch(JSONObject().put("InputSymbol", symbol))}) {input(value); shift=shift.consumed()}
                                    }
                                    if (index == 2) Key("⌫",model.enabled,model.epoch,Modifier.width(unit*1.5f),
                                        repeat=true,description="退格",function=true,clearAction={dispatch("ClearEditor")}) {dispatch("Backspace")}
                                }
                            }
                        }
                    }
                }
                "numbers" -> {
                    Row(Modifier.fillMaxWidth()) {
                        Column(Modifier.weight(1f)) {
                            for (label in if(model.layout=="phone") listOf("+", "*", "#", ",") else listOf("+", "-", "/", "=")) {
                                Key(label,model.enabled,model.epoch,Modifier.fillMaxWidth(),function=true,previewEnabled=!model.privateInput) {input(label)}
                            }
                        }
                        Column(Modifier.weight(4.5f)) {
                            for (row in listOf(listOf("1","2","3"),listOf("4","5","6"),listOf("7","8","9"),listOf(".","0",","))) {
                                Row { for(label in row) Key(label,model.enabled,model.epoch,Modifier.weight(1f),previewEnabled=!model.privateInput) {input(label)} }
                            }
                        }
                        Column(Modifier.weight(1.5f)) {
                            Key("⌫",model.enabled,model.epoch,Modifier.fillMaxWidth(),repeat=true,description="退格",function=true,clearAction={dispatch("ClearEditor")}) {dispatch("Backspace")}
                            Key("ABC",model.enabled,model.epoch,Modifier.fillMaxWidth(),function=true) {panel="letters"}
                            Key("符号",model.enabled,model.epoch,Modifier.fillMaxWidth(),function=true) {panel="symbols"}
                            Key(model.enterLabel,model.enabled,model.epoch,Modifier.fillMaxWidth(),accent=true) {dispatch("Enter")}
                        }
                    }
                }
                else -> {
                    val rows = if (asciiSymbols) listOf("!@#$%^&*", "()[]{}<>", "-_=+\\|/~", "`'\":;?,.")
                        else listOf("，。！？、：；", "（）【】《》“”", "@#￥%&*+", "-_=/'\"~")
                    for (row in rows) {
                        Row(Modifier.fillMaxWidth()) {
                            for (character in row) Key(character.toString(),model.enabled,model.epoch,Modifier.weight(1f),previewEnabled=!model.privateInput) {input(character.toString())}
                        }
                    }
                }
            }
            if (panel != "numbers" || showExpanded) Row(Modifier.fillMaxWidth()) {
                val showBackspace = showExpanded || panel == "symbols"
                Key(if(panel=="letters") "123" else "ABC",model.enabled,model.epoch,Modifier.weight(1.3f),function=true) {
                    panel=if(panel=="letters") "numbers" else "letters"; expanded=false
                }
                Key(if(showBackspace) "⌫" else "符",model.enabled,model.epoch,Modifier.weight(1f),
                    repeat=showBackspace,description=if(showBackspace) "退格" else "符号",function=true,clearAction={dispatch("ClearEditor")}) {
                    if(showBackspace) dispatch("Backspace") else {panel="symbols";expanded=false}
                }
                val punctuation = when(model.layout) { "email" -> "@"; "url" -> "/"; else -> if(model.base=="Main" && !model.privateInput) "，" else "," }
                if (panel == "symbols") {
                    Key(if(asciiSymbols) "英符" else "中符",model.enabled,model.epoch,Modifier.weight(1.3f),
                        description=if(asciiSymbols) "切换为中文符号" else "切换为英文符号",function=true) {asciiSymbols=!asciiSymbols}
                } else Key(punctuation,model.enabled,model.epoch,Modifier.weight(0.9f),previewEnabled=!model.privateInput) {input(punctuation)}
                Key("空格",model.enabled,model.epoch,Modifier.weight(3.4f)) {dispatch("Space")}
                Key(if(model.base=="Main" && model.layout=="letters" && !model.privateInput) "。" else ".",model.enabled,model.epoch,Modifier.weight(0.9f),previewEnabled=!model.privateInput) {
                    input(if(model.base=="Main" && model.layout=="letters" && !model.privateInput) "。" else ".")
                }
                Key(if(model.base=="Main") "中" else "英",baseEnabled,model.epoch,Modifier.weight(1.3f),description=baseLabel,function=true) {toggleBase()}
                Key(model.enterLabel,model.enabled,model.epoch,Modifier.weight(1.7f),accent=true) {dispatch("Enter")}
            }
        }
        val density = LocalDensity.current
        // Some IME hosts consume navigation insets before Compose receives them.
        // Keep a dedicated bottom band even then, clear of the OEM's IME buttons.
        val bottom = with(density) {
            maxOf(WindowInsets.navigationBarsIgnoringVisibility.getBottom(this),
                WindowInsets.mandatorySystemGestures.getBottom(this)).toDp().coerceAtLeast(32.dp)
        }
        val container=Modifier.fillMaxWidth().background(colors.surface)
            .windowInsetsPadding(WindowInsets.navigationBars.union(WindowInsets.displayCutout).only(WindowInsetsSides.Horizontal))
            .padding(start=3.dp, end=3.dp, top=4.dp, bottom=bottom)
            .pointerInput(Unit) {
                var pressed = false
                try {
                    awaitPointerEventScope {
                        while (true) {
                            val event = awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Initial)
                            val next = event.changes.any { it.pressed }
                            if (next != pressed) { pressed = next; currentTouch(next) }
                        }
                    }
                } finally { if (pressed) currentTouch(false) }
            }
        CompositionLocalProvider(LocalKeyPreview provides preview, LocalKeyboardKeyHeight provides keyHeight) {
            Box(Modifier.fillMaxWidth().onGloballyPositioned { origin=it.positionInRoot(); keyboardWidth=it.size.width.toFloat() }) {
                BoxWithConstraints(container, contentAlignment=androidx.compose.ui.Alignment.TopCenter) {
                    // Keep candidates above every panel in either orientation. Capping the
                    // shared width avoids stretching keys and moving controls on expansion.
                    if (landscape && model.expression != null) {
                        // Query controls must not consume the typing area's vertical budget.
                        // On short screens the left pane scrolls; all four key rows remain visible.
                        val paneHeight=minOf(maxHeight,keyHeight*4+28.dp)
                        Row(Modifier.widthIn(max=1100.dp).fillMaxWidth()) {
                            Column(Modifier.weight(0.36f).height(paneHeight)
                                .verticalScroll(rememberScrollState()).semantics { contentDescription="表情查询与操作" }) { controls() }
                            Column(Modifier.weight(0.64f)) { keys() }
                        }
                    } else {
                        Column(Modifier.widthIn(max=600.dp).fillMaxWidth()) {
                            controls()
                            keys()
                        }
                    }
                }
                KeyPreviewBubble(preview.current, origin, keyboardWidth)
            }
        }
    }
}

@Composable
private fun ToolbarIconButton(icon: Int, description: String, enabled: Boolean = true,
                              selected: Boolean = false, badge: @Composable () -> Unit = {}, action: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    IconButton(onClick=action, enabled=enabled,
        modifier=Modifier.size(48.dp)
            .background(if(selected) colors.secondaryContainer else androidx.compose.ui.graphics.Color.Transparent,
                androidx.compose.foundation.shape.CircleShape)
            .imeAction(description, enabled, action),
        colors=IconButtonDefaults.iconButtonColors(contentColor=if(selected) colors.primary else colors.onSurfaceVariant)) {
        BadgedBox(badge={ badge() }) {
            Icon(androidx.compose.ui.res.painterResource(icon), null, Modifier.size(24.dp))
        }
    }
}

@Composable
private fun PinyinButton(mode: KeyMode, description: String, enabled: Boolean, action: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    ToolbarIconButton(R.drawable.ic_pinyin, description, enabled, selected=mode != KeyMode.Off,
        badge={
            if (mode == KeyMode.Locked) Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_mode_lock), null,
                Modifier.size(10.dp), tint=colors.primary)
            else if (mode == KeyMode.Once) Box(Modifier.size(5.dp)
                .background(colors.primary, androidx.compose.foundation.shape.CircleShape))
        }, action=action)
}

@Composable
private fun CandidateButton(item: CandidateItem, enabled: Boolean, choose: () -> Unit) {
    val display = remember(item) { ExpressionDisplay.display(item.text, item.hint, item.expressionKind) }
    val description = "候选 ${item.text}" + (if(item.hint.isEmpty()) "" else "，${item.spokenHint}") +
        (if(display.fallback) "，字体不支持完整显示，选择后输入原始表情" else "")
    TextButton(onClick=choose,enabled=enabled,modifier=Modifier.heightIn(min=48.dp).imeAction(description,enabled,choose)) {
        Text(display.text,fontSize=19.sp,softWrap=false,
            style=androidx.compose.ui.text.TextStyle(platformStyle=androidx.compose.ui.text.PlatformTextStyle(includeFontPadding=true)))
        if(item.hint.isNotEmpty() && !display.fallback) Text(" ${item.hint}",fontSize=11.sp,softWrap=false,color=MaterialTheme.colorScheme.primary)
    }
}

@Composable
private fun ExpandedCandidateButton(item: CandidateItem, enabled: Boolean, showHints: Boolean, choose: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val display = remember(item) { ExpressionDisplay.display(item.text, item.hint, item.expressionKind) }
    val description = "候选 ${item.text}" + (if(item.hint.isEmpty()) "" else "，${item.spokenHint}") +
        (if(display.fallback) "，字体不支持完整显示，选择后输入原始表情" else "")
    Surface(onClick=choose, enabled=enabled, shape=RectangleShape, color=colors.surface,
        contentColor=if(enabled) colors.onSurface else colors.onSurface.copy(alpha=0.38f),
        border=BorderStroke(0.5.dp, colors.onSurface.copy(alpha=0.22f)),
        modifier=Modifier.fillMaxWidth().heightIn(min=72.dp).horizontalScroll(rememberScrollState()).imeAction(description,enabled,choose)) {
        Column(Modifier.padding(horizontal=8.dp, vertical=12.dp),
            verticalArrangement=Arrangement.Center, horizontalAlignment=androidx.compose.ui.Alignment.CenterHorizontally) {
            // The whole surface scrolls, including its hint and padding. A drag over
            // any part of a long candidate must cancel the surface's click gesture.
            Box(Modifier.padding(vertical=4.dp)) {
                Text(display.text, fontSize=20.sp, softWrap=false,
                    style=androidx.compose.ui.text.TextStyle(platformStyle=androidx.compose.ui.text.PlatformTextStyle(includeFontPadding=true)))
            }
            // Reserve the same hint line in every cell, and let text measure its own height.
            // This keeps rows aligned even with Android's nonlinear font scaling.
            if(showHints && !display.fallback) Text(item.hint.ifEmpty { " " }, Modifier.padding(top=4.dp), fontSize=12.sp,
                color=colors.onSurfaceVariant)
        }
    }
}

@Composable
private fun Key(label:String,enabled:Boolean,epoch:Long,modifier:Modifier,repeat:Boolean=false,
                description:String=label,function:Boolean=false,accent:Boolean=false,selected:Boolean=false,
                alternate:String?=null,previewEnabled:Boolean=false,alternateAction:()->Unit={},clearAction:()->Unit={},action:()->Unit) {
    // The entire column is touchable. KeyTouch draws the inset keycap inside it.
    val keyModifier=modifier.height(LocalKeyboardKeyHeight.current)
    val colors=MaterialTheme.colorScheme
    val background=when {accent || selected -> colors.primary; function -> colors.secondaryContainer; else -> colors.surfaceContainerLowest}
    val foreground=when {accent || selected -> colors.onPrimary; function -> colors.onSecondaryContainer; else -> colors.onSurface}
    val viewConfiguration = LocalViewConfiguration.current
    val keyViewConfiguration = remember(viewConfiguration) {
        object : ViewConfiguration by viewConfiguration {
            override val minimumTouchTargetSize = DpSize.Zero
        }
    }
    // Ten columns cannot each be 48 dp on a phone. Keep hit regions inside
    // their column instead of letting Material expand them over adjacent keys.
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides 0.dp, LocalViewConfiguration provides keyViewConfiguration) {
    if (!repeat) {
        KeyTouchSurface(label,enabled,epoch,keyModifier,description,background,foreground,alternate,
            previewEnabled && label.length == 1,alternateAction,action) { KeyLabel(label) }
    } else {
        DeleteKeySurface(enabled, epoch, keyModifier, background, foreground, action, clearAction) { KeyLabel(label) }
    }
}
}

@Composable
private fun KeyLabel(label: String) {
    val icon = when(label) { "⇧", "⇪" -> R.drawable.ic_shift; "⌫" -> R.drawable.ic_backspace; else -> null }
    if (icon != null) Column(horizontalAlignment=androidx.compose.ui.Alignment.CenterHorizontally) {
        Icon(androidx.compose.ui.res.painterResource(icon), null, Modifier.size(22.dp))
        if (label == "⇪") Box(Modifier.padding(top=2.dp).size(width=16.dp, height=2.dp).background(LocalContentColor.current))
    }
    else Text(label, fontSize=16.sp, maxLines=1)
}

/** Keep the label and action on one accessibility node, including the IME's non-Activity window. */
private fun Modifier.imeAction(label:String,enabled:Boolean,action:()->Unit) = clearAndSetSemantics {
    role=Role.Button;contentDescription=label
    if(!enabled) disabled()
    onClick {if(enabled)action();enabled}
}
