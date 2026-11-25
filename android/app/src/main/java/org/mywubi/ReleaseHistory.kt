package org.mywubi

internal const val PAGE_SIZE = 10

private val stableTag = Regex("[vV](0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)")

/** Draft creation order can differ from global version order when builds overlap. */
internal fun <T> orderedStableReleases(page: (Int) -> List<T>, tag: (T) -> String?): List<T> {
    val versions = mutableListOf<Pair<List<Long>, T>>()
    var index = 1
    while (true) {
        val batch = page(index++)
        for (release in batch) {
            val match = stableTag.matchEntire(tag(release) ?: continue) ?: continue
            versions += match.groupValues.drop(1).map(String::toLong) to release
        }
        if (batch.size < PAGE_SIZE) break
    }
    return versions.sortedWith(compareByDescending<Pair<List<Long>, T>> { it.first[0] }
        .thenByDescending { it.first[1] }.thenByDescending { it.first[2] }).map { it.second }
}
