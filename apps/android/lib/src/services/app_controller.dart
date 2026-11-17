import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../../bridge.dart';
import '../localization/app_strings.dart';
import '../models/app_settings.dart';
import 'platform_service.dart';
import 'saf_documents.dart';
import 'settings_store.dart';

/// Lifecycle of one `AppBridge.runProfile` invocation.
enum TaskPhase { idle, running, success, failure, cancelled }

/// One diagnostic row returned by the Rust application layer.
class AppDiagnostic {
  const AppDiagnostic({
    required this.code,
    required this.message,
    this.severity = 'error',
  });

  final String code;
  final String message;
  final String severity;

  bool get isWarning => severity.toLowerCase() == 'warning';
}

/// Parsed outcome of a profile action.
class TaskOutcome {
  const TaskOutcome({
    required this.ok,
    this.cancelled = false,
    required this.message,
    required this.diagnostics,
    required this.log,
    required this.done,
    required this.total,
    required this.raw,
  });

  final bool ok;

  /// True only when the shared Rust sequence reported an authoritative user
  /// cancellation. Hosts must not infer this from an empty failure text.
  final bool cancelled;
  final String message;
  final List<AppDiagnostic> diagnostics;
  final String log;
  final int done;
  final int total;
  final Map<String, dynamic> raw;

  bool get unfinished => raw['Unfinished'] == true;

  /// Same outcome with the shared final progress (delivery/finish tail).
  TaskOutcome withProgress(int done, int total) {
    return TaskOutcome(
      ok: ok,
      cancelled: cancelled,
      message: message,
      diagnostics: diagnostics,
      log: log,
      done: done,
      total: total,
      raw: raw,
    );
  }

  /// Task-owned folder holding the product while Flutter exports it to SAF.
  ///
  /// The shared Rust sequence sets it only when verification succeeded and the
  /// platform still has to deliver the product.
  String? get deliveryDirectory {
    final value = raw['FinalDirectory'];
    if (value is String && value.trim().isNotEmpty) {
      return value;
    }
    return null;
  }

  factory TaskOutcome.fromJson(Map<String, dynamic> json) {
    return TaskOutcome(
      ok: json['Ok'] == true,
      cancelled: json['Cancelled'] == true,
      message: _stringValue(json['Message']),
      diagnostics: _parseDiagnostics(json['Diagnostics']),
      log: _stringValue(json['Log']),
      done: _intValue(json['ProgressDone']),
      total: _intValue(json['ProgressTotal']),
      raw: json,
    );
  }

  factory TaskOutcome.failure(String message) {
    return TaskOutcome(
      ok: false,
      message: message,
      diagnostics: const <AppDiagnostic>[],
      log: '',
      done: 0,
      total: 0,
      raw: const <String, dynamic>{},
    );
  }

  /// Successful outcome for an operation the shared Rust side already finished,
  /// such as retrying the delivery cleanup after a failed finish.
  factory TaskOutcome.success(
    String message, {
    int done = 0,
    int total = 0,
    String log = '',
  }) {
    return TaskOutcome(
      ok: true,
      message: message,
      diagnostics: const <AppDiagnostic>[],
      log: log,
      done: done,
      total: total,
      raw: const <String, dynamic>{'Ok': true},
    );
  }

  /// Tolerant parser: the native bridge always returns JSON, but an unexpected
  /// payload should surface as a readable failure instead of crashing.
  static TaskOutcome parse(String raw) {
    try {
      final dynamic decoded = jsonDecode(raw);
      if (decoded is Map) {
        return TaskOutcome.fromJson(Map<String, dynamic>.from(decoded));
      }
    } on FormatException {
      // Fall through to a raw failure message.
    }
    return TaskOutcome.failure(raw);
  }

  static String _stringValue(Object? value) {
    if (value is String) {
      return value;
    }
    if (value == null) {
      return '';
    }
    return value.toString();
  }

  static int _intValue(Object? value) {
    if (value is int) {
      return value;
    }
    if (value is num) {
      return value.toInt();
    }
    if (value is String) {
      return int.tryParse(value) ?? 0;
    }
    return 0;
  }

  static List<AppDiagnostic> _parseDiagnostics(Object? value) {
    if (value is! List) {
      return const <AppDiagnostic>[];
    }
    final result = <AppDiagnostic>[];
    for (final item in value) {
      if (item is Map) {
        final map = Map<String, dynamic>.from(item);
        result.add(
          AppDiagnostic(
            code: _stringValue(map['Code']),
            message: _stringValue(map['Message']),
            severity: _stringValue(map['Severity'] ?? map['severity']).isEmpty
                ? 'error'
                : _stringValue(map['Severity'] ?? map['severity']),
          ),
        );
      } else if (item != null) {
        result.add(AppDiagnostic(code: '', message: item.toString()));
      }
    }
    return result;
  }
}

/// Owns the editable profile, persisted settings, and the running task state.
///
/// `AppBridge.runProfile` runs the Rust work off the UI thread; this class
/// polls `AppBridge.progress()` on a timer so the interface never blocks.
/// A formatted log view produced by the shared Rust log formatter.
class FormattedLogView {
  const FormattedLogView({
    required this.text,
    required this.problemText,
    required this.problems,
  });

  final String text;
  final String problemText;
  final int problems;
}

typedef LogFormatter = FormattedLogView Function(String language, String log);

