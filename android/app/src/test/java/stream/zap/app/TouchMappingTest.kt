package stream.zap.app

import org.junit.Assert.assertEquals
import org.junit.Test

class TouchMappingTest {
    @Test
    fun cameraOnLeftShiftsTapByOneInset() {
        val (dx, dy) = TouchMapping.offset(149, 0)
        assertEquals(2028.9f + dx, 1879.9f, 0.01f)
        assertEquals(920.25f + dy, 920.25f, 0.01f)
    }

    @Test
    fun cameraOnRightNeedsNoShift() {
        val (dx, dy) = TouchMapping.offset(0, 0)
        assertEquals(0f, dx, 0f)
        assertEquals(0f, dy, 0f)
    }

    @Test
    fun portraitTopInsetShiftsY() {
        val (dx, dy) = TouchMapping.offset(0, 149)
        assertEquals(0f, dx, 0f)
        assertEquals(-149f, dy, 0f)
    }
}
