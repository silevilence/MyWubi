package org.mywubi

import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
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
    private val editor = EditorAdapter()
    private var panel: LinearLayout? = null
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
        .put("supports_preedit", true).put("supports_insert", true).put("owns_preedit", false)
        .put("purpose", "Text").put("selection_available", true).put("surrounding_available", true)
        .put("enter_action", "Newline")

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
        editorEpoch++; active = true
        attachEditor(); refresh()
    }

    override fun onFinishInput() {
        active = false; editorEpoch++; editor.abandon()
        session?.let { NativeBridge.call("Context", "session" to it.value, "context" to hostContext(), "preserve_base" to true) }
        refresh(); super.onFinishInput()
    }

    override fun onEvaluateFullscreenMode() = false
    override fun onCreateInputView(): View = LinearLayout(this).also { it.orientation = LinearLayout.VERTICAL; panel = it; render() }

    private fun render() {
        val root = panel ?: return
        root.removeAllViews()
        root.addView(TextView(this).apply { text = error ?: state.optJSONObject("view")?.optString("spelling") ?: "正在加载码表…" })
        val row = LinearLayout(this)
        val items = candidates.optJSONArray("candidates") ?: JSONArray()
        for (i in 0 until minOf(items.length(), 5)) {
            val item = items.getJSONObject(i)
            val revision = candidates.getJSONObject("query_revision")
            row.addView(Button(this).apply {
                text = item.getString("text")
                setOnClickListener { dispatch(JSONObject().put("SelectCandidate", JSONArray().put(revision).put(item.getLong("id")))) }
            })
        }
        root.addView(row)
        for (letters in listOf("qwertyuiop", "asdfghjkl", "zxcvbnm")) {
            root.addView(LinearLayout(this).apply {
                for (letter in letters) addView(Button(context).apply {
                    text = letter.toString(); setPadding(0,0,0,0)
                    setOnClickListener { dispatch(JSONObject().put("InputText", JSONArray().put(letter.toString()).put("Normal"))) }
                }, LinearLayout.LayoutParams(0, 120, 1f))
            })
        }
        root.addView(LinearLayout(this).apply {
            for ((label, action) in listOf("退格" to "Backspace", "空格" to "Space", "回车" to "Enter"))
                addView(Button(context).apply { text = label; setOnClickListener { dispatch(action) } }, LinearLayout.LayoutParams(0,120,1f))
        })
    }

    private fun dispatch(action: Any) {
        val s = session ?: return
        val connection = currentInputConnection ?: return
        if (!active) return
        try {
            val plan = NativeBridge.objectCall("Prepare", "session" to s.value, "action" to action)
            applyPlan(plan)
            if (plan.getString("disposition") == "NotHandled") when (action) {
                "Space" -> connection.commitText(" ",1)
                "Enter" -> connection.commitText("\n",1)
                "Backspace" -> connection.deleteSurroundingTextInCodePoints(1,0)
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
            candidates = NativeBridge.objectCall("Candidates", "session" to it.value, "offset" to 0, "count" to 256)
        }
        render()
    }

    override fun onDestroy() {
        destroyed = true; active = false
        session?.let { NativeBridge.call("Close", "session" to it.value); it.close() }
        runtime?.close(); loader.shutdown(); panel = null
        super.onDestroy()
    }
}