/// Injectable bridge entry points so widget/SAF tests never need a live Rust
/// library; production keeps the shared `AppBridge` implementations.
typedef TaskBridgeRunner =
    Future<String> Function(String profileJson, String action);
typedef ProfileIssueValidator =
    List<ProfileIssueView> Function(String valuesJson, bool forTask);
typedef ProfileIssueText = String Function(String language, String code);

// Explicit adapters: `AppBridge.profileValidate` takes named arguments, so its
// tear-off is not assignable to the positional typedef above.
List<ProfileIssueView> _defaultValidateProfile(
  String valuesJson,
  bool forTask,
) => AppBridge.profileValidate(valuesJson: valuesJson, forTask: forTask);

/// Default formatter: classification, hiding and localization are shared in
/// Rust, so the Flutter layer never owns log rules.
FormattedLogView defaultLogFormatter(String language, String log) {
  final formatted = AppBridge.formatLog(language, log);
  return FormattedLogView(
    text: formatted.text,
    problemText: formatted.problemText,
    problems: formatted.problems.toInt(),
  );
}

enum _DiscardOutcome { deleted, nothing, failed }

class _DiscardResult {
  const _DiscardResult(this.outcome, [this.error = '']);

  final _DiscardOutcome outcome;
  final String error;
}

class AppController extends ChangeNotifier {
  AppController({
    required this.settings,
    required this.store,
    required this.platform,
    this.logFormatter = defaultLogFormatter,
    TaskBridgeRunner? bridgeRunProfile,
    ProfileIssueValidator? bridgeValidateProfile,
    ProfileIssueText? bridgeIssueMessage,
  }) : _bridgeRunProfile = bridgeRunProfile ?? AppBridge.runProfile,
       _bridgeValidateProfile =
           bridgeValidateProfile ?? _defaultValidateProfile,
       _bridgeIssueMessage =
           bridgeIssueMessage ?? AppBridge.profileIssueMessage {
    platform.onProgress = _handleIoProgress;
  }

  static bool _bridgeInitialized = false;

  final AppSettings settings;
  final SettingsStore store;
  final PlatformService platform;
  final LogFormatter logFormatter;
  final TaskBridgeRunner _bridgeRunProfile;
  final ProfileIssueValidator _bridgeValidateProfile;
  final ProfileIssueText _bridgeIssueMessage;

  static const String _safDescriptorPath = 'saf-descriptors';
  final List<SafDocumentCollection> _documentCollections = [];

  TaskPhase _phase = TaskPhase.idle;
  String _activeAction = '';
  int _done = 0;
  int _total = 0;
  String _progressText = '';
  String _logText = '';
  String _message = '';
  String _failureAction = '';
  List<AppDiagnostic> _diagnostics = const <AppDiagnostic>[];
  bool _cancelRequested = false;
  String? _initError;
  Timer? _pollTimer;
  Timer? _saveTimer;
  DateTime? _startedAt;
  int _elapsedSeconds = 0;
  bool _disposed = false;
  bool _runtimePathsReady = true;
  bool _deliveryCleanupPending = false;
  String _cleanupDetail = '';
  String _lastNotificationSignature = '';
  String _formattedLogSource = '';
  FormattedLogView _formattedLog = const FormattedLogView(
    text: '',
    problemText: '',
    problems: 0,
  );

  TaskPhase get phase => _phase;
  String get activeAction => _activeAction;
  int get done => _done;
  int get total => _total;
  String get progressText => _progressText;

  /// Bridge log lines rendered by the shared Rust formatter, so Android and
  /// Windows show identical text.
  String get logText {
    _ensureLogFormatted();
    return _formattedLog.text;
  }

  /// Only the visible problem lines, also classified in Rust.
  String get problemLogText {
    _ensureLogFormatted();
    return _formattedLog.problemText;
  }

  void _ensureLogFormatted() {
    if (_formattedLogSource == _logText) {
      return;
    }
    _formattedLogSource = _logText;
    _formattedLog = logFormatter(settings.language, _logText);
  }

  String get message => _message;
  List<AppDiagnostic> get diagnostics => _diagnostics;
  bool get cancelRequested => _cancelRequested;
  String? get initError => _initError;
  bool get isRunning => _phase == TaskPhase.running;
  bool get isCancelled => _phase == TaskPhase.cancelled;

  /// True when the SAF export succeeded but the shared finish (cleanup) failed.
  ///
  /// The exported product exists and the task workspace is retained, so the
  /// user can retry the cleanup without rebuilding or re-exporting.
  bool get deliveryCleanupPending => _deliveryCleanupPending;

  /// Native detail of the failed finish, kept for diagnostics.
  String get cleanupDetail => _cleanupDetail;
  int get elapsedSeconds => _elapsedSeconds;
  int get problemCount {
    _ensureLogFormatted();
    return _formattedLog.problems > 0
        ? _formattedLog.problems
        : _diagnostics.length;
  }

  AppStrings get strings => AppStrings.forCode(settings.language);

  double? get progressFraction {
    if (_total <= 1) {
      return null;
    }
    final value = _done / _total;
    if (value < 0) {
      return 0;
    }
    if (value > 1) {
      return 1;
    }
    return value;
  }

  /// Progress on the shared 0-100 scale, rendered as a percentage only.
  int? get progressPercent {
    if (_total <= 1) {
      return null;
    }
    final clamped = _done.clamp(0, _total);
    return (clamped * 100 / _total).round().clamp(0, 100);
  }

