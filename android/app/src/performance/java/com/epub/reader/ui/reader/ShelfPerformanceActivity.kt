package com.zhongbai233.epub.reader.ui.reader

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.lifecycle.lifecycleScope
import com.zhongbai233.epub.reader.BuildConfig
import com.zhongbai233.epub.reader.MainActivity
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject

/** Separate installer process: provisioning allocations do not become app peak RSS. */
class ShelfPerformanceActivity : ComponentActivity() {
    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        check(BuildConfig.READER_PERFORMANCE_CHECKS && !BuildConfig.DEBUG && packageName == "local.reader.performance")
        lifecycleScope.launch {
            val external=checkNotNull(getExternalFilesDir(null))
            val run=checkNotNull(intent.getStringExtra("performance_run"))
            try {
                require(run.matches(Regex("[A-Za-z0-9-]{1,80}")))
                val installed=withContext(Dispatchers.IO) {
                    val corpus=File(external,"dense-shelf").canonicalFile
                    val manifestFile=File(corpus,"manifest.json")
                    check(manifestFile.length() in 1L..4L*1024*1024)
                    val bytes=manifestFile.readBytes();val manifest=JSONObject(String(bytes,Charsets.UTF_8))
                    check(manifest.getInt("schema")==1 && manifest.getString("scenario")=="dense-shelf")
                    val entries=manifest.getJSONArray("books");val count=manifest.getInt("count")
                    check(count in 1..10000 && entries.length()==count)
                    val root=filesDir;val books=File(root,"books")
                    check(!File(root,"library.json").exists() && (books.listFiles()?.isEmpty()!=false)) {"Use a fresh isolated app"}
                    check(books.mkdirs() || books.isDirectory)
                    val identities=mutableSetOf<String>();val catalog=JSONArray()
                    for(index in 0 until count) {
                        val source=entries.getJSONObject(index);val id=source.getString("id")
                        check(java.util.UUID.fromString(id).toString()==id && identities.add(id))
                        val relative="books/$id.epub";check(source.getString("file")==relative)
                        val input=File(corpus,relative).canonicalFile
                        check(input.path.startsWith(corpus.path+File.separator) && input.isFile)
                        check(input.length()==source.getLong("bytes") && input.length() in 1L..1024L*1024)
                        val digest=MessageDigest.getInstance("SHA-256")
                        val destination=File(books,"$id.epub")
                        input.inputStream().use { from -> destination.outputStream().use { to ->
                            val buffer=ByteArray(65536)
                            while(true) {val size=from.read(buffer);if(size<0)break;digest.update(buffer,0,size);to.write(buffer,0,size)}
                        } }
                        check(digest.digest().joinToString(""){"%02x".format(it)}==source.getString("sha256"))
                        val title=source.getString("title");check(title.length<=4096)
                        val book=JSONObject().put("id",id).put("title",title).put("path",destination.path)
                            .put("last_chapter",0).put("last_block",0).put("last_char_offset",0)
                            .put("last_chapter_title","第1章 $title").put("last_opened",1700000000000L-index*1000L)
                        catalog.put(book)
                        val config=JSONObject(book.toString());config.remove("path")
                        config.put("epub_path",destination.path).put("created_at",1700000000L).put("updated_at",1700000000L).put("settings",JSONObject())
                        File(books,"$id.json").writeText(config.toString())
                    }
                    File(root,"library.json").writeText(JSONObject().put("books",catalog).toString())
                    val manifestHash=MessageDigest.getInstance("SHA-256").digest(bytes).joinToString(""){"%02x".format(it)}
                    count to manifestHash
                }
                getSharedPreferences("reader_settings",0).edit().putBoolean("reader.introduction.v1",true).commit()
                startActivity(Intent(this@ShelfPerformanceActivity,MainActivity::class.java)
                    .putExtra("shelf_performance",true).putExtra("performance_run",run)
                    .putExtra("shelf_expected",installed.first).putExtra("shelf_manifest_sha256",installed.second)
                    .putExtra("shelf_started_ns",android.os.SystemClock.elapsedRealtimeNanos())
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))
            } catch(error:Throwable) {
                File(external,"shelf-performance.json").writeText(JSONObject().put("passed",false).put("run_id",run).put("error",error.stackTraceToString()).toString(2))
            } finally {
                finish()
                // This activity is declared in :fixture_installer, never the app process.
                android.os.Process.killProcess(android.os.Process.myPid())
            }
        }
    }
}
