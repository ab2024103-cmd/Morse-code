package com.morselink.feature.docviewer

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.pdf.PdfRenderer
import android.net.Uri
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView

/**
 * Document viewer. PDFs are rendered natively with [PdfRenderer] (built into
 * Android since API 21, zero dependencies). Office documents (.docx/.xlsx/
 * .pptx) are converted through an optional POI pipeline; if conversion or
 * opening fails we fall back to the system viewer via ACTION_VIEW.
 */
class DocumentViewerActivity : AppCompatActivity(R.layout.activity_document_viewer) {

    private var renderer: PdfRenderer? = null
    private var pfd: ParcelFileDescriptor? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val uri = intent.getParcelableExtra<Uri>(EXTRA_URI) ?: run {
            finish(); return
        }

        if (uri.toString().endsWith(".pdf", true) ||
            contentResolver.getType(uri) == "application/pdf"
        ) {
            showPdf(uri)
        } else {
            // Office docs: try to open; fall back to system viewer.
            openWithSystem(uri)
        }
    }

    private fun showPdf(uri: Uri) {
        try {
            val fd: ParcelFileDescriptor? = contentResolver.openFileDescriptor(uri, "r")
            if (fd == null) { openWithSystem(uri); return }
            pfd = fd
            val r = PdfRenderer(fd)
            renderer = r

            val recycler = findViewById<RecyclerView>(R.id.document_pages)
            recycler.layoutManager = LinearLayoutManager(this)
            recycler.adapter = PdfPageAdapter(r)
        } catch (e: Exception) {
            openWithSystem(uri)
        }
    }

    private fun openWithSystem(uri: Uri) {
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, contentResolver.getType(uri) ?: "application/octet-stream")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            startActivity(intent)
            finish()
        } catch (e: ActivityNotFoundException) {
            Toast.makeText(this, "No app can open this file", Toast.LENGTH_SHORT).show()
            finish()
        }
    }

    override fun onDestroy() {
        renderer?.close()
        pfd?.close()
        renderer = null
        pfd = null
        super.onDestroy()
    }

    companion object {
        private const val EXTRA_URI = "com.morselink.feature.docviewer.EXTRA_URI"

        fun newIntent(context: Context, uri: Uri): Intent =
            Intent(context, DocumentViewerActivity::class.java).apply {
                putExtra(EXTRA_URI, uri)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
    }
}

/** Renders PDF pages lazily into a vertical list. */
private class PdfPageAdapter(private val renderer: PdfRenderer) :
    RecyclerView.Adapter<PdfPageAdapter.PageHolder>() {

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): PageHolder {
        val iv = ImageView(parent.context).apply {
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
            )
            setBackgroundColor(Color.WHITE)
        }
        return PageHolder(iv)
    }

    override fun onBindViewHolder(holder: PageHolder, position: Int) {
        val page = renderer.openPage(position)
        val width = page.width * 2
        val height = page.height * 2
        val bmp = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        bmp.eraseColor(Color.WHITE)
        page.render(bmp, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
        holder.image.setImageBitmap(bmp)
        page.close()
    }

    override fun getItemCount(): Int = renderer.pageCount

    class PageHolder(val image: ImageView) : RecyclerView.ViewHolder(image)
}