  String get statusText {
    if (_phase == TaskPhase.running) {
      return _cancelRequested ? strings.statusStopping : strings.statusRunning;
    }
    if (_phase == TaskPhase.success) {
      return strings.statusDone;
    }
    if (_phase == TaskPhase.failure) {
      if (_failureAction.startsWith('build')) {
        return strings.statusBuildFailed;
      }
      return strings.statusFailed;
    }
    if (_phase == TaskPhase.cancelled) {
      return strings.statusCancelled;
    }
    return strings.statusReady;
  }

  Future<void> initialize() async {
    if (!_bridgeInitialized) {
      try {
        await AppBridge.init();
        _bridgeInitialized = true;
      } catch (error) {
        _initError = error.toString();
      }
    }
    // The bundled menu font is copied from the APK asset into app-private
    // storage before any Rust task runs. A development build without the asset
    // reports a warning instead of blocking the rest of the app.
    try {
      final fonts = await platform.ensureRuntimeFonts();
      if (fonts != null && fonts['path'] is! String) {
        final reason = fonts['error'] ?? 'unknown error';
        _initError ??= 'Cannot install the bundled menu font: $reason';
      }
    } catch (error) {
      _initError ??= error.toString();
    }
    Map<String, String>? paths;
    try {
      paths = await platform.appPaths();
    } catch (error) {
      paths = null;
      _initError = error.toString();
    }
    final files = paths?['filesDir'];
    final build = paths?['buildDir'];
    if (files != null &&
        files.isNotEmpty &&
        build != null &&
        build.isNotEmpty) {
      _runtimePathsReady = true;
      settings.appFilesDir = files;
      settings.appWorkDir = build;
    } else {
      _runtimePathsReady = false;
      _initError ??= 'Cannot access the app-private working directories';
    }
    _normalizeFontPaths();
    // A failed delivery cleanup survives a process restart so the user can
    // retry the shared finish without rebuilding/exporting again.
    _deliveryCleanupPending = store.loadDeliveryCleanupPending();
    _cleanupDetail = _deliveryCleanupPending
        ? store.loadDeliveryCleanupDetail()
        : '';
    if (!_isCurrentPrivatePath(settings.sourceStagingPath)) {
      settings.sourceStagingPath = '';
    }
    if (!_isCurrentPrivatePath(settings.externalStagingPath)) {
      settings.externalStagingPath = '';
    }
    await persistNow();
    _notify();
  }

  void update(void Function() mutate) {
    if (isRunning) {
      return;
    }
    mutate();
    _schedulePersist();
    _notify();
  }

  void setLanguage(String code) {
    if (isRunning ||
        !settings.languages.contains(code) ||
        settings.language == code) {
      return;
    }
    settings.language = code;
    _schedulePersist();
    _notify();
  }

  void setTheme(String theme) {
    if (!const <String>{'system', 'light', 'dark'}.contains(theme)) {
      return;
    }
    settings.theme = theme;
    _schedulePersist();
    _notify();
  }

  Future<void> persistNow() async {
    _saveTimer?.cancel();
    _saveTimer = null;
    try {
      await store.save(settings);
    } catch (_) {
      // Persistence is best effort; the in-memory form stays authoritative.
    }
  }

  String buildProfileJson() => jsonEncode(settings.toProfileJson());

  String buildTaskProfileJson() {
    final profile = settings.toProfileJson();
    final values = Map<String, Object>.from(profile['Values']! as Map);
    values['DVDA_SRC'] = settings.sourceStagingPath == _safDescriptorPath
        ? settings.source
        : settings.sourceStagingPath;
    final documentMap = <String, String>{};
    for (final collection in _documentCollections) {
      documentMap.addAll(collection.toProfileMap());
    }
    if (documentMap.isNotEmpty) {
      values['DVDA_SOURCE_DOCUMENTS'] = jsonEncode(documentMap);
    }
    // The shared sequence publishes into a task-owned transport folder and the
    // SAF adapter exports it; the profile only carries the user-visible folder
    // so every shared log/diagnostic names the real destination.
    values['DVDA_FINAL_DIR'] = '';
    values['DVDA_FINAL_DISPLAY_DIR'] = settings.finalDir;
    // Session-only SAF display path: the Android bridge maps source mirror
    // paths in logs/diagnostics back to the user-visible folder. Windows has no
    // such key and its output is unchanged.
    values['DVDA_SRC_DISPLAY_DIR'] = settings.source;
    values['DVDA_BUILD_DIR'] = settings.effectiveWorkDir;
    values['DVDA_MLP_EXTERNAL_DIR'] =
        settings.externalStagingPath == _safDescriptorPath
        ? settings.externalDir
        : settings.externalStagingPath;
    profile['Values'] = values;
    return jsonEncode(profile);
  }

  /// Whether a previous task left resumable progress behind.
  ///
  /// The bridge reports the status payload inside the outcome message, so both
  /// the nested and the flat layout are accepted here.
  Future<bool> checkUnfinished() async {
    if (isRunning || !_runtimePathsReady) {
      return false;
    }
    await persistNow();
    try {
      final raw = await _bridgeRunProfile(buildTaskProfileJson(), 'status');
      return unfinishedFromStatus(raw);
    } catch (_) {
      return false;
    }
  }

