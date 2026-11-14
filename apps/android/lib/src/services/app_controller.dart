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

  TaskPhase get phase => _phase;
  String get activeAction => _activeAction;
  int get done => _done;
  int get total => _total;
  String get progressText => _progressText;
  String get logText => _logText;
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
    try {
      await AppBridge.init();
    } catch (error) {
      _initError = error.toString();
    }
    if (settings.sourceStagingPath.contains('/saf-links/')) {
      settings.sourceStagingPath = '';
    }
    if (settings.externalStagingPath.contains('/saf-links/')) {
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
    values['DVDA_FINAL_DIR'] = kDefaultTaskOutputDir;
    values['DVDA_BUILD_DIR'] = kDefaultWorkDir;
    values['DVDA_MLP_EXTERNAL_DIR'] = settings.externalStagingPath;
    profile['Values'] = values;
    return jsonEncode(profile);
  }

  Future<bool> checkUnfinished() async {
    if (isRunning) {
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
    if (isRunning) {
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
    final validationError = _validateAction(action);
    if (validationError != null) {
      _phase = TaskPhase.failure;
      _message = validationError;
      _logText = validationError;
      _notify();
      return TaskOutcome.failure(validationError);
    }
    // Claim the running state before the first await so a double tap cannot
    // start two concurrent native tasks.
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
    await persistNow();
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
      kDefaultTaskOutputDir,
      settings.finalTreeUri,
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
      _logText,
      suggestedName: 'dvda-log-$stamp.txt',
      mimeType: 'text/plain',
    );
  }

  String? _validateAction(String action) {
    if (action == 'status' || action == 'discard') {
      return null;
    }
    if (settings.sourceStagingPath.isEmpty) {
      return strings.selectSourceRequired;
    }
    if (action != 'prepare' && settings.finalTreeUri.isEmpty) {
      return strings.selectFinalRequired;
    }
    if (settings.audioMode == 'external' &&
        settings.externalStagingPath.isEmpty) {
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
        final changed =
            progress.done != _done ||
            progress.total != _total ||
            progress.text != _progressText ||
            progress.log != _logText ||
            elapsed != _elapsedSeconds;
        if (progress.done >= _done) {
          _done = progress.done;
        }
        _total = progress.total;
        _progressText = progress.text;
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
      notifyListeners();
    }
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
