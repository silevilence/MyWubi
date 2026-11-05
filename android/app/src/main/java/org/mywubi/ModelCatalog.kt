package org.mywubi

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract as Docs
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

/** SAF reads and native package hashes run on the settings worker, never the IME. */
class ModelCatalog(private val context: Context) {
    private val config = EngineStore(context).configFile
    fun call(op:String, vararg fields:Pair<String,Any>):JSONArray {
        val request=JSONObject().put("op",op).put("config",config.absolutePath)
        fields.forEach { (k,v)->request.put(k,v) }
        val response=JSONObject(NativeBridge.modelPackages(request.toString().toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8))
        check(response.getBoolean("ok")) {response.getString("error")}
        return response.getJSONArray("value")
    }
    fun importTree(tree:Uri):JSONArray {
        val resolver=context.contentResolver
        fun child(parent:String,name:String):String {
            val uri=Docs.buildChildDocumentsUriUsingTree(tree,parent)
            resolver.query(uri,arrayOf(Docs.Document.COLUMN_DOCUMENT_ID,Docs.Document.COLUMN_DISPLAY_NAME),null,null,null)?.use { cursor ->
                while(cursor.moveToNext()) if(cursor.getString(1)==name) return cursor.getString(0)
            }
            error("模型包缺少文件：$name")
        }
        val root=Docs.getTreeDocumentId(tree)
        fun document(path:String):Uri {
            check(path.length<=240 && path.split('/').all { it.isNotEmpty() && it!="." && it!=".." && !it.contains('\\') && !it.contains(':') }) {"模型文件路径非法"}
            var id=root
            path.split('/').forEach { id=child(id,it) }
            return Docs.buildDocumentUriUsingTree(tree,id)
        }
        val manifestBytes=resolver.openInputStream(document("manifest.json"))!!.use { input ->
            val bytes=java.io.ByteArrayOutputStream(); val buffer=ByteArray(4096)
            while(bytes.size()<=65536) { val n=input.read(buffer,0,minOf(buffer.size,65537-bytes.size())); if(n<0) break; bytes.write(buffer,0,n) }
            bytes.toByteArray()
        }
        check(manifestBytes.size<=65536) {"模型清单过大"}
        val manifest=JSONObject(manifestBytes.toString(Charsets.UTF_8))
        val files=manifest.getJSONArray("files")
        check(files.length() in 1..32) {"模型文件数超限"}
        var total=0L
        for(i in 0 until files.length()) {
            val size=files.getJSONObject(i).getLong("bytes")
            check(size in 0..4L*1024*1024*1024 && total<=4L*1024*1024*1024-size) {"模型包超过 4 GiB"}
            total+=size
        }
        check(context.filesDir.usableSpace>total+64L*1024*1024) {"空间不足，原有模型未改变"}
        val stage=File(context.cacheDir,"model-import-${UUID.randomUUID()}").apply { check(mkdir()) }
        try {
            for(i in 0 until files.length()) {
                val entry=files.getJSONObject(i); val path=entry.getString("path")
                val uri=try {document(path)} catch(e:IllegalStateException) {
                    val reason=when(path) {
                        manifest.optString("vocabulary") -> "缺少分词器：character-step-v1 必须包含已校验的 vocabulary.json"
                        manifest.optString("prediction_policy"),manifest.optString("generation_policy") -> "缺少预测策略：请提供与权重绑定的评分准入策略或生成策略；不能用于字词预测"
                        else -> "模型包损坏或不完整：${e.message}；请重新准备并校验模型包"
                    }
                    error(reason)
                }
                val output=File(stage,path)
                check(output.canonicalPath.startsWith(stage.canonicalPath+File.separator))
                output.parentFile!!.mkdirs()
                val expected=entry.getLong("bytes")
                resolver.openInputStream(uri)!!.use { input -> output.outputStream().use { out ->
                    val buffer=ByteArray(65536); var read=0L
                    while(true) { val n=input.read(buffer); if(n<0) break; read+=n; check(read<=expected) {"文件大小不匹配"}; out.write(buffer,0,n) }
                    check(read==expected) {"模型文件不完整"}
                } }
            }
            File(stage,"manifest.json").writeBytes(manifestBytes)
            return call("Import","source" to stage.absolutePath)
        } finally { stage.deleteRecursively() }
    }
}
