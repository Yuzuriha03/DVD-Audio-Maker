import 'dart:convert';

import 'package:dvda_maker/bridge.dart';
import 'package:dvda_maker/src/models/app_settings.dart';
import 'package:dvda_maker/src/models/profile_catalog.dart';
import 'package:dvda_maker/src/services/app_controller.dart';
import 'package:dvda_maker/src/services/platform_service.dart';
import 'package:dvda_maker/src/services/settings_store.dart';
import 'package:dvda_maker/src/services/saf_documents.dart';
import 'package:dvda_maker/src/ui/home_screen.dart';
import 'package:dvda_maker/src/ui/sections/task_section.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:saf/saf.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'profile_catalog_fixture.dart';

const String _filesRoot = '/data/user/0/com.dvda.maker/files';
const String _sourceMirror =
    '/data/user/0/com.dvda.maker/files/saf-mirror/source';
const String _delivery =
    '/data/user/0/com.dvda.maker/files/build/unfinished-task/deliver';
const String _externalMirror =
    '/data/user/0/com.dvda.maker/files/saf-mirror/external';

/// Platform double: only the SAF I/O adapter is simulated, no widget tree.
class _FakePlatform extends PlatformService {
  @override
  bool get supportsSafDescriptors => false;

  Map<String, String>? paths = <String, String>{
    'filesDir': _filesRoot,
    'buildDir': '/data/user/0/com.dvda.maker/files/build',
  };

  ExportResult exportResult = const ExportResult(ok: true, cancelled: false);
  bool exportCalled = false;
  bool throwOnExport = false;
  List<String> exportCachePaths = const <String>[];
  String? savedText;
  void Function()? onExport;

  PlatformBridgeException? sourceLinkError;
  int sourceLinkCalls = 0;

  CleanupResult cleanupResult = const CleanupResult(
    ok: true,
    freedBytes: 2048,
    deleted: <String>['saf-mirror'],
  );
  int cleanupCalls = 0;

  @override
  Future<Map<String, String>?> appPaths() async => paths;

  @override
  Future<Map<String, Object?>?> ensureRuntimeFonts() async => <String, Object?>{
    'path':
        '/data/user/0/com.dvda.maker/files/runtime/fonts/DvdaNotoCJK-Regular.ttc',
    'installed': false,
  };

  @override
  Future<bool> validatePrivateDirectory(String path) async => true;

  @override
  Future<DirectorySelection?> ensureSourceLink(String treeUri) async {
    sourceLinkCalls++;
    if (sourceLinkError != null) {
      throw sourceLinkError!;
    }
    final external = treeUri.contains('external');
    return DirectorySelection(
      displayPath: external ? '/sdcard/External' : '/sdcard/Music',
      nativePath: external ? _externalMirror : _sourceMirror,
      treeUri: treeUri,
    );
  }

  @override
  Future<ExportResult> exportDirectory(
    String sourcePath,
    String treeUri, {
    List<String> cachePaths = const <String>[],
  }) async {
    exportCalled = true;
    exportCachePaths = cachePaths;
    onExport?.call();
    if (throwOnExport) {
      throw PlatformBridgeException('provider failed', code: 'SAF_MIRROR');
    }
    return exportResult;
  }

  @override
  Future<bool> saveTextFile(
    String content, {
    String suggestedName = 'dvda-log.txt',
    String mimeType = 'text/plain',
  }) async {
    savedText = content;
    return true;
  }

  @override
  Future<void> cancelExport() async {}

  @override
  Future<CleanupResult> cleanupInternalCaches() async {
    cleanupCalls++;
    return cleanupResult;
  }

  @override
  Future<void> updateStatusNotification({
    required String title,
    required String text,
    double? progress,
    required bool ongoing,
  }) async {}

  @override
  Future<void> clearStatusNotification() async {}
}

class _DescriptorPlatform extends _FakePlatform {
  @override
  bool get supportsSafDescriptors => true;

