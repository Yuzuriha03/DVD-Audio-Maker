package com.dvda.maker

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.net.URLConnection
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Android shell for the Flutter application.
 *
 * All user files are reached through the Storage Access Framework. The shell
 * never asks for broad storage permissions and never resolves a filesystem path
 * outside the app sandbox. For native readers it mirrors a granted SAF tree
 * under `filesDir/saf-mirror` by streaming every document through
 * `ContentResolver.openInputStream` into regular private files. The mirror
 * preserves the directory layout and never creates symlinks or platform
 * reparse points.
 */
class MainActivity : FlutterActivity() {
    private var directoryResult: MethodChannel.Result? = null
    private var directoryLinkForNative = false
    private var openProfileResult: MethodChannel.Result? = null
    private var saveProfileResult: MethodChannel.Result? = null
    private var saveProfileContent: String? = null
    private var saveTextFileResult: MethodChannel.Result? = null
    private var saveTextFileContent: String? = null
    private var pickFontResult: MethodChannel.Result? = null

    private class SafMirrorState {
        var entries: Int = 0
        var truncated: Boolean = false
    }

    private data class SafChild(val uri: Uri, val isDirectory: Boolean)

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        if (startupCleanupDone.compareAndSet(false, true)) {
            // Remove mirror trees and any legacy /proc/self/fd symlink trees from
            // earlier versions before the first selection is mirrored.
            safExecutor.execute {
                deleteTree(File(filesDir, SAF_MIRROR_DIRECTORY))
                deleteTree(File(filesDir, LEGACY_LINK_DIRECTORY))
            }
        }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
            .setMethodCallHandler { call, result -> handleCall(call, result) }
    }

    private fun handleCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "pickDirectory" -> pickDirectory(call, result)
            "linkDirectory" -> {
                val treeUri = call.argument<String>("treeUri").orEmpty()
                safExecutor.execute {
                    result.success(if (treeUri.isEmpty()) null else linkDirectory(treeUri))
                }
            }
            "exportDirectory" -> {
                val sourcePath = call.argument<String>("sourcePath").orEmpty()
                val treeUri = call.argument<String>("treeUri").orEmpty()
                safExecutor.execute {
                    result.success(
                        sourcePath.isNotEmpty() && treeUri.isNotEmpty() &&
                            exportDirectory(sourcePath, treeUri),
                    )
                }
            }
            "openOutput" -> {
                val treeUri = call.argument<String>("treeUri")
                    ?: call.argument<String>("path").orEmpty()
                result.success(openOutput(treeUri))
            }
            "openProfile" -> pickProfile(result)
            "saveProfile" -> saveProfile(call, result)
            "saveTextFile" -> saveTextFile(call, result)
            "pickFont" -> pickFont(result)
            // SAF needs no broad storage permission; kept as a compatibility no-op.
            "ensureStorageAccess" -> result.success(null)
            else -> result.notImplemented()
        }
    }

    private fun pickDirectory(call: MethodCall, result: MethodChannel.Result) {
        directoryResult?.success(null)
        directoryResult = result
        directoryLinkForNative = call.argument<Boolean>("linkForNative") ?: false
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION,
            )
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                treeUriForPath(call.argument<String>("initial"))?.let { uri ->
                    putExtra(DocumentsContract.EXTRA_INITIAL_URI, uri)
                }
            }
        }
        startActivityForResult(intent, REQUEST_DIRECTORY)
    }

    private fun pickProfile(result: MethodChannel.Result) {
        openProfileResult?.success(null)
        openProfileResult = result
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/json"
        }
        startActivityForResult(intent, REQUEST_OPEN_PROFILE)
    }

    private fun pickFont(result: MethodChannel.Result) {
        pickFontResult?.success(null)
        pickFontResult = result
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_MIME_TYPES, FONT_MIME_TYPES)
        }
        startActivityForResult(intent, REQUEST_PICK_FONT)
    }

    private fun saveProfile(call: MethodCall, result: MethodChannel.Result) {
        saveProfileResult?.success(false)
        saveProfileResult = result
        saveProfileContent = call.argument<String>("content").orEmpty()
        val requested = call.argument<String>("suggestedName")?.trim()
        val suggestedName = if (requested.isNullOrEmpty()) DEFAULT_PROFILE_NAME else requested
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/json"
            putExtra(Intent.EXTRA_TITLE, suggestedName)
        }
        startActivityForResult(intent, REQUEST_SAVE_PROFILE)
    }

    private fun saveTextFile(call: MethodCall, result: MethodChannel.Result) {
        saveTextFileResult?.success(false)
        saveTextFileResult = result
        saveTextFileContent = call.argument<String>("content").orEmpty()
        val requested = call.argument<String>("suggestedName")?.trim()
        val suggestedName = if (requested.isNullOrEmpty()) DEFAULT_TEXT_NAME else requested
        val requestedMime = call.argument<String>("mimeType")?.trim()
        val mimeType = if (requestedMime.isNullOrEmpty()) DEFAULT_TEXT_MIME else requestedMime
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = mimeType
            putExtra(Intent.EXTRA_TITLE, suggestedName)
        }
        startActivityForResult(intent, REQUEST_SAVE_TEXT)
    }

    private fun openOutput(treeUriText: String): Boolean {
        if (treeUriText.isBlank()) {
            return false
        }
        val uri = runCatching { Uri.parse(treeUriText) }.getOrNull() ?: return false
        if (uri.scheme != ContentResolver.SCHEME_CONTENT) {
            return false
        }
        val document = runCatching {
            DocumentsContract.buildDocumentUriUsingTree(
                uri,
                DocumentsContract.getTreeDocumentId(uri),
            )
        }.getOrDefault(uri)
        return viewDirectory(document)
    }

    private fun viewDirectory(uri: Uri): Boolean = runCatching {
        startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, DocumentsContract.Document.MIME_TYPE_DIR)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
        )
    }.isSuccess

    private fun persistTreePermission(uri: Uri) {
        runCatching {
            contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        }
        runCatching {
            contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
        }
    }

    private fun directorySelection(uri: Uri, linkForNative: Boolean): Map<String, String>? {
        val nativePath = if (linkForNative) mirrorTree(uri) ?: return null else ""
        return mapOf(
            "displayPath" to (treeDisplayPath(uri) ?: uri.toString()),
            "nativePath" to nativePath,
            "treeUri" to uri.toString(),
        )
    }

    private fun linkDirectory(treeUriText: String): Map<String, String>? {
        val uri = runCatching { Uri.parse(treeUriText) }.getOrNull() ?: return null
        if (uri.scheme != ContentResolver.SCHEME_CONTENT) {
            return null
        }
        persistTreePermission(uri)
        return directorySelection(uri, true)
    }

    private fun mirrorTree(treeUri: Uri): String? {
        val key = treeKey(treeUri)
        val root = File(File(filesDir, SAF_MIRROR_DIRECTORY), key)
        deleteTree(root)
        if (!root.mkdirs() && !root.isDirectory) {
            return null
        }
        val state = SafMirrorState()
        return runCatching {
            val rootId = DocumentsContract.getTreeDocumentId(treeUri)
            if (!walkTree(treeUri, rootId, root, 0, state)) {
                error("Cannot read the granted SAF tree")
            }
            if (state.truncated) {
                error("The granted SAF tree is too large or contains unreadable entries")
            }
            root.absolutePath
        }.getOrElse {
            deleteTree(root)
            null
        }
    }

    private fun walkTree(
        treeUri: Uri,
        documentId: String,
        directory: File,
        depth: Int,
        state: SafMirrorState,
    ): Boolean {
        if (depth > MAX_SAF_DEPTH) {
            state.truncated = true
            return false
        }
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, documentId)
        val cursor = contentResolver.query(
            childrenUri,
            arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            ),
            null,
            null,
            null,
        )
        if (cursor == null) {
            state.truncated = true
            return false
        }
        cursor.use {
            while (it.moveToNext()) {
                if (state.entries >= MAX_SAF_ENTRIES) {
                    state.truncated = true
                    return@use
                }
                state.entries++
                val childId = it.getString(0)
                if (childId != null) {
                    val name = safeEntryName(it.getString(1), ENTRY_FALLBACK_NAME)
                    val mimeType = it.getString(2)
                    val childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, childId)
                    val target = uniqueChild(directory, name)
                    if (mimeType == DocumentsContract.Document.MIME_TYPE_DIR) {
                        if (target.mkdirs() || target.isDirectory) {
                            walkTree(treeUri, childId, target, depth + 1, state)
                        } else {
                            state.truncated = true
                        }
                    } else if (!copyDocument(childUri, target)) {
                        state.truncated = true
                    }
                }
            }
        }
        return true
    }

    private fun copyDocument(uri: Uri, target: File): Boolean {
        val parent = target.parentFile ?: return false
        parent.mkdirs()
        val temporary = File(parent, "${target.name}.part")
        val copied = runCatching {
            contentResolver.openInputStream(uri)?.use { input ->
                temporary.outputStream().use { output -> input.copyTo(output) }
            } != null
        }.getOrDefault(false)
        if (!copied) {
            temporary.delete()
            return false
        }
        target.delete()
        if (!temporary.renameTo(target)) {
            temporary.delete()
            return false
        }
        return true
    }

    private fun deleteTree(file: File) {
        // Mirrors contain regular files/directories only. Legacy /proc links
        // pointed at regular files, so isDirectory stays false and delete()
        // removes the link itself without following it.
        if (file.isDirectory) {
            file.listFiles()?.forEach(::deleteTree)
        }
        file.delete()
    }

    private fun treeKey(uri: Uri): String {
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(uri.toString().toByteArray(Charsets.UTF_8))
        return digest.take(TREE_KEY_BYTES).joinToString("") {
            "%02x".format(it.toInt() and 0xff)
        }
    }

    private fun uniqueChild(directory: File, name: String): File {
        var candidate = File(directory, name)
        var index = 1
        while (candidate.exists()) {
            candidate = File(directory, withSuffix(name, index))
            index++
        }
        return candidate
    }

    private fun safeEntryName(raw: String?, fallback: String): String {
        val cleaned = raw.orEmpty()
            .map { character ->
                if (character == '/' || character == '\\' ||
                    character.code < 0x20 || character.code == 0x7f
                ) {
                    '_'
                } else {
                    character
                }
            }
            .joinToString("")
            .trim()
            .trim('.')
            .take(MAX_ENTRY_NAME)
        return when {
            cleaned.isEmpty() -> fallback
            cleaned == "." || cleaned == ".." -> fallback
            else -> cleaned
        }
    }

    private fun withSuffix(name: String, index: Int): String {
        val dot = name.lastIndexOf('.')
        return if (dot > 0) {
            "${name.substring(0, dot)} ($index)${name.substring(dot)}"
        } else {
            "$name ($index)"
        }
    }

    private fun privateSource(sourcePath: String): File? {
        val filesRoot = runCatching { filesDir.canonicalFile }.getOrNull() ?: return null
        val direct = runCatching { File(sourcePath).canonicalFile }.getOrNull()
        if (direct != null &&
            (direct == filesRoot || direct.path.startsWith(filesRoot.path + File.separator))
        ) {
            return direct
        }
        // The caller may carry a path captured under a different user/data alias.
        val marker = "/${packageName}/files/"
        val markerIndex = sourcePath.indexOf(marker)
        if (markerIndex < 0) {
            return null
        }
        val relative = sourcePath.substring(markerIndex + marker.length)
        if (relative.isEmpty()) {
            return null
        }
        val candidate = runCatching { File(filesRoot, relative).canonicalFile }.getOrNull()
            ?: return null
        if (!candidate.path.startsWith(filesRoot.path + File.separator)) {
            return null
        }
        return candidate
    }

    private fun exportDirectory(sourcePath: String, treeUriText: String): Boolean {
        val source = privateSource(sourcePath) ?: return false
        if (!source.isDirectory) {
            return false
        }
        val treeUri = runCatching { Uri.parse(treeUriText) }.getOrNull() ?: return false
        if (treeUri.scheme != ContentResolver.SCHEME_CONTENT) {
            return false
        }
        persistTreePermission(treeUri)
        return runCatching {
            val parent = DocumentsContract.buildDocumentUriUsingTree(
                treeUri,
                DocumentsContract.getTreeDocumentId(treeUri),
            )
            exportInto(source, treeUri, parent, 0)
        }.getOrDefault(false)
    }

    private fun exportInto(source: File, treeUri: Uri, parent: Uri, depth: Int): Boolean {
        if (depth > MAX_SAF_DEPTH) {
            return false
        }
        val children = source.listFiles() ?: return true
        if (children.isEmpty()) {
            return true
        }
        val existing = childEntries(treeUri, parent)
        val used = existing.keys.toMutableSet()
        var success = true
        for (child in children) {
            if (child.isDirectory) {
                val desired = safeEntryName(child.name, ENTRY_FALLBACK_NAME)
                val previous = existing[desired.lowercase()]
                val target = if (previous != null && previous.isDirectory) {
                    previous.uri
                } else {
                    val name = uniqueExportName(desired, used)
                    createDocument(
                        parent,
                        DocumentsContract.Document.MIME_TYPE_DIR,
                        name,
                    )?.also { existing[name.lowercase()] = SafChild(it, true) }
                }
                if (target == null) {
                    success = false
                } else if (!exportInto(child, treeUri, target, depth + 1)) {
                    success = false
                }
            } else if (child.isFile) {
                val desired = safeEntryName(child.name, ENTRY_FALLBACK_NAME)
                val previous = existing[desired.lowercase()]
                val target = if (previous != null && !previous.isDirectory) {
                    previous.uri
                } else {
                    val name = uniqueExportName(desired, used)
                    createDocument(parent, mimeTypeFor(name), name)?.also {
                        existing[name.lowercase()] = SafChild(it, false)
                    }
                }
                if (target == null) {
                    success = false
                    continue
                }
                val written = runCatching {
                    val stream = contentResolver.openOutputStream(target, "wt")
                        ?: contentResolver.openOutputStream(target, "w")
                    stream?.use { output ->
                        child.inputStream().use { input -> input.copyTo(output) }
                    } != null
                }.getOrDefault(false)
                if (!written) {
                    success = false
                }
            }
        }
        return success
    }

    private fun childEntries(treeUri: Uri, parent: Uri): MutableMap<String, SafChild> = runCatching {
        val parentId = DocumentsContract.getDocumentId(parent)
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId)
        val entries = mutableMapOf<String, SafChild>()
        contentResolver.query(
            childrenUri,
            arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            ),
            null,
            null,
            null,
        )?.use { cursor ->
            while (cursor.moveToNext()) {
                val documentId = cursor.getString(0)
                val displayName = cursor.getString(1)
                if (documentId != null && displayName != null) {
                    val uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)
                    entries[displayName.lowercase()] = SafChild(
                        uri,
                        cursor.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR,
                    )
                }
            }
        }
        entries
    }.getOrDefault(mutableMapOf())

    private fun createDocument(parent: Uri, mimeType: String, name: String): Uri? =
        runCatching {
            DocumentsContract.createDocument(contentResolver, parent, mimeType, name)
        }.getOrNull()

    private fun uniqueExportName(desired: String, used: MutableSet<String>): String {
        var candidate = desired
        var index = 1
        while (!used.add(candidate.lowercase())) {
            candidate = withSuffix(desired, index)
            index++
        }
        return candidate
    }

    private fun mimeTypeFor(name: String): String {
        URLConnection.guessContentTypeFromName(name)?.let { return it }
        return when (name.substringAfterLast('.', "").lowercase()) {
            "flac" -> "audio/flac"
            "xml" -> "application/xml"
            "png" -> "image/png"
            "jpg", "jpeg" -> "image/jpeg"
            else -> "application/octet-stream"
        }
    }

    private fun displayName(uri: Uri): String? = runCatching {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }
    }.getOrNull()

    private fun safeFontName(raw: String?): String? {
        val name = raw?.substringAfterLast('/')?.substringAfterLast('\\')?.trim().orEmpty()
        if (name.isEmpty()) {
            return null
        }
        val extension = name.substringAfterLast('.', "").lowercase()
        if (extension !in FONT_EXTENSIONS) {
            return null
        }
        val stem = name.substringBeforeLast('.')
            .replace(Regex("[^A-Za-z0-9._-]"), "_")
            .trim('_', '.')
            .take(MAX_FONT_STEM)
        return "${stem.ifEmpty { "font" }}.$extension"
    }

    private fun uniqueFontFile(directory: File, name: String): File {
        val stem = name.substringBeforeLast('.')
        val extension = name.substringAfterLast('.')
        var candidate = File(directory, name)
        var index = 1
        while (candidate.exists()) {
            candidate = File(directory, "$stem-$index.$extension")
            index++
        }
        return candidate
    }

    private fun importFont(uri: Uri): String? {
        val fallbackName = runCatching {
            uri.lastPathSegment?.let { Uri.decode(it) }
        }.getOrNull()
        val safeName = safeFontName(displayName(uri) ?: fallbackName) ?: return null
        val directory = File(filesDir, FONT_DIRECTORY)
        if (!directory.isDirectory && !directory.mkdirs()) {
            return null
        }
        val target = uniqueFontFile(directory, safeName)
        val temporary = File(directory, "${target.name}.part")
        val copied = runCatching {
            contentResolver.openInputStream(uri)?.use { input ->
                temporary.outputStream().use { output -> input.copyTo(output) }
            } != null
        }.getOrDefault(false)
        if (!copied || temporary.length() == 0L) {
            temporary.delete()
            return null
        }
        if (!temporary.renameTo(target)) {
            temporary.delete()
            return null
        }
        return target.absolutePath
    }

    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        when (requestCode) {
            REQUEST_DIRECTORY -> {
                val result = directoryResult ?: return
                directoryResult = null
                val linkForNative = directoryLinkForNative
                directoryLinkForNative = false
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(null)
                    return
                }
                persistTreePermission(uri)
                safExecutor.execute {
                    result.success(directorySelection(uri, linkForNative))
                }
            }
            REQUEST_OPEN_PROFILE -> {
                val result = openProfileResult ?: return
                openProfileResult = null
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(null)
                    return
                }
                val text = runCatching {
                    contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() }
                }.getOrNull()
                result.success(text)
            }
            REQUEST_SAVE_PROFILE -> {
                val result = saveProfileResult ?: return
                saveProfileResult = null
                val content = saveProfileContent.orEmpty()
                saveProfileContent = null
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(false)
                    return
                }
                val saved = runCatching {
                    contentResolver.openOutputStream(uri)?.use { stream ->
                        stream.write(content.toByteArray(Charsets.UTF_8))
                    } != null
                }.getOrDefault(false)
                result.success(saved)
            }
            REQUEST_SAVE_TEXT -> {
                val result = saveTextFileResult ?: return
                saveTextFileResult = null
                val content = saveTextFileContent.orEmpty()
                saveTextFileContent = null
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(false)
                    return
                }
                val saved = runCatching {
                    contentResolver.openOutputStream(uri)?.use { stream ->
                        stream.write(content.toByteArray(Charsets.UTF_8))
                    } != null
                }.getOrDefault(false)
                result.success(saved)
            }
            REQUEST_PICK_FONT -> {
                val result = pickFontResult ?: return
                pickFontResult = null
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(null)
                    return
                }
                result.success(importFont(uri))
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun treeUriForPath(path: String?): Uri? {
        val value = path?.trim().orEmpty()
        if (value.isEmpty()) {
            return null
        }
        val root = Environment.getExternalStorageDirectory().absolutePath.trimEnd('/')
        val document = when {
            value == root -> "primary:"
            value.startsWith("$root/") -> "primary:" + value.removePrefix("$root/")
            value.startsWith("/storage/") -> {
                val parts = value.removePrefix("/storage/").split("/", limit = 2)
                val volume = parts[0]
                if (volume.isEmpty() || volume == "emulated") {
                    return null
                }
                if (parts.size < 2 || parts[1].isEmpty()) "$volume:" else "$volume:" + parts[1]
            }
            else -> return null
        }
        return runCatching {
            DocumentsContract.buildTreeDocumentUri(EXTERNAL_STORAGE_AUTHORITY, document)
        }.getOrNull()
    }

    @Suppress("DEPRECATION")
    private fun treeDisplayPath(uri: Uri): String? = runCatching {
        val document = DocumentsContract.getTreeDocumentId(uri)
        val parts = document.split(":", limit = 2)
        val base = when (parts[0]) {
            "primary", "home" -> Environment.getExternalStorageDirectory().absolutePath
            else -> "/storage/" + parts[0]
        }
        if (parts.size < 2 || parts[1].isEmpty()) base else "$base/" + parts[1]
    }.getOrNull()

    companion object {
        private const val CHANNEL = "dvda/platform"
        private const val DEFAULT_PROFILE_NAME = "dvda-profile.json"
        private const val DEFAULT_TEXT_NAME = "dvda-log.txt"
        private const val DEFAULT_TEXT_MIME = "text/plain"
        private const val EXTERNAL_STORAGE_AUTHORITY = "com.android.externalstorage.documents"
        private const val REQUEST_DIRECTORY = 11
        private const val REQUEST_SAVE_PROFILE = 12
        private const val REQUEST_OPEN_PROFILE = 13
        private const val REQUEST_SAVE_TEXT = 14
        private const val REQUEST_PICK_FONT = 15
        private const val FONT_DIRECTORY = "fonts"
        private const val MAX_FONT_STEM = 64
        private const val SAF_MIRROR_DIRECTORY = "saf-mirror"
        private const val LEGACY_LINK_DIRECTORY = "saf-links"
        private const val TREE_KEY_BYTES = 12
        private const val MAX_SAF_DEPTH = 32
        private const val MAX_SAF_ENTRIES = 4096
        private const val MAX_ENTRY_NAME = 96
        private const val ENTRY_FALLBACK_NAME = "item"
        private val FONT_EXTENSIONS = setOf("ttf", "otf", "ttc")
        private val FONT_MIME_TYPES = arrayOf(
            "font/ttf",
            "font/otf",
            "font/ttc",
            "font/collection",
            "application/x-font-ttf",
            "application/x-font-opentype",
            "application/x-font-truetype",
            "application/vnd.ms-opentype",
            "application/font-sfnt",
            "application/octet-stream",
        )
        private val safExecutor = Executors.newSingleThreadExecutor()
        private val startupCleanupDone = AtomicBoolean(false)
    }
}
