package org.mywubi

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject

internal val minSpellingHeight = 24.dp

@Composable
internal fun SpellingStrip(model: KeyboardModel, fontSize: TextUnit, modifier: Modifier, dispatch: (Any) -> Unit) {
    val lineHeight = fontSize * 1.25f
    val height = maxOf(minSpellingHeight, with(LocalDensity.current) { lineHeight.toDp() } + 4.dp)
    val strip = modifier.height(height).padding(horizontal = 8.dp)
    val codeColor = skinColor("code", MaterialTheme.colorScheme.primary)
    if (model.privateInput || model.groupPositions.isEmpty()) {
        Box(strip, contentAlignment = Alignment.CenterStart) {
            if (!model.privateInput && model.spelling.isNotEmpty()) {
                Text(model.spelling, color = codeColor, fontSize = fontSize, lineHeight = lineHeight,
                    maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        return
    }

    val codes = remember(model.candidates) {
        model.candidates.asSequence().flatMap { it.reverseCodes }.distinct().joinToString("  ")
    }
    var menuIndex by remember(model.epoch, model.query) { mutableStateOf<Int?>(null) }
    BoxWithConstraints(strip) {
        val available = if (codes.isEmpty()) maxWidth else maxWidth * .72f
        Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
            Row(Modifier.widthIn(max = available).fillMaxHeight().horizontalScroll(rememberScrollState()),
                verticalAlignment = Alignment.CenterVertically) {
                model.groupPositions.forEachIndexed { index, (letters, selected) ->
                    GroupPosition(index, letters, selected, model.enabled, fontSize, menuIndex == index,
                        open = { menuIndex = index }, dismiss = { menuIndex = null },
                        choose = { choice ->
                            val args = JSONArray().put(JSONObject(model.query)).put(index)
                            val action = if (choice == null) {
                                JSONObject().put("ClearGroupResolution", args)
                            } else {
                                JSONObject().put("ResolveGroup", args.put(choice))
                            }
                            dispatch(action)
                        }, cancel = { dispatch("Cancel") })
                }
            }
            if (codes.isNotEmpty()) key(model.query) {
                Text(codes, Modifier.padding(start = 8.dp).horizontalScroll(rememberScrollState()),
                    color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = fontSize,
                    lineHeight = lineHeight, maxLines = 1, softWrap = false)
            }
        }
    }
}

@Composable
private fun GroupPosition(index: Int, letters: String, selected: String?, enabled: Boolean, fontSize: TextUnit,
                          expanded: Boolean, open: () -> Unit, dismiss: () -> Unit,
                          choose: (String?) -> Unit, cancel: () -> Unit) {
    val options = remember(letters) { letters.codePoints().toArray().map { String(Character.toChars(it)) } }
    val color = skinColor("code", MaterialTheme.colorScheme.primary)
    if (options.size == 1) {
        Text(letters, color = color, fontSize = fontSize, lineHeight = fontSize * 1.25f, maxLines = 1)
        return
    }
    val group = "[$letters]"
    val label = selected ?: group
    Box(contentAlignment = Alignment.Center) {
        Text(label, Modifier.fillMaxHeight().widthIn(min = 24.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = open)
            .imeAction("第 ${index + 1} 位 $label", enabled, open)
            .wrapContentSize().padding(horizontal = 2.dp),
            color = color, fontSize = fontSize, lineHeight = fontSize * 1.25f,
            textDecoration = TextDecoration.Underline, maxLines = 1)
        if (expanded && enabled) {
            DropdownMenu(expanded = true, onDismissRequest = dismiss) {
                for (choice in listOf(null) + options) {
                    val current = selected == choice
                    DropdownMenuItem(text = { Text(choice ?: group) },
                        trailingIcon = { if (current) Text("✓") },
                        modifier = Modifier.semantics { this.selected = current },
                        onClick = { dismiss(); if (!current) choose(choice) })
                }
                HorizontalDivider()
                DropdownMenuItem(text = { Text("取消当前输入") }, onClick = { dismiss(); cancel() })
            }
        }
    }
}
