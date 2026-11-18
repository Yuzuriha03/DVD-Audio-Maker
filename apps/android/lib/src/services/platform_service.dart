import 'dart:io' show Platform;
import 'package:flutter/services.dart';
import 'saf_documents.dart';

/// Failure raised by a platform picker or file action.
class PlatformBridgeException implements Exception {
  PlatformBridgeException(this.message, {this.code = ''});

  final String message;

  /// Native PlatformException code, e.g. "SAF_CANCELLED" for a user cancel.
  final String code;

  @override
  String toString() => message;
}

/// One progress event emitted by the SAF I/O adapters while exporting or while
/// exporting ISOs. [kind] identifies the adapter event.
class TaskIoProgress {
  const TaskIoProgress({
    required this.kind,
    required this.done,
    required this.total,
    this.name = '',
  });

  final String kind;
  final int done;
  final int total;
  final String name;
}

/// Outcome of the transactional SAF export.
class ExportResult {
  const ExportResult({
    required this.ok,
    required this.cancelled,
    this.failureCode = '',
    this.failureFile = '',
    this.exported = const <String>[],
    this.freedBytes = 0,
  });

  final bool ok;
  final bool cancelled;
  final String failureCode;
  final String failureFile;
  final List<String> exported;
  final int freedBytes;

  static const ExportResult failed = ExportResult(
    ok: false,
    cancelled: false,
    failureCode: 'EXPORT_FAILED',
  );

  static ExportResult failure(String code, [String file = '']) {
    return ExportResult(
      ok: false,
      cancelled: false,
      failureCode: code,
      failureFile: file,
    );
  }

  static ExportResult fromMap(Object? value) {
    if (value is bool) {
      return ExportResult(ok: value, cancelled: false);
    }
    if (value is! Map) {
      return failed;
    }
    final exported = value['exported'];
    return ExportResult(
      ok: value['ok'] == true,
      cancelled: value['cancelled'] == true,
      failureCode: value['failureCode'] is String
          ? value['failureCode']! as String
          : '',
      failureFile: value['failureFile'] is String
          ? value['failureFile']! as String
          : '',
      exported: exported is List
          ? exported.whereType<String>().toList(growable: false)
          : const <String>[],
      freedBytes: value['freedBytes'] is int ? value['freedBytes']! as int : 0,
    );
  }
}

/// Outcome of the explicit internal-cache cleanup entry.
class CleanupResult {
  const CleanupResult({
    required this.ok,
    this.freedBytes = 0,
    this.deleted = const <String>[],
    this.error = '',
    this.progressDiscarded = false,
  });

  final bool ok;
  final int freedBytes;
  final List<String> deleted;
  final String error;

  /// True when the merged action also removed a real unfinished task. False is
  /// also valid (no progress existed) and must not be reported as a failure.
  final bool progressDiscarded;

  static CleanupResult fromMap(Object? value) {
    if (value is! Map) {
      return const CleanupResult(ok: false, error: 'cleanup unavailable');
    }
    final deleted = value['deleted'];
    return CleanupResult(
      ok: value['ok'] == true,
      freedBytes: value['freedBytes'] is int ? value['freedBytes']! as int : 0,
      deleted: deleted is List
          ? deleted.whereType<String>().toList(growable: false)
          : const <String>[],
      error: value['error'] is String ? value['error']! as String : '',
    );
  }
}

/// Installed Android package version.
class AppVersion {
  const AppVersion({required this.version, required this.buildNumber});

  /// `PackageManager` versionName; the shared update rule compares this value.
  final String version;

  /// `PackageManager` versionCode, shown in the About page only.
  final int buildNumber;
}

/// A directory granted through the Storage Access Framework.
///
/// [treeUri] is the persistable SAF tree URI, [displayPath] is a human-readable
/// path for the UI only. [nativePath] is the absolute app-private mirror path
/// returned when preparing a mirror; a picker result leaves it empty.
class DirectorySelection {
  const DirectorySelection({
    required this.displayPath,
    required this.nativePath,
    required this.treeUri,
  });

  /// Human-readable path for display only.
  final String displayPath;