  @override
  Future<SafDocumentCollection> openSourceDocuments(
    String root,
    String treeUri,
  ) async {
    return SafDocumentCollection(
      <SafNativeDocument>[
        SafNativeDocument(
          uri: 'content://source/01.flac',
          relativePath: 'Album/01.flac',
          logicalPath: '$root/Album/01.flac',
          nativePath: '/proc/self/fd/101',
          size: 10,
          lastModified: DateTime.fromMillisecondsSinceEpoch(0),
        ),
      ],
      const <SafOpenFd>[],
      (_) async {},
    );
  }
}

class _Bridge {
  final List<String> calls = <String>[];
  bool finishOk = true;
  bool finishThrows = false;
  String finishMessage = '';
  bool buildOk = true;
  String buildMessage = '';
  String buildLog = '';
  bool buildCancelled = false;
  bool discardOk = true;
  bool discardDiscarded = true;
  bool discardMalformed = false;
  String discardMessage = '';
  bool statusUnfinished = false;
  final Map<String, String> lastProfile = <String, String>{};

  Future<String> run(String profileJson, String action) async {
    calls.add(action);
    lastProfile[action] = profileJson;
    if (action.startsWith('build')) {
      // The task workspace exists while the build/delivery is in flight. A
      // failed/cancelled build keeps it as resumable progress.
      statusUnfinished = true;
      if (!buildOk) {
        return jsonEncode(<String, Object?>{
          'Ok': false,
          'Cancelled': buildCancelled,
          'Message': buildMessage,
          'Log': buildLog,
          'ProgressDone': 15,
          'ProgressTotal': 100,
        });
      }
      return jsonEncode(<String, Object?>{
        'Ok': true,
        'Cancelled': false,
        'Message': 'done',
        'Log': buildLog,
        'ProgressDone': 99,
        'ProgressTotal': 100,
        'FinalDirectory': _delivery,
      });
    }
    if (action == 'finish_delivery') {
      if (finishThrows) {
        throw StateError('finish io error');
      }
      // The delivered workspace exists until the shared finish succeeds.
      statusUnfinished = !finishOk;
      return jsonEncode(<String, Object?>{
        'Ok': finishOk,
        'Message': finishMessage,
        'Log': '',
        'ProgressDone': finishOk ? 100 : 99,
        'ProgressTotal': 100,
      });
    }
    if (action == 'discard') {
      // Real bridge shape: the action result is a JSON string nested in the
      // outer Message field; the top level has no Discarded key.
      return jsonEncode(<String, Object?>{
        'Ok': discardOk,
        'Message': discardOk
            ? (discardMalformed
                  ? 'not-json'
                  : jsonEncode(<String, Object?>{
                      'Ok': true,
                      'Discarded': discardDiscarded,
                    }))
            : (discardMessage.isEmpty ? 'delete failed' : discardMessage),
        'Log': '',
      });
    }
    if (action == 'status') {
      return jsonEncode(<String, Object?>{
        'Ok': true,
        'Message': jsonEncode(<String, Object?>{
          'Ok': true,
          'Unfinished': statusUnfinished,
        }),
      });
    }
    return jsonEncode(<String, Object?>{
      'Ok': true,
      'Message': 'ok',
      'Log': '',
      'ProgressDone': 100,
      'ProgressTotal': 100,
    });
  }
}

Future<AppController> _controller(
  _FakePlatform platform,
  _Bridge bridge, {
  LogFormatter? logFormatter,
  SettingsStore? store,
  bool resetPreferences = true,
}) async {
  if (resetPreferences) {
    SharedPreferences.setMockInitialValues(<String, Object>{});
  }
  final settings = AppSettings();
  settings.language = 'en';
  settings.source = '/sdcard/Music';
  settings.sourceStagingPath = _sourceMirror;
  settings.sourceTreeUri = 'content://tree/source';
  settings.finalDir = '/sdcard/Out';
  settings.finalTreeUri = 'content://tree/output';
  return AppController(
    settings: settings,
    store: store ?? await SettingsStore.open(),
    platform: platform,
    bridgeRunProfile: bridge.run,
    bridgeValidateProfile: (valuesJson, forTask) => const <ProfileIssueView>[],
    bridgeIssueMessage: (language, code) => code,
    logFormatter: logFormatter ?? defaultLogFormatter,
  );
}

