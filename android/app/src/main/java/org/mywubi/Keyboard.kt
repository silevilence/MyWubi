package org.mywubi

import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.systemGestureExclusion
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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Color
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

internal data class RankOptions(val query: String, val id: Long, val show: (Int, Boolean) -> Unit)
internal data class MoveCandidate(val query: String, val id: Long, val rank: Int)
internal data class LoadCandidateWindow(val query: String)

data class CandidateItem(val id: Long, val text: String, val hint: String, val expressionKind: String? = null, val dateTime: Boolean = false) {
    val hasCodeHint: Boolean get() = expressionKind == null && !dateTime && hint.isNotEmpty()
    val spokenHint: String get() = if(hasCodeHint) "编码提示 $hint" else hint
}
data class ExpressionQueryUi(val kind: String, val phase: String, val text: String, val message: String?, val input: String = "Words", val browsing: Boolean = false)

private const val defaultCandidateSize = 17f
private const val defaultCodeHintSize = 10f
private val portraitCandidateHeight = 54.dp
private val LocalKeyboardKeyHeight = staticCompositionLocalOf { 54.dp }
data class KeyboardModel(
    val customLayout: KeyboardLayout? = null,
    val mainKeyboards: List<MainKeyboardOption> = emptyList(),
    val mainKeyboardSelection: KeyboardSelection? = null,
    val switchingKeyboard: Boolean = false,
    val allowKeyboardSwitch: Boolean = true,
    val mappingIdentity: JSONObject? = null,
    val groupPositions: List<Pair<String,String?>> = emptyList(),
    val skin: JSONObject? = null,
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
    val partialWindow: Boolean = false, val query: String = "{}", val total: Int = 0, val offset: Int = 0,
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
    Keyboard(rememberUpdatedState(model), dispatch, window, openSettings, onTouch, hideKeyboard, switchIme)
}

@Composable
internal fun Keyboard(latest: State<KeyboardModel>, dispatch: (Any) -> Unit, window: (Int) -> Unit,
                      openSettings: () -> Unit, onTouch: (Boolean) -> Unit,
                      hideKeyboard: () -> Unit, switchIme: () -> Unit) {
    // Only the candidate controls and the expanded grid subscribe to these frequent updates.
    val keys by remember(latest) {
        derivedStateOf(structuralEqualityPolicy()) {
            latest.value.copy(spelling = "", candidates = emptyList(), query = "{}", partialWindow = false, total = 0, offset = 0,
                groupPositions = emptyList(), idle = false, clipboard = null, prediction = false,
                status = "", statusError = false)
        }
    }
    KeyboardSurface(keys, latest, dispatch, window, openSettings, onTouch, hideKeyboard, switchIme)
}

