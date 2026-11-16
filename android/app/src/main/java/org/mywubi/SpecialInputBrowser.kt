package org.mywubi

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.json.JSONArray
import org.json.JSONObject

/** A palette backed by revision-checked core candidates, including imported libraries. */
@Composable
internal fun SpecialInputBrowser(model: KeyboardModel, dispatch: (Any) -> Unit,
                                 window: (Int) -> Unit, hideKeyboard: () -> Unit,
                                 modifier: Modifier = Modifier) {
    val query = model.expression ?: return
    val colors = MaterialTheme.colorScheme
    val categories = listOf("Emoji" to "Emoji", "Kaomoji" to "颜文字", "Kana" to "假名",
        "Math" to "数学", "Punctuation" to "标点", "Symbols" to "其他符号")
    val categoryScroll = rememberLazyListState(
        initialFirstVisibleItemIndex=(categories.indexOfFirst { it.first.equals(query.kind,true) }-1).coerceAtLeast(0))
    Column(modifier) {
        Row(Modifier.fillMaxWidth().heightIn(min=48.dp), verticalAlignment=Alignment.CenterVertically) {
            Text("全部", Modifier.padding(horizontal=12.dp), color=colors.primary, fontSize=16.sp)
            TextButton(onClick={dispatch("EditExpressionQuery")}, enabled=model.enabled,
                modifier=Modifier.heightIn(min=48.dp)) { Text("搜索") }
            Spacer(Modifier.weight(1f))
            TextButton(onClick={dispatch("EndTemporary")}, enabled=model.enabled,
                modifier=Modifier.heightIn(min=48.dp)) { Text("返回键盘") }
            IconButton(onClick=hideKeyboard, modifier=Modifier.size(48.dp).imeAction("收起键盘",true,hideKeyboard)) {
                Icon(painterResource(R.drawable.ic_hide_keyboard), null, Modifier.size(24.dp), tint=colors.onSurface)
            }
        }
        LazyRow(Modifier.fillMaxWidth().heightIn(min=48.dp),
            state=categoryScroll,
            contentPadding=PaddingValues(horizontal=8.dp), horizontalArrangement=Arrangement.spacedBy(8.dp)) {
            categories.forEach { (mode, label) ->
                item(key=mode) {
                    val capability = model.specialCapabilities[mode] ?: when(mode) {
                        "Emoji" -> model.emojiAvailable to model.emojiReason
                        "Kaomoji" -> model.kaomojiAvailable to model.kaomojiReason
                        else -> false to "词库未加载"
                    }
                    val selected = mode.equals(query.kind, ignoreCase=true)
                    FilterChip(selected=selected, enabled=model.enabled && capability.first,
                        onClick={
                            if (!selected) {
                                dispatch("ClearExpressionQuery")
                                dispatch(JSONObject().put("BeginTemporary",mode))
                                dispatch("BrowseExpressions")
                            }
                        }, label={Text(label, maxLines=1)},
                        modifier=Modifier.heightIn(min=48.dp).semantics {
                            contentDescription=if(capability.first) label else "$label，${capability.second ?: "不可用"}"
                        })
                }
            }
        }
        if (model.statusError || model.candidates.isEmpty()) {
            Box(Modifier.fillMaxWidth().weight(1f).padding(16.dp), contentAlignment=Alignment.Center) {
                Text(if(model.statusError) model.status else query.message ?: "当前词库没有可浏览的词条",
                    color=colors.onSurfaceVariant, textAlign=TextAlign.Center)
            }
        } else key(query.kind, model.offset) {
            val cellWidth = (80 * LocalDensity.current.fontScale.coerceIn(1f,2f)).dp
            // Fallback fonts give mathematical glyphs different line metrics.
            // Keep the grid aligned while reserving room for enlarged text.
            val cellHeight = (64 * LocalDensity.current.fontScale.coerceAtLeast(1f) + 12).dp
            LazyVerticalGrid(columns=GridCells.Adaptive(cellWidth),
                modifier=Modifier.fillMaxWidth().weight(1f).semantics { contentDescription="特殊码表全部词条" },
                contentPadding=PaddingValues(horizontal=4.dp, vertical=4.dp)) {
                items(model.candidates, key={it.id}, span={ item ->
                    GridItemSpan(if(item.expressionKind == "kaomoji" || item.text.codePointCount(0,item.text.length)>12) maxLineSpan else 1)
                }) { item ->
                    val display = remember(item) { ExpressionDisplay.display(item.text,item.hint,item.expressionKind) }
                    val choose = { dispatch(JSONObject().put("SelectCandidate",JSONArray().put(JSONObject(model.query)).put(item.id))) }
                    Surface(onClick=choose, enabled=model.enabled, shape=RectangleShape,
                        color=colors.surfaceContainerLowest, contentColor=colors.onSurface,
                        border=BorderStroke(0.5.dp,colors.onSurface.copy(alpha=0.15f)),
                        modifier=Modifier.fillMaxWidth().height(cellHeight)
                            .imeAction("候选 ${item.text}，${item.hint}",model.enabled,choose)) {
                        Column(Modifier.padding(horizontal=8.dp,vertical=8.dp),
                            verticalArrangement=Arrangement.Center, horizontalAlignment=Alignment.CenterHorizontally) {
                            Text(display.text, fontSize=24.sp, maxLines=1,
                                modifier=Modifier.horizontalScroll(rememberScrollState()))
                            Text(item.hint, fontSize=11.sp, maxLines=1, overflow=TextOverflow.Ellipsis,
                                color=colors.onSurfaceVariant)
                        }
                    }
                }
            }
        }
        if(model.offset>0 || model.offset+model.candidates.size<model.total) Row(
            Modifier.fillMaxWidth().heightIn(min=48.dp), verticalAlignment=Alignment.CenterVertically) {
            TextButton(onClick={window((model.offset-256).coerceAtLeast(0))}, enabled=model.enabled && model.offset>0,
                modifier=Modifier.heightIn(min=48.dp)) { Text("上一组") }
            Text(if(model.total==0) "0 项" else "${model.offset+1}–${model.offset+model.candidates.size} / ${model.total}",
                Modifier.weight(1f), fontSize=12.sp, textAlign=TextAlign.Center, color=colors.onSurfaceVariant)
            TextButton(onClick={window(model.offset+256)}, enabled=model.enabled && model.offset+model.candidates.size<model.total,
                modifier=Modifier.heightIn(min=48.dp)) { Text("下一组") }
        }
    }
}