  /// Public for tests; the UI only consumes [checkUnfinished].
  static bool unfinishedFromStatus(String raw) {
    Object? decoded;
    try {
      decoded = jsonDecode(raw);
    } catch (_) {
      return false;
    }
    if (decoded is! Map) {
      return false;
    }
    final message = decoded['Message'];
    if (message is String && message.trim().startsWith('{')) {
      try {
        final payload = jsonDecode(message);
        if (payload is Map && payload['Unfinished'] is bool) {
          return payload['Unfinished'] == true;
        }
      } catch (_) {
        // Fall through to the flat layout below.
      }
    }
    return decoded['Unfinished'] == true;
  }

  /// Deletes the unfinished task through the shared discard contract.
  ///
  /// Reads the shared discard result.
  ///
  /// The Windows/Android bridge wraps the action payload in the outer
  /// `{Ok, Message, ...}` envelope: the real `Discarded` flag lives inside the
  /// JSON `Message` string. `Discarded:false` is a valid "no progress to delete"
  /// result, not an error; only a failed bridge/discard keeps the pending retry
  /// state and aborts the merged cleanup.
  Future<_DiscardResult> _discardProgressResult() async {
    if (isRunning || !_runtimePathsReady) {
      return const _DiscardResult(
        _DiscardOutcome.failed,
        'A task is running or the app directories are unavailable',
      );
    }
    await persistNow();
    try {
      final raw = await _bridgeRunProfile(buildTaskProfileJson(), 'discard');
      final dynamic decoded = jsonDecode(raw);
      if (decoded is! Map || decoded['Ok'] != true) {
        return _DiscardResult(
          _DiscardOutcome.failed,
          _discardErrorMessage(decoded),
        );
      }
      bool? discarded;
      final topLevel = decoded['Discarded'];
      if (topLevel is bool) {
        discarded = topLevel;
      } else {
        final message = decoded['Message'];
        if (message is Map && message['Discarded'] is bool) {
          discarded = message['Discarded'] as bool;
        } else if (message is String) {
          try {
            final inner = jsonDecode(message);
            if (inner is Map && inner['Discarded'] is bool) {
              discarded = inner['Discarded'] as bool;
            }
          } on FormatException {
            // A malformed inner payload is a real failure below.
          }
        }
      }
      if (discarded == null) {
        return const _DiscardResult(
          _DiscardOutcome.failed,
          'Cannot read the shared discard result',
        );
      }
      if (!discarded) {
        return const _DiscardResult(_DiscardOutcome.nothing);
      }
      _phase = TaskPhase.idle;
      _activeAction = '';
      _failureAction = '';
      _deliveryCleanupPending = false;
      _cleanupDetail = '';
      await _saveDeliveryCleanup();
      _message = '';
      _logText = '';
      _progressText = '';
      _done = 0;
      _total = 0;
      _diagnostics = const <AppDiagnostic>[];
      _notify();
      return const _DiscardResult(_DiscardOutcome.deleted);
    } catch (error) {
      return _DiscardResult(_DiscardOutcome.failed, error.toString());
    }
  }

  String _discardErrorMessage(Object? decoded) {
    if (decoded is Map) {
      final message = decoded['Message'];
      if (message is String && message.trim().isNotEmpty) {
        return message;
      }
    }
    return 'Cannot delete the unfinished task';
  }

  /// True only when the shared action really removed a task directory.
  Future<bool> discardProgress() async =>
      (await _discardProgressResult()).outcome == _DiscardOutcome.deleted;

  Future<TaskOutcome?> runAction(String action) async {
    if (!action.startsWith('build') || isRunning) {
      return null;
    }
    _normalizeFontPaths();
    final validationError = _validateAction(action);
    if (validationError != null) {
      _phase = TaskPhase.failure;
      _message = validationError;
      _logText = validationError;
      _notify();
      return TaskOutcome.failure(validationError);
    }
    if (!_runtimePathsReady) {
      return _failBeforeTask(
        _initError ?? 'Cannot access the app-private working directories',
      );
    }
    // The shared menu renderer reads the bundled font from app-private storage;
    // install it from the APK asset before any Rust task starts (idempotent).
    try {
      await platform.ensureRuntimeFonts();
    } catch (_) {
      // The Rust task reports the shared font failure if the asset is missing.
    }
    // Claim the running state before the first await so a double tap cannot
    // start two concurrent native tasks. The SAF mirror for a newly picked
    // folder is built here, which can take a while for a large folder.
    _beginAction(action);
    await persistNow();
    if (_disposed) {
      return null;
    }
    try {
      if (action != 'status' && action != 'discard') {
        final sourceError = await _ensureSourceReady();
        if (sourceError != null) {
          return _failBeforeTask(sourceError);
        }
        if (settings.audioMode == 'external') {
          final externalError = await _ensureExternalReady();
          if (externalError != null) {
            return _failBeforeTask(externalError);
          }
        }
        if (_cancelRequested) {
          _applyOutcome(action, TaskOutcome.failure(''), cancelled: true);
          return null;
        }
      }
      if (_disposed) {
        return null;
      }
      _startPolling();

      TaskOutcome outcome;
      try {
        final raw = await _bridgeRunProfile(buildTaskProfileJson(), action);
        outcome = TaskOutcome.parse(raw);
      } catch (error) {
        outcome = TaskOutcome.failure(error.toString());
      }

      var cancelled = false;
      if (outcome.ok && action.startsWith('build')) {
        final delivery = outcome.deliveryDirectory;
        if (delivery != null) {
          // The shared sequence owns the final 100%; while the SAF adapter
          // exports, the bar shows the pending 99 and never 100.
          _stopPolling();
          _done = 99;
          _total = 100;
          _progressText = strings.exportingOutput;
          _notify();
          if (_cancelRequested) {
            cancelled = true;
            outcome = TaskOutcome.failure('');
          } else {
            final exported = await exportOutput(delivery);
            if (exported.cancelled) {
              cancelled = true;
              outcome = TaskOutcome.failure('');
            } else if (!exported.ok) {
              outcome = TaskOutcome.failure(strings.exportFailed);
            } else {
              // The export adapter released this task's SAF source mirrors; drop
              // their stale paths so the next task rebuilds them from the grant.
              settings.sourceStagingPath = '';
              settings.externalStagingPath = '';
              // Only a verified export may finish the shared task. A null, failed
              // or throwing finish is a cleanup failure: never success, keep the
              // resumable workspace and allow a retry.
              final finished = await finishDeliveredTask();
              if (finished == null || !finished.ok) {
                _deliveryCleanupPending = true;
                _cleanupDetail = finished?.message ?? '';
                await _saveDeliveryCleanup();
                outcome = TaskOutcome.failure(strings.cleanupFailedRetained);
              } else {
                _deliveryCleanupPending = false;
                _cleanupDetail = '';
                await _saveDeliveryCleanup();
                outcome = outcome.withProgress(finished.done, finished.total);
              }
            }
          }
        }
      }
      _stopPolling();
      // Only the shared authoritative cancellation flag (or an adapter-level
      // cancel that already aborted the platform I/O) may paint the outcome as
      // a user stop. A real failure that happened after a stop request stays a
      // failure and keeps its diagnostics.
      if (cancelled || outcome.cancelled) {
        _applyOutcome(action, outcome, cancelled: true);
      } else {
        _applyOutcome(action, outcome);
      }
      return outcome;
    } finally {
      await _closeDocumentCollections();
    }
  }