@Composable
@OptIn(ExperimentalLayoutApi::class)
private fun KeyboardSurface(model: KeyboardModel, latest: State<KeyboardModel>, dispatch: (Any) -> Unit,
                            window: (Int) -> Unit, openSettings: () -> Unit, onTouch: (Boolean) -> Unit,
                            hideKeyboard: () -> Unit, switchIme: () -> Unit) {
    val currentTouch by rememberUpdatedState(onTouch)
    val configuration = LocalConfiguration.current
    val landscape = configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val variant = model.skin?.optJSONObject(if (androidx.compose.foundation.isSystemInDarkTheme()) "dark" else "light")
    val parameters = variant?.takeUnless { it.optBoolean("legacy") }?.optJSONObject("parameters")
    val preferredHeight = parameters?.optDouble("keyboard_height", 216.0)?.toFloat()?.div(4f)
    val baseHeight = (preferredHeight?.times(if (landscape) 48f/54f else 1f) ?: if (landscape) 48f else 54f)
    val keyHeight = maxOf(48f, baseHeight, ((parameters?.optDouble("key_text_size",20.0)?.toFloat() ?: 16f)+12f)*LocalDensity.current.fontScale+16f).dp
    val shiftState = remember(model.epoch) { mutableStateOf(KeyMode.Off) }
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
    var keyboardMenu by remember(model.epoch, model.mainKeyboards, model.base, model.temporary, panel) { mutableStateOf(false) }
    val showExpanded = expanded && latest.value.candidates.isNotEmpty() && !model.privateInput
    var asciiSymbols by remember(model.epoch, model.base, model.privateInput, model.layout) {
        mutableStateOf(model.privateInput || model.base == "English" || model.layout != "letters")
    }
    LaunchedEffect(latest) {
        snapshotFlow { latest.value.spelling.isEmpty() }.collect { empty -> if (empty) expanded = false }
    }
    val originalColors = myWubiColors()
    val colors = remember(originalColors, parameters) {
        originalColors.copy(
            surface=skinColorValue(parameters,"background",originalColors.surface),
            onSurface=skinColorValue(parameters,"text",originalColors.onSurface),
            onSurfaceVariant=skinColorValue(parameters,"hint",originalColors.onSurfaceVariant),
            primary=skinColorValue(parameters,"selected_background",originalColors.primary),
            onPrimary=skinColorValue(parameters,"selected_text",originalColors.onPrimary),
            surfaceContainerLowest=skinColorValue(parameters,"key_background",originalColors.surfaceContainerLowest),
            secondaryContainer=skinColorValue(parameters,"function_background",originalColors.secondaryContainer),
            onSecondaryContainer=skinColorValue(parameters,"function_text",originalColors.onSecondaryContainer))
    }
    val baseEnabled = model.enabled && model.modesAvailable && !model.privateInput && model.expression == null
    val baseLabel = if (model.base == "Main") "中 → 英" else "英 → 中"
    val toggleBase = { dispatch(JSONObject().put("SetBaseMode", if (model.base == "Main") "English" else "Main")) }
    fun input(label: String) { dispatch(JSONObject().put("InputText", JSONArray().put(label).put("Normal"))) }
    fun choose(item: CandidateItem, query: String) {
        dispatch(JSONObject().put("SelectCandidate", JSONArray().put(JSONObject(query)).put(item.id)))
        expanded = false
    }
    CompositionLocalProvider(LocalSkinParameters provides parameters, LocalSkinImages provides ((variant?.opt("decodedImages") as? SkinImages) ?: EmptySkinImages)) { MaterialTheme(colorScheme=colors) {
        val pinyinControl: @Composable () -> Unit = {
            PinyinButton(pinyinMode, pinyinLabel, baseEnabled && model.pinyinAvailable, cyclePinyin)
        }
        val controls: @Composable () -> Unit = {
            val model = latest.value
            val groupControls = !model.privateInput && model.groupPositions.isNotEmpty()
            val showToolbar = (model.privateInput || model.spelling.isEmpty() || model.candidates.isNotEmpty()) && model.expression == null
            // Reserve scaled text space even when the candidate list is empty.
            val density = LocalDensity.current
            val wordSize = skinNumber("font_size", defaultCandidateSize)
            val hintSize = skinNumber("hint_size", defaultCodeHintSize)
            val spellingSize = kotlin.math.round((wordSize + hintSize) / 2f).sp
            val barHeight = if (landscape) maxOf(48.dp, (28 * density.fontScale + 16).dp) else {
                // Budget both lines including the CJK fallback font's ascent and descent.
                maxOf(portraitCandidateHeight, ((wordSize + hintSize + 10) * density.fontScale + 8).dp)
            }
            model.expression?.let { query ->
                val results = query.phase == "Results"
                val name = specialLabel(query.kind)
                FlowRow(Modifier.fillMaxWidth().heightIn(min=48.dp).padding(horizontal=10.dp), horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick={expanded=false; dispatch("BrowseExpressions")}, enabled=model.enabled) { Text("全部") }
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
                    color=skinColor("code",colors.primary), fontSize=spellingSize, maxLines=1)
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
                            color=skinColor("code",colors.primary), fontSize=spellingSize, maxLines=1,
                            overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                    } else Column(Modifier.weight(1f).padding(horizontal=12.dp)) {
                        Text(heading,
                            color=skinColor("code",colors.primary), fontSize=spellingSize, maxLines=1,
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
                if (!landscape && model.expression == null) Text(if (showSpelling) model.spelling else "",
                    Modifier.fillMaxWidth().heightIn(min=28.dp).padding(horizontal=12.dp, vertical=4.dp)
                        .then(if (showSpelling) Modifier else Modifier.clearAndSetSemantics {}),
                    color=skinColor("code",colors.primary), fontSize=spellingSize, lineHeight=spellingSize * 1.3f, maxLines=1,
                    overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                Row(Modifier.fillMaxWidth().heightIn(min=barHeight), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    if (landscape && model.expression == null && showSpelling) Text(model.spelling,
                        Modifier.widthIn(max=112.dp).padding(horizontal=8.dp), color=skinColor("code",colors.primary),
                        fontSize=spellingSize, maxLines=1, overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                    val showCandidates = model.candidates.isNotEmpty() && !model.privateInput
                    CandidateToolbarSlot(Modifier.weight(1f), showCandidates, candidates = {
                        Row(verticalAlignment=Alignment.CenterVertically) {
                            if (model.prediction) Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_prediction),
                                "字词预测", Modifier.padding(start=8.dp).size(18.dp), tint=colors.primary)
                            val scroll = androidx.compose.foundation.lazy.rememberLazyListState()
                            LaunchedEffect(model.query) {
                                if (scroll.firstVisibleItemIndex != 0 || scroll.firstVisibleItemScrollOffset != 0) scroll.scrollToItem(0)
                            }
                            val items = if (showCandidates) model.candidates else emptyCandidateSlots
                            val stackHints = !landscape && remember(items) { items.any { it.hasCodeHint } }
                            LazyRow(state=scroll, modifier=Modifier.weight(1f).heightIn(min=48.dp).drawWithContent {
                                // A content-free marker lets Perfetto identify the real IME buffer
                                // without attaching an accessibility inspector or instrumenting the process.
                                if (showCandidates) android.os.Trace.beginSection("MyWubi candidate strip")
                                try { drawContent() } finally { if (showCandidates) android.os.Trace.endSection() }
                            }.skinImage("candidate").padding(skinContent("candidate"))) {
                                // Subcompose a few adjacent cells together; the list still virtualizes long windows.
                                items((items.size + 3) / 4) { group ->
                                    Row {
                                        for (index in group * 4 until minOf(items.size, group * 4 + 4)) {
                                            val item = items[index]
                                            if (model.partialWindow && index == items.size - 3) {
                                                LaunchedEffect(model.query) { dispatch(LoadCandidateWindow(model.query)) }
                                            }
                                            CandidateButton(item, model.query, showCandidates && model.enabled, stackHints, barHeight, dispatch) {
                                                choose(item, model.query)
                                            }
                                        }
                                    }
                                }
                            }
                            ToolbarIconButton(R.drawable.ic_expand_candidates, "展开") {
                                if (model.partialWindow) dispatch(LoadCandidateWindow(model.query))
                                expanded = true
                            }
                        }
                    }, toolbar = {
                        Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.SpaceEvenly, verticalAlignment=Alignment.CenterVertically) {
                            if (showClipboard) {
                                TextButton(onClick = { dispatch("PasteClipboard") }, enabled = model.enabled,
                                    modifier = Modifier.weight(1f).heightIn(min = 48.dp).imeAction("粘贴剪贴板内容", model.enabled) { dispatch("PasteClipboard") }) {
                                    Text("粘贴：${model.clipboard}", maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                                }
                                ToolbarIconButton(R.drawable.ic_close, "关闭剪贴板建议") { dispatch("DismissClipboard") }
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
                                        badge=if (showUpdate) ({ Badge() }) else null, action=openSettings)
                                    val mainMode = model.base == "Main" && model.temporary == null
                                    if (model.allowKeyboardSwitch && ((mainMode && model.mainKeyboards.isNotEmpty()) || model.customLayout != null) &&
                                        model.modesAvailable && !model.privateInput && supportsTextKeyboard(model.layout) && panel == "letters") Box {
                                        val menuLabel = if (mainMode) "切换形码键盘" else "切换键盘布局"
                                        ToolbarIconButton(R.drawable.ic_keyboard_layout, menuLabel,
                                            enabled=model.enabled && !model.switchingKeyboard) { keyboardMenu=true }
                                        MainKeyboardMenu(model, !showCandidates && keyboardMenu && model.enabled && !model.switchingKeyboard,
                                            mainMode=mainMode, dismiss={keyboardMenu=false}, restore={dispatch("RestoreKeyboard")}) {
                                            dispatch(SwitchMainKeyboard(it))
                                        }
                                    }
                                    Box {
                                        ToolbarIconButton(R.drawable.ic_emoji, "特殊输入", enabled=baseEnabled) { specialMenu=true }
                                        SpecialCategoryMenu(model,!showCandidates && specialMenu && baseEnabled,dismiss={specialMenu=false}) { mode ->
                                            specialMenu=false
                                            dispatch(JSONObject().put("BeginTemporary",mode))
                                            dispatch("BrowseExpressions")
                                        }
                                    }
                                    ToolbarIconButton(R.drawable.ic_switch_ime, "切换输入法", action=switchIme)
                                    pinyinControl()
                                } else {
                                    Spacer(Modifier.weight(1f))
                                }
                            }
                        }
                    })
                    if (groupControls) GroupConstraints(model.groupPositions,model.query,model.enabled,dispatch)
                    IconButton(onClick=hideKeyboard,
                        modifier=Modifier.size(48.dp).imeAction("收起键盘", true, hideKeyboard)) {
                        Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_hide_keyboard), null,
                            Modifier.size(24.dp), tint=colors.onSurface)
                    }
                }
            }
        }
        val expandedExtra = if (landscape) 0.dp else 28.dp + portraitCandidateHeight - 48.dp
        val keys: @Composable () -> Unit = {
            val rowHeight = LocalKeyboardKeyHeight.current
            val footerHeight = if (landscape && showExpanded) maxOf(48.dp, rowHeight).coerceAtMost(rowHeight * 4) else rowHeight
            if (showExpanded) {
                val model = latest.value
                if (model.partialWindow) LaunchedEffect(model.query) { dispatch(LoadCandidateWindow(model.query)) }
                val hasPages = model.offset > 0 || model.offset + model.candidates.size < model.total
                val showHints = model.candidates.any { it.hint.isNotEmpty() }
                // Spend the former spelling-strip space on the grid; retain the typing area's height.
                val gridHeight = (rowHeight * 4 + expandedExtra - footerHeight - (if (hasPages) 48.dp else 0.dp)).coerceAtLeast(0.dp)
                key(model.query, model.offset) {
                    val columnWidth = (112 * LocalDensity.current.fontScale.coerceIn(1f, 1.5f)).dp
                    LazyVerticalGrid(columns=GridCells.Adaptive(columnWidth),
                        modifier=Modifier.fillMaxWidth().height(gridHeight)
                            .semantics { contentDescription = "全部候选" }) {
                        items(model.candidates, key={it.id}, span={item ->
                            GridItemSpan(if (item.expressionKind != null || item.text.codePointCount(0, item.text.length) > 4) maxLineSpan else 1)
                        }) { item -> ExpandedCandidateButton(item,model.query,model.enabled,showHints,landscape,dispatch) {choose(item, model.query)} }
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
            } else {
                val layout = remember(panel, asciiSymbols, model.layout, model.enterLabel, model.customLayout, model.privateInput) {
                    when(panel) {
                        "letters" -> model.customLayout?.takeIf { !model.privateInput && supportsTextKeyboard(model.layout) }
                            ?: BuiltinKeyboardLayouts.letters()
                        "numbers" -> BuiltinKeyboardLayouts.numbers(model.layout == "phone", model.enterLabel)
                        else -> BuiltinKeyboardLayouts.symbols(asciiSymbols)
                    }
                }
                TypingGrid(layout, model.enabled, model.epoch, model.enterLabel, model.privateInput,
                    !model.privateInput && model.layout == "letters" && (model.base == "Main" || model.temporary == "Pinyin"),
                    model.base, model.mappingIdentity, shiftState, { panel = it }, dispatch)
            }
            if (panel != "numbers" || showExpanded) CompositionLocalProvider(LocalKeyboardKeyHeight provides footerHeight) { Row(Modifier.fillMaxWidth()) {
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
                Key("空格",model.enabled,model.epoch,Modifier.weight(3.4f),space=true) {dispatch("Space")}
                Key(if(model.base=="Main" && model.layout=="letters" && !model.privateInput) "。" else ".",model.enabled,model.epoch,Modifier.weight(0.9f),previewEnabled=!model.privateInput) {
                    input(if(model.base=="Main" && model.layout=="letters" && !model.privateInput) "。" else ".")
                }
                Key(if(model.base=="Main") "中" else "英",baseEnabled,model.epoch,Modifier.weight(1.3f),description=baseLabel,function=true) {toggleBase()}
                Key(model.enterLabel,model.enabled,model.epoch,Modifier.weight(1.7f),accent=true) {dispatch("Enter")}
            } }
        }
        val density = LocalDensity.current
        // Some IME hosts consume navigation insets before Compose receives them.
        // Keep a dedicated bottom band even then, clear of the OEM's IME buttons.
        val bottom = with(density) {
            maxOf(WindowInsets.navigationBarsIgnoringVisibility.getBottom(this),
                WindowInsets.mandatorySystemGestures.getBottom(this)).toDp().coerceAtLeast(if (landscape) 8.dp else 32.dp)
        }
        val backgroundPadding = skinContent("background")
        val keyRows = when {
            showExpanded -> 4f
            panel == "symbols" -> 5f
            model.customLayout != null && panel == "letters" && !model.privateInput ->
                model.customLayout.height + 1f
            else -> 4f
        }
        val heightLimit = if (landscape && configuration.screenHeightDp > 0)
            Modifier.heightIn(max = configuration.screenHeightDp.dp * 2 / 3) else Modifier
        val container=Modifier.fillMaxWidth().then(heightLimit).background(colors.surface).skinImage("background").padding(backgroundPadding)
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
            Box(Modifier.fillMaxWidth().onGloballyPositioned {
                origin=it.positionInRoot(); keyboardWidth=it.size.width.toFloat()
                preview.keyboardBounds = androidx.compose.ui.geometry.Rect(origin, androidx.compose.ui.geometry.Size(keyboardWidth, it.size.height.toFloat()))
            }) {
                BoxWithConstraints(container, contentAlignment=androidx.compose.ui.Alignment.TopCenter) {
                    val rows = keyRows
                    val gridExtra = if (showExpanded) expandedExtra else 0.dp
                    // The toolbar keeps its place when a keyboard panel or candidate page changes.
                    if (model.expression?.browsing == true && !model.privateInput) {
                        SpecialInputBrowser(latest.value, dispatch, window, hideKeyboard,
                            Modifier.widthIn(max=600.dp).fillMaxWidth()
                                .height(minOf(maxHeight, keyHeight*4+76.dp)))
                    } else if (landscape && model.expression != null) {
                        // Query controls must not consume the typing area's vertical budget.
                        // On short screens the left pane scrolls; all four key rows remain visible.
                        val paneHeight=minOf(maxHeight,keyHeight*4+28.dp)
                        val fittedHeight = minOf(keyHeight, (maxHeight-gridExtra).coerceAtLeast(0.dp) / rows)
                        Row(Modifier.widthIn(max=1100.dp).fillMaxWidth()) {
                            Column(Modifier.weight(0.36f).height(paneHeight)
                                .verticalScroll(rememberScrollState()).semantics { contentDescription="表情查询与操作" }) { controls() }
                            CompositionLocalProvider(LocalKeyboardKeyHeight provides fittedHeight) {
                                Column(Modifier.weight(0.64f)) { keys() }
                            }
                        }
                    } else {
                        val availableHeight = maxHeight
                        val panelWidth = if (landscape) 1040.dp else 600.dp
                        Column(Modifier.widthIn(max=panelWidth).fillMaxWidth()) {
                            // Keep a 48 dp row budget before scaling text in a short window.
                            val controlScale = minOf(density.fontScale,
                                ((availableHeight.value - rows*48f - 24f)/48f).coerceAtLeast(1f))
                            CompositionLocalProvider(LocalDensity provides remember(density, controlScale) { androidx.compose.ui.unit.Density(density.density, controlScale) }) {
                                controls()
                            }
                            BoxWithConstraints(if (availableHeight == androidx.compose.ui.unit.Dp.Infinity) Modifier else Modifier.weight(1f, fill=false)) {
                                // Large fonts must not push the last key rows out of a
                                // short window. Fit all rows into the space left by controls.
                                val fittedHeight = minOf(keyHeight, (maxHeight-gridExtra).coerceAtLeast(0.dp) / rows)
                                val fittedScale = minOf(density.fontScale,
                                    ((fittedHeight.value - 16f) /
                                        ((parameters?.optDouble("key_text_size",20.0)?.toFloat() ?: 16f) + 12f)).coerceAtLeast(0.5f))
                                CompositionLocalProvider(LocalKeyboardKeyHeight provides fittedHeight,
                                    LocalDensity provides remember(density, fittedScale) { androidx.compose.ui.unit.Density(density.density, fittedScale) }) {
                                    val split = landscape && maxWidth >= 560.dp && !showExpanded &&
                                        !model.privateInput && model.layout !in listOf("number", "phone") &&
                                        (panel == "numbers" || (panel == "letters" && model.customLayout?.isNineKeyGrid == true))
                                    if (split) {
                                        Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                                            QuickSymbols(Modifier.weight(0.4f), asciiSymbols, model.enabled, model.epoch, ::input)
                                            Column(Modifier.weight(0.6f)) { keys() }
                                        }
                                    } else Column { keys() }
                                }
                            }
                        }
                    }
                }
                Box(Modifier.matchParentSize()) { KeyPreviewBubble(preview, origin, keyboardWidth) }
            }
        }
    } }
}

