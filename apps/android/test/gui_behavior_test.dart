import 'dart:convert';

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
import 'package:shared_preferences/shared_preferences.dart';

Future<AppController> _controller() async {
  SharedPreferences.setMockInitialValues(<String, Object>{});
  return AppController(
    settings: AppSettings(),
    store: await SettingsStore.open(),
    platform: PlatformService(),
  );
}

class FakeRunningController extends AppController {
  FakeRunningController({
    required super.settings,
    required super.store,
    required super.platform,
  });

  bool running = false;
  bool stopRequested = false;

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

  testWidgets('LPCM hides 20-bit depth and normalizes existing 20-bit', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
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
    expect(controller.settings.bits, '24');

    await tester.tap(find.byType(DropdownButton<String>).at(2));
    await tester.pumpAndSettle();
    expect(find.text('20 bit'), findsNothing);
    expect(find.text('16 bit'), findsWidgets);
    expect(find.text('24 bit'), findsWidgets);
  });

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
      controller.settings.appOutputDir =
          '/data/user/999/com.dvda.maker/files/output';
      controller.settings.sourceStagingPath =
          '/data/user/999/com.dvda.maker/files/saf-mirror/source';
      controller.settings.finalTreeUri = 'content://documents/tree/output';
      controller.settings.externalStagingPath =
          '/data/user/999/com.dvda.maker/files/saf-mirror/external';

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
      expect(
        taskValues['DVDA_FINAL_DIR'],
        '/data/user/999/com.dvda.maker/files/output',
      );
      expect(
        taskValues['DVDA_MLP_EXTERNAL_DIR'],
        '/data/user/999/com.dvda.maker/files/saf-mirror/external',
      );

      final saved =
          jsonDecode(controller.buildProfileJson()) as Map<String, dynamic>;
      final savedValues = saved['Values'] as Map<String, dynamic>;
      expect(savedValues['DVDA_SRC'], '/sdcard/Music');
      expect(savedValues['DVDA_FINAL_DIR'], '/sdcard/Download/DSH-Go');
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
          'outputDir': '/data/user/999/com.dvda.maker/files/output',
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
    expect(controller.settings.fontSc, kDefaultFont);
    expect(
      controller.settings.fontJp,
      '/data/user/999/com.dvda.maker/files/fonts/jp.ttf',
    );
  });

  testWidgets('an empty staging path is rebuilt from its SAF grant', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    controller.settings.sourceStagingPath = '';
    controller.settings.sourceTreeUri = 'content://documents/tree/source';
    controller.settings.menu = true;
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
      if (call.method == 'validateReadableFont') {
        return false;
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
    expect(outcome?.message, contains('menu font'));
  });

  testWidgets('a freshly picked folder rebuilds its mirror before the task', (
    tester,
  ) async {
    final controller = await _controller();
    addTearDown(controller.dispose);
    controller.settings.appFilesDir = '/data/user/0/com.dvda.maker/files';
    controller.settings.sourceTreeUri = 'content://documents/tree/source';
    controller.settings.sourceStagingPath = '';
    controller.settings.sourceMirrorDirty = true;
    controller.settings.menu = true;
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
      if (call.method == 'validateReadableFont') {
        return false;
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
    expect(outcome?.message, contains('menu font'));
  });

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

  test('statistics uses Windows elapsed time and problem count format', () {
    final strings = AppStrings.forCode('en');
    expect(strings.statistics(3661, 2), 'Elapsed 1:01:01 · Notices 2');
  });
}
