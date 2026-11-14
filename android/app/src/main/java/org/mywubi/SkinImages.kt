package org.mywubi

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import android.util.Base64
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.unit.dp
import org.json.JSONObject

internal data class SkinImage(val bitmap: Bitmap, val mode: String, val slice: IntArray,
    val content: IntArray, val opacity: Float)
internal class SkinImages(val roles: Map<String,SkinImage>)
internal val LocalSkinImages = staticCompositionLocalOf { SkinImages(emptyMap()) }

/** Runs on the appearance loader; decoded bitmap ownership follows the whole snapshot. */
internal fun decodeSkinImages(variant: JSONObject, bitmaps: MutableMap<String,Bitmap>): SkinImages {
    val images=variant.optJSONObject("images") ?: return SkinImages(emptyMap())
    val roles=mutableMapOf<String,SkinImage>()
    for(role in images.keys()) {
        val image=images.getJSONObject(role)
        val bitmap=bitmaps.getOrPut(image.getString("path")) {
            val bytes=Base64.decode(image.getString("png"),Base64.DEFAULT)
            val bounds=BitmapFactory.Options().apply { inJustDecodeBounds=true }
            BitmapFactory.decodeByteArray(bytes,0,bytes.size,bounds)
            require(bounds.outWidth==image.getInt("width") && bounds.outHeight==image.getInt("height")) { "PNG 尺寸不一致" }
            val options=BitmapFactory.Options().apply { inPreferredConfig=Bitmap.Config.ARGB_8888; inScaled=false }
            val decoded=BitmapFactory.decodeByteArray(bytes,0,bytes.size,options) ?: error("PNG 图片解码失败")
            require(bitmaps.values.sumOf {it.allocationByteCount.toLong()}+decoded.allocationByteCount<=32L*1024*1024) { "图片解码超过 32 MiB" }
            decoded
        }
        val style=image.getJSONObject("style")
        fun insets(key:String)=IntArray(4) {style.getJSONArray(key).getInt(it)}
        roles[role]=SkinImage(bitmap,style.getString("mode"),insets("slice"),insets("content"),style.getDouble("opacity").toFloat())
    }
    // Drop encoded payloads after preparation; drawing only retains decoded bitmaps.
    variant.remove("images")
    return SkinImages(roles)
}

@Composable internal fun skinContent(role:String):PaddingValues {
    val content=LocalSkinImages.current.roles[role]?.content ?: return PaddingValues(0.dp)
    return PaddingValues(start=content[0].dp,top=content[1].dp,end=content[2].dp,bottom=content[3].dp)
}

@Composable internal fun Modifier.skinImage(role:String):Modifier {
    val image=LocalSkinImages.current.roles[role] ?: return this
    val paint=remember(image) { Paint(Paint.FILTER_BITMAP_FLAG).apply { alpha=(255*image.opacity).toInt() } }
    return drawWithContent {
        val bitmap=image.bitmap;val sw=bitmap.width;val sh=bitmap.height
        drawIntoCanvas { canvas ->
            fun part(l:Int,t:Int,r:Int,b:Int,x:Float,y:Float,right:Float,bottom:Float) {
                if(r>l && b>t && right>x && bottom>y) canvas.nativeCanvas.drawBitmap(bitmap,Rect(l,t,r,b),RectF(x,y,right,bottom),paint)
            }
            when(image.mode) {
                "stretch"->part(0,0,sw,sh,0f,0f,size.width,size.height)
                "tile"->{
                    var y=0f
                    canvas.nativeCanvas.save();canvas.nativeCanvas.clipRect(0f,0f,size.width,size.height)
                    while(y<size.height) {var x=0f;while(x<size.width) {part(0,0,sw,sh,x,y,x+sw*density,y+sh*density);x+=sw*density};y+=sh*density}
                    canvas.nativeCanvas.restore()
                }
                "nine_slice"->{
                    val (l,t,r,b)=image.slice
                    val sx=minOf(density,size.width/(l+r).coerceAtLeast(1));val sy=minOf(density,size.height/(t+b).coerceAtLeast(1))
                    val xs=intArrayOf(0,l,sw-r,sw);val ys=intArrayOf(0,t,sh-b,sh)
                    val dx=floatArrayOf(0f,l*sx,size.width-r*sx,size.width);val dy=floatArrayOf(0f,t*sy,size.height-b*sy,size.height)
                    for(y in 0..2) for(x in 0..2) part(xs[x],ys[y],xs[x+1],ys[y+1],dx[x],dy[y],dx[x+1],dy[y+1])
                }
            }
        }
        drawContent()
    }
}
