package io.github.kiliankoe.publictransport.smoke;

import static org.junit.Assert.assertEquals;

import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import org.junit.runner.RunWith;

// In Java: Kotlin code would call into the Kotlin library of the app, which R8 has shrunk.
@RunWith(AndroidJUnit4.class)
public class SmokeTest {
    @Test
    public void shrunkAppLoadsTheRustLibraryAndReadsAnAnswer() {
        assertEquals("10 360", Smoke.departures(InstrumentationRegistry.getInstrumentation().getTargetContext()));
    }
}
