package org.mywubi

import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors
import kotlinx.coroutines.*

class MyWubiService : InputMethodService() {
    private val loader = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    private var destroyed = false
    private var runtime: NativeHandle? = null
    private var session: NativeHandle? = null
    private var editorEpoch = 0L
    private var active = false
    private var inputPolicy = InputPolicy.from(0, 0)
    private var hasSelection = false
    private val editor = EditorAdapter()
    private val inputQueue = InputQueue()
    private var connection: android.view.inputmethod.InputConnection? = null
    private var editorInfo: EditorInfo? = null
    private var supportsPreedit = true
    private var viewOwner: ImeViewOwner? = null
    private var viewVisible = false
    private val updateScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val idle = KeyboardIdle()
    private var updateAvailable = false
    private val idleRefresh = Runnable { render() }
    private var candidateOffset = 0
    private var keyboard by mutableStateOf(KeyboardModel())
    private var error: String? = null
    private var state = JSONObject()
    private var candidates = JSONObject()
    private var tableLabel = ""
    private var pinyinMode = KeyMode.Off
    private val loads = LoadGeneration()
    private val updates by lazy { EngineUpdates.preferences(this) }
    private val updateListener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == "requested") loadRuntime()
    }

    override fun onCreate() {
        super.onCreate()
        updates.registerOnSharedPreferenceChangeListener(updateListener)
        updateScope.launch {
            AppUpdates.get(this@MyWubiService).state.collect {
                updateAvailable = it.available != null
                render()
            }
        }
        loadRuntime()
    }

    private fun loadRuntime() {
        val ticket = loads.next()
        val requested = updates.getString("requested", "initial")
        loader.execute {
            var loaded: NativeHandle? = null
            try {
                val path = Resources.prepare(this)
                loaded = NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to path.path).getLong("handle"))
                val config = NativeBridge.objectCall("ReadConfig", "path" to path.path)
                NativeHandle(config.getLong("version")).close()
                val label = TableCatalog(this).label(config.getJSONObject("config").getJSONObject("dictionary").getString("system_table"))
                val result = loaded
                main.post {
                    if (destroyed || !loads.accepts(ticket)) result.close() else {
                        val previous = runtime
                        runtime = result
                        try {
                            val s = session
                            if (s != null && active) {
                                val plan = NativeBridge.call("ApplyRuntime", "session" to s.value, "runtime" to result.value) as? JSONObject
                                if (plan != null) applyPlan(plan)
                            } else {
                                session?.close(); session = null; attachEditor()
                            }
                            updates.edit().putString("applied", requested).putString("error", null).apply()
                            tableLabel = label
                            pinyinMode = KeyMode.Off
                            error = null
                        } catch (_: RuntimeException) {
                            error = "配置已保存，输入会话更新失败；请重新进入输入框"
                            updates.edit().putString("error", error).apply()
                            session?.close(); session = null; attachEditor()
                        } finally { previous?.close(); refresh() }
                    }
                }
            } catch (_: Exception) {
                loaded?.close()
                main.post {
                    if (!destroyed && loads.accepts(ticket)) {
                        error = if (runtime == null) "输入资源加载失败，请在设置中修复" else "配置已保存但未应用，继续使用原码表"
                        updates.edit().putString("error", error).apply(); render()
                    }
                }
            }
        }
    }

    private fun hostContext() = JSONObject().put("editor", editorEpoch).put("epoch", editorEpoch)
        .put("supports_preedit", supportsPreedit && inputPolicy.purpose == "Text").put("supports_insert", true).put("owns_preedit", false)
        .put("purpose", inputPolicy.purpose).put("selection_available", true).put("surrounding_available", true)
        .put("enter_action", inputPolicy.action)

    private fun attachEditor() {
        pinyinMode = KeyMode.Off
        val r = runtime ?: return
        if (!active) return
        editor.abandon()
        val s = session
        if (s == null) session = NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to r.value, "context" to hostContext()).getLong("handle"))
        else NativeBridge.call("Context", "session" to s.value, "context" to hostContext(), "preserve_base" to true)
    }

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        val nextConnection = currentInputConnection
        val sameEditor = restarting && active && connection === nextConnection &&
            editorInfo?.packageName == attribute?.packageName && editorInfo?.fieldId == attribute?.fieldId &&
            editorInfo?.inputType == attribute?.inputType && editorInfo?.imeOptions == attribute?.imeOptions
        if (sameEditor) { refresh(); return }
        inputQueue.invalidate()
        connection = nextConnection
        editorInfo = attribute
        supportsPreedit = attribute?.inputType != android.text.InputType.TYPE_NULL &&
            !getSharedPreferences("android-ui", 0).getBoolean("local_preedit", false)
        inputPolicy = InputPolicy.from(attribute?.inputType ?: 0, attribute?.imeOptions ?: 0)
        hasSelection = attribute != null && attribute.initialSelStart >= 0 && attribute.initialSelStart != attribute.initialSelEnd
        editorEpoch++; active = true
        keyboardActivity()
        attachEditor(); refresh()
    }

    override fun onFinishInput() {
        inputQueue.invalidate(); connection = null; editorInfo = null
        active = false; editorEpoch++; editor.abandon()
        pinyinMode = KeyMode.Off
        session?.let { NativeBridge.call("Context", "session" to it.value, "context" to hostContext(), "preserve_base" to true) }
        refresh(); super.onFinishInput()
    }

    override fun onEvaluateFullscreenMode() = false

    override fun onUpdateSelection(oldSelStart: Int, oldSelEnd: Int, newSelStart: Int, newSelEnd: Int, candidatesStart: Int, candidatesEnd: Int) {
        super.onUpdateSelection(oldSelStart, oldSelEnd, newSelStart, newSelEnd, candidatesStart, candidatesEnd)
        hasSelection = newSelStart >= 0 && newSelStart != newSelEnd
        if (active && editor.ownsPreedit && (candidatesStart < 0 || newSelStart != newSelEnd || newSelEnd != candidatesEnd)) {
            // The host moved its selection. Preserve text and release our old range before
            // the next action; never use stale composition state to delete the new selection.
            try { currentInputConnection?.finishComposingText() } catch (_: RuntimeException) {
                error = "输入连接暂不可用"
            }
            inputQueue.invalidate()
            editor.abandon()
            editorEpoch++
            attachEditor()
            refresh()
        }
    }
    override fun onCreateInputView(): View {
        viewOwner?.destroy()
        val owner = ImeViewOwner().also { viewOwner = it; it.visible(viewVisible) }
        window?.window?.decorView?.let {
            it.setViewTreeLifecycleOwner(owner)
            it.setViewTreeSavedStateRegistryOwner(owner)
        }
        return ComposeView(this).apply {
            setViewTreeLifecycleOwner(owner)
            setViewTreeSavedStateRegistryOwner(owner)
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)
            setContent { Keyboard(keyboard, ::dispatch, { keyboardActivity(); candidateOffset = it; refresh() }, onTouch = { pressed ->
                idle.touch(pressed, android.os.SystemClock.elapsedRealtime())
                keyboardActivity()
            }, openSettings = {
                startActivity(android.content.Intent(this@MyWubiService, MainActivity::class.java)
                    .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
            }) {
                getSystemService(InputMethodManager::class.java).showInputMethodPicker()
            } }
        }
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info,restarting)
        viewVisible = true; viewOwner?.visible(true); keyboardActivity(); refresh()
        AppUpdates.get(this).check()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        viewVisible = false; idle.touch(false, android.os.SystemClock.elapsedRealtime())
        main.removeCallbacks(idleRefresh); render(); viewOwner?.visible(false)
        super.onFinishInputView(finishingInput)
    }

    private fun render() {
        if (destroyed) return
        val items = candidates.optJSONArray("candidates") ?: JSONArray()
        val spelling = state.optJSONObject("view")?.optString("spelling") ?: ""
        val temporary = state.optString("temporary").takeIf { it == "Pinyin" || it == "English" }
        val stableIdle = active && viewVisible && session != null && idle.isIdle(
            android.os.SystemClock.elapsedRealtime(), spelling, items.length() > 0, editor.ownsPreedit, temporary != null)
        keyboard = KeyboardModel(
            epoch = editorEpoch, enabled = active && viewVisible && session != null,
            spelling = spelling,
            idle = stableIdle, updateAvailable = updateAvailable,
            status = error ?: if(session == null) "正在加载码表…" else tableLabel,
            pinyinMode = pinyinMode,
            base = state.optString("base", "Main"),
            temporary = temporary,
            pinyinAvailable = state.optJSONObject("pinyin")?.optBoolean("available") == true,
            modesAvailable = inputPolicy.purpose == "Text",
            layout = inputPolicy.layout,
            enterLabel = inputPolicy.enterLabel,
            privateInput = inputPolicy.purpose == "Password",
            candidates = (0 until items.length()).map { i ->
                val item = items.getJSONObject(i)
                val hints = item.getJSONArray("hints")
                val hint = (0 until hints.length()).joinToString(" ") { n ->
                    val h = hints.getJSONObject(n)
                    if(h.has("CodeSuffix")) h.getString("CodeSuffix") else {
                        val codes=h.getJSONArray("ReverseCodes")
                        (0 until codes.length()).joinToString("/") { codes.getString(it) }
                    }
                }
                CandidateItem(item.getLong("id"),item.getString("text"),hint)
            },
            query = candidates.optJSONObject("query_revision")?.toString() ?: "{}",
            total = candidates.optInt("total"), offset = candidates.optInt("offset")
        )
    }

    private fun dispatch(action: Any) {
        if (!active || !viewVisible) return
        keyboardActivity()
        if (getSharedPreferences("android-ui", 0).getBoolean("vibration", false)) {
            window?.window?.decorView?.performHapticFeedback(android.view.HapticFeedbackConstants.KEYBOARD_TAP)
        }
        if (!inputQueue.submit { dispatchSerial(action) }) {
            error = "输入队列已满，请稍后重试"; render()
        }
    }

    private fun keyboardActivity() {
        idle.activity(android.os.SystemClock.elapsedRealtime())
        main.removeCallbacks(idleRefresh)
        main.postDelayed(idleRefresh, 1800)
        render()
    }

    override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent?): Boolean {
        keyboardActivity()
        return super.onKeyDown(keyCode, event)
    }

    private fun dispatchSerial(request: Any) {
        val s = session ?: return
        val host = connection ?: run { error = "输入连接暂不可用"; render(); return }
        val epoch = editorEpoch
        if (!active || !viewVisible) return
        candidateOffset = 0
        try {
            val recovery = NativeBridge.call("Recover", "session" to s.value) as? JSONObject
            if (recovery != null && !applyPlan(recovery)) { refresh(); return }
            val cycling = request == "CyclePinyin"
            val nextPinyin = if (cycling) pinyinMode.next() else pinyinMode
            if (cycling && nextPinyin == KeyMode.Locked) {
                pinyinMode = nextPinyin; refresh(); return
            }
            val action = if (!cycling) request else if (nextPinyin == KeyMode.Off) "EndTemporary"
                else JSONObject().put("BeginTemporary", "Pinyin")
            val plan = NativeBridge.objectCall("Prepare", "session" to s.value, "action" to action)
            val applied = applyPlan(plan)
            if (applied && active && epoch == editorEpoch && connection === host &&
                plan.getString("disposition") == "NotHandled") when (action) {
                "Space" -> host.commitText(" ",1)
                "Enter" -> inputPolicy.enter(host)
                "Backspace" -> UnicodeEditor.backspace(host, hasSelection)
            }
            if (applied && active && epoch == editorEpoch && connection === host) {
                error = null
                if (cycling) pinyinMode = nextPinyin
                if (action is JSONObject && action.has("SetBaseMode")) pinyinMode = KeyMode.Off
                val current = NativeBridge.objectCall("State", "session" to s.value)
                if (current.optString("temporary") != "Pinyin") {
                    if (pinyinMode == KeyMode.Locked && current.optJSONObject("pinyin")?.optBoolean("available") == true) {
                        val resume = NativeBridge.objectCall("Prepare", "session" to s.value,
                            "action" to JSONObject().put("BeginTemporary", "Pinyin"))
                        if (!applyPlan(resume)) pinyinMode = KeyMode.Off
                    } else pinyinMode = KeyMode.Off
                }
            } else pinyinMode = KeyMode.Off
        } catch (_: ProtocolException) { error = "当前操作不可用" }
        catch (_: RuntimeException) {
            // Never retry an editor operation after an unknown connection failure.
            editor.abandon()
            editorEpoch++; inputQueue.invalidate(); attachEditor()
            error = "输入连接暂不可用"
        }
        refresh()
    }

    private fun applyPlan(initial: JSONObject): Boolean {
        val s = session ?: return false
        val host = connection ?: return false
        val epoch = editorEpoch
        var plan: JSONObject? = initial
        while (plan != null) {
            val token = plan.getJSONObject("token")
            fun current() = active && epoch == editorEpoch && connection === host &&
                NativeBridge.call("IsCurrent", "session" to s.value, "token" to token) == true
            if (!current()) return false
            val outcome = editor.execute(host, plan, ::current)
            if (!current()) return false
            val ack = NativeBridge.objectCall("Ack", "session" to s.value, "token" to token, "outcome" to outcome, "owns_preedit" to editor.ownsPreedit)
            if (outcome != "Applied") error = "编辑结果未确认，已停止本次输入；请检查正文后继续"
            plan = ack.optJSONObject("follow_up")
            if (outcome != "Applied" && plan == null) return false
        }
        return true
    }

    private fun refresh() {
        if (destroyed) return
        session?.let {
            state = NativeBridge.objectCall("State", "session" to it.value)
            candidates = NativeBridge.objectCall("Candidates", "session" to it.value, "offset" to candidateOffset, "count" to 256)
        }
        render()
    }

    override fun onDestroy() {
        destroyed = true; active = false; connection = null; inputQueue.invalidate()
        main.removeCallbacks(idleRefresh); updateScope.cancel()
        loads.next(); updates.unregisterOnSharedPreferenceChangeListener(updateListener)
        session?.let { NativeBridge.call("Close", "session" to it.value); it.close() }
        session = null
        runtime?.close(); runtime = null; loader.shutdown(); viewOwner?.destroy(); viewOwner = null
        super.onDestroy()
    }
}
