package org.mywubi

import android.content.Context
import android.os.Build
import java.io.File

/** Pinned to the C API used by the Rust ort adapter. The APK owns this manifest. */
internal object OnnxComponent {
    const val version = "1.31.1"
    const val library = "libonnxruntime.so"
    const val url = "https://repo.maven.apache.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/" +
        "$version/onnxruntime-android-$version.aar"
    const val archiveBytes = 55_138_794L
    const val archiveHash = "d4d50660300ed2c4f38ffdfd5ab5bfa3a22f995f282dda24f42ea3ea55467b49"
    data class Payload(val abi: String, val bytes: Long, val hash: String)
    val payloads = listOf(
        Payload("arm64-v8a", 34_208_680, "46e0c16cd515c62a1dd6c5994448b5413705478354bc20a756a5a82634c3ca03"),
        Payload("x86_64", 40_827_944, "cfd9ede9b5a168750af260027a4dd012345bb407933156b9d4738c62d3454607")
    )
    fun payload(): Payload = Build.SUPPORTED_ABIS.firstNotNullOf { abi -> payloads.find { it.abi == abi } }
    fun bundled(context: Context) = File(context.applicationInfo.nativeLibraryDir, library)
    fun installed(context: Context) = File(context.noBackupFilesDir, "components/onnx-$version/${payload().abi}/$library")
    fun location(context: Context): File = bundled(context).takeIf(File::isFile) ?: installed(context)
}
