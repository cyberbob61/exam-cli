package com.cyberbob61.exam

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UpdateTest {
    // trimmed GitHub release JSON, as the android-latest job creates it
    private val release = """
        {"tag_name":"android-latest","name":"Android build 57","prerelease":true,
         "assets":[{"name":"exam-android.apk","browser_download_url":"https://github.com/x/y/releases/download/android-latest/exam-android.apk"}]}
    """

    @Test
    fun newerBuildIsAnUpdate() {
        val u = parseUpdate(release, current = 56)!!
        assertEquals(57, u.build)
        assertEquals("https://github.com/x/y/releases/download/android-latest/exam-android.apk", u.apk)
    }

    @Test
    fun sameOrOlderBuildIsNot() {
        assertNull(parseUpdate(release, current = 57))
        assertNull(parseUpdate(release, current = 90))
    }

    @Test
    fun releaseWithoutApkIsNot() {
        assertNull(parseUpdate("""{"name":"Android build 57","assets":[]}""", current = 1))
    }
}