private val chineseQuickSymbols = listOf("，。！？", "、：；…", "（）《》", "“”—～")
private val asciiQuickSymbols = listOf(",.!?", "/:;@", "()[]", "+-*_")

@Composable
private fun QuickSymbols(modifier: Modifier, ascii: Boolean, enabled: Boolean, epoch: Long, input: (String) -> Unit) {
    Column(modifier.semantics { contentDescription = "常用符号" }) {
        for (row in if (ascii) asciiQuickSymbols else chineseQuickSymbols) {
            Row(Modifier.fillMaxWidth()) {
                for (symbol in row) {
                    val label = symbol.toString()
                    Key(label, enabled, epoch, Modifier.weight(1f), description="常用符号 $label") { input(label) }
                }
            }
        }
    }
}

@Composable
private fun ToolbarIconButton(icon: Int, description: String, enabled: Boolean = true,
                              selected: Boolean = false, badge: (@Composable () -> Unit)? = null, action: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    IconButton(onClick=action, enabled=enabled,
        modifier=Modifier.size(48.dp)
            .background(if(selected) colors.secondaryContainer else androidx.compose.ui.graphics.Color.Transparent,
                androidx.compose.foundation.shape.CircleShape)
            .imeAction(description, enabled, action),
        colors=IconButtonDefaults.iconButtonColors(contentColor=if(selected) colors.primary else colors.onSurfaceVariant)) {
        val image: @Composable () -> Unit = {
            Icon(androidx.compose.ui.res.painterResource(icon), null, Modifier.size(24.dp))
        }
        if (badge == null) image() else BadgedBox(badge = { badge() }) { image() }
    }
}