  Future<void> _closeDocumentCollections() async {
    final collections = List<SafDocumentCollection>.from(_documentCollections);
    _documentCollections.clear();
    for (final collection in collections) {
      await collection.close();
    }
  }

  void cancel() {
    if (!isRunning || _cancelRequested) {
      return;
    }
    _cancelRequested = true;
    try {
      AppBridge.cancel();
    } catch (_) {
      // The task will fail on its own if the bridge is unavailable.
    }
    // Also abort the SAF I/O adapters: they run before/around the Rust task and
    // otherwise keep copying a large ISO after the user pressed cancel.
    unawaited(_cancelPlatformIo());
    _notify();
  }

  Future<void> _cancelPlatformIo() async {
    try {
      await platform.cancelExport();
    } catch (_) {
      // The next adapter call reports a real failure.
    }
  }

  Future<void> cancelAndWait() async {
    cancel();
    while (isRunning && !_disposed) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
  }

  Future<DirectorySelection?> pickDirectory(
    String current, {
    bool linkForNative = true,
  }) => platform.pickDirectory(initial: current, linkForNative: linkForNative);

  Future<String?> pickFont() => platform.pickFont();

  Future<bool> openProfileFromDevice() async {
    final text = await platform.openProfile();
    if (text == null) {
      return false;
    }
    settings.applyProfileJson(text);
    await persistNow();
    _notify();
    return true;
  }

  Future<bool> saveProfileToDevice() async {
    await persistNow();
    return platform.saveProfile(
      buildProfileJson(),
      suggestedName: 'dvda-profile.json',
    );
  }

  /// Runs the transactional SAF export of the task-local product.
  ///
  /// The adapter writes and verifies temporary SAF documents and only then
  /// replaces the visible ISO; the app-private SAF mirrors consumed by this task
  /// are released by the adapter itself on success.
  Future<ExportResult> exportOutput(String sourcePath) async {
    if (settings.finalTreeUri.isEmpty || sourcePath.isEmpty) {
      return ExportResult.failure('NO_TARGET');
    }
    final cachePaths = <String>[
      if (settings.sourceStagingPath.isNotEmpty) settings.sourceStagingPath,
      if (settings.externalStagingPath.isNotEmpty) settings.externalStagingPath,
    ];
    try {
      return await platform.exportDirectory(
        sourcePath,
        settings.finalTreeUri,
        cachePaths: cachePaths,
      );
    } catch (error) {
      return ExportResult.failure('EXPORT_EXCEPTION: $error');
    }
  }

  void _normalizeFontPaths() {
    String normalize(String value, String fallback) {
      final font = AppSettings.migrateLegacyFont(value);
      if (font.startsWith('${settings.appFilesDir}/fonts/')) {
        return font;
      }
      return AppSettings.portableFont(font, fallback);
    }

    // Empty means "use the shared language default"; the desktop profile
    // keeps the same empty value, so profiles stay portable across platforms.
    settings.fontSc = normalize(settings.fontSc, '');
    settings.fontJp = normalize(settings.fontJp, '');
    settings.fontKr = normalize(settings.fontKr, '');
  }

  void _beginAction(String action) {
    _phase = TaskPhase.running;
    _activeAction = action;
    _cancelRequested = false;
    _failureAction = '';
    if (action.startsWith('build')) {
      // A new build owns a fresh workspace; an earlier cleanup failure must not
      // keep offering a retry for a workspace the shared sequence replaced.
      _deliveryCleanupPending = false;
      _cleanupDetail = '';
      unawaited(_saveDeliveryCleanup());
    }
    _done = 0;
    _total = 0;
    _progressText = '';
    _logText = '';
    _message = '';
    _diagnostics = const <AppDiagnostic>[];
    _startedAt = DateTime.now();
    _elapsedSeconds = 0;
    _notify();
  }

