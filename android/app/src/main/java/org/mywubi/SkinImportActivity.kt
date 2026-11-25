package org.mywubi

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.IntentCompat
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.util.UUID

internal fun incomingSkinUri(intent: Intent): Uri? {
    val uri = when (intent.action) {
        Intent.ACTION_VIEW -> intent.data
        Intent.ACTION_SEND -> IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
            ?: intent.clipData?.takeIf { it.itemCount == 1 }?.getItemAt(0)?.uri
        else -> null
    }
    return uri?.takeIf { it.scheme == "content" }
}

class SkinImportModel(app: Application) : AndroidViewModel(app) {
    var item by mutableStateOf<JSONObject?>(null); private set
    var result by mutableStateOf<JSONObject?>(null); private set
    var error by mutableStateOf(""); private set
    var importing by mutableStateOf(false); private set
    private var started = false
    private val confirmation = CompletableDeferred<Unit>()

    fun open(uri: Uri?) {
        if (started) return
        started = true
        if (uri == null) { error = "未收到可读取的文件，请从文件管理器选择皮肤包。"; return }
        viewModelScope.launch {
            val app = getApplication<Application>()
            val temporary = File(app.cacheDir, "skin-incoming-${UUID.randomUUID()}.zip")
            try {
                item = withContext(Dispatchers.IO) {
                    app.contentResolver.openInputStream(uri)?.use { stageSkinDocument(temporary, it) }
                        ?: error("文件无法读取，请重新选择。")
                    readSkinDocument(temporary)
                }
                confirmation.await()
                result = withContext(Dispatchers.IO) { readSkinDocument(temporary, Resources.prepare(app)) }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: SecurityException) {
                error = "没有文件读取权限。请重新分享，或将文件保存到下载目录后从设置中导入。"
            } catch (failure: Exception) {
                error = failure.localizedMessage ?: "无法读取皮肤包"
            } finally {
                temporary.delete()
                importing = false
            }
        }
    }

    fun confirm() {
        if (item != null && confirmation.complete(Unit)) importing = true
    }
}

class SkinImportActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val model = ViewModelProvider(this)[SkinImportModel::class.java]
        model.open(incomingSkinUri(intent))
        setContent { MyWubiTheme {
            Surface(Modifier.fillMaxSize()) {
                Column(Modifier.safeDrawingPadding().verticalScroll(rememberScrollState()).padding(20.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    val item = model.item
                    val result = model.result
                    Text(when {
                        model.error.isNotEmpty() -> "无法导入皮肤"
                        result?.optBoolean("already_present") == true -> "皮肤已在库中"
                        result != null -> "皮肤已导入"
                        else -> "导入皮肤"
                    }, style = MaterialTheme.typography.titleLarge)
                    if (item != null) {
                        Text("${item.getString("name")} · ${item.getString("version")}")
                        val platforms = item.getJSONArray("platforms")
                        Text("适用平台 · " + (0 until platforms.length()).joinToString("、") { platforms.getString(it) })
                    }
                    when {
                        model.error.isNotEmpty() -> Text(model.error)
                        result != null -> {
                            val imported = result.getJSONObject("item")
                            val components = imported.getJSONArray("components")
                            val options = (0 until components.length()).map(components::getJSONObject)
                                .filter { it.getString("kind") == "appearance" && it.getBoolean("enabled") }
                            if (options.isEmpty()) Text("皮肤已入库，此包没有可预览的 Android 外观。")
                            for (option in options) {
                                val component = option.getString("id")
                                Button(onClick = {
                                    startActivity(Intent(this@SkinImportActivity, MainActivity::class.java)
                                        .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                                        .putExtra("skin.id", imported.getString("id")).putExtra("skin.version", imported.getString("version"))
                                        .putExtra("skin.name", imported.getString("name")).putExtra("skin.component", component))
                                    finish()
                                }) { Text(if (options.size == 1) "预览" else "预览 · $component") }
                            }
                        }
                        item == null || model.importing -> LinearProgressIndicator(Modifier.fillMaxWidth())
                        else -> Button(onClick = model::confirm) { Text("确认导入") }
                    }
                    TextButton(onClick = { finish() }, enabled = !model.importing) {
                        Text(if (model.error.isEmpty() && result == null) "取消" else "关闭")
                    }
                }
            }
        } }
    }
}
