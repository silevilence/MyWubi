package org.mywubi

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
@OptIn(ExperimentalLayoutApi::class)
fun ExpressionPreview() {
    var text by remember { mutableStateOf("🚳") }
    var emoji by remember { mutableStateOf(true) }
    var title by remember { mutableStateOf("禁止自行车") }
    SettingsDisclosure("表情显示预览") {
        Text("预览使用候选栏相同的系统字体。粘贴单个 Emoji 或一条颜文字检查显示；宿主应用可能使用不同字体。", style=MaterialTheme.typography.bodySmall)
        FlowRow(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
            for ((name, value, isEmoji) in listOf(Triple("禁止自行车", "🚳", true), Triple("家庭", "👨‍👩‍👧‍👦", true),
                Triple("旗帜", "🇨🇳", true), Triple("肤色与性别", "👩🏽‍⚕️", true), Triple("键帽", "1️⃣", true),
                Triple("颜文字", "(╯°□°）╯︵ ┻━┻", false), Triple("组合附加符", "(๑•̀ㅂ•́)و✧", false))) {
                TextButton(onClick={text=value; emoji=isEmoji; title=name}) { Text(name) }
            }
        }
        Row { Checkbox(checked=emoji,onCheckedChange={emoji=it}); Text("按单个 Emoji 组合检查",Modifier.padding(top=12.dp)) }
        OutlinedTextField(value=text,onValueChange={ if(it.codePointCount(0,it.length)<=1024) { text=it; title="自定义表情" } },
            label={Text("预览文字")},modifier=Modifier.fillMaxWidth())
        val display = remember(text, emoji) { ExpressionDisplay.display(text,title,if(emoji) "emoji" else "kaomoji") }
        Text(display.text,Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(vertical=12.dp)
            .semantics { contentDescription="表情预览：$title，$text" },fontSize=28.sp,softWrap=false,
            style=TextStyle(platformStyle=PlatformTextStyle(includeFontPadding=true)))
        Text(if(display.fallback) "使用名称与码点回退；输入时仍提交原始完整字符串。" else "字形覆盖检查通过，请检查实际预览。",style=MaterialTheme.typography.bodySmall)
    }
}