  TaskOutcome _failBeforeTask(String message) {
    _stopPolling();
    _startedAt = null;
    _cancelRequested = false;
    _phase = TaskPhase.failure;
    _message = message;
    _logText = message;
    _notify();
    return TaskOutcome.failure(message);
  }

  bool _isCurrentPrivatePath(String path) {
    if (path.isEmpty || path == _safDescriptorPath) {
      return true;
    }
    final root = settings.appFilesDir;
    return root.isNotEmpty &&
        path.startsWith('$root/saf-mirror/') &&
        !path.contains('/saf-links/');
  }

  Future<String?> _ensureSourceReady() => _ensureNativeInput(
    treeUri: settings.sourceTreeUri,
    nativePath: settings.sourceStagingPath,
    displayRoot: settings.source,
    missingMessage: strings.selectSourceRequired,
    force: settings.sourceMirrorDirty,
    apply: (path) => settings.sourceStagingPath = path,
    markClean: () => settings.sourceMirrorDirty = false,
  );

  Future<String?> _ensureExternalReady() => _ensureNativeInput(
    treeUri: settings.externalTreeUri,
    nativePath: settings.externalStagingPath,
    displayRoot: settings.externalDir,
    allowDescriptors: false,
    missingMessage: strings.selectExternalRequired,
    force: settings.externalMirrorDirty,
    apply: (path) => settings.externalStagingPath = path,
    markClean: () => settings.externalMirrorDirty = false,
  );

  /// Returns null when the app-private input for a SAF tree is ready, or the
  /// message that must be shown when it cannot be prepared.
  Future<String?> _ensureNativeInput({
    required String treeUri,
    required String nativePath,
    required String displayRoot,
    bool allowDescriptors = true,
    required String missingMessage,
    required bool force,
    required ValueChanged<String> apply,
    required VoidCallback markClean,
  }) async {
    if (treeUri.isEmpty) {
      if (nativePath.isEmpty) {
        return missingMessage;
      }
      if (!_isCurrentPrivatePath(nativePath)) {
        apply('');
        return missingMessage;
      }
      try {
        if (await platform.validatePrivateDirectory(nativePath)) {
          return null;
        }
      } catch (error) {
        apply('');
        return error.toString();
      }
      apply('');
      return missingMessage;
    }
    if (allowDescriptors && platform.supportsSafDescriptors) {
      try {
        final collection = await platform.openSourceDocuments(
          displayRoot,
          treeUri,
        );
        if (collection.documents.isNotEmpty) {
          _documentCollections.add(collection);
          apply(_safDescriptorPath);
          markClean();
          return null;
        }
        await collection.close();
      } catch (_) {
        // Descriptor-less providers use the bounded mirror fallback below.
      }
    }
    DirectorySelection? linked;
    try {
      linked = force
          ? await platform.linkDirectory(treeUri)
          : await platform.ensureSourceLink(treeUri);
    } on PlatformBridgeException catch (error) {
      apply('');
      markClean();
      if (error.code == 'SAF_NO_SPACE') {
        return strings.sourceSpaceInsufficient;
      }
      return error.message;
    } catch (error) {
      apply('');
      markClean();
      return error.toString();
    }
    final path = linked?.nativePath ?? '';
    markClean();
    if (path.isEmpty || !_isCurrentPrivatePath(path)) {
      apply('');
      return linked == null
          ? missingMessage
          : 'Cannot read the selected folder: ${linked.displayPath}';
    }
    if (nativePath != path) {
      apply(path);
      await persistNow();
    }
    return null;
  }

  /// Runs the shared finish tail after a verified SAF export.
  ///
  /// The shared sequence decides and performs the cleanup; this adapter only
  /// tells it that the user-visible delivery completed. Failure is reported, not
  /// swallowed: the caller shows a localized cleanup failure and keeps the
  /// resumable workspace so the finish can be retried.
  @protected
  Future<TaskOutcome?> finishDeliveredTask() async {
    if (!_runtimePathsReady) {
      return TaskOutcome.failure(
        'The app-private working directories are unavailable',
      );
    }
    try {
      return TaskOutcome.parse(
        await _bridgeRunProfile(buildTaskProfileJson(), 'finish_delivery'),
      );
    } catch (error) {
      return TaskOutcome.failure(error.toString());
    }
  }

  /// Retries the shared finish tail after a failed delivery cleanup.
  ///
  /// The exported product is already in the user folder, so only the workspace
  /// cleanup is repeated; success is only reported when the shared finish
  /// reports it.
  Future<TaskOutcome?> retryDeliveryCleanup() async {
    if (isRunning || !_deliveryCleanupPending || !_runtimePathsReady) {
      return null;
    }
    _phase = TaskPhase.running;
    _activeAction = 'finish_delivery';
    _cancelRequested = false;
    _done = 99;
    _total = 100;
    _progressText = strings.cleanupRetrying;
    _message = '';
    _notify();
    final finished = await finishDeliveredTask();
    _stopPolling();
    if (finished != null && finished.ok) {
      _deliveryCleanupPending = false;
      _cleanupDetail = '';
      await _saveDeliveryCleanup();
      final total = finished.total > 1 ? finished.total : 100;
      final done = finished.done > 0 ? finished.done : total;
      final outcome = TaskOutcome.success(
        strings.statusDone,
        done: done,
        total: total,
        log: _logText,
      );
      _applyOutcome('finish_delivery', outcome);
      _notify();
      return outcome;
    }
    _deliveryCleanupPending = true;
    _cleanupDetail = finished?.message ?? '';
    await _saveDeliveryCleanup();
    final failure = TaskOutcome.failure(strings.cleanupFailedRetained);
    _applyOutcome('finish_delivery', failure);
    _notify();
    return failure;
  }

