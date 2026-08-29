package com.morselink.feature.imageviewer

import android.content.Context
import android.graphics.Matrix
import android.graphics.drawable.Drawable
import android.util.AttributeSet
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import androidx.appcompat.widget.AppCompatImageView

/**
 * Minimal pinch-zoom + pan image view (no external library). Uses a [Matrix]
 * over the drawn bitmap, driven by [ScaleGestureDetector] and a
 * [GestureDetector] for tap/double-tap.
 */
class ZoomImageView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : AppCompatImageView(context, attrs, defStyleAttr) {

    private val matrix = Matrix()
    private var scaleFactor = MIN_SCALE
    private var lastScale = MIN_SCALE
    private var lastFocusX = 0f
    private var lastFocusY = 0f

    private lateinit var scaleDetector: ScaleGestureDetector
    private lateinit var gestureDetector: GestureDetector

    init {
        scaleType = ScaleType.MATRIX
        scaleDetector = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            override fun onScale(detector: ScaleGestureDetector): Boolean {
                val f = detector.scaleFactor
                lastScale = (lastScale * f).coerceIn(MIN_SCALE, MAX_SCALE)
                matrix.postScale(f, f, detector.focusX, detector.focusY)
                lastFocusX = detector.focusX
                lastFocusY = detector.focusY
                imageMatrix = matrix
                invalidate()
                return true
            }
        })
        gestureDetector = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(e: MotionEvent) = true
            override fun onDoubleTap(e: MotionEvent): Boolean {
                if (lastScale > 1f) reset() else zoomTo(2f, e.x, e.y)
                return true
            }
        })
    }

    override fun setImageDrawable(drawable: Drawable?) {
        super.setImageDrawable(drawable)
        if (drawable != null) reset()
    }

    private fun reset() {
        matrix.reset()
        lastScale = MIN_SCALE
        imageMatrix = matrix
        invalidate()
    }

    private fun zoomTo(scale: Float, cx: Float, cy: Float) {
        lastScale = scale.coerceIn(MIN_SCALE, MAX_SCALE)
        matrix.postScale(scale, scale, cx, cy)
        imageMatrix = matrix
        invalidate()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        gestureDetector.onTouchEvent(event)
        scaleDetector.onTouchEvent(event)
        if (scaleDetector.isInProgress) {
            // Pan while zooming.
            when (event.actionMasked) {
                MotionEvent.ACTION_MOVE -> {
                    val dx = (lastFocusX - event.x)
                    val dy = (lastFocusY - event.y)
                    matrix.postTranslate(dx, dy)
                    imageMatrix = matrix
                    lastFocusX = event.x
                    lastFocusY = event.y
                    invalidate()
                }
            }
            parent?.requestDisallowInterceptTouchEvent(true)
            return true
        }
        return super.onTouchEvent(event) || scaleDetector.isInProgress
    }

    companion object {
        private const val MIN_SCALE = 0.5f
        private const val MAX_SCALE = 6f
    }
}
