package org.mywubi

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AppUpdateTest {
    private fun update() = AppUpdate("0.5.2", 5, 26, "说明", "${RELEASE_ROOT}v0.5.2/MyWubi-0.5.2-Android.apk", "a".repeat(64), listOf("arm64-v8a", "x86_64"))

    @Test fun versionsCompatibilityAndMalformedMetadata() {
        val update = AppUpdate.parse(update().json())
        assertTrue(applicableUpdate(update, 4, 34, listOf("x86_64")))
        assertFalse(applicableUpdate(update, 5, 34, listOf("x86_64")))
        assertFalse(applicableUpdate(update, 6, 34, listOf("x86_64")))
        assertFalse(applicableUpdate(update, 4, 25, listOf("x86_64")))
        assertFalse(applicableUpdate(update, 4, 34, listOf("armeabi-v7a")))
        for ((field, value) in listOf("schema" to 2, "sha256" to "bad", "apkUrl" to "https://evil.test/app.apk", "versionCode" to 0)) {
            assertTrue(runCatching { AppUpdate.parse(update.json().put(field, value)) }.isFailure)
        }
    }

    @Test fun onlyPublishedStableReleasesWithExactAssetAreAccepted() {
        fun release() = JSONObject("""{"draft":false,"prerelease":false,"published_at":"2026-11-03T00:00:00Z","tag_name":"v0.5.2","assets":[{"name":"android-update.json","browser_download_url":"${RELEASE_ROOT}v0.5.2/android-update.json"}]}""")
        assertNotNull(releaseManifestUrl(release()))
        assertEquals(update(), parseReleaseUpdate(releaseManifestUrl(release())!!,update().json()))
        assertTrue(runCatching { parseReleaseUpdate("${RELEASE_ROOT}v0.5.1/android-update.json",update().json()) }.isFailure)
        assertNull(releaseManifestUrl(release().put("draft", true)))
        assertNull(releaseManifestUrl(release().put("prerelease", true)))
        assertTrue(runCatching { releaseManifestUrl(release().put("published_at", JSONObject.NULL)) }.isFailure)
        assertTrue(runCatching { releaseManifestUrl(release().put("assets", org.json.JSONArray())) }.isFailure)
    }

    @Test fun throttleBackoffAndClockRecovery() {
        val initial = CheckSchedule()
        val failed = initial.completed(1000, false)
        assertFalse(failed.permits(1001))
        assertTrue(failed.permits(failed.next))
        assertTrue(failed.completed(1000, false).next > failed.next)
        assertEquals(0, failed.completed(1000, true).failures)
        assertTrue(CheckSchedule(Long.MAX_VALUE, 3).permits(1000))
    }

    @Test fun slowFailureDoesNotBlockMainThreadAndManualRetryWorks() {
        val instrumentation = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val name = "update-test-${System.nanoTime()}"
        val gate = java.util.concurrent.CountDownLatch(1)
        val calls = java.util.concurrent.atomic.AtomicInteger()
        lateinit var updates: AppUpdates
        instrumentation.runOnMainSync {
            updates = AppUpdates(context, name) {
                check(android.os.Looper.myLooper() != android.os.Looper.getMainLooper())
                when (calls.incrementAndGet()) {
                    1 -> { gate.await(5, java.util.concurrent.TimeUnit.SECONDS); error("offline") }
                    2 -> update().copy(versionCode = 2_000_000_000)
                    else -> null
                }
            }
            updates.check()
            repeat(20) { updates.check(manual = true) }
            val restarted = AppUpdates(context, name) { error("Process restart must respect the reserved interval") }
            restarted.check()
            assertFalse(restarted.state.value.checking)
        }
        instrumentation.runOnMainSync { assertTrue(updates.state.value.checking) }
        gate.countDown()
        fun awaitResult() {
            val deadline = android.os.SystemClock.uptimeMillis() + 10000
            while (updates.state.value.checking && android.os.SystemClock.uptimeMillis() < deadline) Thread.sleep(20)
            assertFalse(updates.state.value.checking)
        }
        awaitResult()
        assertEquals(1, calls.get())
        assertTrue(updates.state.value.message.contains("失败"))
        instrumentation.runOnMainSync { updates.check() }
        assertEquals(1, calls.get())
        instrumentation.runOnMainSync { updates.check(manual = true) }
        awaitResult()
        assertNotNull(updates.state.value.available)
        instrumentation.runOnMainSync {
            val recreated = AppUpdates(context, name) { error("cache should prevent request") }
            assertEquals(updates.state.value.available, recreated.state.value.available)
            recreated.check()
            assertFalse(recreated.state.value.checking)
        }
        instrumentation.runOnMainSync { updates.check(manual = true) }
        awaitResult()
        assertNull(updates.state.value.available)
        assertFalse(context.getSharedPreferences(name,0).contains("available"))
        context.getSharedPreferences(name, 0).edit().clear().commit()
    }

    @Test fun cachedInstalledVersionCannotLeaveAnUpdateBadgeAfterUpgrade() {
        val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        val name = "installed-cache-${System.nanoTime()}"
        @Suppress("DEPRECATION")
        val installed = packageVersion(context.packageManager.getPackageInfo(context.packageName,0))
        context.getSharedPreferences(name,0).edit().putString("available",update().copy(versionCode=installed).json().toString()).commit()
        assertNull(AppUpdates(context,name) { error("No network needed to discard stale badge") }.state.value.available)
        context.getSharedPreferences(name,0).edit().clear().commit()
    }
}
