package org.mywubi

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.platform.LocalConfiguration
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
    val epoch: Long = 0, val enabled: Boolean = false, val spelling: String = "",
    val status: String = "正在加载码表…", val base: String = "Main", val temporary: String? = null,
    val pinyinAvailable: Boolean = false, val modesAvailable: Boolean = false, val candidates: List<CandidateItem> = emptyList(),
    val query: String = "{}", val total: Int = 0, val offset: Int = 0,
    val layout: String = "letters", val enterLabel: String = "回车", val privateInput: Boolean = false,
    val idle: Boolean = false, val updateAvailable: Boolean = false
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
fun Keyboard(model: KeyboardModel, dispatch: (Any) -> Unit, window: (Int) -> Unit,
             openSettings: () -> Unit = {}, onTouch: (Boolean) -> Unit = {}, switchIme: () -> Unit) {
    val currentTouch by rememberUpdatedState(onTouch)
    var upper by remember(model.epoch) { mutableStateOf(false) }
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
        val controls: @Composable () -> Unit = {
            if (model.spelling.isNotEmpty() && !model.privateInput) {
                Row(Modifier.fillMaxWidth(), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                    Text(model.spelling, Modifier.weight(1f).padding(horizontal=12.dp, vertical=4.dp),
                        color=colors.primary, fontSize=16.sp, maxLines=1)
                    if (model.candidates.isNotEmpty()) {
                        val label = if(model.temporary=="Pinyin") "拼音 · 本次" else "拼音"
                        val enabled = baseEnabled && model.pinyinAvailable
                        TextButton(onClick={dispatch(JSONObject().put("BeginTemporary","Pinyin"))}, enabled=enabled,
                            modifier=Modifier.imeAction(label, enabled) {dispatch(JSONObject().put("BeginTemporary","Pinyin"))}) {Text(label)}
                    }
                }
            }
            Row(Modifier.fillMaxWidth().heightIn(min=48.dp), verticalAlignment=androidx.compose.ui.Alignment.CenterVertically) {
                if (model.candidates.isEmpty()) IconButton(onClick={tools=!tools}, modifier=Modifier.imeAction("键盘工具", true) {tools=!tools}) {
                    androidx.compose.foundation.Image(androidx.compose.ui.res.painterResource(R.drawable.mywubi_brand), null, Modifier.size(28.dp))
                }
                if (model.candidates.isNotEmpty() && !model.privateInput) {
                    LazyRow(Modifier.weight(1f).height(48.dp)) {
                        items(model.candidates, key={it.id}) { item -> CandidateButton(item, model.enabled) {choose(item)} }
                    }
                    TextButton(onClick={expanded=!expanded}, modifier=Modifier.imeAction(if(expanded) "收起" else "展开", true) {expanded=!expanded}) {
                        Text(if(expanded) "收起" else "展开")
                    }
                } else {
                    Text(if(model.privateInput) "安全输入" else model.status, Modifier.weight(1f).padding(horizontal=8.dp),
                        color=colors.onSurface, fontSize=14.sp, maxLines=1)
                    if (model.spelling.isEmpty() && model.temporary == null) {
                        val showUpdate = model.idle && model.updateAvailable
                        val label = if (showUpdate) "设置，有可用更新" else "设置"
                        TextButton(onClick=openSettings, modifier=Modifier.imeAction(label, true, openSettings)) {
                            BadgedBox(badge = { if (showUpdate) Badge() }) { Text("设置") }
                        }
                    }
                    val pinyinLabel = if(model.temporary=="Pinyin") "拼音 · 本次" else "拼音"
                    val enabled = baseEnabled && model.pinyinAvailable
                    TextButton(onClick={dispatch(JSONObject().put("BeginTemporary","Pinyin"))}, enabled=enabled,
                        modifier=Modifier.imeAction(pinyinLabel, enabled) {dispatch(JSONObject().put("BeginTemporary","Pinyin"))}) {Text(pinyinLabel)}
                }
            }
            if (tools) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.SpaceEvenly) {
                    TextButton(onClick=openSettings) {Text("设置 / 码表")}
                    TextButton(onClick=switchIme) {Text("切换输入法")}
                    val enabled = baseEnabled && model.pinyinAvailable
                    TextButton(onClick={dispatch(JSONObject().put("BeginTemporary","Pinyin")); tools=false}, enabled=enabled) {Text("临时拼音")}
                }
            }
        }
        val keys: @Composable () -> Unit = {
            if (expanded && model.candidates.isNotEmpty() && !model.privateInput) {
                LazyVerticalGrid(columns=GridCells.Adaptive(88.dp), modifier=Modifier.fillMaxWidth().height(160.dp)) {
                    items(model.candidates,key={it.id}) { item -> CandidateButton(item,model.enabled) {choose(item)} }
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
                                    if (index == 2) Key("⇧",model.enabled,model.epoch,Modifier.width(unit*1.5f),
                                        description=if(upper) "大写" else "小写", function=true, selected=upper) {upper=!upper}
                                    for (character in row) {
                                        val value = if(upper) character.uppercaseChar().toString() else character.toString()
                                        Key(value,model.enabled,model.epoch,Modifier.width(unit)) {input(value)}
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
                                Key(label,model.enabled,model.epoch,Modifier.fillMaxWidth(),function=true) {input(label)}
                            }
                        }
                        Column(Modifier.weight(4.5f)) {
                            for (row in listOf(listOf("1","2","3"),listOf("4","5","6"),listOf("7","8","9"),listOf(".","0",","))) {
                                Row { for(label in row) Key(label,model.enabled,model.epoch,Modifier.weight(1f)) {input(label)} }
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
                            for (character in row) Key(character.toString(),model.enabled,model.epoch,Modifier.weight(1f)) {input(character.toString())}
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
                } else Key(punctuation,model.enabled,model.epoch,Modifier.weight(0.9f)) {input(punctuation)}
                Key("空格",model.enabled,model.epoch,Modifier.weight(3.4f)) {dispatch("Space")}
                Key(if(model.base=="Main" && model.layout=="letters" && !model.privateInput) "。" else ".",model.enabled,model.epoch,Modifier.weight(0.9f)) {
                    input(if(model.base=="Main" && model.layout=="letters" && !model.privateInput) "。" else ".")
                }
                Key(if(model.base=="Main") "中/英" else "英/中",baseEnabled,model.epoch,Modifier.weight(1.3f),description=baseLabel,function=true) {toggleBase()}
                Key(model.enterLabel,model.enabled,model.epoch,Modifier.weight(1.7f),accent=true) {dispatch("Enter")}
            }
        }
        val container=Modifier.fillMaxWidth().background(colors.surface).navigationBarsPadding().padding(horizontal=3.dp,vertical=4.dp)
            .pointerInput(Unit) {
                try {
                    awaitPointerEventScope {
                        while (true) {
                            val event = awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Initial)
                            currentTouch(event.changes.any { it.pressed })
                        }
                    }
                } finally { currentTouch(false) }
            }
        if(LocalConfiguration.current.orientation==android.content.res.Configuration.ORIENTATION_LANDSCAPE) {
            Row(container,horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                Column(Modifier.weight(0.3f).heightIn(max=224.dp).verticalScroll(rememberScrollState())) {controls()}
                Column(Modifier.weight(0.7f)) {keys()}
            }
        } else Column(container) {controls();keys()}
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
                description:String=label,function:Boolean=false,accent:Boolean=false,selected:Boolean=false,action:()->Unit) {
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
        FilledTonalButton(onClick=action,enabled=enabled,modifier=keyModifier.imeAction(description,enabled,action),shape=MaterialTheme.shapes.small,colors=ButtonDefaults.filledTonalButtonColors(containerColor=background,contentColor=foreground),contentPadding=PaddingValues(0.dp)) { KeyLabel(label) }
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
    val icon = when(label) { "⇧" -> R.drawable.ic_shift; "⌫" -> R.drawable.ic_backspace; else -> null }
    if (icon != null) Icon(androidx.compose.ui.res.painterResource(icon), null, Modifier.size(22.dp))
    else Text(label, fontSize=16.sp, maxLines=1)
}

/** Keep the label and action on one accessibility node, including the IME's non-Activity window. */
private fun Modifier.imeAction(label:String,enabled:Boolean,action:()->Unit) = clearAndSetSemantics {
    role=Role.Button;contentDescription=label
    if(!enabled) disabled()
    onClick {if(enabled)action();enabled}
}
