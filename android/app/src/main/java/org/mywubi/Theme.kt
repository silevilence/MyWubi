package org.mywubi

import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

@Composable
fun myWubiColors() = if (androidx.compose.foundation.isSystemInDarkTheme()) darkColorScheme(
        primary=Color(0xFF9CD3C1), onPrimary=Color(0xFF00382C),
        surface=Color(0xFF191F1D), onSurface=Color(0xFFE1E8E4),
        secondaryContainer=Color(0xFF344B42), onSecondaryContainer=Color(0xFFDCEBE3)
    ) else lightColorScheme(
        primary=Color(0xFF245D51), onPrimary=Color.White,
        surface=Color(0xFFE9EEEB), onSurface=Color(0xFF192C27),
        secondaryContainer=Color(0xFFD4E1DA), onSecondaryContainer=Color(0xFF192C27)
    )

@Composable
fun MyWubiTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme=myWubiColors(), content=content)
}
