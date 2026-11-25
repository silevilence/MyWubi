package org.mywubi

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import kotlin.math.roundToInt

internal data class KeyPreview(val owner: Any, val text: String, val bounds: Rect, val deletion: Boolean = false,
    val picker: HoldPicker? = null, val selectedSlot: Int? = null)
internal class KeyPreviewState {
    var current by mutableStateOf<KeyPreview?>(null)
    var keyboardBounds = Rect.Zero
    fun clear(owner: Any) { if (current?.owner === owner) current = null }
}
internal val LocalKeyPreview = staticCompositionLocalOf { KeyPreviewState() }

/** Symbols are explicit literal input, even when a custom table accepts digits as codes. */
internal fun letterSymbol(letter: Char, chinese: Boolean): String {
    val symbols = "1234567890~!@#%&*?\"()-_:;/"
    val symbol = symbols["qwertyuiopasdfghjklzxcvbnm".indexOf(letter)].toString()
    return if (chinese) when (symbol) {
        "!" -> "！"; "?" -> "？"; "(" -> "（"; ")" -> "）"; ":" -> "："; ";" -> "；"
        else -> symbol
    } else symbol
}

/** Drawn in the keyboard's own window: it never takes focus or intercepts touches. */
@Composable
internal fun KeyPreviewBubble(preview: KeyPreview?, origin: Offset, widthPx: Float) {
    if (preview == null) return
    preview.picker?.let { HoldPickerBubble(it, preview.selectedSlot, origin); return }
    val density = LocalDensity.current
    val fontSize = if (preview.deletion) 16.sp else skinNumber("bubble_text_size",30f).sp
    val available = with(density) { widthPx.toDp() }
    val imageInset = LocalSkinImages.current.roles["bubble"]?.content ?: intArrayOf(0,0,0,0)
    val extraWidth=(imageInset[0]+imageInset[2]).dp
    val extraHeight=(imageInset[1]+imageInset[3]).dp
    val measured = rememberTextMeasurer().measure(preview.text, TextStyle(fontSize = fontSize),
        constraints = androidx.compose.ui.unit.Constraints(maxWidth = (widthPx - with(density) { (24.dp+extraWidth).toPx() }).toInt().coerceAtLeast(1)))
    val width = minOf(available, maxOf(if (preview.deletion) 196.dp else 56.dp, with(density) { measured.size.width.toDp() } + 24.dp + extraWidth))
    val height = maxOf(if (preview.deletion) 48.dp else 64.dp, with(density) { measured.size.height.toDp() } + 16.dp + extraHeight)
    val x = (preview.bounds.center.x - origin.x - with(density) { width.toPx() } / 2)
        .coerceIn(0f, (widthPx - with(density) { width.toPx() }).coerceAtLeast(0f))
    val y = (preview.bounds.top - origin.y - with(density) { height.toPx() } + with(density) { 4.dp.toPx() }).coerceAtLeast(0f)
    Box(Modifier.offset { IntOffset(x.roundToInt(), y.roundToInt()) }.size(width, height)
        .shadow(skinNumber("shadow",6f).dp, RoundedCornerShape(skinNumber("key_radius",12f).dp)).background(skinColor("bubble_background",MaterialTheme.colorScheme.primary)).skinImage("bubble")
        .clearAndSetSemantics { contentDescription = if (preview.deletion) preview.text else "按键预览" }, contentAlignment = Alignment.Center) {
        Text(preview.text, Modifier.padding(skinContent("bubble")).padding(horizontal = 12.dp, vertical = 8.dp), color = skinColor("bubble_text",MaterialTheme.colorScheme.onPrimary), fontSize = fontSize)
    }
}