@Composable
private fun PinyinButton(mode: KeyMode, description: String, enabled: Boolean, action: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    ToolbarIconButton(R.drawable.ic_pinyin, description, enabled, selected=mode != KeyMode.Off,
        badge=if (mode == KeyMode.Off) null else ({
            if (mode == KeyMode.Locked) Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_mode_lock), null,
                Modifier.size(10.dp), tint=colors.primary)
            else if (mode == KeyMode.Once) Box(Modifier.size(5.dp)
                .background(colors.primary, androidx.compose.foundation.shape.CircleShape))
        }), action=action)
}

// Candidate updates leave the typing grid in place while the toolbar changes.
@Composable
private fun TypingGrid(layout: KeyboardLayout, enabled: Boolean, epoch: Long, enterLabel: String,
                       privateInput: Boolean, chineseSymbols: Boolean, base: String, mappingIdentity: JSONObject?,
                       shiftState: MutableState<KeyMode>, changePanel: (String) -> Unit,
                       dispatch: (Any) -> Unit) {
    var shift by shiftState
    fun input(text: String) { dispatch(JSONObject().put("InputText", JSONArray().put(text).put("Normal"))) }
    // On gesture-navigation devices the side strips may cover the first
    // and last key centres. Protect only the typing grid, within Android's
    // exclusion quota. The navigation band remains available below it.
    BoxWithConstraints(Modifier.fillMaxWidth().height(LocalKeyboardKeyHeight.current*layout.height)
        .systemGestureExclusion().graphicsLayer()) {
        val unit=maxWidth/layout.width
        for (key in layout.keys) {
            val character=key.action.value.singleOrNull()
            val isLetter=key.action.kind == "Character" && character?.isLowerCase() == true
            val value=if(isLetter && shift != KeyMode.Off) key.action.value.uppercase() else key.action.value
            val label=when {
                key.action.kind == "Shift" -> if(shift == KeyMode.Locked) "⇪" else "⇧"
                isLetter && key.label == key.action.value -> value
                key.action.kind == "Enter" -> enterLabel
                else -> key.label
            }
            val symbol=key.alternate ?: if(layout.id == "builtin.26" && character != null)
                letterSymbol(character, chineseSymbols) else null
            val up = key.directions[0]?.takeIf { it.isSymbol }?.value
            val down = key.directions[4]?.takeIf { it.isSymbol }?.value
            val pairedHints = if (key.action.kind == "Group" && key.groupCharacters.size == 2 &&
                up != null && down != null) up to down else null
            fun perform(action: LayoutAction) {
                if (pairedHints != null && action.isSymbol && (action.value == up || action.value == down)) {
                    dispatch(JSONObject().put("InputSymbol", action.value))
                    return
                }
                when(action.kind) {
                    "InputSymbol" -> dispatch(JSONObject().put("InputSymbol",action.value))
                    "Character" -> {input(if(action == key.action) value else action.value); if(isLetter) shift = shift.consumed()}
                    "Group" -> mappingIdentity?.let { identity -> dispatch(JSONObject().put("InputGroup",org.json.JSONArray().put(identity).put(action.value))) }
                    "Shift" -> shift = shift.next()
                    "Letters" -> changePanel("letters")
                    "Numbers" -> changePanel("numbers")
                    "Symbols" -> changePanel("symbols")
                    "English" -> dispatch(JSONObject().put("SetBaseMode", if (base == "Main") "English" else "Main"))
                    "Pinyin" -> dispatch("CyclePinyin")
                    "PageNext" -> dispatch(JSONObject().put("MoveCandidatePage",true))
                    "PagePrevious" -> dispatch(JSONObject().put("MoveCandidatePage",false))
                    "Backspace", "Space", "Enter", "Cancel" -> dispatch(action.kind)
                }
            }
            fun actionName(action: LayoutAction)=when {
                action.kind == "Group" -> "字符组 ${action.value}"
                action.kind == "Character" && action == key.action -> value
                else -> action.value.ifEmpty { action.kind }
            }
            val holdOptions = remember(key, symbol) { key.holdOptions(symbol) }
            Key(label,enabled,epoch,
                Modifier.offset(x=unit*key.x,y=LocalKeyboardKeyHeight.current*key.y).width(unit*key.width),
                heightFactor=key.height,
                repeat=key.action.kind == "Backspace", description=when(key.action.kind) {
                    "Backspace" -> "退格"
                    "Shift" -> when(shift) { KeyMode.Off -> "小写"; KeyMode.Once -> "大写 · 本次"; KeyMode.Locked -> "大写 · 锁定" }
                    else -> label
                }, space=key.action.kind == "Space",function=key.role == "function",accent=key.role == "selected",
                selected=key.action.kind == "Shift" && shift != KeyMode.Off,
                alternate=holdOptions.firstOrNull()?.takeIf {
                    key.hold != null || it.isSymbol
                }?.let(::actionName), pairedHints=pairedHints, previewEnabled=!privateInput,
                holdChoices=holdOptions.distinctBy(::actionName).map { TouchChoice(actionName(it)) { perform(it) } },
                bindingRevision=layout, directions=if(layout.id == "builtin.26") null else key.directions.mapValues { (_,action) -> TouchChoice(actionName(action)) {perform(action)} },
                clearAction={dispatch("ClearEditor")}) {
                perform(key.action)
            }
        }
    }
}

