package org.mywubi

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** A bounded two-column chooser; category count never widens the keyboard. */
@Composable
internal fun SpecialCategoryMenu(model:KeyboardModel, expanded:Boolean, current:String?=null,
                                 dismiss:()->Unit, choose:(String)->Unit) {
    DropdownMenu(expanded=expanded, onDismissRequest=dismiss,
        modifier=Modifier.width(304.dp)) {
        // DropdownMenu applies its modifier to the scrolling content. Constraining
        // that content makes its scroll range zero; bound an inner viewport instead.
        Column(Modifier.heightIn(max=320.dp).verticalScroll(rememberScrollState())
            .semantics { contentDescription="特殊码表分类列表" }) {
        Text("选择特殊码表 · ${specialCategories.size} 类",Modifier.padding(horizontal=12.dp,vertical=8.dp),
            style=MaterialTheme.typography.labelLarge)
        for (row in specialCategoryDefinitions.chunked(2)) {
            Row(Modifier.fillMaxWidth().padding(horizontal=4.dp)) {
                for (category in row) {
                    val capability=model.specialCapabilities[category.mode] ?: when(category.kind) {
                        "emoji" -> model.emojiAvailable to model.emojiReason
                        "kaomoji" -> model.kaomojiAvailable to model.kaomojiReason
                        else -> false to "词库未加载"
                    }
                    val active=current==category.kind
                    TextButton(onClick={choose(category.mode)},enabled=model.enabled && capability.first,
                        modifier=Modifier.weight(1f).heightIn(min=48.dp).semantics {
                            selected=active
                            contentDescription=category.label + if(capability.first) "" else "，${capability.second ?: "不可用"}"
                        }) {
                        Text((if(active) "✓ " else "")+category.label,fontSize=14.sp)
                    }
                }
                if(row.size==1) Spacer(Modifier.weight(1f))
            }
        }
        }
    }
}
