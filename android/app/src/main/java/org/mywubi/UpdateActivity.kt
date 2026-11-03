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
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.CancellationException

class UpdateActivity : ComponentActivity() {
    private val downloads by lazy { UpdateDownload.get(this) }
    private var preparing by mutableStateOf(false)
    private var installMessage by mutableStateOf("")
    private val permission = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        installMessage = if (packageManager.canRequestPackageInstalls()) "已允许安装，请再次点击「安装更新」确认" else "未授予安装权限，安装包已保留，可重试"
    }
    private val installer = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        downloads.installResult(if (it.resultCode == RESULT_OK) "安装已完成，请重新打开应用" else "安装未完成或已取消，安装包已保留，可重试")
    }

    private fun install() {
        if (preparing) return
        preparing = true
        installMessage = ""
        lifecycleScope.launch {
            try {
                val file = downloads.fileForInstall()
                if (!packageManager.canRequestPackageInstalls()) {
                    permission.launch(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")))
                } else {
                    val uri = FileProvider.getUriForFile(this@UpdateActivity, "$packageName.updates", file)
                    @Suppress("DEPRECATION")
                    val intent = Intent(Intent.ACTION_INSTALL_PACKAGE).setDataAndType(uri, "application/vnd.android.package-archive")
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION).putExtra(Intent.EXTRA_RETURN_RESULT, true)
                    installer.launch(intent)
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                installMessage = "无法开始安装：${error.message ?: "请重试"}"
            } finally { preparing = false }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val updates = AppUpdates.get(this)
        setContent { MyWubiTheme {
            val state by updates.state.collectAsState()
            val download by downloads.state.collectAsState()
            var confirmInstall by remember { mutableStateOf(false) }
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
                        if (download.phase != DownloadPhase.Ready || download.update != it) {
                            Button(onClick = { downloads.start(it) }, enabled = !download.busy && !preparing) { Text("下载更新") }
                        }
                    }
                    HorizontalDivider()
                    Text(download.message)
                    if (installMessage.isNotEmpty()) Text(installMessage)
                    if (download.busy) {
                        if (download.phase == DownloadPhase.Downloading && download.total > 0) {
                            LinearProgressIndicator(progress={ (download.received.toFloat() / download.total).coerceIn(0f,1f) }, modifier=Modifier.fillMaxWidth())
                            Text("${download.received / 1024} / ${download.total / 1024} KB")
                        } else LinearProgressIndicator(Modifier.fillMaxWidth())
                        if (download.phase == DownloadPhase.Downloading) OutlinedButton(onClick=downloads::cancel) { Text("取消下载") }
                    }
                    if (download.phase in setOf(DownloadPhase.Cancelled, DownloadPhase.Failed) && download.update != null) {
                        OutlinedButton(onClick={ downloads.start(requireNotNull(download.update)) }, enabled=!preparing) { Text("重新下载") }
                    }
                    if (download.phase == DownloadPhase.Ready) {
                        Text("待安装 ${download.update?.versionName}")
                        Button(onClick={ confirmInstall=true }, enabled=!preparing) { Text(if(preparing) "正在复核安装包…" else "安装更新") }
                    }
                    Text("键盘仅在后台检查版本，不会自动下载或安装，也不会发送输入内容。")
                    OutlinedButton(onClick = { finish() }) { Text("返回设置") }
                }
            }
            if (confirmInstall) AlertDialog(onDismissRequest={confirmInstall=false},
                title={Text("安装更新？")}, text={Text("请先结束正在进行的输入。系统安装会重启输入法，当前输入会话可能中断；已保存的配置、导入码表和用户词库会保留。")},
                confirmButton={TextButton(onClick={confirmInstall=false; install()}) {Text("继续安装")}},
                dismissButton={TextButton(onClick={confirmInstall=false}) {Text("稍后")}})
        } }
    }
}