/** Repeat stays active inside the key; only a deliberate held upward release clears the editor. */
@Composable
internal fun DeleteKeySurface(enabled: Boolean, epoch: Long, modifier: Modifier, background: Color,
    foreground: Color, delete: () -> Unit, clear: () -> Unit, content: @Composable () -> Unit) {
    val currentDelete by rememberUpdatedState(delete)
    val currentClear by rememberUpdatedState(clear)
    val previews = LocalKeyPreview.current
    val scope = rememberCoroutineScope()
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var active by remember(lifecycle) { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) }
    var revision by remember { mutableIntStateOf(0) }
    var job by remember { mutableStateOf<Job?>(null) }
    var pressed by remember { mutableStateOf(false) }
    var bounds by remember { mutableStateOf(Rect.Zero) }
    var owner by remember { mutableStateOf<Any?>(null) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, _ ->
            active = lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
            if (!active) { revision++; job?.cancel(); pressed = false; owner?.let(previews::clear) }
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer); job?.cancel(); owner?.let(previews::clear) }
    }
    val colors = MaterialTheme.colorScheme
    Surface(modifier.onGloballyPositioned { bounds = it.boundsInRoot() }.clearAndSetSemantics {
        role = Role.Button
        contentDescription = "退格"
        if (!enabled) disabled()
        onClick { if (enabled) currentDelete(); enabled }
        customActions = listOf(CustomAccessibilityAction("清空输入框") { if (enabled) currentClear(); enabled })
    }.pointerInput(enabled, epoch, active, revision) {
        if (!enabled || !active) return@pointerInput
        awaitEachGesture {
            val down = awaitFirstDown()
            down.consume()
            val token = Any()
            val started = revision
            owner = token
            pressed = true
            var held = false
            var armed = false
            var rise = 0f
            val threshold = 40.dp.toPx()
            fun updatePreview() {
                armed = held && rise >= threshold
                if (held) previews.current = KeyPreview(token, if (armed) "松手清空 · 下滑取消" else "↑ 上滑清空", bounds, deletion = true)
            }
            val repeat = scope.launch {
                delay(400)
                if (active && revision == started) {
                    held = true
                    updatePreview()
                    while (active && revision == started) {
                        if (!armed) currentDelete()
                        delay(65)
                    }
                }
            }
            job = repeat
            try {
                while (true) {
                    val event = awaitPointerEvent()
                    val change = event.changes.firstOrNull { it.id == down.id } ?: break
                    if (change.isConsumed || !active || revision != started || event.changes.count { it.pressed } > 1) break
                    val point = change.position
                    val slop = viewConfiguration.touchSlop
                    if (point.x < -slop || point.x > size.width + slop || point.y > size.height + slop) break
                    rise = down.position.y - point.y
                    updatePreview()
                    change.consume()
                    if (!change.pressed) {
                        repeat.cancel()
                        if (armed) currentClear() else if (!held && rise < slop) currentDelete()
                        break
                    }
                }
            } finally {
                repeat.cancel()
                pressed = false
                previews.clear(token)
            }
        }
    }.padding(horizontal=(skinNumber("key_gap",4f)/2).dp, vertical=(skinNumber("key_gap",6f)/2).dp), shape = RoundedCornerShape(skinNumber("key_radius",4f).dp),
        color = if (!enabled) colors.onSurface.copy(alpha = 0.12f) else if (pressed) skinColor("pressed_background",colors.primaryContainer) else background,
        contentColor = if (!enabled) skinColor("disabled",colors.onSurface.copy(alpha = 0.38f)) else if (pressed) skinColor("pressed_text",foreground) else foreground) {
        Box(Modifier.fillMaxSize().skinImage(if(!enabled) "disabled" else if(pressed) "pressed" else "function").padding(skinContent("function")),contentAlignment = Alignment.Center) { content() }
    }
}

