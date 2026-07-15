package com.altsendme.plugin.native_utils

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.annotation.Keep
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicReference

@InvokeArg
class SelectorArgs {
    lateinit var channel: Channel
}

@InvokeArg
class CancelJobArgs(
    var channelId: Long = 0
)

@InvokeArg
class ExportToTreeArgs {
    var treeUri: String = ""
    var sourceDir: String = ""
}

@Keep
data class DownloadFolderSelectionResponse(
    val uri: String,
    val path: String,
)

@TauriPlugin
class NativeUtils(private val activity: Activity) : Plugin(activity) {
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private val jobs = ConcurrentHashMap<Long, Pair<Job, String>>()
    private val pendingShareUri = AtomicReference<Uri?>(null)

    companion object {
        private const val RW_PERMISSION_FLAGS =
            Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION
        private const val SHARE_RECEIVED_EVENT = "shareReceived"
    }

    @Command
    fun select_download_folder(invoke: Invoke) = startActivityForResult(
        invoke,
        Intent(Intent.ACTION_OPEN_DOCUMENT_TREE),
        this::handleDownloadFolderSelection.name
    )

    @Command
    fun select_send_document(invoke: Invoke) = startActivityForResult(
        invoke,
        Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            type = "*/*"
        },
        this::handleSendSelection.name
    )

    @Command
    fun select_send_folder(invoke: Invoke) = startActivityForResult(
        invoke,
        Intent(Intent.ACTION_OPEN_DOCUMENT_TREE),
        this::handleSendSelection.name
    )

    @Command
    fun consume_share_intent(invoke: Invoke) {
        val args = invoke.parseArgs(SelectorArgs::class.java)
        val uri = pendingShareUri.getAndSet(null)
            ?: takeShareUri(activity.intent)
            ?: return invoke.resolveObject(false)

        startUriCopy(uri, args.channel)
        invoke.resolveObject(true)
    }

    @Command
    fun cancel_job(invoke: Invoke) {
        val args = invoke.parseArgs(CancelJobArgs::class.java)
        val channelId = args.channelId
        val (job, tempFolder) = jobs.remove(channelId)
            ?: return invoke.reject("Trying to cancel a non existing job")
        scope.launch {
            try {
                job.cancelAndJoin()
                File(tempFolder).deleteRecursively()
                invoke.resolve()
            } catch (e: Exception) {
                invoke.reject(e.message)
            }
        }
    }

    @Command
    fun export_to_tree(invoke: Invoke) {
        val args = invoke.parseArgs(ExportToTreeArgs::class.java)
        scope.launch {
            try {
                val treeUri = Uri.parse(args.treeUri)
                val sourceDir = File(args.sourceDir)
                val result = exportDirectoryToTree(activity, treeUri, sourceDir)
                invoke.resolveObject(result)
            } catch (e: SecurityException) {
                invoke.reject(e.message ?: "SAF permission denied")
            } catch (e: Exception) {
                invoke.reject(e.message ?: "Failed to export to selected folder")
            }
        }
    }

    @ActivityCallback
    fun handleDownloadFolderSelection(invoke: Invoke, result: ActivityResult) {
        if (Activity.RESULT_OK != result.resultCode) return invoke.resolve(null)

        val uri = result.data?.data ?: return invoke.resolve(null)

        try {
            activity.contentResolver.takePersistableUriPermission(uri, RW_PERMISSION_FLAGS)

            invoke.resolveObject(
                DownloadFolderSelectionResponse(
                    uri.toString(),
                    uri.extractFolderOsPath(),
                )
            )

            activity.contentResolver.persistedUriPermissions.stream()
                .filter { it.uri != uri }
                .forEach {
                    activity.contentResolver.releasePersistableUriPermission(
                        it.uri,
                        RW_PERMISSION_FLAGS
                    )
                }
        } catch (e: Exception) {
            invoke.reject(e.message)
        }
    }

    @ActivityCallback
    fun handleSendSelection(invoke: Invoke, result: ActivityResult) {
        val args = invoke.parseArgs(SelectorArgs::class.java)
        val channel = args.channel

        if (Activity.RESULT_OK != result.resultCode) return invoke.resolveObject(false)

        val uri = result.data?.data ?: return invoke.resolveObject(false)

        startUriCopy(uri, channel)
        invoke.resolveObject(true)
    }

    override fun load(webView: WebView) {
        super.load(webView)

        // Cold start: capture share URI before the frontend mounts and asks to consume it.
        // Skip wiping file_cache when a share is pending so cleanup cannot race the copy.
        val shareUri = takeShareUri(activity.intent)
        if (shareUri != null) {
            pendingShareUri.set(shareUri)
        } else {
            scope.launch {
                activity.cacheDir.resolve("file_cache").deleteRecursively()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        val uri = takeShareUri(intent) ?: return
        pendingShareUri.set(uri)
        trigger(SHARE_RECEIVED_EVENT, JSObject())
    }

    override fun onDestroy() {
        jobs.forEach { _, (job, tempFolder) ->
            try {
                job.cancel()
                File(tempFolder).deleteRecursively()
            } catch (_: Exception) {
            }
        }

        scope.cancel()
        super.onDestroy()
    }

    private fun startUriCopy(uri: Uri, channel: Channel) {
        val path = listOf(
            activity.cacheDir.absolutePath,
            "file_cache",
            System.currentTimeMillis().toString(),
        ).joinToString(File.separator)

        val tempFolder = File(path)

        val job = scope.launch(start = CoroutineStart.LAZY) {
            try {
                tempFolder.parentFile?.mkdirs()
                    ?: throw IOException("Unable to create parent folders for ${tempFolder.absolutePath}")

                copyUri(activity, uri, tempFolder).collect {
                    channel.send(it.toJSObject())
                }
            } catch (_: Exception) {
                tempFolder.deleteRecursively()
            } finally {
                jobs.remove(channel.id)
            }
        }

        jobs[channel.id] = job to tempFolder.absolutePath
        job.start()
    }

    private fun takeShareUri(intent: Intent?): Uri? {
        if (intent == null || intent.action != Intent.ACTION_SEND) {
            return null
        }

        val uri = extractShareUri(intent) ?: return null

        // Prevent the same intent from being consumed again after process/activity recreation.
        intent.removeExtra(Intent.EXTRA_STREAM)
        intent.clipData = null
        intent.action = Intent.ACTION_MAIN

        return uri
    }

    private fun extractShareUri(intent: Intent): Uri? {
        parcelableStreamExtra(intent)?.let { return it }

        val clip = intent.clipData
        if (clip != null && clip.itemCount > 0) {
            clip.getItemAt(0)?.uri?.let { return it }
        }

        return null
    }

    @Suppress("DEPRECATION")
    private fun parcelableStreamExtra(intent: Intent): Uri? {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
        }
    }
}

fun Uri.extractFolderOsPath(): String {
    require(DocumentsContract.isTreeUri(this))

    val path = this.path
        ?: throw IOException("Unable to get path from selected download folder uri: $this")
    val baseExternalPath = Environment.getExternalStorageDirectory().path
    return try {
        val docId = DocumentsContract.getTreeDocumentId(this)
        val segments = docId.split(":")
        when {
            "primary" == segments[0] && segments.size > 1 -> "${baseExternalPath}/${segments[1]}"
            "primary" == segments[0] -> baseExternalPath
            segments.size > 1 -> "/storage/${segments[0]}/${segments[1]}"
            else -> "/storage/${segments[0]}/"
        }
    } catch (_: Exception) {
        path
    }
}
