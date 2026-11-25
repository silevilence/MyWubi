package org.mywubi

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.text.InputType
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import java.util.UUID

/** Settings and the IME share a process. Only the marked editor receives the prepared preview. */
internal object SkinTrials {
    private var token: String? = null
    private var preview: SkinPreview? = null

    fun start(context: Context, value: SkinPreview) {
        val id = UUID.randomUUID().toString()
        token = id
        preview = value
        context.startActivity(Intent(context, SkinTrialActivity::class.java).putExtra("trial", id))
    }

    fun find(id: String?): SkinPreview? = if (id != null && id == token) preview else null

    fun forEditor(info: EditorInfo?, packageName: String): SkinPreview? =
        if (info?.packageName == packageName) find(info.privateImeOptions) else null

    fun end(id: String?) {
        if (token == id) { preview = null; token = null }
    }
}

private enum class TrialField(val label: String, val type: Int) {
    Text("普通文本", InputType.TYPE_CLASS_TEXT),
    Multiline("多行文本", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE),
    Number("数字", InputType.TYPE_CLASS_NUMBER),
    Phone("电话", InputType.TYPE_CLASS_PHONE),
    Email("邮箱", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS),
    Url("网址", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI),
    Password("密码", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD)
}

class SkinTrialActivity : ComponentActivity() {
    private val id get() = intent.getStringExtra("trial")
    private var editor: EditText? = null
    private var currentIme by mutableStateOf("")
    private var imeEnabled by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val trial = SkinTrials.find(id)
        if (trial == null) {
            Toast.makeText(this, "试打已结束，请从皮肤预览重新进入", Toast.LENGTH_SHORT).show()
            finish()
            return
        }
        val imm = getSystemService(InputMethodManager::class.java)
        setContent { MyWubiTheme {
            var type by rememberSaveable { mutableStateOf(TrialField.Text) }
            var menu by remember { mutableStateOf(false) }
            Surface(Modifier.fillMaxSize()) {
                Column(Modifier.safeDrawingPadding().imePadding().padding(12.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text("皮肤试打", style = MaterialTheme.typography.titleMedium)
                            Text("${trial.name} · ${trial.keyboard.label}", style = MaterialTheme.typography.bodySmall)
                        }
                        TextButton(onClick = { finish() }) { Text("返回预览") }
                    }
                    if (currentIme != packageName) {
                        TextButton(onClick = {
                            if (imeEnabled) imm.showInputMethodPicker()
                            else startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                        }) { Text(if (imeEnabled) "切换到 MyWubi" else "启用 MyWubi 输入法") }
                    }
                    Box {
                        OutlinedButton(onClick = { menu = true }) { Text("文本框类型 · ${type.label}") }
                        DropdownMenu(menu, onDismissRequest = { menu = false }) {
                            TrialField.entries.forEach { field ->
                                DropdownMenuItem(text = { Text(field.label) }, onClick = { type = field; menu = false })
                            }
                        }
                    }
                    key(type) {
                        AndroidView(factory = { context -> EditText(context).apply {
                            setSingleLine(type != TrialField.Multiline)
                            inputType = type.type
                            hint = "在这里试打"
                            privateImeOptions = this@SkinTrialActivity.id
                            imeOptions = if (type == TrialField.Multiline) EditorInfo.IME_FLAG_NO_ENTER_ACTION
                                else EditorInfo.IME_ACTION_DONE
                            isSaveEnabled = false
                            editor = this
                            post { if (hasWindowFocus()) focusEditor() }
                        } }, modifier = Modifier.fillMaxWidth())
                    }
                }
            }
        } }
    }

    private fun focusEditor() {
        val field = editor ?: return
        if (field.isAttachedToWindow && field.requestFocus()) {
            androidx.core.view.WindowCompat.getInsetsController(window, field)
                .show(androidx.core.view.WindowInsetsCompat.Type.ime())
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            editor?.post { focusEditor() }
            currentIme = Settings.Secure.getString(contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
                ?.substringBefore('/').orEmpty()
            imeEnabled = getSystemService(InputMethodManager::class.java).enabledInputMethodList.any { it.packageName == packageName }
        }
    }

    override fun onDestroy() {
        if (isFinishing) SkinTrials.end(id)
        super.onDestroy()
    }
}