  /// Absolute app-private mirror path, or empty before mirror preparation.
  final String nativePath;

  /// Persistable `content://` tree URI.
  final String treeUri;

  /// Whether the selection carries no display path.
  bool get isEmpty => displayPath.isEmpty;

  static DirectorySelection? fromMap(Object? value) {
    if (value is! Map<Object?, Object?>) {
      return null;
    }
    final displayPath = value['displayPath'];
    final nativePath = value['nativePath'];
    final treeUri = value['treeUri'];
    if (displayPath is! String || treeUri is! String || treeUri.isEmpty) {
      return null;
    }
    return DirectorySelection(
      displayPath: displayPath,
      nativePath: nativePath is String ? nativePath : '',
      treeUri: treeUri,
    );
  }
}

/// Thin wrapper over `MethodChannel('dvda/platform')`.
///
/// All user files are reached through the Storage Access Framework; no broad
/// storage permission is requested.
///
/// Native contract (owned by the Android shell task):
/// * `pickDirectory` — args `{initial: String?, linkForNative: bool}`,
///   returns a `DirectorySelection` map, or `null` when cancelled or failed.
/// * `linkDirectory` — args `{treeUri: String}`, rebuilds the app-private SAF
///   mirror and returns a `DirectorySelection` map, or `null`.
/// * `ensureSourceLink` — same arguments and result; reuses a complete mirror
///   if available, otherwise rebuilds it.
/// * `exportDirectory` — args `{sourcePath: String, treeUri: String,
///   cachePaths: a list of app-private mirrors}`, returns the transactional
///   export result map
///   (`ok`, `cancelled`, `failureCode`, `failureFile`, `exported`); the adapter
///   verifies temporary SAF documents before replacing the visible ISO and
///   releases the task's app-private mirrors after success.
/// * `cancelExport` — no args, aborts the running transactional export.
/// * `ensureRuntimeFonts` — no args, installs the APK font asset under
///   app-private runtime storage and returns `{path, installed}`.
/// * `cleanupInternalCaches` — no args, removes known rebuildable caches from
///   older versions and returns `{ok, freedBytes, deleted}`.
/// * `openOutput` — args `{treeUri: String}`, returns `true` when the SAF
///   directory was handed to a viewer.
/// * `openProfile` — no args, returns the picked profile JSON text.
/// * `saveProfile` — args `{content: String, suggestedName: String}`,
///   returns `true`/null when saved.
/// * `saveTextFile` — args `{content: String, suggestedName: String,
///   mimeType: String}`, returns `true` when the text file was written.
/// * `pickFont` — no args, returns the absolute path of a copied
///   `.ttf`/`.otf`/`.ttc` font, or `null` when cancelled or unsupported.
///
/// A missing plugin is treated as a no-op so the UI can still be rendered and
/// exercised without the Android shell; result-returning helpers fall back to
/// `null` or `false` instead of throwing.
class PlatformService {
  PlatformService({MethodChannel? channel})
    : _channel = channel ?? const MethodChannel('dvda/platform') {
    try {
      _channel.setMethodCallHandler(_handlePlatformCall);
    } catch (_) {
      // A pure unit test may construct the service before a Flutter binding
      // exists; progress events are optional and the method calls still fail
      // gracefully as MissingPluginException in that environment.
    }
  }

  final MethodChannel _channel;
  late final SafDocumentAdapter _safDocuments = SafDocumentAdapter();

  /// The real shell enables descriptors on Android; host-side widget tests
  /// keep using the mocked mirror adapter and never call the platform plugin.
  bool get supportsSafDescriptors => Platform.isAndroid;

  Future<SafDocumentCollection> openSourceDocuments(
    String root,
    String treeUri,
  ) => _safDocuments.openTree(root, treeUri);

  /// Receives export progress events pushed by the SAF adapter.
  void Function(TaskIoProgress progress)? onProgress;

