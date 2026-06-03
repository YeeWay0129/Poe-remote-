package com.remotepoe.app.remote

import android.view.KeyEvent
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteModelsTest {
    @Test
    fun keyboardDownIsAcceptedAsSingleUserAction() {
        assertTrue(RemoteInputEvent.Keyboard(KeyEvent.KEYCODE_W, KeyEvent.ACTION_DOWN).isSingleUserAction())
    }

    @Test
    fun zeroMouseMoveIsIgnored() {
        assertFalse(RemoteInputEvent.MouseMove(0, 0, PointerMode.Relative).isSingleUserAction())
    }

    @Test
    fun wheelDeltaIsAccepted() {
        assertTrue(RemoteInputEvent.MouseWheel(0, -1).isSingleUserAction())
    }
}
