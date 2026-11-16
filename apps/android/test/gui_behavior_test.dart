import 'dart:convert';

import 'package:dvda_maker/bridge.dart';
import 'package:dvda_maker/src/localization/app_strings.dart';
import 'package:dvda_maker/src/models/app_settings.dart';
import 'package:dvda_maker/src/services/app_controller.dart';
import 'package:dvda_maker/src/services/platform_service.dart';
import 'package:dvda_maker/src/services/settings_store.dart';
import 'package:dvda_maker/src/ui/home_screen.dart';
import 'package:dvda_maker/src/ui/sections/basics_section.dart';
import 'package:dvda_maker/src/ui/sections/audio_section.dart';
import 'package:dvda_maker/src/ui/sections/menu_section.dart';
import 'package:dvda_maker/src/ui/sections/task_section.dart';
import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dvda_maker/src/models/profile_catalog.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'profile_catalog_fixture.dart';

Future<AppController> _controller({
  TaskBridgeRunner? bridgeRunProfile,
  ProfileIssueValidator? bridgeValidateProfile,
  ProfileIssueText? bridgeIssueMessage,
}) async {
  SharedPreferences.setMockInitialValues(<String, Object>{});
  return AppController(
    settings: AppSettings(),
    store: await SettingsStore.open(),
    platform: PlatformService(),
    bridgeRunProfile: bridgeRunProfile,
    bridgeValidateProfile: bridgeValidateProfile,
    bridgeIssueMessage: bridgeIssueMessage,
  );
}

/// Shared-validation/bridge double: widget tests must not need a live Rust
/// library, and SAF tests must exercise only the platform mirror flow.
Future<String> _bridgeOk(String profileJson, String action) async {
  return jsonEncode(<String, Object>{
    'Ok': true,
    'Message': 'done',
    'Log': '',
    'ProgressDone': 100,
    'ProgressTotal': 100,
  });
}

List<ProfileIssueView> _noIssues(String valuesJson, bool forTask) {
  return const <ProfileIssueView>[];
}

class FakeRunningController extends AppController {
  FakeRunningController({
    required super.settings,
    required super.store,
    required super.platform,
  });

  bool running = false;
  bool stopRequested = false;
  String fakeProgressText = '';
  int? fakePercent;

  @override
  String get progressText => fakeProgressText;

  @override
  int? get progressPercent => fakePercent;

  @override
  bool get isRunning => running;

  @override
  bool get cancelRequested => stopRequested;

  @override
  Future<bool> checkUnfinished() async => false;

  @override
  Future<TaskOutcome?> runAction(String action) async {
    running = true;
    notifyListeners();
    return null;
  }

  @override
  void cancel() {
    stopRequested = true;
    notifyListeners();
  }

  @override
  Future<void> cancelAndWait() async {
    stopRequested = true;
    running = false;
    notifyListeners();
  }
}

