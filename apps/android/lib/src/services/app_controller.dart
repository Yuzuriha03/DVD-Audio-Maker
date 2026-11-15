import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../../bridge.dart';
import '../localization/app_strings.dart';
import '../models/app_settings.dart';
import 'platform_service.dart';
import 'settings_store.dart';

/// Lifecycle of one `AppBridge.runProfile` invocation.
enum TaskPhase { idle, running, success, failure, cancelled }

/// One diagnostic row returned by the Rust application layer.
class AppDiagnostic {
  const AppDiagnostic({required this.code, required this.message});

  final String code;
  final String message;
}

/// Parsed outcome of a profile action.
class TaskOutcome {
  const TaskOutcome({
    required this.ok,
    required this.message,
    required this.diagnostics,
    required this.log,
    required this.done,
    required this.total,
    required this.raw,
  });

  final bool ok;
  final String message;
  final List<AppDiagnostic> diagnostics;
  final String log;
  final int done;
  final int total;
  final Map<String, dynamic> raw;

  bool get unfinished => raw['Unfinished'] == true;

  factory TaskOutcome.fromJson(Map<String, dynamic> json) {
    return TaskOutcome(
      ok: json['Ok'] == true,
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
class AppController extends ChangeNotifier {
  AppController({
    required this.settings,
    required this.store,
    required this.platform,
  });

  static bool _bridgeInitialized = false;

  final AppSettings settings;
  final SettingsStore store;
  final PlatformService platform;

  TaskPhase _phase = TaskPhase.idle;
  String _activeAction = '';
  int _done = 0;
  int _total = 0;
  String _progressText = '';
  String _logText = '';
  String _message = '';
  List<AppDiagnostic> _diagnostics = const <AppDiagnostic>[];
  bool _cancelRequested = false;
  String? _initError;
  Timer? _pollTimer;
  Timer? _saveTimer;
  DateTime? _startedAt;
  int _elapsedSeconds = 0;
  bool _disposed = false;
  bool _runtimePathsReady = true;
  String _lastNotificationSignature = '';
  String _formattedLogSource = '';
  String _formattedLog = '';

  TaskPhase get phase => _phase;
  String get activeAction => _activeAction;
  int get done => _done;
  int get total => _total;
  String get progressText => _progressText;

  /// Bridge log lines in the same readable form the Windows client shows:
  /// structured [Kind] events are localized and [METRIC] diagnostics hidden.
  String get logText {
    if (_formattedLogSource != _logText) {
      _formattedLogSource = _logText;
      _formattedLog = formatLogForDisplay(_logText);
    }
    return _formattedLog;
  }

  String get message => _message;
  List<AppDiagnostic> get diagnostics => _diagnostics;
  bool get cancelRequested => _cancelRequested;
  String? get initError => _initError;
  bool get isRunning => _phase == TaskPhase.running;
  bool get isCancelled => _phase == TaskPhase.cancelled;
  int get elapsedSeconds => _elapsedSeconds;
  int get problemCount {
    final logged = _logText.split('\n').where((line) {
      final lower = line.toLowerCase();
      return lower.contains('[error]') ||
          lower.contains('[warn]') ||
          line.contains('[错误]') ||
          line.contains('[警告]');
    }).length;
    return logged > 0 ? logged : _diagnostics.length;
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

  String get statusText {
    if (_phase == TaskPhase.running) {
      return _cancelRequested ? strings.statusStopping : strings.statusRunning;
    }
    if (_phase == TaskPhase.success) {
      return strings.statusDone;
    }
    if (_phase == TaskPhase.failure) {
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
    Map<String, String>? paths;
    try {
      paths = await platform.appPaths();
    } catch (error) {
      paths = null;
      _initError = error.toString();
    }
    final files = paths?['filesDir'];
    final build = paths?['buildDir'];
    final output = paths?['outputDir'];
    if (files != null &&
        files.isNotEmpty &&
        build != null &&
        build.isNotEmpty &&
        output != null &&
        output.isNotEmpty) {
      _runtimePathsReady = true;
      settings.appFilesDir = files;
      settings.appWorkDir = build;
      settings.appOutputDir = output;
    } else {
      _runtimePathsReady = false;
      _initError ??= 'Cannot access the app-private working directories';
    }
    _normalizeFontPaths();
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
    if (isRunning || !kLanguages.contains(code) || settings.language == code) {
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
    final values = Map<String, String>.from(profile['Values']! as Map);
    values['DVDA_SRC'] = settings.sourceStagingPath;
    values['DVDA_FINAL_DIR'] = settings.appOutputDir;
    values['DVDA_BUILD_DIR'] = settings.effectiveWorkDir;
    values['DVDA_MLP_EXTERNAL_DIR'] = settings.externalStagingPath;
    profile['Values'] = values;
    return jsonEncode(profile);
  }

  Future<bool> checkUnfinished() async {
    if (isRunning || !_runtimePathsReady) {
      return false;
    }
    await persistNow();
    try {
      final raw = await AppBridge.runProfile(buildTaskProfileJson(), 'status');
      final dynamic decoded = jsonDecode(raw);
      return decoded is Map && decoded['Unfinished'] == true;
    } catch (_) {
      return false;
    }
  }

  Future<bool> discardProgress() async {
    if (isRunning || !_runtimePathsReady) {
      return false;
    }
    await persistNow();
    try {
      final raw = await AppBridge.runProfile(buildTaskProfileJson(), 'discard');
      final dynamic decoded = jsonDecode(raw);
      return decoded is Map && decoded['Ok'] == true;
    } catch (_) {
      return false;
    }
  }

  Future<TaskOutcome?> runAction(String action) async {
    if (isRunning) {
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
    // Claim the running state before the first await so a double tap cannot
    // start two concurrent native tasks. The SAF mirror for a newly picked
    // folder is built here, which can take a while for a large folder.
    _beginAction(action);
    await persistNow();
    if (_disposed) {
      return null;
    }
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
      final fontError = await _fontPathError();
      if (fontError != null) {
        return _failBeforeTask(fontError);
      }
    }
    if (_disposed) {
      return null;
    }
    _startPolling();

    TaskOutcome outcome;
    try {
      final raw = await AppBridge.runProfile(buildTaskProfileJson(), action);
      outcome = TaskOutcome.parse(raw);
    } catch (error) {
      outcome = TaskOutcome.failure(error.toString());
    }

    if (outcome.ok && action.startsWith('build')) {
      final exported = await exportOutput();
      if (!exported) {
        outcome = TaskOutcome.failure(strings.exportFailed);
      }
    }
    _stopPolling();
    final cancelled = _cancelRequested && !outcome.ok;
    _applyOutcome(action, outcome, cancelled: cancelled);
    return outcome;
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
    _notify();
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

  Future<bool> exportOutput() {
    if (settings.finalTreeUri.isEmpty) {
      return Future<bool>.value(false);
    }
    return platform.exportDirectory(
      settings.appOutputDir,
      settings.finalTreeUri,
    );
  }

  void _normalizeFontPaths() {
    String normalize(String value, String fallback) {
      final font = value.trim();
      if (font.startsWith('${settings.appFilesDir}/fonts/')) {
        return font;
      }
      return AppSettings.portableFont(font, fallback);
    }

    settings.fontSc = normalize(settings.fontSc, kDefaultFont);
    settings.fontJp = normalize(settings.fontJp, '');
    settings.fontKr = normalize(settings.fontKr, '');
  }

  Future<String?> _fontPathError() async {
    if (!settings.menu) {
      return null;
    }
    final paths = <String>[
      settings.fontSc.trim().isEmpty ? kDefaultFont : settings.fontSc.trim(),
      settings.fontJp.trim(),
      settings.fontKr.trim(),
    ];
    for (final path in paths.where((path) => path.isNotEmpty)) {
      try {
        if (!await platform.validateReadableFont(path)) {
          return 'Cannot read menu font: $path';
        }
      } catch (_) {
        return 'Cannot read menu font: $path';
      }
    }
    return null;
  }

  void _beginAction(String action) {
    _phase = TaskPhase.running;
    _activeAction = action;
    _cancelRequested = false;
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
    if (path.isEmpty) {
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
    missingMessage: strings.selectSourceRequired,
    force: settings.sourceMirrorDirty,
    apply: (path) => settings.sourceStagingPath = path,
    markClean: () => settings.sourceMirrorDirty = false,
  );

  Future<String?> _ensureExternalReady() => _ensureNativeInput(
    treeUri: settings.externalTreeUri,
    nativePath: settings.externalStagingPath,
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
    DirectorySelection? linked;
    try {
      linked = force
          ? await platform.linkDirectory(treeUri)
          : await platform.ensureSourceLink(treeUri);
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

  Future<bool> openOutputFolder() {
    if (settings.finalTreeUri.isEmpty) {
      return Future<bool>.value(false);
    }
    return platform.openOutput(settings.finalTreeUri);
  }

  Future<bool> exportLog() {
    final stamp = DateTime.now().toIso8601String().replaceAll(':', '-');
    return platform.saveTextFile(
      logText,
      suggestedName: 'dvda-log-$stamp.txt',
      mimeType: 'text/plain',
    );
  }

  String? _validateAction(String action) {
    if (action == 'status' || action == 'discard') {
      return null;
    }
    if (settings.sourceStagingPath.isEmpty && settings.sourceTreeUri.isEmpty) {
      return strings.selectSourceRequired;
    }
    if (action != 'prepare' && settings.finalTreeUri.isEmpty) {
      return strings.selectFinalRequired;
    }
    if (settings.audioMode == 'external' &&
        settings.externalStagingPath.isEmpty &&
        settings.externalTreeUri.isEmpty) {
      return strings.selectExternalRequired;
    }
    return null;
  }

  void _applyOutcome(
    String action,
    TaskOutcome outcome, {
    bool cancelled = false,
  }) {
    _phase = cancelled
        ? TaskPhase.cancelled
        : outcome.ok
        ? TaskPhase.success
        : TaskPhase.failure;
    _activeAction = action;
    if (_startedAt != null) {
      _elapsedSeconds = DateTime.now().difference(_startedAt!).inSeconds;
      _startedAt = null;
    }
    _cancelRequested = false;
    _message = outcome.message;
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
        final progressText = _displayLine(progress.text);
        final changed =
            progress.done != _done ||
            progress.total != _total ||
            progressText != _progressText ||
            progress.log != _logText ||
            elapsed != _elapsedSeconds;
        if (progress.done >= _done) {
          _done = progress.done;
        }
        _total = progress.total;
        _progressText = progressText;
        _logText = progress.log;
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
      'prepare' => strings.actionPrepare,
      'verify' => strings.actionVerify,
      _ => strings.actionBuild,
    };
    if (_total > 0) {
      final done = _done.clamp(0, _total);
      return '$label $done/$_total';
    }
    return label;
  }

  /// Converts one bridge log line into the readable text the Windows client
  /// shows. Returns an empty string for lines that must stay hidden.
  String _displayLine(String raw) {
    final line = raw.trim();
    if (line.isEmpty || line.startsWith('[METRIC] ')) {
      return '';
    }
    if (!line.startsWith('{')) {
      return line;
    }
    Object? decoded;
    try {
      decoded = jsonDecode(line);
    } catch (_) {
      return line;
    }
    if (decoded is! Map) {
      return line;
    }
    final kind = decoded['Kind'];
    final name = decoded['Name'];
    if (kind is! String || name is! String) {
      return line;
    }
    final detail = decoded['Value'];
    String fill(String template, List<String> values) {
      var result = template;
      for (var index = 0; index < values.length; index++) {
        result = result.replaceAll('{${index + 1}}', values[index]);
      }
      return result.replaceAll('{0}', name);
    }

    switch (kind) {
      case 'Started':
        return fill(strings.logStarted, const <String>[]);
      case 'PcmStarted':
        return fill(strings.logPcmStarted, const <String>[]);
      case 'PcmProgress':
        final percent = detail is num ? detail.clamp(0, 100).toString() : '?';
        return fill(strings.logPcmProgress, <String>[percent]);
      case 'PcmFinished':
        return fill(strings.logPcmFinished, const <String>[]);
      case 'Encoded':
        final values = <String>['?', '?', '?', '?'];
        if (detail is Map) {
          values[0] = '${detail['Rate'] ?? '?'}';
          values[1] = '${detail['Bits'] ?? '?'}';
          values[2] = '${detail['Channels'] ?? '?'}';
          values[3] = '${detail['Bytes'] ?? '?'}';
        }
        return fill(strings.logEncoded, values);
      case 'PcmError':
        return fill(strings.logPcmError, <String>[
          detail is String && detail.isNotEmpty ? detail : '?',
        ]);
      case 'Detail':
        return detail is String && detail.isNotEmpty ? '$name: $detail' : name;
      default:
        return line;
    }
  }

  /// Public for tests; the UI reads it through [logText].
  String formatLogForDisplay(String raw) {
    if (raw.isEmpty) {
      return '';
    }
    final lines = <String>[];
    for (final line in raw.split('\n')) {
      final display = _displayLine(line);
      if (display.isNotEmpty) {
        lines.add(display);
      }
    }
    return lines.join('\n');
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
    _stopPolling();
    _saveTimer?.cancel();
    _saveTimer = null;
    super.dispose();
  }
}