  /// Removes known rebuildable app-private caches left by older versions.
  ///
  /// Explicit user-confirmed recovery only; the current task still uses the
  /// shared finish, and user-granted SAF folders, the unfinished workspace and
  /// exported ISOs are never deleted.
  Future<CleanupResult> cleanupInternalCaches() async {
    if (isRunning) {
      return const CleanupResult(ok: false, error: 'A task is running');
    }
    try {
      return await platform.cleanupInternalCaches();
    } catch (error) {
      return CleanupResult(ok: false, error: error.toString());
    }
  }

  /// Deletes the unfinished task through the shared discard contract and then
  /// releases the rebuildable app-private I/O caches through the existing
  /// Kotlin cache adapter.
  ///
  /// This does not introduce a second cleanup policy: the workspace is removed
  /// by the shared discard action and the adapters own the app-private mirrors.
  /// A running task or an unreachable app directory fails the whole operation.
  Future<CleanupResult> discardProgressAndCleanup() async {
    if (isRunning || !_runtimePathsReady) {
      return const CleanupResult(
        ok: false,
        error: 'A task is running or the app directories are unavailable',
      );
    }
    final discard = await _discardProgressResult();
    if (discard.outcome == _DiscardOutcome.failed) {
      // A real discard failure keeps the pending retry state and aborts the
      // merged cleanup; it must never look like a successful cleanup.
      return CleanupResult(ok: false, error: discard.error);
    }
    final cleanup = await cleanupInternalCaches();
    if (!cleanup.ok) {
      return CleanupResult(
        ok: false,
        freedBytes: cleanup.freedBytes,
        deleted: cleanup.deleted,
        error: cleanup.error,
      );
    }
    // No progress to delete is a valid outcome: the cache cleanup still runs.
    final progressDiscarded = discard.outcome == _DiscardOutcome.deleted;
    if (!progressDiscarded) {
      // No workspace exists, so an earlier finish-cleanup retry flag is stale.
      _deliveryCleanupPending = false;
      _cleanupDetail = '';
      await _saveDeliveryCleanup();
      _notify();
    }
    return CleanupResult(
      ok: true,
      freedBytes: cleanup.freedBytes,
      deleted: cleanup.deleted,
      progressDiscarded: progressDiscarded,
    );
  }

  Future<bool> openOutputFolder() {
    if (settings.finalTreeUri.isEmpty) {
      return Future<bool>.value(false);
    }
    return platform.openOutput(settings.finalTreeUri);
  }

  Future<bool> exportLog() {
    final stamp = DateTime.now().toIso8601String().replaceAll(':', '-');
    return platform.saveTextFile(
      // Match Windows: export the complete task log, not the filtered widget
      // text. The visible log panel still uses the shared readable formatter.
      _logText,
      suggestedName: 'dvda-log-$stamp.txt',
      mimeType: 'text/plain',
    );
  }

  String? _validateAction(String action) {
    if (action == 'status' || action == 'discard') {
      return null;
    }
    try {
      final issues = _bridgeValidateProfile(
        jsonEncode(settings.toProfileValues()),
        true,
      );
      if (issues.isNotEmpty) {
        final issue = issues.first;
        try {
          return _bridgeIssueMessage(settings.language, issue.code);
        } catch (_) {
          // The bridge answered validation, so the issue code is still usable
          // even if the message lookup is unavailable.
          return issue.code;
        }
      }
    } catch (_) {
      // Bridge unavailable: the run itself reports the failure.
    }
    if (settings.sourceStagingPath.isEmpty && settings.sourceTreeUri.isEmpty) {
      return strings.selectSourceRequired;
    }
    if (settings.finalTreeUri.isEmpty) {
      return strings.selectFinalRequired;
    }
    if (settings.audioMode == 'external' &&
        settings.externalStagingPath.isEmpty &&
        settings.externalTreeUri.isEmpty) {
      return strings.selectExternalRequired;
    }
    return null;
  }

  /// Returns the real problem text for a failed action, rather than the generic
  /// shared "failed" status. The shared Rust log formatter owns problem
  /// classification, so the dialog and task page use identical wording.
  String failureDetail(TaskOutcome outcome) {
    final problems = outcome.log.trim().isEmpty
        ? ''
        : logFormatter(settings.language, outcome.log).problemText.trim();
    if (problems.isNotEmpty) {
      return problems;
    }
    final diagnostics = outcome.diagnostics
        .where((issue) => issue.message.trim().isNotEmpty)
        .map((issue) => issue.message.trim())
        .join('\n');
    if (diagnostics.isNotEmpty) {
      return diagnostics;
    }
    return outcome.message.trim().isNotEmpty
        ? outcome.message.trim()
        : statusText;
  }