void main() {
  setUpAll(() => ProfileCatalog.install(testCatalog()));
  setUp(AppBridge.resetInitializationForTests);

  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'audio options follow Windows visibility and external import rules',
    (tester) async {
      final controller = await _controller();
      addTearDown(controller.dispose);
      await tester.pumpWidget(
        MaterialApp(
          home: AnimatedBuilder(
            animation: controller,
            builder: (context, _) =>
                Scaffold(body: AudioSection(controller: controller)),
          ),
        ),
      );
      final strings = controller.strings;

      expect(find.text(strings.dither16), findsNothing);
      await tester.tap(find.byType(DropdownButton<String>).at(2));
      await tester.pumpAndSettle();
      await tester.tap(find.text('16 bit').last);
      await tester.pumpAndSettle();
      expect(find.text(strings.dither16), findsOneWidget);

      await tester.tap(find.byType(DropdownButton<String>).at(0));
      await tester.pumpAndSettle();
      await tester.tap(find.text(strings.modeExternal).last);
      await tester.pumpAndSettle();

      final dropdowns = tester
          .widgetList<DropdownButton<String>>(
            find.byType(DropdownButton<String>),
          )
          .toList();
      expect(dropdowns[1].onChanged, isNull);
      expect(dropdowns[2].onChanged, isNull);
      expect(find.text(strings.externalDir), findsOneWidget);
      expect(find.text(strings.dither16), findsNothing);
    },
  );

  testWidgets(
    'LPCM hides the invalid 20-bit option but preserves an external 20-bit value',
    (tester) async {
      final controller = await _controller();
      addTearDown(controller.dispose);
      // An imported profile may carry 20-bit LPCM; it must not be rewritten.
      controller.settings.bits = '20';
      await tester.pumpWidget(
        MaterialApp(
          home: AnimatedBuilder(
            animation: controller,
            builder: (context, _) =>
                Scaffold(body: AudioSection(controller: controller)),
          ),
        ),
      );
      final strings = controller.strings;

      await tester.tap(find.byType(DropdownButton<String>).at(0));
      await tester.pumpAndSettle();
      await tester.tap(find.text(strings.modeLpcm).last);
      await tester.pumpAndSettle();
      expect(controller.settings.audioMode, 'lpcm');
      // No silent 20 -> 24 rewrite: the shared Rust invalid_bits rule rejects
      // this combination when a task starts (core/bridge tests cover it).
      expect(controller.settings.bits, '20');

      await tester.tap(find.byType(DropdownButton<String>).at(2));
      await tester.pumpAndSettle();
      expect(find.text('20 bit'), findsNothing);
      expect(find.text('16 bit'), findsWidgets);
      expect(find.text('24 bit'), findsWidgets);
    },
  );
  testWidgets('directory field taps open the Android system picker', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    const channel = MethodChannel('dvda/platform');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      if (call.method == 'pickDirectory') {
        return <String, Object?>{
          'displayPath': '/sdcard/Audio',
          'nativePath': '',
          'treeUri':
              'content://com.android.externalstorage.documents/tree/primary%3AAudio',
        };
      }
      return null;
    });
    addTearDown(() {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      );
    });
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: BasicsSection(controller: controller)),
      ),
    );

    final field = tester.widget<TextField>(find.byType(TextField).first);
    expect(field.readOnly, isTrue);
    expect(field.onTap, isNotNull);
    await tester.tap(find.byType(TextField).first);
    await tester.pump();
    // The first selection must take effect immediately; the mirror is only
    // built when a task starts.
    expect(controller.settings.source, '/sdcard/Audio');
    expect(
      controller.settings.sourceTreeUri,
      'content://com.android.externalstorage.documents/tree/primary%3AAudio',
    );
    expect(controller.settings.sourceStagingPath, isEmpty);
    expect(controller.settings.sourceMirrorDirty, isTrue);
  });

  testWidgets('menu fonts stay under advanced settings like Windows', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(
        home: AnimatedBuilder(
          animation: controller,
          builder: (context, _) =>
              Scaffold(body: MenuSection(controller: controller)),
        ),
      ),
    );
    final strings = controller.strings;
    expect(find.byType(Slider), findsNothing);
    expect(find.text(strings.menuStills), findsNothing);
    controller.update(() => controller.settings.menu = true);
    await tester.pump();
    expect(find.byType(Slider), findsOneWidget);
    expect(find.text(strings.menuStills), findsOneWidget);
    expect(find.text(strings.fontSc), findsNothing);
    expect(find.text(strings.fontJp), findsNothing);

    controller.update(() => controller.settings.advanced = true);
    await tester.pump();
    expect(find.text(strings.fontSc), findsOneWidget);
    expect(find.text(strings.fontJp), findsOneWidget);
    expect(find.text(strings.fontKr), findsOneWidget);
    expect(find.byIcon(Icons.folder_open), findsNWidgets(3));
  });

  testWidgets('portrait bottom bar fits and task actions stay labeled', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(393, 850);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final controller = await _controller();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    final strings = controller.strings;

    expect(find.text(strings.actionPrepare), findsOneWidget);
    expect(find.text(strings.actionBuild), findsOneWidget);
    expect(tester.takeException(), isNull);

    await tester.tap(find.text(strings.tabTask));
    await tester.pumpAndSettle();
    expect(find.text(strings.actionVerify), findsOneWidget);
    expect(find.text(strings.actionOpenOutput), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('author page exposes the bundled project license', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    final strings = controller.strings;

    await tester.tap(find.byIcon(Icons.settings_outlined));
    await tester.pumpAndSettle();
    await tester.tap(find.text(strings.authorCredits).last);
    await tester.pumpAndSettle();

    expect(find.text('Yuzuriha03'), findsOneWidget);
    expect(find.text(strings.bundledLicenses), findsOneWidget);
    expect(find.text('LICENSE.txt'), findsOneWidget);
    expect(find.text('THIRD-PARTY.txt'), findsOneWidget);
    expect(find.text(strings.ffmpegLicenses), findsOneWidget);

    await tester.ensureVisible(find.text('LICENSE.txt'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('LICENSE.txt'));
    await tester.pumpAndSettle();
    expect(find.textContaining('GNU GENERAL PUBLIC LICENSE'), findsWidgets);
  });

  testWidgets('advanced switch lives in the settings sheet', (tester) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    final strings = controller.strings;

    expect(
      find.byKey(const ValueKey('advanced-settings-switch')),
      findsNothing,
    );
    await tester.tap(find.byIcon(Icons.settings_outlined));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('advanced-settings-switch')),
      findsOneWidget,
    );
    expect(find.text(strings.advancedSettingsHint), findsOneWidget);
    expect(find.byKey(const ValueKey('discard-and-cleanup')), findsOneWidget);
    expect(find.text(strings.cleanupInternalCaches), findsNothing);
    expect(find.text(strings.discardProgress), findsNothing);
    expect(find.text(strings.isoPrefix), findsNothing);
    await tester.tap(find.byKey(const ValueKey('advanced-settings-switch')));
    await tester.pumpAndSettle();
    expect(controller.settings.advanced, isTrue);
    await tester.tapAt(const Offset(10, 10));
    await tester.pumpAndSettle();
    expect(find.text(strings.isoPrefix), findsOneWidget);
    expect(
      find.byKey(const ValueKey('advanced-settings-switch')),
      findsNothing,
    );
  });

  testWidgets('merged discard and cleanup lives in the settings sheet', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    await tester.pumpWidget(
      MaterialApp(home: HomeScreen(controller: controller)),
    );
    final strings = controller.strings;

    await tester.tap(find.byIcon(Icons.settings_outlined));
    await tester.pumpAndSettle();

    final tile = find.byKey(const ValueKey('discard-and-cleanup'));
    expect(tile, findsOneWidget);
    expect(find.text(strings.cleanupInternalCaches), findsNothing);
    expect(find.text(strings.discardProgress), findsNothing);

    await tester.ensureVisible(tile);
    await tester.pumpAndSettle();
    await tester.tap(tile);
    await tester.pumpAndSettle();

    expect(find.text(strings.discardAndCleanupBody), findsWidgets);
    expect(find.text(strings.discardAndCleanupAction), findsOneWidget);
    await tester.tap(find.text(strings.cancel).last);
    await tester.pumpAndSettle();
  });

  testWidgets(
    'starting a build locks navigation and cancel disables after request',
    (tester) async {
      SharedPreferences.setMockInitialValues(<String, Object>{});
      final controller = FakeRunningController(
        settings: AppSettings(),
        store: await SettingsStore.open(),
        platform: PlatformService(),
      );
      addTearDown(controller.dispose);
      await tester.pumpWidget(
        MaterialApp(home: HomeScreen(controller: controller)),
      );
      final strings = controller.strings;

      await tester.tap(find.text(strings.actionBuild));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.byType(TaskSection), findsOneWidget);
      await tester.tap(find.text(strings.tabSetup));
      await tester.pump();
      expect(find.byType(TaskSection), findsOneWidget);

      final cancel = find
          .widgetWithText(FilledButton, strings.actionCancel)
          .first;
      expect(tester.widget<FilledButton>(cancel).onPressed, isNotNull);
      await tester.tap(cancel);
      await tester.pump();
      expect(tester.widget<FilledButton>(cancel).onPressed, isNull);
      expect(controller.stopRequested, isTrue);
    },
  );

  testWidgets(
    'running back action asks before cancelling and theme remains available',
    (tester) async {
      SharedPreferences.setMockInitialValues(<String, Object>{});
      final controller = FakeRunningController(
        settings: AppSettings(),
        store: await SettingsStore.open(),
        platform: PlatformService(),
      );
      addTearDown(controller.dispose);
      await tester.pumpWidget(
        MaterialApp(home: HomeScreen(controller: controller)),
      );
      final strings = controller.strings;

      await tester.tap(find.byIcon(Icons.brightness_auto));
      await tester.pumpAndSettle();
      await tester.tap(find.text(strings.themeLight).last);
      await tester.pumpAndSettle();
      expect(controller.settings.theme, 'light');

      await tester.tap(find.text(strings.actionBuild));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      await tester.binding.handlePopRoute();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text(strings.runningConfirm), findsOneWidget);

      await tester.tap(find.text(strings.stopAndExit));
      await tester.pumpAndSettle();
      expect(controller.stopRequested, isTrue);
      expect(controller.running, isFalse);
    },
  );

  test(
    'task profile uses SAF mappings while saved profile stays portable',
    () async {
      final controller = await _controller();
      addTearDown(controller.dispose);
      controller.settings.appFilesDir = '/data/user/999/com.dvda.maker/files';
      controller.settings.appWorkDir =
          '/data/user/999/com.dvda.maker/files/build';
      controller.settings.sourceStagingPath =
          '/data/user/999/com.dvda.maker/files/saf-mirror/source';
      controller.settings.finalTreeUri = 'content://documents/tree/output';
      controller.settings.finalDir = '/sdcard/Download/out';
      controller.settings.externalStagingPath =
          '/data/user/999/com.dvda.maker/files/saf-mirror/external';
      controller.settings.source = '/sdcard/Music';

      final task =
          jsonDecode(controller.buildTaskProfileJson()) as Map<String, dynamic>;
      final taskValues = task['Values'] as Map<String, dynamic>;
      expect(
        taskValues['DVDA_SRC'],
        '/data/user/999/com.dvda.maker/files/saf-mirror/source',
      );
      expect(
        taskValues['DVDA_BUILD_DIR'],
        '/data/user/999/com.dvda.maker/files/build',
      );
      // The shared sequence owns the real task-local delivery folder; the
      // profile never points Rust at the app-private output directory.
      expect(taskValues['DVDA_FINAL_DIR'], isEmpty);
      expect(taskValues['DVDA_FINAL_DISPLAY_DIR'], '/sdcard/Download/out');
      // Session-only source display path for shared log/diagnostic mapping.
      expect(taskValues['DVDA_SRC_DISPLAY_DIR'], '/sdcard/Music');
      expect(
        taskValues['DVDA_MLP_EXTERNAL_DIR'],
        '/data/user/999/com.dvda.maker/files/saf-mirror/external',
      );

      final saved =
          jsonDecode(controller.buildProfileJson()) as Map<String, dynamic>;
      final savedValues = saved['Values'] as Map<String, dynamic>;
      expect(savedValues['DVDA_SRC'], '/sdcard/Music');
      expect(savedValues['DVDA_FINAL_DIR'], '/sdcard/Download/out');
    },
  );

  testWidgets('initialize drops staging paths from another app sandbox', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    controller.settings.sourceStagingPath =
        '/data/user/0/com.dvda.maker/files/saf-mirror/old';
    controller.settings.externalStagingPath =
        '/data/user/0/com.dvda.maker/files/saf-links/old';
    controller.settings.sourceTreeUri = 'content://documents/tree/source';
    controller.settings.fontSc = '/sdcard/fonts/unreadable.ttf';
    controller.settings.fontJp =
        '/data/user/999/com.dvda.maker/files/fonts/jp.ttf';
    const channel = MethodChannel('dvda/platform');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      if (call.method == 'appPaths') {
        return <String, Object?>{
          'filesDir': '/data/user/999/com.dvda.maker/files',
          'buildDir': '/data/user/999/com.dvda.maker/files/build',
        };
      }
      return null;
    });
    addTearDown(() {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      );
    });

    await controller.initialize();

    expect(controller.settings.sourceStagingPath, isEmpty);
    expect(controller.settings.externalStagingPath, isEmpty);
    expect(
      controller.settings.appWorkDir,
      '/data/user/999/com.dvda.maker/files/build',
    );
    expect(controller.settings.sourceTreeUri, isNotEmpty);
    // A stale public font path falls back to the shared platform default.
    expect(controller.settings.fontSc, isEmpty);
    expect(
      controller.settings.fontJp,
      '/data/user/999/com.dvda.maker/files/fonts/jp.ttf',
    );
  });

  testWidgets('an empty staging path is rebuilt from its SAF grant', (
    tester,
  ) async {
    final controller = await _controller(
      bridgeRunProfile: _bridgeOk,
      bridgeValidateProfile: _noIssues,
    );
    addTearDown(controller.dispose);
    controller.settings.sourceStagingPath = '';
    controller.settings.sourceTreeUri = 'content://documents/tree/source';
    var relinked = false;
    const channel = MethodChannel('dvda/platform');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      if (call.method == 'ensureSourceLink') {
        relinked = true;
        return <String, Object?>{
          'displayPath': '/sdcard/Music',
          'nativePath': '/data/user/0/com.dvda.maker/files/saf-mirror/rebuilt',
          'treeUri': 'content://documents/tree/source',
        };
      }
      return null;
    });
    addTearDown(() {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      );
    });

    final outcome = await controller.runAction('prepare');

    expect(relinked, isTrue);
    expect(
      controller.settings.sourceStagingPath,
      '/data/user/0/com.dvda.maker/files/saf-mirror/rebuilt',
    );
    expect(outcome?.ok, isTrue);
    expect(outcome?.message, 'done');
  });

  testWidgets('a freshly picked folder rebuilds its mirror before the task', (
    tester,
  ) async {
    final controller = await _controller(
      bridgeRunProfile: _bridgeOk,
      bridgeValidateProfile: _noIssues,
    );
    addTearDown(controller.dispose);
    controller.settings.appFilesDir = '/data/user/0/com.dvda.maker/files';
    controller.settings.sourceTreeUri = 'content://documents/tree/source';
    controller.settings.sourceStagingPath = '';
    controller.settings.sourceMirrorDirty = true;
    var rebuilt = false;
    const channel = MethodChannel('dvda/platform');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      if (call.method == 'linkDirectory') {
        rebuilt = true;
        return <String, Object?>{
          'displayPath': '/sdcard/Music',
          'nativePath': '/data/user/0/com.dvda.maker/files/saf-mirror/rebuilt',
          'treeUri': 'content://documents/tree/source',
        };
      }
      return null;
    });
    addTearDown(() {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      );
    });

    final outcome = await controller.runAction('prepare');

    expect(rebuilt, isTrue);
    expect(
      controller.settings.sourceStagingPath,
      '/data/user/0/com.dvda.maker/files/saf-mirror/rebuilt',
    );
    expect(controller.settings.sourceMirrorDirty, isFalse);
    expect(outcome?.ok, isTrue);
    expect(outcome?.message, 'done');
  });

  testWidgets(
    'shared profile validation issues are surfaced without SAF work',
    (tester) async {
      final controller = await _controller(
        bridgeValidateProfile: (valuesJson, forTask) =>
            const <ProfileIssueView>[
              ProfileIssueView(
                key: 'DVDA_MENU_FONT',
                code: 'MENU_FONT_UNAVAILABLE',
              ),
            ],
        bridgeIssueMessage: (language, code) => 'menu font unavailable',
      );
      addTearDown(controller.dispose);
      controller.settings.sourceStagingPath = '';
      controller.settings.sourceTreeUri = 'content://documents/tree/source';
      var relinked = false;
      const channel = MethodChannel('dvda/platform');
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
        call,
      ) async {
        if (call.method == 'ensureSourceLink') {
          relinked = true;
        }
        return null;
      });
      addTearDown(() {
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          channel,
          null,
        );
      });

      final outcome = await controller.runAction('prepare');

      expect(outcome?.ok, isFalse);
      expect(outcome?.message, 'menu font unavailable');
      expect(relinked, isFalse);
    },
  );
  testWidgets('a broken SAF mirror reports the platform reason', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    controller.settings.appFilesDir = '/data/user/0/com.dvda.maker/files';
    controller.settings.sourceTreeUri = 'content://documents/tree/broken';
    controller.settings.sourceStagingPath = '';
    const channel = MethodChannel('dvda/platform');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      if (call.method == 'ensureSourceLink') {
        throw PlatformException(
          code: 'SAF_MIRROR',
          message: 'Cannot read the file broken.flac',
        );
      }
      return null;
    });
    addTearDown(() {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      );
    });

    final outcome = await controller.runAction('prepare');

    expect(outcome?.ok, isFalse);
    expect(outcome?.message, contains('Cannot read the file broken.flac'));
    expect(controller.isRunning, isFalse);
    expect(controller.settings.sourceStagingPath, isEmpty);
  });

  testWidgets('app path initialization failure stays readable', (tester) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    // An unmocked platform channel would never answer inside the fake-async
    // widget-test zone, so surface the failure through an explicit exception.
    const channel = MethodChannel('dvda/platform');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      if (call.method == 'appPaths') {
        throw PlatformException(
          code: 'APP_PATHS',
          message: 'app-private directories are unavailable',
        );
      }
      return null;
    });
    addTearDown(() {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      );
    });

    await controller.initialize();
    expect(controller.initError, isNotNull);
    controller.settings.sourceTreeUri = 'content://documents/tree/source';
    final outcome = await controller.runAction('prepare');

    expect(outcome?.ok, isFalse);
    expect(outcome?.message, contains('app-private'));
    expect(await controller.checkUnfinished(), isFalse);
  });

  test('unfinished progress is detected from the bridge status payload', () {
    const unfinished =
        '{"Ok":true,"Message":"{\\"Ok\\":true,\\"Unfinished\\":true}",'
        '"Diagnostics":null,"Log":"","ProgressDone":0,"ProgressTotal":100}';
    const finished =
        '{"Ok":true,"Message":"{\\"Ok\\":true,\\"Unfinished\\":false}",'
        '"Diagnostics":null,"Log":"","ProgressDone":0,"ProgressTotal":100}';

    expect(AppController.unfinishedFromStatus(unfinished), isTrue);
    expect(AppController.unfinishedFromStatus(finished), isFalse);
    expect(
      AppController.unfinishedFromStatus('{"Ok":true,"Unfinished":true}'),
      isTrue,
    );
    expect(AppController.unfinishedFromStatus('not json'), isFalse);
  });

  test('pending delivery exposes exactly the task-local export folder', () {
    final pending = TaskOutcome.parse(
      '{"Ok":true,"FinalDirectory":"/data/user/0/com.dvda.maker/files/build/unfinished-task/deliver"}',
    );
    expect(
      pending.deliveryDirectory,
      '/data/user/0/com.dvda.maker/files/build/unfinished-task/deliver',
    );
    final delivered = TaskOutcome.parse('{"Ok":true,"FinalDirectory":null}');
    expect(delivered.deliveryDirectory, isNull);
    expect(TaskOutcome.parse('{"Ok":true}').deliveryDirectory, isNull);
  });

  test('build validation requires SAF source and output selections', () async {
    final controller = await _controller();
    addTearDown(controller.dispose);

    final missingSource = await controller.runAction('build');
    expect(missingSource?.ok, isFalse);
    expect(missingSource?.message, controller.strings.selectSourceRequired);

    controller.settings.sourceStagingPath =
        '/data/user/0/com.dvda.maker/files/linked';
    final missingOutput = await controller.runAction('build');
    expect(missingOutput?.ok, isFalse);
    expect(missingOutput?.message, controller.strings.selectFinalRequired);
  });

  testWidgets('task progress shows readable text plus a percentage only', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues(<String, Object>{});
    final controller = FakeRunningController(
      settings: AppSettings(),
      store: await SettingsStore.open(),
      platform: PlatformService(),
    );
    addTearDown(controller.dispose);
    controller.running = true;
    controller.fakeProgressText = '打包 ISO，光盘： 2/149';
    controller.fakePercent = 65;
    await tester.pumpWidget(
      MaterialApp(
        // Production keeps TaskSection inside home_screen's scroll view; the
        // widget test supplies the same scrolling host explicitly.
        home: Scaffold(
          body: SingleChildScrollView(
            child: TaskSection(controller: controller),
          ),
        ),
      ),
    );

    expect(find.text('65%'), findsOneWidget);
    final detail = tester.widget<Text>(find.text('打包 ISO，光盘： 2/149'));
    expect(detail.maxLines, 1);
    expect(detail.overflow, TextOverflow.ellipsis);
    final texts = tester
        .widgetList<Text>(find.byType(Text))
        .map((widget) => widget.data ?? '')
        .join('\n');
    expect(texts, isNot(contains('{"Action"')));
    expect(texts, isNot(contains('[TASK]')));
    expect(
      RegExp(r'\b\d+\s*/\s*100\b').hasMatch(texts),
      isFalse,
      reason: texts,
    );
    expect(texts, isNot(contains('/100')));
  });

  test('statistics uses Windows elapsed time and problem count format', () {
    final strings = AppStrings.forCode('en');
    expect(strings.statistics(3661, 2), 'Elapsed 1:01:01 · Notices 2');
  });
}
