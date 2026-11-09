package org.mywubi

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.os.Build
import android.text.StaticLayout
import android.text.TextPaint
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** System font evidence; Compose/IME/host screenshots are a separate acceptance step. */
class ExpressionFontTest {
    @Test fun fullLibrariesKeepOriginalSequencesAndReportMissingSystemGlyphs() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val out=File(context.getExternalFilesDir(null),"expression-font-report").apply { mkdirs() }
        val reports=JSONObject().put("sdk",Build.VERSION.SDK_INT).put("fingerprint",Build.FINGERPRINT)
        for(kind in listOf("emoji","kaomoji")) {
            val text=context.assets.open("expressions/$kind.mwe").bufferedReader().use { it.readText() }
            val entries=JSONObject(text.substringAfter('\n')).getJSONArray("entries")
            val failures=JSONArray()
            val chars=mutableSetOf<Int>()
            for(index in 0 until entries.length()) {
                val entry=entries.getJSONObject(index)
                val original=entry.getString("text")
                chars.addAll(original.codePoints().toArray().toList())
                val display=ExpressionDisplay.display(original,entry.getString("name"),kind)
                assertEquals(original, String(original.toByteArray(Charsets.UTF_8),Charsets.UTF_8))
                assertEquals(original,entry.getString("text"))
                if(display.fallback) {
                    assertTrue(display.text.contains("字体不支持"))
                    failures.put(JSONObject().put("name",entry.getString("name")).put("text",original))
                } else assertEquals(original,display.text)
            }
            val paint=Paint().apply { typeface=Typeface.DEFAULT; textSize=32f }
            val missing=JSONArray(chars.filter { !Character.isWhitespace(it) && !Character.isSpaceChar(it) &&
                Character.getType(it)!=Character.FORMAT.toInt() && !paint.hasGlyph(String(Character.toChars(it))) }
                .sorted().map { "U+%04X".format(it) })
            reports.put(kind,JSONObject().put("entries",entries.length()).put("fallback_entries",failures.length())
                .put("distinct_characters",chars.size).put("missing_characters",missing).put("failures",failures))
        }
        File(out,"coverage.json").writeText(reports.toString(2))
        assertTrue(ExpressionDisplay.supported("🚳",true))
        assertTrue(ExpressionDisplay.display(String(Character.toChars(0x10FFFF)),"缺字样本","kaomoji").fallback)
        val text="禁止自行车 🚳  家庭 👨‍👩‍👧‍👦  旗帜 🇨🇳\n肤色/性别 👩🏽‍⚕️  键帽 1️⃣  文本心 ♥︎  彩色心 ❤️\n(╯°□°）╯︵ ┻━┻  (๑•̀ㅂ•́)و✧\n组合 ā̈́  空格 [　][ ][ ] 新增 🫩 🫪"
        val paint=TextPaint(Paint.ANTI_ALIAS_FLAG).apply { typeface=Typeface.DEFAULT; textSize=38f; color=Color.BLACK }
        val layout=StaticLayout.Builder.obtain(text,0,text.length,paint,1000).setIncludePad(true).build()
        val bitmap=Bitmap.createBitmap(1024,layout.height+24,Bitmap.Config.ARGB_8888)
        val canvas=Canvas(bitmap); canvas.drawColor(Color.WHITE); canvas.translate(12f,12f); layout.draw(canvas)
        File(out,"samples.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG,100,it) }
        val pixels=IntArray(bitmap.width*bitmap.height); bitmap.getPixels(pixels,0,bitmap.width,0,0,bitmap.width,bitmap.height)
        assertTrue(pixels.any { kotlin.math.abs(Color.red(it)-Color.blue(it))>60 })
        bitmap.recycle()
    }
}
