package org.mywubi

import android.content.Context
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.os.Build
import java.io.File
import java.security.MessageDigest
import java.util.zip.ZipFile

internal const val MAX_APK_BYTES = 256L * 1024 * 1024

internal fun sha256(file: File): String {
    val digest = MessageDigest.getInstance("SHA-256")
    file.inputStream().use { input ->
        val buffer = ByteArray(64 * 1024)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            digest.update(buffer, 0, count)
        }
    }
    return digest.digest().joinToString("") { "%02x".format(it) }
}

@Suppress("DEPRECATION")
internal fun packageVersion(info: PackageInfo): Long =
    if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()

/** Accept verified signer rotation on API 28+, exact signer sets on older systems/multi-signers. */
@Suppress("DEPRECATION")
internal fun compatibleSignatures(installed: PackageInfo, archive: PackageInfo): Boolean {
    fun keys(signatures: Array<android.content.pm.Signature>?) = signatures?.map { it.toCharsString() }?.toSet().orEmpty()
    if (Build.VERSION.SDK_INT < 28) {
        val current = keys(installed.signatures)
        return current.isNotEmpty() && current == keys(archive.signatures)
    }
    val old = installed.signingInfo ?: return false
    val new = archive.signingInfo ?: return false
    val current = keys(old.apkContentsSigners)
    val next = keys(new.apkContentsSigners)
    if (current.isEmpty() || next.isEmpty()) return false
    if (old.hasMultipleSigners() || new.hasMultipleSigners()) return current == next
    return current == next || keys(new.signingCertificateHistory).containsAll(current)
}

@Suppress("DEPRECATION")
internal fun verifyApk(context: Context, file: File, update: AppUpdate) {
    require(file.isFile && file.length() in 1..MAX_APK_BYTES) { "安装包不存在或大小异常，请重新下载" }
    require(sha256(file) == update.sha256) { "安装包校验和不符，请重新下载" }
    val pm = context.packageManager
    val flags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
    val archive = pm.getPackageArchiveInfo(file.path, flags) ?: error("无法验证安装包")
    val installed = pm.getPackageInfo(context.packageName, flags)
    require(archive.packageName == context.packageName) { "安装包包名不符" }
    require(packageVersion(archive) == update.versionCode && archive.versionName == update.versionName) { "安装包版本与更新信息不符" }
    require(packageVersion(archive) > packageVersion(installed)) { "不能安装相同或更旧的版本" }
    val minSdk = archive.applicationInfo?.minSdkVersion ?: error("缺少系统兼容信息")
    require(minSdk == update.minSdk && minSdk <= Build.VERSION.SDK_INT) { "安装包不兼容当前系统" }
    require(compatibleSignatures(installed, archive)) { "安装包签名与当前应用不兼容" }
    ZipFile(file).use { zip ->
        require(Build.SUPPORTED_ABIS.any { abi -> abi in update.abis && zip.getEntry("lib/$abi/libmywubi_android.so") != null }) { "安装包不兼容此设备架构" }
    }
}
