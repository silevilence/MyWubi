package org.mywubi

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

class UpdateActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val updates = AppUpdates.get(this)
        setContent { MyWubiTheme {
            val state by updates.state.collectAsState()
            Surface {
                Column(Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("应用更新", style = MaterialTheme.typography.headlineMedium)
                    Text("当前版本 ${updates.installedName}（${updates.installedCode}）")
                    Text(state.message)
                    if (state.checking) LinearProgressIndicator(Modifier.fillMaxWidth())
                    Button(onClick = { updates.check(manual = true) }, enabled = !state.checking) { Text("检查更新") }
                    state.available?.let {
                        Text("${it.versionName} 更新说明", style = MaterialTheme.typography.titleLarge)
                        Text(it.notes)
                    }
                    Text("键盘仅在后台检查版本，不会自动下载或安装，也不会发送输入内容。")
                    OutlinedButton(onClick = { finish() }) { Text("返回设置") }
                }
            }
        } }
    }
}