// Keep the collapsed strip measured while the toolbar occupies its place. Empty cells
// have no labels, hit targets or screen position until a query has produced real choices.
private val emptyCandidateSlots = List(8) { CandidateItem(it.toLong(), "", "") }

@Composable
private fun CandidateToolbarSlot(modifier: Modifier, showCandidates: Boolean,
                                 candidates: @Composable () -> Unit, toolbar: @Composable () -> Unit) {
    val hidden = Modifier.clearAndSetSemantics {}
    androidx.compose.ui.layout.Layout(modifier=modifier, content={
        Box(Modifier.fillMaxWidth().then(if (showCandidates) Modifier else hidden)) { candidates() }
        Box(Modifier.fillMaxWidth().then(if (showCandidates) hidden else Modifier)) { toolbar() }
    }) { children, constraints ->
        val strip = children[0].measure(constraints)
        val visible = if (showCandidates) strip else children[1].measure(constraints)
        val height = maxOf(visible.height, strip.height)
        layout(visible.width, height) { visible.placeRelative(0, (height - visible.height) / 2) }
    }
}

@Composable
private fun CandidateButton(item: CandidateItem, query: String, enabled: Boolean, stackHints: Boolean,
                            minHeight: Dp, dispatch: (Any) -> Unit, choose: () -> Unit) {
    val display = remember(item) { ExpressionDisplay.display(item.text, item.hint, item.expressionKind) }
    val description = "候选 ${item.text}" + (if(item.hint.isEmpty()) "" else "，${item.spokenHint}") +
        (if(display.fallback) "，字体不支持完整显示，选择后输入原始表情" else "")
    val colors = ButtonDefaults.textButtonColors()
    val color = if (enabled) colors.contentColor else colors.disabledContentColor
    Box(Modifier.imeAction(description, enabled, choose), contentAlignment=Alignment.Center) {
        Box(Modifier.sizeIn(minWidth=48.dp, minHeight=minHeight).padding(horizontal=5.dp, vertical=3.dp),
            contentAlignment=Alignment.Center) {
            CandidateLabel(item, display, stackHints=stackHints, expanded=false, showHints=item.hint.isNotEmpty() || stackHints,
                wordColor=skinColor(if (enabled) "text" else "disabled", color),
                hintColor=skinColor("hint", MaterialTheme.colorScheme.primary))
        }
        // Rebind the hit target to the confirmed query. The label and its text layout stay mounted.
        key(query, item.id) {
            RankTarget(item, query, enabled, dispatch, choose, Modifier.matchParentSize().clip(ButtonDefaults.textShape))
        }
    }
}

