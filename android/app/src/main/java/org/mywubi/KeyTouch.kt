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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import kotlin.math.roundToInt

internal data class KeyPreview(val owner: Any, val text: String, val bounds: Rect)
internal class KeyPreviewState {
    var current by mutableStateOf<KeyPreview?>(null)
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
    val density = LocalDensity.current
    val width = 56.dp
    val height = 64.dp
    val x = (preview.bounds.center.x - origin.x - with(density) { width.toPx() } / 2)
        .coerceIn(0f, (widthPx - with(density) { width.toPx() }).coerceAtLeast(0f))
    val y = (preview.bounds.top - origin.y - with(density) { height.toPx() } + with(density) { 4.dp.toPx() }).coerceAtLeast(0f)
    Box(Modifier.offset { IntOffset(x.roundToInt(), y.roundToInt()) }.size(width, height)
        .shadow(6.dp, RoundedCornerShape(12.dp)).background(MaterialTheme.colorScheme.primary)
        .clearAndSetSemantics { contentDescription = "按键预览" }, contentAlignment = Alignment.Center) {
        Text(preview.text, color = MaterialTheme.colorScheme.onPrimary, fontSize = 30.sp, maxLines = 1)
    }
}

@Composable
internal fun KeyTouchSurface(label: String, enabled: Boolean, epoch: Long, modifier: Modifier,
    description: String, background: Color, foreground: Color, alternate: String?, previewEnabled: Boolean,
    alternateAction: () -> Unit, action: () -> Unit, content: @Composable () -> Unit) {
    val primary by rememberUpdatedState(action)
    val secondary by rememberUpdatedState(alternateAction)
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
            if (alternate != null) onLongClick("输入 $alternate") { if (enabled) secondary(); enabled }
        }
        .pointerInput(enabled, active, lifecycleRevision, epoch, label, alternate, previewEnabled) {
            if (!enabled || !active) return@pointerInput
            awaitEachGesture {
                val down = awaitFirstDown()
                down.consume()
                val token = Any()
                val revision = lifecycleRevision
                owner = token
                pressed = true
                var held = false
                var slid = false
                var committed = false
                fun preview(symbol: Boolean) {
                    if (previewEnabled) previews.current = KeyPreview(token, if (symbol) alternate ?: label else label, bounds)
                }
                preview(false)
                val longPress = if (alternate != null) scope.launch {
                    delay(viewConfiguration.longPressTimeoutMillis)
                    if (active && revision == lifecycleRevision) { held = true; preview(true) }
                } else null
                longPressJob = longPress
                try {
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (change.isConsumed || !active || revision != lifecycleRevision) break
                        val point = change.position
                        val slop = viewConfiguration.touchSlop
                        if (point.x < -slop || point.x > size.width + slop || point.y > size.height + slop) break
                        // Without a secondary symbol, leaving the key cancels the tap.
                        if (alternate == null && point.y < -slop) break
                        val nextSlid = alternate != null && point.y <= down.position.y - 20.dp.toPx()
                        if (nextSlid != slid && !held) { slid = nextSlid; preview(slid) }
                        if (!change.pressed) {
                            change.consume()
                            if (held || slid) secondary() else primary()
                            committed = true
                            break
                        }
                        change.consume()
                    }
                } finally {
                    longPress?.cancel()
                    pressed = false
                    if (committed && previewEnabled) scope.launch { delay(80); previews.clear(token) }
                    else previews.clear(token)
                }
            }
        }
    Surface(touch.graphicsLayer { scaleX = if (pressed) 1.06f else 1f; scaleY = if (pressed) 1.06f else 1f },
        shape = MaterialTheme.shapes.small,
        color = if (!enabled) colors.onSurface.copy(alpha = 0.12f) else if (pressed) colors.secondaryContainer else background,
        contentColor = if (!enabled) colors.onSurface.copy(alpha = 0.38f) else if (pressed) colors.onSecondaryContainer else foreground) {
        Box(contentAlignment = Alignment.Center) {
            if (alternate != null) Text(alternate, Modifier.align(Alignment.TopEnd).padding(top = 2.dp, end = 4.dp),
                fontSize = 10.sp, lineHeight = 12.sp, maxLines = 1)
            Box(Modifier.padding(top = if (alternate == null) 0.dp else 9.dp), contentAlignment = Alignment.Center) { content() }
        }
    }
}