  void _applyOutcome(
    String action,
    TaskOutcome outcome, {
    bool cancelled = false,
  }) {
    final userCancelled = cancelled || outcome.cancelled;
    _phase = userCancelled
        ? TaskPhase.cancelled
        : outcome.ok
        ? TaskPhase.success
        : TaskPhase.failure;
    if (_phase == TaskPhase.failure) {
      _failureAction = action;
    } else {
      _failureAction = '';
    }
    _activeAction = action;
    if (_startedAt != null) {
      _elapsedSeconds = DateTime.now().difference(_startedAt!).inSeconds;
      _startedAt = null;
    }
    _cancelRequested = false;
    _message = outcome.message;
    if (_phase == TaskPhase.success) {
      _progressText = '';
    }
    _diagnostics = outcome.diagnostics;
    if (outcome.done > 0) {
      _done = outcome.done;
    }
    if (outcome.total > 1) {
      _total = outcome.total;
    }
    if (outcome.log.isNotEmpty) {
      _logText = outcome.log;
    }
    _notify();
  }

  void _startPolling() {
    _pollTimer?.cancel();
    _pollTimer = Timer.periodic(const Duration(milliseconds: 150), (_) {
      if (_disposed || _phase != TaskPhase.running) {
        return;
      }
      try {
        final progress = AppBridge.progress();
        final elapsed = _startedAt == null
            ? _elapsedSeconds
            : DateTime.now().difference(_startedAt!).inSeconds;
        final progressText = _formatSingleLine(progress.text);
        final progressLog = progress.log.isEmpty ? _logText : progress.log;
        final changed =
            progress.done != _done ||
            progress.total != _total ||
            progressText != _progressText ||
            progressLog != _logText ||
            elapsed != _elapsedSeconds;
        if (progress.done >= _done) {
          _done = progress.done;
        }
        _total = progress.total;
        _progressText = progressText;
        _logText = progressLog;
        _elapsedSeconds = elapsed;
        if (changed) {
          _notify();
        }
      } catch (_) {
        // Keep polling; the bridge may still be initializing.
      }
    });
  }

  void _stopPolling() {
    _pollTimer?.cancel();
    _pollTimer = null;
  }

  void _schedulePersist() {
    _saveTimer?.cancel();
    _saveTimer = Timer(const Duration(milliseconds: 450), () {
      persistNow();
    });
  }

  Future<void> _saveDeliveryCleanup() async {
    try {
      await store.saveDeliveryCleanupPending(
        _deliveryCleanupPending,
        detail: _cleanupDetail,
      );
    } catch (_) {
      // Persistence is best effort; the in-memory state still drives the UI.
    }
  }

  void _notify() {
    if (!_disposed) {
      _syncStatusNotification();
      notifyListeners();
    }
  }

  /// Short human-readable status for the notification and Live Update.
  ///
  /// The bridge progress text is a JSON event line, which must never leak into
  /// the system UI.
  String _notificationProgressText() {
    final label = switch (_activeAction) {
      _ => strings.actionBuild,
    };
    final percent = progressPercent;
    if (percent != null) {
      return '$label $percent%';
    }
    return label;
  }

  /// Drops raw wire payloads; only readable text may reach the progress label.
  @visibleForTesting
  static String sanitizeProgressText(String text) {
    final trimmed = text.trim();
    if (trimmed.isEmpty) {
      return '';
    }
    if (trimmed.startsWith('{') || trimmed.startsWith('[TASK]')) {
      return '';
    }
    return trimmed;
  }

  /// Progress event from the transactional SAF export adapter.
  ///
  /// The shared Rust progress still owns the final 100 after finish reports it.
  void _handleIoProgress(TaskIoProgress progress) {
    if (!isRunning) {
      return;
    }
    final text = strings.exportProgress(progress.done, progress.total);
    if (text == _progressText) {
      return;
    }
    _progressText = text;
    _notify();
  }

  /// Formats one bridge progress line with the same shared formatter.
  String _formatSingleLine(String raw) {
    if (raw.trim().isEmpty) {
      return '';
    }
    return sanitizeProgressText(logFormatter(settings.language, raw).text);
  }

  /// Test hook: seeds progress counters without a running Rust task.
  @visibleForTesting
  void applyProgressForTest({
    required int done,
    required int total,
    String text = '',
  }) {
    _done = done;
    _total = total;
    _progressText = text;
  }

  void _syncStatusNotification() {
    final signature = [
      _phase.name,
      _activeAction,
      _done,
      _total,
      _progressText,
      _message,
      _cancelRequested,
    ].join('|');
    if (signature == _lastNotificationSignature) {
      return;
    }
    _lastNotificationSignature = signature;
    if (_phase == TaskPhase.idle) {
      unawaited(platform.clearStatusNotification().onError((_, _) {}));
      return;
    }
    final running = _phase == TaskPhase.running;
    final text = running
        ? _notificationProgressText()
        : (_message.isNotEmpty ? _message : statusText);
    final progress = running
        ? progressFraction
        : (_phase == TaskPhase.success ? 1.0 : null);
    unawaited(
      platform
          .updateStatusNotification(
            title: 'DVD-Audio Maker',
            text: text,
            progress: progress,
            ongoing: running,
          )
          .onError((_, _) {}),
    );
  }

  @override
  void dispose() {
    _disposed = true;
    platform.onProgress = null;
    _stopPolling();
    _saveTimer?.cancel();
    _saveTimer = null;
    super.dispose();
  }
}
