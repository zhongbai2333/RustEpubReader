package com.zhongbai233.epub.reader.tts

import com.zhongbai233.epub.reader.i18n.I18n

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.util.Log
import com.zhongbai233.epub.reader.model.ContentBlock
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.File
import java.io.FileOutputStream

/**
 * TTS playback manager — handles block-by-block synthesis and playback
 * with prefetch for gapless audio.
 */
class TtsManager(context: Context) {

    companion object {
        private const val TAG = "TtsManager"
    }

    private val engine = EdgeTtsEngine(context.cacheDir)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val cacheDir = File(context.cacheDir, "tts_audio").also { it.mkdirs() }

    // ── Public state ──
    private val _playing = MutableStateFlow(false)
    val playing: StateFlow<Boolean> = _playing.asStateFlow()

    private val _paused = MutableStateFlow(false)
    val paused: StateFlow<Boolean> = _paused.asStateFlow()

    private val _currentBlock = MutableStateFlow(-1)
    val currentBlock: StateFlow<Int> = _currentBlock.asStateFlow()

    private val _status = MutableStateFlow("")
    val status: StateFlow<String> = _status.asStateFlow()

    // ── Configuration ──
    var voiceName: String = "zh-CN-XiaoxiaoNeural"
    var rate: Int = 0
    var volume: Int = 0

    // ── Internal state ──
    private var blocks: List<ContentBlock> = emptyList()
    private var mediaPlayer: MediaPlayer? = null
    val audible: Boolean get() = runCatching { mediaPlayer?.isPlaying == true }.getOrDefault(false)
    private var prefetchJob: Job? = null
    private var prefetchBlock: Int = -1
    private var prefetchBytes: ByteArray? = null
    private var synthesisJob: Job? = null
    private var currentTempFile: File? = null
    private var prefetchTempFile: File? = null

    /**
     * Start TTS playback from the beginning of the chapter.
     */
    fun start(chapterBlocks: List<ContentBlock>) {
        stop()
        blocks = chapterBlocks
        val first = nextReadableBlock(0)
        if (first >= blocks.size) {
            _status.value = I18n.t("tts.book_done")
            return
        }
        _playing.value = true
        _paused.value = false
        _currentBlock.value = first
        synthesizeAndPlay(first)
    }

    /**
     * Start TTS playback from a specific block index.
     */
    fun startFrom(chapterBlocks: List<ContentBlock>, blockIndex: Int) {
        stop()
        blocks = chapterBlocks
        val target = nextReadableBlock(blockIndex)
        if (target >= blocks.size) {
            _status.value = I18n.t("tts.book_done")
            return
        }
        _playing.value = true
        _paused.value = false
        _currentBlock.value = target
        synthesizeAndPlay(target)
    }

    fun pause() {
        mediaPlayer?.pause()
        _paused.value = true
    }

    fun resume() {
        mediaPlayer?.start()
        _paused.value = false
    }

    fun togglePause() {
        if (_paused.value) resume() else pause()
    }

    fun stop() {
        synthesisJob?.cancel()
        prefetchJob?.cancel()
        mediaPlayer?.apply {
            try { stop() } catch (_: Exception) {}
            release()
        }
        mediaPlayer = null
        prefetchBytes = null
        prefetchBlock = -1
        _playing.value = false
        _paused.value = false
        _currentBlock.value = -1
        _status.value = ""
        cleanupTempFiles()
    }

    fun destroy() {
        stop()
        scope.cancel()
        engine.shutdown()
    }

    // ── Internal logic ──

    private fun synthesizeAndPlay(blockIdx: Int) {
        val text = blockText(blockIdx)
        if (text.isBlank()) {
            advanceToNext()
            return
        }

        _status.value = I18n.t("tts.synthesizing_label")
        synthesisJob?.cancel()
        synthesisJob = scope.launch {
            val bytes = engine.synthesize(text, voiceName, rate, volume)
            if (!isActive) return@launch

            if (bytes != null) {
                _status.value = ""
                playBytes(bytes)
                // Start prefetching next block
                startPrefetch(blockIdx)
            } else {
                _status.value = I18n.t("tts.error_synthesis_failed")
                _playing.value = false
            }
        }
    }

    private fun startPrefetch(currentIdx: Int) {
        val next = nextReadableBlock(currentIdx + 1)
        if (next >= blocks.size) {
            prefetchBytes = null
            prefetchBlock = -1
            return
        }
        val text = blockText(next)
        if (text.isBlank()) {
            prefetchBytes = null
            prefetchBlock = -1
            return
        }

        prefetchBlock = next
        prefetchBytes = null
        prefetchJob?.cancel()
        prefetchJob = scope.launch {
            val bytes = engine.synthesize(text, voiceName, rate, volume)
            if (isActive && bytes != null) {
                prefetchBytes = bytes
            }
        }
    }

    private fun advanceToNext() {
        val next = nextReadableBlock(_currentBlock.value + 1)
        if (next >= blocks.size) {
            // Chapter finished
            _status.value = I18n.t("tts.chapter_done")
            _playing.value = false
            _currentBlock.value = -1
            return
        }
        _currentBlock.value = next

        // Check if prefetch is ready
        if (prefetchBlock == next && prefetchBytes != null) {
            val bytes = prefetchBytes!!
            prefetchBytes = null
            _status.value = ""
            scope.launch {
                playBytes(bytes)
                startPrefetch(next)
            }
        } else {
            synthesizeAndPlay(next)
        }
    }

    private fun playBytes(bytes: ByteArray) {
        mediaPlayer?.apply {
            try { stop() } catch (_: Exception) {}
            release()
        }
        cleanupTempFiles()

        try {
            // Write to temp file (MediaPlayer needs a FileDescriptor)
            val tempFile = File(cacheDir, "tts_current.mp3")
            FileOutputStream(tempFile).use { it.write(bytes) }
            currentTempFile = tempFile

            mediaPlayer = MediaPlayer().apply {
                setAudioAttributes(
                    AudioAttributes.Builder()
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .build()
                )
                setDataSource(tempFile.absolutePath)
                prepare()
                setOnCompletionListener { advanceToNext() }
                setOnErrorListener { _, what, extra ->
                    Log.e(TAG, "MediaPlayer error: what=$what extra=$extra")
                    _status.value = I18n.t("tts.error_playback")
                    _playing.value = false
                    true
                }
                start()
            }
        } catch (e: Exception) {
            Log.e(TAG, "playBytes failed", e)
            _status.value = I18n.t("tts.error_playback")
            _playing.value = false
        }
    }

    private fun nextReadableBlock(from: Int): Int {
        var idx = from
        while (idx < blocks.size) {
            when (blocks[idx]) {
                is ContentBlock.Paragraph,
                is ContentBlock.Heading -> return idx
                else -> idx++
            }
        }
        return idx
    }

    private fun blockText(idx: Int): String {
        if (idx < 0 || idx >= blocks.size) return ""
        return when (val block = blocks[idx]) {
            is ContentBlock.Paragraph -> block.spans.joinToString("") { it.text }
            is ContentBlock.Heading -> block.spans.joinToString("") { it.text }
            else -> ""
        }
    }

    private fun cleanupTempFiles() {
        currentTempFile?.delete()
        currentTempFile = null
        prefetchTempFile?.delete()
        prefetchTempFile = null
    }
}