@Composable
internal fun KeyTouchSurface(label: String, enabled: Boolean, epoch: Long, modifier: Modifier,
    description: String, background: Color, foreground: Color, imageRole: String, alternate: String?, previewEnabled: Boolean,
    holdChoices: List<TouchChoice>, action: () -> Unit, directions: Map<Int, TouchChoice>? = null,
    bindingRevision: Any = epoch, content: @Composable () -> Unit) {
    val primary by rememberUpdatedState(action)
    val currentHold by rememberUpdatedState(holdChoices)
    val holdLabels = holdChoices.map { it.name }
    val choices = directions ?: holdChoices.firstOrNull()?.let { mapOf(0 to it) }.orEmpty()
    val currentChoices by rememberUpdatedState(choices)
    val previews = LocalKeyPreview.current
    val scope = rememberCoroutineScope()
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var active by remember(lifecycle) { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) }
    var lifecycleRevision by remember { mutableIntStateOf(0) }
    var longPressJob by remember { mutableStateOf<Job?>(null) }
    var pressed by remember { mutableStateOf(false) }
    var bounds by remember { mutableStateOf(Rect.Zero) }
    var owner by remember { mutableStateOf<Any?>(null) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, _ ->
            active = lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
            if (!active) { lifecycleRevision++; longPressJob?.cancel(); pressed = false; owner?.let(previews::clear) }
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer); longPressJob?.cancel(); owner?.let(previews::clear) }
    }
    val colors = MaterialTheme.colorScheme
    val touch = modifier.onGloballyPositioned { bounds = it.boundsInRoot() }
        .clearAndSetSemantics {
            role = Role.Button
            contentDescription = description
            if (!enabled) disabled()
            onClick { if (enabled) primary(); enabled }
            holdChoices.firstOrNull()?.let { onLongClick("输入 ${it.name}") { if (enabled) it.action(); enabled } }
            customActions = choices.map { (direction, choice) -> CustomAccessibilityAction("${directionNames[direction]}滑：${choice.name}") { if(enabled) choice.action(); enabled } } +
                holdChoices.filter { it.name != label && it.name != holdLabels.firstOrNull() && choices.values.none { choice -> choice.name == it.name } }
                    // Compose rejects 32 or more accessibility actions on a single node.
                    .take(31 - choices.size)
                    .map { CustomAccessibilityAction("输入 ${it.name}") { if (enabled) it.action(); enabled } }
        }
        .pointerInput(enabled, active, lifecycleRevision, epoch, label, holdLabels, previewEnabled, bindingRevision, choices.mapValues { it.value.name }) {
            if (!enabled || !active) return@pointerInput
            awaitPointerEventScope {
                while (true) {
                    var incoming: androidx.compose.ui.input.pointer.PointerInputChange?
                    do {
                        val event = awaitPointerEvent()
                        incoming = event.changes.firstOrNull {
                            it.pressed && !it.previousPressed && !it.isConsumed &&
                                it.position.x in 0f..size.width.toFloat() && it.position.y in 0f..size.height.toFloat()
                        }
                    } while (incoming == null)
                    val down = incoming
                    down.consume()
                    val token = Any()
                    val revision = lifecycleRevision
                    owner = token
                    pressed = true
                    val gesture=DirectionalGesture(currentChoices.keys, holdLabels.isNotEmpty())
                    var finger = bounds.topLeft + down.position
                    var picker: HoldPicker? = null
                    var selectedSlot: Int? = null
                    fun startHold() {
                        gesture.hold()
                        if (gesture.holding && picker == null && previewEnabled && holdLabels.size > 1 && previews.keyboardBounds.width > 0) {
                            picker = HoldPicker.place(holdLabels, previews.keyboardBounds, bounds, finger,
                                maxOf(36.dp.toPx(), 24.sp.toPx()), maxOf(44.dp.toPx(), 30.sp.toPx()), 4.dp.toPx())
                            selectedSlot = picker?.defaultSlot
                        }
                    }
                    var committed = false
                    fun preview() {
                        val text=when {
                            gesture.cancelled -> null
                            gesture.holding -> holdLabels.first()
                            gesture.sliding -> currentChoices[gesture.direction]?.name
                            else -> label
                        }
                        if(previewEnabled && text != null) previews.current=KeyPreview(token,text,bounds,picker=picker,selectedSlot=selectedSlot)
                        else previews.clear(token)
                    }
                    preview()
                    val longPress = if (holdLabels.isNotEmpty()) scope.launch {
                        delay(viewConfiguration.longPressTimeoutMillis)
                        if (active && revision == lifecycleRevision) { startHold(); preview() }
                    } else null
                    longPressJob = longPress
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (change.isConsumed || !active || revision != lifecycleRevision) { gesture.cancel(); break }
                            // Independent keys retain two-thumb typing; a second finger on this key cancels it.
                            if(event.changes.any { it.id != down.id && it.pressed &&
                                it.position.x in 0f..size.width.toFloat() && it.position.y in 0f..size.height.toFloat() }) {
                                gesture.cancel(); break
                            }
                            val delta=change.position-down.position
                            if (!gesture.holding && change.uptimeMillis - down.uptimeMillis >= viewConfiguration.longPressTimeoutMillis) startHold()
                            finger = bounds.topLeft + change.position
                            if (gesture.holding && picker != null) selectedSlot = picker?.hit(finger)
                            else if (gesture.holding && delta.getDistance() > 160.dp.toPx()) gesture.cancel()
                            else gesture.move(delta.x/density,delta.y/density)
                            if(gesture.sliding || gesture.cancelled) longPress?.cancel()
                            preview()
                            if (!change.pressed) {
                                change.consume()
                                when(val selected=gesture.release()) {
                                    -1 -> { primary(); committed=true }
                                    -2 -> {
                                        val menu = picker
                                        val index = if (menu == null) 0 else selectedSlot?.let(menu::option)
                                        if (index != null) { currentHold[index].action(); committed = true }
                                    }
                                    null -> Unit
                                    else -> currentChoices[selected]?.let { it.action();committed=true }
                                }
                                break
                            }
                            change.consume()
                        }
                    } finally {
                        longPress?.cancel()
                        pressed = false
                        if (committed && previewEnabled && picker == null) scope.launch { delay(80); previews.clear(token) }
                        else previews.clear(token)
                    }
                }
            }
        }
    // Apply visual spacing AFTER pointerInput so gutters still belong to this key.
    // Press feedback changes color only; scaling must not move the hit region.
    Surface(touch.padding(horizontal=(skinNumber("key_gap",4f)/2).dp, vertical=(skinNumber("key_gap",6f)/2).dp),
        shape = RoundedCornerShape(skinNumber("key_radius",4f).dp),
        color = if (!enabled) colors.onSurface.copy(alpha = 0.12f) else if (pressed) skinColor("pressed_background",colors.secondaryContainer) else background,
        contentColor = if (!enabled) skinColor("disabled",colors.onSurface.copy(alpha = 0.38f)) else if (pressed) skinColor("pressed_text",colors.onSecondaryContainer) else foreground) {
        Box(Modifier.fillMaxSize().skinImage(if(!enabled) "disabled" else if(pressed) "pressed" else imageRole).padding(skinContent(imageRole)),contentAlignment = Alignment.Center) {
            if (alternate != null) Text(alternate, Modifier.align(Alignment.TopEnd).padding(top = 2.dp, end = 4.dp),
                fontSize = 10.sp, lineHeight = 12.sp, maxLines = 1)
            Box(Modifier.padding(top = if (alternate == null) 0.dp else (12f * LocalDensity.current.fontScale).dp), contentAlignment = Alignment.Center) { content() }
        }
    }
}