Finder _inDialog(String text) =>
    find.descendant(of: find.byType(AlertDialog), matching: find.text(text));

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() => ProfileCatalog.install(testCatalog()));

  test('successful export finishes the shared task at 100', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge();
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    platform.onExport = () {
      // The SAF export is pending: the bar stays at 99 until Rust finishes.
      expect(controller.progressPercent, 99);
      expect(controller.phase, TaskPhase.running);
    };

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isTrue);
    expect(controller.phase, TaskPhase.success);
    expect(controller.progressPercent, 100);
    expect(platform.exportCalled, isTrue);
    expect(platform.exportCachePaths, <String>[_sourceMirror]);
    expect(bridge.calls, contains('finish_delivery'));
    expect(controller.deliveryCleanupPending, isFalse);
    expect(controller.progressText, isEmpty);
  });

  test(
    'descriptor documents are passed to the shared profile without mirrors',
    () async {
      final platform = _DescriptorPlatform();
      final bridge = _Bridge();
      final controller = await _controller(platform, bridge);
      addTearDown(controller.dispose);

      final outcome = await controller.runAction('build_restart');
      expect(outcome?.ok, isTrue);
      expect(platform.sourceLinkCalls, 0);
      final profile =
          jsonDecode(bridge.lastProfile['build_restart']!)
              as Map<String, dynamic>;
      final values = profile['Values'] as Map<String, dynamic>;
      expect(values['DVDA_SRC'], '/sdcard/Music');
      final documents =
          jsonDecode(values['DVDA_SOURCE_DOCUMENTS'] as String)
              as Map<String, dynamic>;
      expect(documents['/sdcard/Music/Album/01.flac'], '/proc/self/fd/101');
    },
  );

  test('failed export keeps the workspace and never finishes', () async {
    final platform = _FakePlatform()
      ..exportResult = ExportResult.failure('SHORT_WRITE', 'disc.iso');
    final bridge = _Bridge();
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.failure);
    expect(outcome?.message, controller.strings.exportFailed);
    expect(bridge.calls, isNot(contains('finish_delivery')));
    expect(bridge.calls, isNot(contains('discard')));
    expect(controller.deliveryCleanupPending, isFalse);
    expect(controller.progressPercent, 99);
  });

  test('export adapter exception is a failure, never a success', () async {
    final platform = _FakePlatform()..throwOnExport = true;
    final bridge = _Bridge();
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.failure);
    expect(outcome?.message, controller.strings.exportFailed);
    expect(bridge.calls, isNot(contains('finish_delivery')));
  });

  test('export cancellation reports cancelled and never finishes', () async {
    final platform = _FakePlatform()
      ..exportResult = const ExportResult(ok: false, cancelled: true);
    final bridge = _Bridge();
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.cancelled);
    expect(bridge.calls, isNot(contains('finish_delivery')));
  });

  test('failed finish is a retryable cleanup failure, never success', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..finishOk = false
      ..finishMessage = 'cleanup io error';
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.failure);
    expect(outcome?.message, controller.strings.cleanupFailedRetained);
    expect(controller.deliveryCleanupPending, isTrue);
    expect(controller.cleanupDetail, 'cleanup io error');
    expect(controller.progressPercent, 99);

    bridge
      ..finishOk = true
      ..finishMessage = '';
    final retried = await controller.retryDeliveryCleanup();

    expect(retried?.ok, isTrue);
    expect(controller.phase, TaskPhase.success);
    expect(controller.deliveryCleanupPending, isFalse);
    expect(controller.progressPercent, 100);
  });

  test(
    'throwing finish is surfaced as a cleanup failure, not swallowed',
    () async {
      final platform = _FakePlatform();
      final bridge = _Bridge()..finishThrows = true;
      final controller = await _controller(platform, bridge);
      addTearDown(controller.dispose);

      final outcome = await controller.runAction('build_restart');

      expect(outcome?.ok, isFalse);
      expect(controller.phase, TaskPhase.failure);
      expect(outcome?.message, controller.strings.cleanupFailedRetained);
      expect(controller.deliveryCleanupPending, isTrue);
      expect(controller.cleanupDetail, contains('finish io error'));
    },
  );

  test(
    'explicit cache cleanup calls the adapter and reports the freed size',
    () async {
      final platform = _FakePlatform();
      final bridge = _Bridge();
      final controller = await _controller(platform, bridge);
      addTearDown(controller.dispose);

      final result = await controller.cleanupInternalCaches();

      expect(result.ok, isTrue);
      expect(platform.cleanupCalls, 1);
      expect(result.freedBytes, 2048);
    },
  );

  test('export result map parses a short write safely', () {
    final result = ExportResult.fromMap(<String, Object?>{
      'ok': false,
      'cancelled': false,
      'failureCode': 'SHORT_WRITE',
      'failureFile': 'disc.iso',
      'exported': <String>[],
    });
    expect(result.ok, isFalse);
    expect(result.cancelled, isFalse);
    expect(result.failureCode, 'SHORT_WRITE');
    expect(result.failureFile, 'disc.iso');
  });

  test(
    'legacy system CJK font preference migrates to the shared default',
    () async {
      SharedPreferences.setMockInitialValues(<String, Object>{
        'font_sc': AppSettings.legacySystemFont,
        'font_jp': AppSettings.legacySystemFont,
        'font_kr': AppSettings.legacySystemFont,
      });
      final settings = (await SettingsStore.open()).load();
      expect(settings.fontSc, isEmpty);
      expect(settings.fontJp, isEmpty);
      expect(settings.fontKr, isEmpty);
      expect(
        AppSettings.portableFont(AppSettings.legacySystemFont, '/kept'),
        isEmpty,
      );
    },
  );

  test('delivery cleanup retry survives a process restart', () async {
    SharedPreferences.setMockInitialValues(<String, Object>{});
    final first = _FakePlatform();
    final firstBridge = _Bridge()
      ..finishOk = false
      ..finishMessage = 'cleanup io error';
    final firstStore = await SettingsStore.open();
    final firstController = await _controller(
      first,
      firstBridge,
      store: firstStore,
      resetPreferences: false,
    );
    final failed = await firstController.runAction('build_restart');
    expect(failed?.ok, isFalse);
    expect(firstController.deliveryCleanupPending, isTrue);
    firstController.dispose();

    // A new process opens the same preferences and restores the pending retry.
    final secondStore = await SettingsStore.open();
    final secondController = await _controller(
      _FakePlatform(),
      _Bridge(),
      store: secondStore,
      resetPreferences: false,
    );
    await secondController.initialize();
    expect(secondController.deliveryCleanupPending, isTrue);
    final retried = await secondController.retryDeliveryCleanup();
    expect(retried?.ok, isTrue);
    expect(secondController.deliveryCleanupPending, isFalse);
    secondController.dispose();

    final thirdStore = await SettingsStore.open();
    final thirdController = await _controller(
      _FakePlatform(),
      _Bridge(),
      store: thirdStore,
      resetPreferences: false,
    );
    await thirdController.initialize();
    expect(thirdController.deliveryCleanupPending, isFalse);
    thirdController.dispose();
  });

  test(
    'merged discard and cleanup uses shared discard then the cache adapter',
    () async {
      final platform = _FakePlatform();
      final bridge = _Bridge();
      final controller = await _controller(platform, bridge);
      addTearDown(controller.dispose);

      final result = await controller.discardProgressAndCleanup();

      expect(result.ok, isTrue);
      expect(bridge.calls, contains('discard'));
      expect(platform.cleanupCalls, 1);
      expect(result.freedBytes, 2048);
    },
  );

  test('merged operation still reports a shared discard failure', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()..discardOk = false;
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final result = await controller.discardProgressAndCleanup();

    expect(result.ok, isFalse);
    expect(bridge.calls, contains('discard'));
    // The merged operation stops before the cache adapter when the shared
    // discard did not really remove the workspace: no success can leak out and
    // the whole operation stays retryable.
    expect(platform.cleanupCalls, 0);
    expect(result.error, isNotEmpty);
  });

  test('merged operation surfaces an adapter cleanup failure', () async {
    final platform = _FakePlatform()
      ..cleanupResult = const CleanupResult(ok: false, error: 'adapter failed');
    final bridge = _Bridge();
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final result = await controller.discardProgressAndCleanup();

    expect(result.ok, isFalse);
    expect(result.error, 'adapter failed');
    expect(bridge.calls, contains('discard'));
  });

  test('build failure keeps the failure phase and its real error', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..buildOk = false
      ..buildMessage = 'encoder exploded';
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.failure);
    expect(controller.isCancelled, isFalse);
    expect(controller.statusText, controller.strings.statusBuildFailed);
    expect(controller.message, 'encoder exploded');
  });

  test('authoritative cancellation is not painted as a failure', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..buildOk = false
      ..buildCancelled = true;
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.cancelled);
    expect(controller.isCancelled, isTrue);
    expect(controller.statusText, controller.strings.statusCancelled);
  });

  test('export failure is a failure with the export message', () async {
    final platform = _FakePlatform()
      ..exportResult = ExportResult.failure('SHORT_WRITE', 'disc.iso');
    final bridge = _Bridge();
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.failure);
    expect(controller.isCancelled, isFalse);
    expect(controller.message, controller.strings.exportFailed);
  });

  test('finish cleanup failure keeps the retry state, not success', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..finishOk = false
      ..finishMessage = 'cleanup io error';
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    final outcome = await controller.runAction('build_restart');

    expect(outcome?.ok, isFalse);
    expect(controller.phase, TaskPhase.failure);
    expect(controller.message, controller.strings.cleanupFailedRetained);
    expect(controller.deliveryCleanupPending, isTrue);
  });

  testWidgets('failure retention dialog shows the failure title and error', (
    tester,
  ) async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..buildOk = false
      ..buildMessage = '失败。'
      ..buildLog = '!! [FAIL] bad.flac: decoder failed';
    final controller = await _controller(
      platform,
      bridge,
      logFormatter: (language, log) => const FormattedLogView(
        text: '!! [FAIL] bad.flac: decoder failed',
        problemText: 'decoder failed: bad.flac',
        problems: 1,
      ),
    );
    addTearDown(controller.dispose);
    await controller.initialize();
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text(controller.strings.actionBuild));
    await tester.pumpAndSettle();

    expect(_inDialog(controller.strings.retentionFailedTitle), findsOneWidget);
    expect(
      _inDialog(
        controller.strings.retentionFailedBody('decoder failed: bad.flac'),
      ),
      findsOneWidget,
    );
    expect(_inDialog(controller.strings.retentionCancelledTitle), findsNothing);
    expect(controller.phase, TaskPhase.failure);
  });

  testWidgets('cancel retention dialog shows the user-stop copy', (
    tester,
  ) async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..buildOk = false
      ..buildCancelled = true;
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);
    await controller.initialize();
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text(controller.strings.actionBuild));
    await tester.pumpAndSettle();

    expect(
      _inDialog(controller.strings.retentionCancelledTitle),
      findsOneWidget,
    );
    expect(
      _inDialog(controller.strings.retentionCancelledBody),
      findsOneWidget,
    );
    expect(_inDialog(controller.strings.retentionFailedTitle), findsNothing);
    expect(controller.phase, TaskPhase.cancelled);
  });

  testWidgets('export failure retention dialog stays a failure', (
    tester,
  ) async {
    final platform = _FakePlatform()
      ..exportResult = ExportResult.failure('SHORT_WRITE', 'disc.iso');
    final bridge = _Bridge();
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);
    await controller.initialize();
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text(controller.strings.actionBuild));
    await tester.pumpAndSettle();

    expect(_inDialog(controller.strings.retentionFailedTitle), findsOneWidget);
    expect(
      _inDialog(
        controller.strings.retentionFailedBody(controller.strings.exportFailed),
      ),
      findsOneWidget,
    );
    expect(controller.phase, TaskPhase.failure);
  });

  testWidgets('finish cleanup failure dialog keeps the retry failure state', (
    tester,
  ) async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..finishOk = false
      ..finishMessage = 'cleanup io error';
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);
    await controller.initialize();
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text(controller.strings.actionBuild));
    await tester.pumpAndSettle();

    expect(_inDialog(controller.strings.retentionFailedTitle), findsOneWidget);
    expect(
      _inDialog(
        controller.strings.retentionFailedBody(
          controller.strings.cleanupFailedRetained,
        ),
      ),
      findsOneWidget,
    );
    expect(controller.phase, TaskPhase.failure);
    expect(controller.deliveryCleanupPending, isTrue);
  });

  test('Discarded:false means no progress and still cleans caches', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()..finishOk = false;
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    await controller.runAction('build_restart');
    expect(controller.deliveryCleanupPending, isTrue);

    bridge.discardDiscarded = false;
    final result = await controller.discardProgressAndCleanup();

    // No progress to delete is valid: the cache adapter still runs and the
    // stale finish-cleanup retry flag is cleared, without claiming that a
    // progress workspace was removed.
    expect(result.ok, isTrue);
    expect(result.progressDiscarded, isFalse);
    expect(platform.cleanupCalls, 1);
    expect(controller.deliveryCleanupPending, isFalse);
  });

  test(
    'consecutive discard failures keep the retry pending until success',
    () async {
      final platform = _FakePlatform();
      final bridge = _Bridge()
        ..finishOk = false
        ..discardOk = false;
      final controller = await _controller(platform, bridge);
      addTearDown(controller.dispose);

      await controller.runAction('build_restart');
      expect(controller.deliveryCleanupPending, isTrue);

      final first = await controller.discardProgressAndCleanup();
      final second = await controller.discardProgressAndCleanup();
      expect(first.ok, isFalse);
      expect(second.ok, isFalse);
      expect(controller.deliveryCleanupPending, isTrue);

      // A later retry that really removes the workspace and cleans the adapter
      // clears the pending state.
      bridge.discardOk = true;
      bridge.discardDiscarded = true;
      final third = await controller.discardProgressAndCleanup();
      expect(third.ok, isTrue);
      expect(controller.deliveryCleanupPending, isFalse);
    },
  );

  test('malformed nested discard payload is a failure', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..finishOk = false
      ..discardMalformed = true;
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    await controller.runAction('build_restart');
    expect(controller.deliveryCleanupPending, isTrue);

    final result = await controller.discardProgressAndCleanup();

    expect(result.ok, isFalse);
    expect(controller.deliveryCleanupPending, isTrue);
    expect(platform.cleanupCalls, 0);
  });

  test('bridge discard failure keeps the pending retry', () async {
    final platform = _FakePlatform();
    final bridge = _Bridge()
      ..finishOk = false
      ..discardOk = false;
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);

    await controller.runAction('build_restart');
    expect(controller.deliveryCleanupPending, isTrue);

    final result = await controller.discardProgressAndCleanup();

    expect(result.ok, isFalse);
    expect(controller.deliveryCleanupPending, isTrue);
    expect(platform.cleanupCalls, 0);
  });

  test(
    'a real discard still clears the pending retry (finished residue path)',
    () async {
      final platform = _FakePlatform();
      final bridge = _Bridge()..finishOk = false;
      final controller = await _controller(platform, bridge);
      addTearDown(controller.dispose);

      await controller.runAction('build_restart');
      expect(controller.deliveryCleanupPending, isTrue);

      // The shared discard removes leftover .finished / partially deleted
      // directories and reports Discarded:true; the adapter cleanup then runs.
      final result = await controller.discardProgressAndCleanup();

      expect(result.ok, isTrue);
      expect(platform.cleanupCalls, 1);
      expect(controller.deliveryCleanupPending, isFalse);
    },
  );

  testWidgets('merged cleanup button surfaces a real discard failure', (
    tester,
  ) async {
    final platform = _FakePlatform();
    final bridge = _Bridge()..discardOk = false;
    final controller = await _controller(platform, bridge);
    addTearDown(controller.dispose);
    await controller.initialize();
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byIcon(Icons.settings_outlined));
    await tester.pumpAndSettle();
    expect(find.text(controller.strings.moreSettings), findsOneWidget);
    final tile = find.byKey(const ValueKey('discard-and-cleanup'));
    await tester.scrollUntilVisible(
      tile,
      180,
      scrollable: find.byType(Scrollable).last,
    );
    await tester.pumpAndSettle();
    await tester.tap(tile);
    await tester.pumpAndSettle();
    expect(
      find.text(controller.strings.discardAndCleanupAction),
      findsOneWidget,
    );

    await tester.tap(find.text(controller.strings.discardAndCleanupAction));
    await tester.pumpAndSettle();

    expect(
      find.textContaining(controller.strings.discardAndCleanupFailed),
      findsOneWidget,
    );
    expect(platform.cleanupCalls, 0);
  });

  testWidgets('task section distinguishes cancelled hints from real failures', (
    tester,
  ) async {
    final platform = _FakePlatform();
    final failedBridge = _Bridge()
      ..buildOk = false
      ..buildMessage = 'build failed';
    final failed = await _controller(platform, failedBridge);
    addTearDown(failed.dispose);
    await failed.initialize();
    await failed.runAction('build_restart');
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(child: TaskSection(controller: failed)),
        ),
      ),
    );
    expect(find.text(failed.strings.statusBuildFailed), findsOneWidget);
    expect(find.text(failed.strings.cancelledHint), findsNothing);

    final cancelBridge = _Bridge()
      ..buildOk = false
      ..buildCancelled = true;
    final cancelled = await _controller(platform, cancelBridge);
    addTearDown(cancelled.dispose);
    await cancelled.initialize();
    await cancelled.runAction('build_restart');
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: TaskSection(controller: cancelled),
          ),
        ),
      ),
    );
    expect(find.text(cancelled.strings.statusCancelled), findsOneWidget);
    expect(find.text(cancelled.strings.cancelledHint), findsOneWidget);
    expect(find.text(cancelled.strings.statusBuildFailed), findsNothing);
  });

  test(
    'log export keeps the complete task log while the panel stays filtered',
    () async {
      const raw = 'visible\ntrack: {"Kind":"Finalized","Track":1}';
      final platform = _FakePlatform();
      final bridge = _Bridge()..buildLog = raw;
      final controller = await _controller(
        platform,
        bridge,
        logFormatter: (language, log) => const FormattedLogView(
          text: 'visible',
          problemText: '',
          problems: 0,
        ),
      );
      addTearDown(controller.dispose);
      controller.settings.finalTreeUri = 'content://tree/output';
      controller.settings.sourceTreeUri = 'content://tree/source';

      final outcome = await controller.runAction('build_restart');
      expect(outcome?.ok, isTrue, reason: outcome?.message);
      expect(await controller.exportLog(), isTrue);
      expect(platform.savedText, raw);
    },
  );
}
