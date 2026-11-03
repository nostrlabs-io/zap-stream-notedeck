package stream.zap.app

object TouchMapping {
    fun offset(viewLeftInWindow: Int, viewTopInWindow: Int): Pair<Float, Float> =
        Pair(-viewLeftInWindow.toFloat(), -viewTopInWindow.toFloat())
}
