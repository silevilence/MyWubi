package org.mywubi

import org.junit.Assert.assertEquals
import org.junit.Test

class ReleaseHistoryTest {
    @Test fun ranksStableVersionsAcrossPagesEvenWhenDraftCreationOrderDiffers() {
        val history = orderedStableReleases(
            page = { if (it == 1) listOf("V2.9.1", "v2.10.0") + List(8) { "prerelease" }
                     else listOf("v10.0.0", "V2.10.12", "draft") },
            tag = { it.takeUnless { name -> name == "draft" } },
        )
        assertEquals(listOf("v10.0.0", "V2.10.12", "v2.10.0", "V2.9.1"), history)
    }

    @Test(expected = java.io.IOException::class)
    fun incompleteDirectoryFailsEvenIfFirstPageContainsAnUpdate() {
        orderedStableReleases(page = { if (it == 1) List(PAGE_SIZE) { "V1.0.0" }
                                       else throw java.io.IOException("offline") }, tag = { it })
    }
}
