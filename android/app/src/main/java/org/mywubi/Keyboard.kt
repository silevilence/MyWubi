package org.mywubi

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalViewConfiguration
import androidx.compose.ui.platform.ViewConfiguration
import androidx.compose.ui.unit.DpSize
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import org.json.JSONArray
import org.json.JSONObject

data class CandidateItem(val id: Long, val text: String, val hint: String)
data class KeyboardModel(
    val prediction: Boolean = false,
    val epoch: Long = 0, val enabled: Boolean = false, val spelling: String = "",
    val status: String = "正在加载码表…", val base: String = "Main", val temporary: String? = null,
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
    var asciiSymbols by remember(model.epoch, model.base, model.privateInput, model.layout) {
        mutableStateOf(model.privateInput || model.base == "English" || model.layout != "letters")
    }
    var tools by remember(model.epoch) { mutableStateOf(false) }
    LaunchedEffect(model.spelling) { if (model.spelling.isEmpty()) expanded = false }
    val colors = myWubiColors()
    val baseEnabled = model.enabled && model.modesAvailable && !model.privateInput
    val baseLabel = if (model.base == "Main") "中 → 英" else "英 → 中"
    val toggleBase = { dispatch(JSONObject().put("SetBaseMode", if (model.base == "Main") "English" else "Main")) }
    fun input(label: String) { dispatch(JSONObject().put("InputText", JSONArray().put(label).put("Normal"))) }
    fun choose(item: CandidateItem) {
        dispatch(JSONObject().put("SelectCandidate", JSONArray().put(JSONObject(model.query)).put(item.id)))
    }
    MaterialTheme(colorScheme=colors) {
        val pinyinControl: @Composable () -> Unit = {
            PinyinButton(pinyinMode, pinyinLabel, baseEnabled && model.pinyinAvailable, cyclePinyin)
        }
        val controls: @Composable () -> Unit = {
            if (landscape && (model.candidates.isEmpty() || model.privateInput)) {
                Text(if(model.privateInput) "安全输入" else model.status,
                    Modifier.fillMaxWidth().padding(horizontal=8.dp, vertical=4.dp),
                    color=colors.onSurface, fontSize=14.sp, maxLines=1,
                    overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
            }
            if (model.spelling.isNotEmpty() && !model.privateInput) {
                Row(Modifier.fillMaxWidth(), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    Text(model.spelling, Modifier.weight(1f).padding(horizontal=12.dp, vertical=4.dp),
                        color=colors.primary, fontSize=16.sp, maxLines=1)
                    if (model.candidates.isNotEmpty()) {
                        pinyinControl()
                    }
                }
            }
            Row(Modifier.fillMaxWidth().heightIn(min=48.dp), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                Row(Modifier.weight(1f), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    if (model.candidates.isEmpty()) IconButton(onClick={tools=!tools}, modifier=Modifier.imeAction("键盘工具", true) {tools=!tools}) {
                        androidx.compose.foundation.Image(androidx.compose.ui.res.painterResource(R.drawable.mywubi_brand), null, Modifier.size(28.dp))
                    }
                    if (model.candidates.isNotEmpty() && !model.privateInput) {
                        if (model.prediction) Text("预测", Modifier.padding(start=8.dp), color=colors.primary, fontSize=12.sp)
                        LazyRow(Modifier.weight(1f).height(48.dp)) {
                            items(model.candidates, key={"${model.query}:${it.id}"}) { item -> CandidateButton(item, model.enabled) {choose(item)} }
                        }
                        TextButton(onClick={expanded=!expanded}, modifier=Modifier.imeAction(if(expanded) "收起" else "展开", true) {expanded=!expanded}) {
                            Text(if(expanded) "收起" else "展开")
                        }
                    } else {
                        if (landscape) Spacer(Modifier.weight(1f))
                        else Text(if(model.privateInput) "安全输入" else model.status, Modifier.weight(1f).padding(horizontal=8.dp),
                            color=colors.onSurface, fontSize=14.sp, maxLines=1,
                            overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                        if (model.spelling.isEmpty() && model.temporary == null) {
                            val showUpdate = model.idle && model.updateAvailable
                            val label = if (showUpdate) "设置，有可用更新" else "设置"
                            TextButton(onClick=openSettings, modifier=Modifier.imeAction(label, true, openSettings)) {
                                BadgedBox(badge = { if (showUpdate) Badge() }) { Text("设置") }
                            }
                        }
                        pinyinControl()
                    }
                }
                IconButton(onClick=hideKeyboard,
                    modifier=Modifier.size(48.dp).imeAction("收起键盘", true, hideKeyboard)) {
                    Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_hide_keyboard), null,
                        Modifier.size(24.dp))
                }
            }
            if (tools && model.spelling.isEmpty() && model.candidates.isEmpty()) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.SpaceEvenly) {
                    TextButton(onClick=openSettings) {Text("设置 / 码表")}
                    TextButton(onClick=switchIme) {Text("切换输入法")}
                    val enabled = baseEnabled && model.pinyinAvailable
                    PinyinButton(pinyinMode, pinyinLabel, enabled) {cyclePinyin(); tools=false}
                }
            }
        }
        val keys: @Composable () -> Unit = {
            if (expanded && model.candidates.isNotEmpty() && !model.privateInput) {
                LazyVerticalGrid(columns=GridCells.Adaptive(88.dp), modifier=Modifier.fillMaxWidth().height(160.dp)) {
                    items(model.candidates,key={"${model.query}:${it.id}"}) { item -> CandidateButton(item,model.enabled) {choose(item)} }
                }
                Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.SpaceBetween) {
                    TextButton(onClick={window((model.offset-256).coerceAtLeast(0))},enabled=model.offset>0) {Text("上一组")}
                    Text("${model.offset+1}–${model.offset+model.candidates.size} / ${model.total}",Modifier.padding(12.dp))
                    TextButton(onClick={window(model.offset+256)},enabled=model.offset+model.candidates.size<model.total) {Text("下一组")}
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
                                        repeat=true,description="退格",function=true) {dispatch("Backspace")}
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
                            Key("⌫",model.enabled,model.epoch,Modifier.fillMaxWidth(),repeat=true,description="退格",function=true) {dispatch("Backspace")}
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
            if (panel != "numbers" || expanded) Row(Modifier.fillMaxWidth()) {
                val showBackspace = expanded || panel == "symbols"
                Key(if(panel=="letters") "123" else "ABC",model.enabled,model.epoch,Modifier.weight(1.3f),function=true) {
                    panel=if(panel=="letters") "numbers" else "letters"; expanded=false
                }
                Key(if(showBackspace) "⌫" else "符",model.enabled,model.epoch,Modifier.weight(1f),
                    repeat=showBackspace,description=if(showBackspace) "退格" else "符号",function=true) {
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
            .windowInsetsPadding(WindowInsets.navigationBars.only(WindowInsetsSides.Horizontal))
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
        CompositionLocalProvider(LocalKeyPreview provides preview) {
            Box(Modifier.fillMaxWidth().onGloballyPositioned { origin=it.positionInRoot(); keyboardWidth=it.size.width.toFloat() }) {
                if(landscape) {
                    Row(container,horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                        Column(Modifier.weight(0.3f).heightIn(max=224.dp).verticalScroll(rememberScrollState())) {controls()}
                        Column(Modifier.weight(0.7f)) {keys()}
                    }
                } else Column(container) {controls();keys()}
                KeyPreviewBubble(preview.current, origin, keyboardWidth)
            }
        }
    }
}

@Composable
private fun PinyinButton(mode: KeyMode, description: String, enabled: Boolean, action: () -> Unit) {
    val selected = mode != KeyMode.Off
    val colors = MaterialTheme.colorScheme
    FilledTonalButton(onClick=action, enabled=enabled,
        modifier=Modifier.imeAction(description, enabled, action), contentPadding=PaddingValues(horizontal=10.dp, vertical=6.dp),
        colors=ButtonDefaults.filledTonalButtonColors(
            containerColor=if(selected) colors.primary else colors.surface,
            contentColor=if(selected) colors.onPrimary else colors.primary)) {
        Text("拼音", maxLines=1)
        if (mode == KeyMode.Locked) Icon(androidx.compose.ui.res.painterResource(R.drawable.ic_mode_lock), null,
            Modifier.padding(start=4.dp).size(14.dp))
        else if (mode == KeyMode.Once) Box(Modifier.padding(start=5.dp).size(5.dp)
            .background(LocalContentColor.current, androidx.compose.foundation.shape.CircleShape))
    }
}

@Composable
private fun CandidateButton(item: CandidateItem, enabled: Boolean, choose: () -> Unit) {
    val description = "候选 ${item.text}" + if(item.hint.isEmpty()) "" else "，编码提示 ${item.hint}"
    TextButton(onClick=choose,enabled=enabled,modifier=Modifier.heightIn(min=48.dp).imeAction(description,enabled,choose)) {
        Text(item.text,fontSize=19.sp)
        if(item.hint.isNotEmpty()) Text(" ${item.hint}",fontSize=11.sp,color=MaterialTheme.colorScheme.primary)
    }
}

@Composable
private fun Key(label:String,enabled:Boolean,epoch:Long,modifier:Modifier,repeat:Boolean=false,
                description:String=label,function:Boolean=false,accent:Boolean=false,selected:Boolean=false,
                alternate:String?=null,previewEnabled:Boolean=false,alternateAction:()->Unit={},action:()->Unit) {
    val currentAction by rememberUpdatedState(action)
    val keyModifier=modifier.height(54.dp).padding(horizontal=2.dp,vertical=3.dp)
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
        val lifecycle = LocalLifecycleOwner.current.lifecycle
        var pressed by remember { mutableStateOf(false) }
        var repeatJob by remember { mutableStateOf<Job?>(null) }
        DisposableEffect(lifecycle) {
            val observer = LifecycleEventObserver { _, event ->
                if (event == Lifecycle.Event.ON_STOP || event == Lifecycle.Event.ON_DESTROY) repeatJob?.cancel()
            }
            lifecycle.addObserver(observer)
            onDispose { repeatJob?.cancel(); lifecycle.removeObserver(observer) }
        }
        Surface(modifier=keyModifier.imeAction(description,enabled) {currentAction()}.pointerInput(enabled,epoch) {
        if(enabled) detectTapGestures(onPress={
            pressed = true
            coroutineScope {
                var repeated = false
                val job=launch {delay(400);repeated=true;while(isActive){currentAction();delay(65)}}
                repeatJob = job
                try { if (tryAwaitRelease() && !repeated) currentAction() } finally {job.cancel();pressed=false}
            }
        })
    },shape=MaterialTheme.shapes.small,color=if(pressed) colors.primaryContainer else if(enabled) background else colors.onSurface.copy(alpha=0.12f),contentColor=if(enabled) foreground else colors.onSurface.copy(alpha=0.38f)) {
        Box(contentAlignment=androidx.compose.ui.Alignment.Center) {KeyLabel(label)}
    }
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