@Composable
private fun ExpandedCandidateButton(item: CandidateItem, query: String, enabled: Boolean, showHints: Boolean, landscape: Boolean, dispatch: (Any) -> Unit, choose: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val display = remember(item) { ExpressionDisplay.display(item.text, item.hint, item.expressionKind) }
    val description = "候选 ${item.text}" + (if(item.hint.isEmpty()) "" else "，${item.spokenHint}") +
        (if(display.fallback) "，字体不支持完整显示，选择后输入原始表情" else "")
    Surface(shape=RectangleShape, color=colors.surface,
        contentColor=if(enabled) colors.onSurface else colors.onSurface.copy(alpha=0.38f),
        border=BorderStroke(0.5.dp, colors.onSurface.copy(alpha=0.22f)),
        modifier=Modifier.fillMaxWidth().heightIn(min=72.dp).horizontalScroll(rememberScrollState()).imeAction(description,enabled,choose)) {
        Box(contentAlignment=Alignment.Center) {
            Box(Modifier.padding(horizontal=8.dp, vertical=8.dp)) {
            CandidateLabel(item, display, stackHints=!landscape, expanded=true, showHints=showHints,
                wordColor=LocalContentColor.current, hintColor=skinColor("hint", colors.onSurfaceVariant))
            }
            key(query, item.id) { RankTarget(item, query, enabled, dispatch, choose, Modifier.matchParentSize()) }
        }
    }
}

