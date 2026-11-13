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
    private lateinit var predictionController: PredictionController
    private var destroyed = false
    private var runtime: NativeHandle? = null
    private var session: NativeHandle? = null
    private var editorEpoch = 0L
    private var active = false
    private var inputPolicy = InputPolicy.from(0, 0)
    private var hasSelection = false
    private val editor = EditorAdapter()
    private val personalCapture = PersonalCapture()
    private val inputQueue = InputQueue()
    private val clipboardSuggestion = ClipboardSuggestion()
    private var connection: android.view.inputmethod.InputConnection? = null
    private var editorInfo: EditorInfo? = null
    private var supportsPreedit = true
    private var projectedSelection = 0
    private var projectedSelectionEnd = 0
    private var projectedPreedit: Int? = null
    private val ownSelections = ArrayDeque<Int>()
    private var writingHost = false
    private var viewOwner: ImeViewOwner? = null
    private var viewVisible = false
    private val updateScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val idle = KeyboardIdle()
    private var updateAvailable = false
    private val idleRefresh = Runnable { render(); predictionController.learning(false,viewVisible) }
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
        predictionController=PredictionController(this) { refresh() }
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
                        personalCapture.configure(path, config.getJSONObject("config").optJSONObject("personal") ?: JSONObject(),config.getJSONObject("config").optJSONObject("prediction") ?: JSONObject())
                        predictionController.configurePersonal(path)
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
        .put("allows_assistance", inputPolicy.purpose == "Text" && (editorInfo?.imeOptions ?: 0) and EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING == 0)
        .put("enter_action", inputPolicy.action)

    private fun attachEditor() {
        personalCapture.gate(active && hostContext().optBoolean("allows_assistance"), editorEpoch.toString())
        pinyinMode = KeyMode.Off
        val r = runtime ?: return
        if (!active) return
        editor.abandon()
        ownSelections.clear(); projectedPreedit = null
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
        clipboardSuggestion.dismiss()
        inputQueue.invalidate()
        connection = nextConnection
        editorInfo = attribute
        supportsPreedit = attribute?.inputType != android.text.InputType.TYPE_NULL &&
            !getSharedPreferences("android-ui", 0).getBoolean("local_preedit", false)
        inputPolicy = InputPolicy.from(attribute?.inputType ?: 0, attribute?.imeOptions ?: 0)
        hasSelection = attribute != null && attribute.initialSelStart >= 0 && attribute.initialSelStart != attribute.initialSelEnd
        projectedSelection = minOf(attribute?.initialSelStart ?: 0, attribute?.initialSelEnd ?: 0).coerceAtLeast(0)
        projectedSelectionEnd = maxOf(attribute?.initialSelStart ?: -1, attribute?.initialSelEnd ?: -1)
        editorEpoch++; active = true
        keyboardActivity()
        attachEditor(); refresh()
    }

    override fun onFinishInput() {
        personalCapture.gate(false, "inactive")
        clipboardSuggestion.dismiss()
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
        val ownIndex = if (!hasSelection) ownSelections.indexOf(newSelEnd) else -1
        val own = writingHost || ownIndex >= 0
        if (ownIndex >= 0) repeat(ownIndex + 1) { ownSelections.removeFirst() }
        if (active && !own && (oldSelStart != newSelStart || oldSelEnd != newSelEnd)) {
            personalCapture.observe(connection, newSelStart, newSelEnd, true)
            ownSelections.clear(); projectedSelection = minOf(newSelStart,newSelEnd).coerceAtLeast(0)
            projectedSelectionEnd = maxOf(newSelStart,newSelEnd)
            projectedPreedit = candidatesStart.takeIf { it >= 0 }
            invalidatePrediction(); refresh()
        }
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
            }, hideKeyboard = { requestHideSelf(0) }, openSettings = {
                startActivity(android.content.Intent(this@MyWubiService, MainActivity::class.java)
                    .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
            }) {
                getSystemService(InputMethodManager::class.java).showInputMethodPicker()
            } }
        }
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info,restarting)
        if (!viewVisible) {
            val allowed = inputPolicy.purpose != "Password" &&
                (editorInfo?.imeOptions ?: 0) and EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING == 0 &&
                getSharedPreferences("android-ui", 0).getBoolean("clipboard_suggestion", true)
            val clip = if (allowed) runCatching {
                getSystemService(android.content.ClipboardManager::class.java).primaryClip
            }.getOrNull() else null
            clipboardSuggestion.offer(clip, allowed)
        }
        viewVisible = true; viewOwner?.visible(true); keyboardActivity(); refresh()
        AppUpdates.get(this).check()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        predictionController.learning(true,false)
        clipboardSuggestion.dismiss()
        predictionController.pause()
        invalidatePrediction()
        inputQueue.invalidate()
        // Returning to the same editor can skip onFinishInput on newer Android.
        // A hidden private query must still be cancelled without touching host text.
        session?.let { current ->
            if (NativeBridge.objectCall("State", "session" to current.value)
                    .optJSONObject("view")?.optJSONObject("expression") != null) {
                applyPlan(NativeBridge.objectCall("Prepare", "session" to current.value, "action" to "EndTemporary"))
            }
        }
        viewVisible = false; idle.touch(false, android.os.SystemClock.elapsedRealtime())
        main.removeCallbacks(idleRefresh); refresh(); viewOwner?.visible(false)
        super.onFinishInputView(finishingInput)
    }

    private fun render() {
        if (destroyed) return
        val items = candidates.optJSONArray("candidates") ?: JSONArray()
        val spelling = state.optJSONObject("view")?.optString("spelling") ?: ""
        val shortcutError = state.optJSONObject("view")?.optString("shortcut_error")?.takeIf { it.isNotBlank() && it != "null" }?.let { "日期时间不可用：$it" }
        val temporary = state.optString("temporary").takeIf { it in listOf("Pinyin", "English", "Emoji", "Kaomoji", "Kana", "Math", "Punctuation", "Symbols") }
        val stableIdle = active && viewVisible && session != null && idle.isIdle(
            android.os.SystemClock.elapsedRealtime(), spelling, items.length() > 0 || state.optBoolean("prediction_waiting"), editor.ownsPreedit, temporary != null)
        keyboard = KeyboardModel(
            expression = state.optJSONObject("view")?.optJSONObject("expression")?.let { query ->
                ExpressionQueryUi(query.getString("kind"), query.getString("phase"), query.getString("text"),
                    query.optString("message").takeIf { it.isNotBlank() && it != "null" }, query.optString("input", "Words"))
            },
            specialCapabilities = listOf("Emoji", "Kaomoji", "Kana", "Math", "Punctuation", "Symbols").associateWith { mode ->
                val capability = state.optJSONObject(mode.lowercase(java.util.Locale.ROOT))
                (capability?.optBoolean("available") == true) to capability?.optString("reason")?.takeIf { it != "null" }
            },
            emojiAvailable = state.optJSONObject("emoji")?.optBoolean("available") == true,
            kaomojiAvailable = state.optJSONObject("kaomoji")?.optBoolean("available") == true,
            emojiReason = state.optJSONObject("emoji")?.optString("reason")?.takeIf { it != "null" },
            kaomojiReason = state.optJSONObject("kaomoji")?.optString("reason")?.takeIf { it != "null" },
            clipboard = clipboardSuggestion.preview(),
            epoch = editorEpoch, enabled = active && viewVisible && session != null,
            spelling = spelling,
            idle = stableIdle, updateAvailable = updateAvailable,
            status = error ?: shortcutError ?: if(session == null) "正在加载码表…" else tableLabel,
            statusError = error != null || shortcutError != null,
            pinyinMode = pinyinMode,
            base = state.optString("base", "Main"),
            temporary = temporary,
            pinyinAvailable = state.optJSONObject("pinyin")?.optBoolean("available") == true,
            modesAvailable = inputPolicy.purpose == "Text",
            layout = inputPolicy.layout,
            enterLabel = inputPolicy.enterLabel,
            privateInput = inputPolicy.purpose == "Password",
            prediction = state.optJSONObject("view")?.optBoolean("prediction") == true,
            candidates = (0 until items.length()).map { i ->
                val item = items.getJSONObject(i)
                val hints = item.getJSONArray("hints")
                val hint = (0 until hints.length()).joinToString(" ") { n ->
                    val h = hints.getJSONObject(n)
                    when {
                        h.has("CodeSuffix") -> h.getString("CodeSuffix")
                        h.has("ExpressionName") -> h.getString("ExpressionName")
                        h.has("ExpressionMatch") -> h.getString("ExpressionMatch")
                        h.has("DateTime") -> "日期时间"
                        h.has("ReverseCodes") -> {
                            val codes = h.getJSONArray("ReverseCodes")
                            (0 until codes.length()).joinToString("/") { codes.getString(it) }
                        }
                        else -> ""
                    }
                }
                CandidateItem(item.getLong("id"),item.getString("text"),hint,
                    item.optString("expression_kind").takeIf { it in listOf("emoji", "kaomoji", "kana", "math", "punctuation", "symbols") },
                    (0 until hints.length()).any { hints.getJSONObject(it).has("DateTime") })
            },
            query = candidates.optJSONObject("query_revision")?.toString() ?: "{}",
            total = candidates.optInt("total"), offset = candidates.optInt("offset")
        )
    }

    private fun dispatch(action: Any) {
        if (!active || !viewVisible) return
        val request = if (action == "PasteClipboard") clipboardSuggestion.text?.let(::PasteClipboard) ?: return else action
        clipboardSuggestion.dismiss()
        keyboardActivity()
        if (request == "DismissClipboard") return
        if (getSharedPreferences("android-ui", 0).getBoolean("vibration", false)) {
            window?.window?.decorView?.performHapticFeedback(android.view.HapticFeedbackConstants.KEYBOARD_TAP)
        }
        if (!inputQueue.submit { dispatchSerial(request) }) {
            error = "输入队列已满，请稍后重试"; render()
        }
    }

    private fun keyboardActivity() {
        if(::predictionController.isInitialized) predictionController.learning(true,viewVisible)
        idle.activity(android.os.SystemClock.elapsedRealtime())
        main.removeCallbacks(idleRefresh)
        main.postDelayed(idleRefresh, 1800)
        render()
    }

    override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent?): Boolean {
        clipboardSuggestion.dismiss()
        keyboardActivity()
        return super.onKeyDown(keyCode, event)
    }

    private fun dispatchSerial(request: Any) {
        personalCapture.method=if(request is JSONObject) request.keys().asSequence().firstOrNull() ?: "unknown" else if(request is String) request else "external"
        predictionController.cancel()
        val s = session ?: return
        val host = connection ?: run { error = "输入连接暂不可用"; render(); return }
        val epoch = editorEpoch
        if (!active || !viewVisible) return
        candidateOffset = 0
        try {
            syncPredictionPrefix()
            val recovery = NativeBridge.call("Recover", "session" to s.value) as? JSONObject
            if (recovery != null && !applyPlan(recovery)) { refresh(); return }
            if (request == "ClearEditor" || request is PasteClipboard) {
                val localQuery = NativeBridge.objectCall("State", "session" to s.value)
                    .optJSONObject("view")?.optJSONObject("expression") != null
                if(localQuery) {
                    if(request == "ClearEditor") applyPlan(NativeBridge.objectCall("Prepare", "session" to s.value, "action" to "EndTemporary"))
                    refresh(); return
                }
                fun current() = active && viewVisible && epoch == editorEpoch && connection === host
                // A stale paste button must never replace a newly started composition.
                if (request is PasteClipboard && (editor.ownsPreedit ||
                    NativeBridge.objectCall("State", "session" to s.value).optJSONObject("view")?.optString("spelling").orEmpty().isNotEmpty())) return
                invalidatePrediction()
                writingHost = true
                val applied = try {
                    if (request is PasteClipboard) EditorShortcuts.paste(host, request.text, ::current)
                    else EditorShortcuts.clear(host, ::current)
                } finally { writingHost = false }
                if (current()) {
                    editor.abandon()
                    editorEpoch++; inputQueue.invalidate(); attachEditor()
                    error = if (applied) null else "当前应用未确认编辑操作，请检查输入框后继续"
                    if (!applied) android.widget.Toast.makeText(this, error, android.widget.Toast.LENGTH_SHORT).show()
                }
                refresh()
                return
            }
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
                if (current.optString("temporary") != "Pinyin" &&
                    current.optJSONObject("view")?.optJSONObject("expression") == null) {
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
            val operation = plan.optJSONObject("op")
            if (!editor.ownsPreedit) personalCapture.observe(host, projectedSelection, projectedSelectionEnd)
            val start = projectedPreedit ?: projectedSelection
            val expected = when {
                operation?.has("CompositionUpdate") == true -> (start + operation.getJSONObject("CompositionUpdate").getString("spelling").length).also { projectedPreedit = start }
                operation?.has("CommitAndReplace") == true -> (start + operation.getJSONObject("CommitAndReplace").getString("text").length).also { projectedPreedit = null }
                operation?.has("CommitThenUpdate") == true -> operation.getJSONObject("CommitThenUpdate").let { p ->
                    projectedPreedit = start + p.getString("text").length
                    projectedPreedit!! + p.getString("spelling").length
                }
                operation?.has("EndComposition") == true -> (if(operation.getJSONObject("EndComposition").getBoolean("delete_text")) start else projectedSelection).also { projectedPreedit = null }
                else -> null
            }
            if (expected != null) {
                projectedSelection = expected; ownSelections.addLast(expected)
                projectedSelectionEnd = expected
                while (ownSelections.size > 32) ownSelections.removeFirst()
            }
            writingHost = true
            val outcome = try { editor.execute(host, plan, ::current) } finally { writingHost = false }
            if (plan.optBoolean("learnable", true)) personalCapture.operation(host, operation, outcome, projectedSelection, projectedSelection)
            if (outcome != "Applied") { ownSelections.clear(); projectedPreedit = null }
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
        personalCapture.display(state, candidates, predictionController.personalVersion)
        session?.let { predictionController.update(it.value,active && viewVisible && inputPolicy.purpose=="Text") }
    }
    private fun invalidatePrediction() {
        if(::predictionController.isInitialized) predictionController.cancel()
        val s = session ?: return
        NativeBridge.call("PredictionContext", "session" to s.value, "editor" to editorEpoch,
            "epoch" to editorEpoch, "prefix" to null, "invalidate" to true)
    }
    private fun syncPredictionPrefix() {
        val s = session ?: return
        if ((!state.optBoolean("prediction_enabled") && !state.optBoolean("personal_prediction_enabled")) || editor.ownsPreedit) return
        val private = inputPolicy.purpose != "Text" ||
            (editorInfo?.imeOptions ?: 0) and EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING != 0
        if (private || hasSelection) { invalidatePrediction(); return }
        val before = try { connection?.getTextBeforeCursor(512, 0)?.toString()?.let(PredictionPrefix::bounded) } catch (_: RuntimeException) { null }
        val right = if(state.optBoolean("personal_prediction_enabled")) try {connection?.getTextAfterCursor(512,0)?.toString()}catch(_:RuntimeException){null} else null
        NativeBridge.call("PredictionContext", "session" to s.value, "editor" to editorEpoch,
            "epoch" to editorEpoch, "prefix" to before, "right" to right, "invalidate" to false)
    }

    override fun onDestroy() {
        personalCapture.close()
        if(::predictionController.isInitialized) predictionController.close()
        destroyed = true; active = false; connection = null; inputQueue.invalidate()
        main.removeCallbacks(idleRefresh); updateScope.cancel()
        loads.next(); updates.unregisterOnSharedPreferenceChangeListener(updateListener)
        session?.let { NativeBridge.call("Close", "session" to it.value); it.close() }
        session = null
        runtime?.close(); runtime = null; loader.shutdown(); viewOwner?.destroy(); viewOwner = null
        super.onDestroy()
    }
    override fun onTrimMemory(level:Int) {
        super.onTrimMemory(level)
        if(level>=android.content.ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW && ::predictionController.isInitialized) predictionController.pause()
    }
    override fun onLowMemory() {
        super.onLowMemory()
        if(::predictionController.isInitialized) predictionController.pause()
    }
}
