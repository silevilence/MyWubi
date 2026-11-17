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

@Composable
internal fun MainKeyboardMenu(model: KeyboardModel, expanded: Boolean, dismiss: () -> Unit,
                              choose: (KeyboardSelection) -> Unit) {
    DropdownMenu(expanded = expanded, onDismissRequest = dismiss, modifier = Modifier.widthIn(max = 304.dp)) {
        Column(Modifier.heightIn(max = 288.dp).verticalScroll(rememberScrollState())
            .semantics { contentDescription = "形码键盘布局列表" }) {
            Text("切换形码键盘", Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                style = MaterialTheme.typography.labelLarge)
            for (option in model.mainKeyboards) {
                val active = model.mainKeyboardSelection == option.selection
                DropdownMenuItem(
                    text = { Column {
                        Text(option.name)
                        if (active) Text("当前布局", style = MaterialTheme.typography.bodySmall)
                        option.reason?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                    } },
                    enabled = model.enabled && !model.switchingKeyboard && option.reason == null,
                    modifier = Modifier.heightIn(min = 48.dp).semantics {
                        selected = active
                        contentDescription = "形码布局 ${option.selection.component}" + if (active) "，当前布局" else ""
                    },
                    onClick = { dismiss(); if (!active) choose(option.selection) })
            }
        }
    }
}