@Composable
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
private fun RankTarget(item: CandidateItem, query: String, enabled: Boolean, dispatch: (Any) -> Unit, choose: () -> Unit, modifier: Modifier) {
    var count by remember { mutableIntStateOf(0) }
    var allowed by remember { mutableStateOf(false) }
    var offset by remember { mutableIntStateOf(0) }
    Box(modifier.combinedClickable(enabled = enabled, onClick = choose, onLongClick = {
        dispatch(RankOptions(query, item.id) { size, canWrite -> count = size; allowed = canWrite; offset = 0 })
    })) {
        DropdownMenu(expanded = count > 0, onDismissRequest = { count = 0 },
            properties = androidx.compose.ui.window.PopupProperties(focusable = false)) {
            if (!allowed) {
                DropdownMenuItem(text = { Text("请在设置中启用用户词库") }, onClick = { count = 0 })
            } else {
                if (offset > 0) DropdownMenuItem(text = { Text("上一组名次") }, onClick = { offset -= 8 })
                for (rank in offset + 1..minOf(count, offset + 8)) {
                    DropdownMenuItem(text = { Text("移到第 $rank 位") }, onClick = {
                        count = 0
                        dispatch(MoveCandidate(query, item.id, rank))
                    })
                }
                if (offset + 8 < count) DropdownMenuItem(text = { Text("下一组名次") }, onClick = { offset += 8 })
            }
        }
    }
}

@Composable
private fun CandidateLabel(item: CandidateItem, display: ExpressionDisplay.Display, stackHints: Boolean,
                           expanded: Boolean, showHints: Boolean, wordColor: Color, hintColor: Color) {
    val wordSize = skinNumber("font_size", defaultCandidateSize)
    val hintSize = skinNumber("hint_size", defaultCodeHintSize)
    val word: @Composable () -> Unit = {
        Text(display.text, color=wordColor, fontSize=wordSize.sp, lineHeight=(wordSize + 5).sp, softWrap=false,
            style=androidx.compose.ui.text.TextStyle(platformStyle=androidx.compose.ui.text.PlatformTextStyle(includeFontPadding=true)))
    }
    val hint: @Composable () -> Unit = {
        Text(item.hint.ifEmpty { " " }, color=hintColor, fontSize=hintSize.sp, lineHeight=(hintSize + 3).sp, softWrap=false)
    }
    val annotation = item.expressionKind != null || item.dateTime
    when {
        display.fallback || !showHints -> word()
        stackHints && !annotation -> Column(horizontalAlignment=Alignment.CenterHorizontally) { hint(); word() }
        expanded && annotation -> Column(horizontalAlignment=Alignment.CenterHorizontally) { word(); hint() }
        item.hint.isEmpty() -> word()
        else -> Row(verticalAlignment=Alignment.CenterVertically, horizontalArrangement=Arrangement.spacedBy(2.dp)) { word(); hint() }
    }
}

@Composable
private fun Key(label:String,enabled:Boolean,epoch:Long,modifier:Modifier,repeat:Boolean=false,
                description:String=label,space:Boolean=false,function:Boolean=false,accent:Boolean=false,selected:Boolean=false,
                alternate:String?=null,pairedHints:Pair<String,String>?=null,previewEnabled:Boolean=false,heightFactor:Float=1f,directions:Map<Int,TouchChoice>? = null,bindingRevision:Any=epoch,holdChoices:List<TouchChoice> = emptyList(),clearAction:()->Unit={},action:()->Unit) {
    // The entire column is touchable. KeyTouch draws the inset keycap inside it.
    val keyModifier=modifier.height(LocalKeyboardKeyHeight.current*heightFactor)
    val colors=MaterialTheme.colorScheme
    val background=when {accent || selected -> colors.primary; function -> colors.secondaryContainer; else -> colors.surfaceContainerLowest}
    val foreground=when {accent || selected -> colors.onPrimary; function -> colors.onSecondaryContainer; else -> skinColor("key_text",colors.onSurface)}
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
        KeyTouchSurface(label,enabled,epoch,keyModifier,description,background,foreground,if(accent || selected) "selected" else if(function) "function" else if(space && "space" in LocalSkinImages.current.roles) "space" else "key",alternate.takeIf { pairedHints == null },
            previewEnabled,holdChoices,action,directions,bindingRevision) {
            if (pairedHints == null) KeyLabel(label) else PairedSymbolLabel(label, pairedHints.first, pairedHints.second)
        }
    } else {
        DeleteKeySurface(enabled, epoch, keyModifier, background, foreground, action, clearAction) { KeyLabel(label) }
    }
}
}

