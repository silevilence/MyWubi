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
    private var viewOwner: ImeViewOwner? = null
    private var viewVisible = false
    private var candidateOffset = 0
    private var keyboard by mutableStateOf(KeyboardModel())
    private var error: String? = null
    private var state = JSONObject()
    private var candidates = JSONObject()

    override fun onCreate() {
        super.onCreate()
        loader.execute {
            var loaded: NativeHandle? = null
            try {
                val path = Resources.prepare(this)
                loaded = NativeHandle(NativeBridge.objectCall("BuildRuntime", "path" to path.path).getLong("handle"))
                val result = loaded
                main.post {
                    if (destroyed) result.close() else { runtime = result; attachEditor(); refresh() }
                }
            } catch (_: Exception) {
                loaded?.close()
                main.post { error = "输入资源加载失败，请重新打开输入法"; render() }
            }
        }
    }

    private fun hostContext() = JSONObject().put("editor", editorEpoch).put("epoch", editorEpoch)
        .put("supports_preedit", inputPolicy.purpose == "Text").put("supports_insert", true).put("owns_preedit", false)
        .put("purpose", inputPolicy.purpose).put("selection_available", true).put("surrounding_available", true)
        .put("enter_action", inputPolicy.action)

    private fun attachEditor() {
        val r = runtime ?: return
        if (!active) return
        editor.abandon()
        val s = session
        if (s == null) session = NativeHandle(NativeBridge.objectCall("CreateSession", "runtime" to r.value, "context" to hostContext()).getLong("handle"))
        else NativeBridge.call("Context", "session" to s.value, "context" to hostContext(), "preserve_base" to true)
    }

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        inputPolicy = InputPolicy.from(attribute?.inputType ?: 0, attribute?.imeOptions ?: 0)
        hasSelection = attribute != null && attribute.initialSelStart >= 0 && attribute.initialSelStart != attribute.initialSelEnd
        editorEpoch++; active = true
        attachEditor(); refresh()
    }

    override fun onFinishInput() {
        active = false; editorEpoch++; editor.abandon()
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
            currentInputConnection?.finishComposingText()
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
            setContent { Keyboard(keyboard, ::dispatch, { candidateOffset = it; refresh() }) {
                getSystemService(InputMethodManager::class.java).showInputMethodPicker()
            } }
        }
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info,restarting)
        viewVisible = true; viewOwner?.visible(true); refresh()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        viewVisible = false; render(); viewOwner?.visible(false)
        super.onFinishInputView(finishingInput)
    }

    private fun render() {
        val items = candidates.optJSONArray("candidates") ?: JSONArray()
        keyboard = KeyboardModel(
            epoch = editorEpoch, enabled = active && viewVisible && session != null,
            spelling = state.optJSONObject("view")?.optString("spelling") ?: "",
            status = error ?: if(session == null) "正在加载码表…" else if(state.optString("temporary") == "Pinyin") "本次拼音" else if(state.optString("base") == "English") "英文" else "五笔 · 示例码表",
            base = state.optString("base", "Main"),
            temporary = state.optString("temporary").takeIf { it == "Pinyin" || it == "English" },
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
        val s = session ?: return
        val connection = currentInputConnection ?: return
        if (!active) return
        candidateOffset = 0
        try {
            val plan = NativeBridge.objectCall("Prepare", "session" to s.value, "action" to action)
            applyPlan(plan)
            if (plan.getString("disposition") == "NotHandled") when (action) {
                "Space" -> connection.commitText(" ",1)
                "Enter" -> inputPolicy.enter(connection)
                "Backspace" -> UnicodeEditor.backspace(connection, hasSelection)
            }
            error = null
        } catch (_: ProtocolException) { error = "当前操作不可用" }
        refresh()
    }

    private fun applyPlan(initial: JSONObject) {
        val s = session ?: return
        val connection = currentInputConnection ?: return
        val epoch = editorEpoch
        var plan: JSONObject? = initial
        while (plan != null) {
            val token = plan.getJSONObject("token")
            if (!active || epoch != editorEpoch || NativeBridge.call("IsCurrent", "session" to s.value, "token" to token) != true) return
            val outcome = editor.execute(connection, plan)
            val ack = NativeBridge.objectCall("Ack", "session" to s.value, "token" to token, "outcome" to outcome, "owns_preedit" to editor.ownsPreedit)
            plan = ack.optJSONObject("follow_up")
        }
    }

    private fun refresh() {
        session?.let {
            state = NativeBridge.objectCall("State", "session" to it.value)
            candidates = NativeBridge.objectCall("Candidates", "session" to it.value, "offset" to candidateOffset, "count" to 256)
        }
        render()
    }

    override fun onDestroy() {
        destroyed = true; active = false
        session?.let { NativeBridge.call("Close", "session" to it.value); it.close() }
        runtime?.close(); loader.shutdown(); viewOwner?.destroy(); viewOwner = null
        super.onDestroy()
    }
}
