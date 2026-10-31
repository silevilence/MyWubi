package org.mywubi

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.platform.LocalConfiguration
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
import androidx.compose.ui.graphics.Color
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
    val layout: String = "letters", val enterLabel: String = "回车", val privateInput: Boolean = false
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
fun Keyboard(model: KeyboardModel, dispatch: (Any) -> Unit, window: (Int) -> Unit, switchIme: () -> Unit) {
    var upper by remember(model.epoch) { mutableStateOf(false) }
    var symbols by remember(model.epoch,model.layout) { mutableStateOf(model.layout == "number" || model.layout == "phone") }
    var expanded by remember(model.epoch) { mutableStateOf(false) }
    val colors = lightColorScheme(primary=Color(0xFF245D51), onPrimary=Color.White, surface=Color(0xFFF1F3F2), onSurface=Color(0xFF192C27), secondaryContainer=Color(0xFFDEE8E1), onSecondaryContainer=Color(0xFF192C27))
    MaterialTheme(colorScheme=colors) {
        val controls: @Composable () -> Unit = {
            Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.SpaceBetween) {
                Text(if(model.privateInput) "安全输入" else model.spelling.ifEmpty { model.status },
                    Modifier.weight(1f).padding(8.dp),color=colors.onSurface,fontSize=15.sp,maxLines=1)
                TextButton(onClick={expanded=!expanded},enabled=expanded || model.candidates.isNotEmpty(),modifier=Modifier.imeAction(if(expanded) "收起" else "展开",expanded || model.candidates.isNotEmpty()) {expanded=!expanded}) { Text(if(expanded) "收起" else "展开") }
            }
            fun choose(item: CandidateItem) {
                dispatch(JSONObject().put("SelectCandidate",JSONArray().put(JSONObject(model.query)).put(item.id)))
            }
            if (expanded && !model.privateInput) {
                LazyVerticalGrid(columns=GridCells.Adaptive(88.dp),modifier=Modifier.fillMaxWidth().height(168.dp)) {
                    items(model.candidates,key={it.id}) { item -> CandidateButton(item, model.enabled) { choose(item) } }
                }
                Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
                    TextButton(onClick={window((model.offset-256).coerceAtLeast(0))},enabled=model.offset>0) { Text("上一组") }
                    Text("${model.offset+1}–${model.offset+model.candidates.size} / ${model.total}",Modifier.padding(12.dp))
                    TextButton(onClick={window(model.offset+256)},enabled=model.offset+model.candidates.size<model.total) { Text("下一组") }
                }
            } else if (!model.privateInput) {
                LazyRow(Modifier.fillMaxWidth().height(50.dp)) { items(model.candidates,key={it.id}) { item -> CandidateButton(item, model.enabled) { choose(item) } } }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.spacedBy(4.dp)) {
                val baseLabel = if(model.base=="Main") "中 → 英" else "英 → 中"
                val baseEnabled = model.enabled && model.modesAvailable && !model.privateInput
                val toggleBase = { dispatch(JSONObject().put("SetBaseMode",if(model.base=="Main") "English" else "Main")) }
                TextButton(onClick=toggleBase,enabled=baseEnabled,modifier=Modifier.imeAction(baseLabel,baseEnabled,toggleBase)) { Text(baseLabel) }
                val pinyinLabel = if(model.temporary=="Pinyin") "拼音 · 本次" else "拼音"
                val pinyinEnabled = baseEnabled && model.pinyinAvailable
                val beginPinyin = { dispatch(JSONObject().put("BeginTemporary","Pinyin")) }
                TextButton(onClick=beginPinyin,enabled=pinyinEnabled,modifier=Modifier.imeAction(pinyinLabel,pinyinEnabled,beginPinyin)) { Text(pinyinLabel) }
                Spacer(Modifier.weight(1f))
                TextButton(onClick=switchIme) { Text("切换输入法") }
            }
        }
        val keys: @Composable () -> Unit = {
            val rows = if(symbols) {
                if(model.layout=="phone") listOf("123", "456", "789", "+0*#")
                else listOf("1234567890", "@#￥%&*()-+", "，。！？：；'\"/.")
            } else listOf("qwertyuiop", "asdfghjkl", "zxcvbnm")
            for (row in rows) Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(3.dp)) {
                for (key in row) {
                    val label=if(upper && !symbols) key.uppercaseChar().toString() else key.toString()
                    Key(label,model.enabled,model.epoch,Modifier.weight(1f)) {
                        dispatch(JSONObject().put("InputText",JSONArray().put(label).put("Normal")))
                    }
                }
            }
            if (!symbols && model.layout in listOf("url", "email")) {
                Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(3.dp)) {
                    for (label in listOf(if(model.layout == "email") "@" else "/", ".")) {
                        Key(label,model.enabled,model.epoch,Modifier.weight(1f)) {
                            dispatch(JSONObject().put("InputText",JSONArray().put(label).put("Normal")))
                        }
                    }
                }
            }
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(3.dp)) {
                Key(if(upper) "大写" else "小写",model.enabled,model.epoch,Modifier.weight(1.2f)) {upper=!upper}
                Key(if(symbols) "ABC" else "123",model.enabled,model.epoch,Modifier.weight(1f)) {symbols=!symbols}
                Key("空格",model.enabled,model.epoch,Modifier.weight(2.4f)) {dispatch("Space")}
                Key("退格",model.enabled,model.epoch,Modifier.weight(1.2f),repeat=true) {dispatch("Backspace")}
                Key(model.enterLabel,model.enabled,model.epoch,Modifier.weight(1.5f)) {dispatch("Enter")}
            }
        }
        val container = Modifier.fillMaxWidth().background(colors.surface).navigationBarsPadding().padding(4.dp)
        if (LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE) {
            Row(container, horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                Column(Modifier.weight(0.3f).heightIn(max=208.dp).verticalScroll(rememberScrollState())) { controls() }
                Column(Modifier.weight(0.7f)) { keys() }
            }
        } else Column(container) { controls(); keys() }
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
private fun Key(label:String,enabled:Boolean,epoch:Long,modifier:Modifier,repeat:Boolean=false,action:()->Unit) {
    val currentAction by rememberUpdatedState(action)
    val keyModifier=modifier.height(50.dp).padding(vertical=2.dp)
    if (!repeat) {
        FilledTonalButton(onClick=action,enabled=enabled,modifier=keyModifier.imeAction(label,enabled,action),shape=MaterialTheme.shapes.small,contentPadding=PaddingValues(0.dp)) { Text(label,fontSize=16.sp,maxLines=1) }
    } else {
        val lifecycle = LocalLifecycleOwner.current.lifecycle
        var repeatJob by remember { mutableStateOf<Job?>(null) }
        DisposableEffect(lifecycle) {
            val observer = LifecycleEventObserver { _, event ->
                if (event == Lifecycle.Event.ON_STOP || event == Lifecycle.Event.ON_DESTROY) repeatJob?.cancel()
            }
            lifecycle.addObserver(observer)
            onDispose { repeatJob?.cancel(); lifecycle.removeObserver(observer) }
        }
        Surface(modifier=keyModifier.imeAction(label,enabled) {currentAction()}.pointerInput(enabled,epoch) {
        if(enabled) detectTapGestures(onPress={
            coroutineScope {
                var repeated = false
                val job=launch {delay(400);repeated=true;while(isActive){currentAction();delay(65)}}
                repeatJob = job
                try { if (tryAwaitRelease() && !repeated) currentAction() } finally {job.cancel()}
            }
        })
    },shape=MaterialTheme.shapes.small,color=MaterialTheme.colorScheme.secondaryContainer) {
        Box(contentAlignment=androidx.compose.ui.Alignment.Center) {Text(label,fontSize=16.sp)}
    }
    }
}

/** Keep the label and action on one accessibility node, including the IME's non-Activity window. */
private fun Modifier.imeAction(label:String,enabled:Boolean,action:()->Unit) = clearAndSetSemantics {
    role=Role.Button;contentDescription=label
    if(!enabled) disabled()
    onClick {if(enabled)action();enabled}
}