@Composable
private fun PairedSymbolLabel(label: String, up: String, down: String) {
    val landscape = LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val density = LocalDensity.current
    BoxWithConstraints(Modifier.fillMaxSize().padding(horizontal = 5.dp, vertical = 2.dp), contentAlignment = Alignment.Center) {
        // Short keys use the horizontal space for hints. On taller keys all three labels share a centreline.
        val corners = landscape || maxHeight < 36.dp
        val hintHeight = maxHeight * if (corners) .44f else .24f
        val labelHeight = maxHeight * if (corners) .8f else .48f
        val labelWidth = maxWidth * if (corners) .42f else .65f
        val hintSize = minOf(11f, with(density) { hintHeight.toSp().value }).sp
        val mainSize = minOf(skinNumber("key_text_size", 16f), with(density) {
            minOf(labelHeight, labelWidth).toSp().value
        }).sp
        val hintColor = LocalContentColor.current.copy(alpha = LocalContentColor.current.alpha * .78f)
        Text(label, fontSize = mainSize, lineHeight = mainSize, maxLines = 1, softWrap = false)
        Text(up, Modifier.align(if (corners) Alignment.TopStart else Alignment.TopCenter),
            color = hintColor, fontSize = hintSize, lineHeight = hintSize, maxLines = 1, softWrap = false)
        Text(down, Modifier.align(if (corners) Alignment.BottomEnd else Alignment.BottomCenter),
            color = hintColor, fontSize = hintSize, lineHeight = hintSize, maxLines = 1, softWrap = false)
    }
}

@Composable
private fun KeyLabel(label: String) {
    val icon = when(label) { "⇧", "⇪" -> R.drawable.ic_shift; "⌫" -> R.drawable.ic_backspace; else -> null }
    if (icon != null) Column(horizontalAlignment=androidx.compose.ui.Alignment.CenterHorizontally) {
        Icon(androidx.compose.ui.res.painterResource(icon), null, Modifier.size(22.dp))
        if (label == "⇪") Box(Modifier.padding(top=2.dp).size(width=16.dp, height=2.dp).background(LocalContentColor.current))
    }
    else BoxWithConstraints(Modifier.fillMaxWidth(), contentAlignment=androidx.compose.ui.Alignment.Center) {
        val desired = skinNumber("key_text_size",16f)
        val measured = androidx.compose.ui.text.rememberTextMeasurer().measure(label,
            androidx.compose.ui.text.TextStyle(fontSize=desired.sp), softWrap=false)
        // Preserve the complete label on narrow functional keys. It grows with system
        // text size until the key width is exhausted; accessibility keeps the full name.
        val available = with(LocalDensity.current) { maxWidth.toPx() } - 8f
        val fit = minOf(1f, available.coerceAtLeast(1f) / measured.size.width.coerceAtLeast(1))
        Text(label, fontSize=(desired*fit).sp, maxLines=1, softWrap=false)
    }
}

/** Keep the label and action on one accessibility node, including the IME's non-Activity window. */
internal fun Modifier.imeAction(label:String,enabled:Boolean,action:()->Unit) = clearAndSetSemantics {
    role=Role.Button;contentDescription=label
    if(!enabled) disabled()
    onClick {if(enabled)action();enabled}
}


@Composable
private fun GroupConstraints(positions: List<Pair<String,String?>>,query: String,enabled: Boolean,dispatch: (Any)->Unit) {
    var open by remember(query) { mutableStateOf(false) }
    var position by remember(query) { mutableStateOf<Int?>(null) }
    Box {
        TextButton(onClick={position=null; open=true}, enabled=enabled) { Text("筛选", maxLines=1) }
        DropdownMenu(expanded=open, onDismissRequest={open=false}) {
            val index = position
            if (index == null) {
                positions.forEachIndexed { i, (letters, resolved) ->
                    DropdownMenuItem(text={Text("第 ${i+1} 位 · ${resolved ?: letters}")},
                        enabled=letters.length > 1, onClick={position=i})
                }
                DropdownMenuItem(text={Text("撤回筛选")}, onClick={
                    open=false
                    dispatch(JSONObject().put("UndoGroupResolution",JSONObject(query)))
                })
                DropdownMenuItem(text={Text("取消当前输入")}, onClick={open=false; dispatch("Cancel")})
            } else {
                positions.getOrNull(index)?.first?.forEach { letter ->
                    DropdownMenuItem(text={Text(letter.uppercaseChar().toString())}, onClick={
                        open=false
                        dispatch(JSONObject().put("ResolveGroup", JSONArray().put(JSONObject(query)).put(index).put(letter.toString())))
                    })
                }
            }
        }
    }
}