  Future<Object?> _handlePlatformCall(MethodCall call) async {
    final handler = onProgress;
    if (handler == null) {
      return null;
    }
    final arguments = call.arguments;
    if (arguments is! Map) {
      return null;
    }
    final done = arguments['done'];
    final total = arguments['total'];
    if (done is! int || total is! int) {
      return null;
    }
    final name = arguments['name'] is String
        ? arguments['name']! as String
        : '';
    if (call.method == 'exportProgress') {
      handler(
        TaskIoProgress(kind: 'export', done: done, total: total, name: name),
      );
    }
    return null;
  }

  /// Opens the system directory picker and persists the returned SAF grant.
  ///
  /// Set [linkForNative] to request read-only access for an input directory.
  /// Output directories also request write access. Picking does not build a
  /// mirror; the task only prepares one later if needed.
  Future<DirectorySelection?> pickDirectory({
    String initial = '',
    bool linkForNative = false,
  }) async {
    final result = await _invoke<Object?>('pickDirectory', <String, Object?>{
      'initial': initial,
      'linkForNative': linkForNative,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Returns canonical app-private roots supplied by the running Android shell.
  Future<Map<String, String>?> appPaths() async {
    final result = await _invoke<Object?>('appPaths');
    if (result is! Map) {
      return null;
    }
    return <String, String>{
      for (final entry in result.entries)
        if (entry.key is String && entry.value is String)
          entry.key as String: entry.value as String,
    };
  }

  Future<DirectorySelection?> ensureSourceLink(String treeUri) async {
    final result = await _invoke<Object?>('ensureSourceLink', <String, Object?>{
      'treeUri': treeUri,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Checks that [path] is a readable directory inside the current filesDir.
  Future<bool> validatePrivateDirectory(String path) async {
    final result = await _invoke<Object?>(
      'validatePrivateDirectory',
      <String, Object?>{'path': path},
    );
    return result == true;
  }

  /// Rebuilds the native mirror for a persisted SAF directory.
  Future<DirectorySelection?> linkDirectory(String treeUri) async {
    final result = await _invoke<Object?>('linkDirectory', <String, Object?>{
      'treeUri': treeUri,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Exports the task-local product into a granted SAF tree as a transaction.
  ///
  /// The native adapter writes temporary SAF documents, verifies their stored
  /// length and SHA-256, backs the visible ISO up and only then renames the
  /// verified temporaries into place. A failure or cancellation leaves the
  /// previously exported ISO untouched. [cachePaths] names the app-private SAF
  /// mirrors the adapter releases after a successful export.
  Future<ExportResult> exportDirectory(
    String sourcePath,
    String treeUri, {
    List<String> cachePaths = const <String>[],
  }) async {
    final result = await _invoke<Object?>('exportDirectory', <String, Object?>{
      'sourcePath': sourcePath,
      'treeUri': treeUri,
      'cachePaths': cachePaths,
    });
    return ExportResult.fromMap(result);
  }

  /// Aborts a running transactional export between copy chunks.
  Future<void> cancelExport() async {
    await _invoke<Object?>('cancelExport');
  }

  /// Installs the bundled menu font collection into app-private storage.
  ///
  /// The APK asset is the only font source. Returns the installed absolute path
  /// in "path" when available; a missing asset or failed copy reports "error".
  Future<Map<String, Object?>?> ensureRuntimeFonts() async {
    final result = await _invoke<Object?>('ensureRuntimeFonts');
    if (result is! Map) {
      return null;
    }
    return <String, Object?>{
      for (final entry in result.entries)
        if (entry.key is String) entry.key as String: entry.value,
    };
  }

  /// Removes known rebuildable app-private caches from older versions.
  ///
  /// This is the explicit user-confirmed recovery entry; it never deletes
  /// user-granted SAF folders, the unfinished workspace or exported ISOs, and it
  /// does not replace the shared finish of the current task.
  Future<CleanupResult> cleanupInternalCaches() async {
    final result = await _invoke<Object?>('cleanupInternalCaches');
    return CleanupResult.fromMap(result);
  }

  Future<String?> openProfile() async {
    final result = await _invoke<Object?>('openProfile');
    if (result is String && result.trim().isNotEmpty) {
      return result;
    }
    return null;
  }

  /// Opens the system document picker for a `.ttf`, `.otf` or `.ttc` font.
  ///
  /// The native side copies the selection into the app-private `files/fonts`
  /// directory under a sanitized file name and returns that absolute path, so
  /// the Rust side can read it without SAF permissions. A cancelled picker, an
  /// unsupported file or a missing platform shell returns `null`.
  Future<String?> pickFont() async {
    final result = await _invoke<Object?>('pickFont');
    if (result is String && result.trim().isNotEmpty) {
      return result;
    }
    return null;
  }

  Future<bool> saveProfile(
    String content, {
    String suggestedName = 'dvda-profile.json',
  }) async {
    final result = await _invoke<Object?>('saveProfile', <String, Object?>{
      'content': content,
      'suggestedName': suggestedName,
    });
    return result != false;
  }

  /// Saves [content] as a UTF-8 text file through the system create-document UI.
  ///
  /// Returns `true` only when the file was actually written; a cancelled picker
  /// or a missing platform shell returns `false`.
  Future<bool> saveTextFile(
    String content, {
    String suggestedName = 'dvda-log.txt',
    String mimeType = 'text/plain',
  }) async {
    final result = await _invoke<Object?>('saveTextFile', <String, Object?>{
      'content': content,
      'suggestedName': suggestedName,
      'mimeType': mimeType,
    });
    return result == true;
  }

  /// Opens the granted SAF [treeUri] with the system directory viewer.
  ///
  /// Returns `true` only when the platform handed the SAF directory to a
  /// viewer; an invalid URI, an unavailable viewer or a missing platform shell
  /// returns `false`.
  Future<bool> openOutput(String treeUri) async {
    final result = await _invoke<Object?>('openOutput', <String, Object?>{
      'treeUri': treeUri,
    });
    return result == true;
  }

  /// Installed package versionName/versionCode from PackageManager.
  Future<AppVersion?> appVersion() async {
    final result = await _invoke<Object?>('appVersion');
    if (result is! Map) {
      return null;
    }
    final version = result['versionName'];
    final code = result['versionCode'];
    if (version is! String || version.isEmpty) {
      return null;
    }
    return AppVersion(
      version: version,
      buildNumber: code is int ? code : int.tryParse('$code') ?? 0,
    );
  }

  /// Posts one separate update notification. Returns false when notification
  /// permission/channel is unavailable; the caller then shows an in-app notice
  /// and must not request the permission itself.
  Future<bool> showUpdateNotification({
    required String title,
    required String text,
    required String downloadUrl,
  }) async {
    final result = await _invoke<Object?>(
      'showUpdateNotification',
      <String, Object?>{
        'title': title,
        'text': text,
        'downloadUrl': downloadUrl,
      },
    );
    return result == true;
  }

  /// Cancels the update notification only; the task progress notification uses
  /// its own id/channel and is never touched.
  Future<void> clearUpdateNotification() async {
    await _invoke<Object?>('clearUpdateNotification');
  }

  /// Opens the release download URL with Android ACTION_VIEW.
  Future<bool> openUpdateUrl(String url) async {
    if (!url.startsWith('https://')) {
      return false;
    }
    final result = await _invoke<Object?>('openUpdateUrl', <String, Object?>{
      'url': url,
    });
    return result == true;
  }

  /// Publishes the ongoing task progress in the status bar and, on
  /// Android 16+, as a promoted ongoing Live Update.
  Future<void> updateStatusNotification({
    required String title,
    required String text,
    double? progress,
    required bool ongoing,
  }) async {
    await _invoke<Object?>('updateStatusNotification', <String, Object?>{
      'title': title,
      'text': text,
      'progress': progress,
      'ongoing': ongoing,
    });
  }

  /// Removes the task notification.
  Future<void> clearStatusNotification() async {
    await _invoke<Object?>('clearStatusNotification');
  }

  Future<T?> _invoke<T>(
    String method, [
    Map<String, Object?>? arguments,
  ]) async {
    try {
      return await _channel.invokeMethod<T>(method, arguments);
    } on MissingPluginException {
      return null;
    } on PlatformException catch (error) {
      throw PlatformBridgeException(
        error.message ?? error.code,
        code: error.code,
      );
    }
  }
}
