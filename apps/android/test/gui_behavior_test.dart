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
          'nativePath': '/data/user/0/com.dvda.maker/files/linked-source',
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
    expect(controller.settings.source, '/sdcard/Audio');
    expect(
      controller.settings.sourceStagingPath,
      '/data/user/0/com.dvda.maker/files/linked-source',
    );
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
    expect(find.byType(Slider), findsOneWidget);
    expect(find.text(strings.fontSc), findsNothing);
    expect(find.text(strings.fontJp), findsNothing);

    controller.update(() => controller.settings.advanced = true);
    await tester.pump();
    expect(find.text(strings.fontSc), findsOneWidget);
    expect(find.text(strings.fontJp), findsOneWidget);
    expect(find.text(strings.fontKr), findsOneWidget);
    expect(find.byIcon(Icons.folder_open), findsNWidgets(3));
  });

  testWidgets('advanced switch belongs to the global app bar, not basics', (
    tester,
  ) async {
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
    await tester.tap(find.byIcon(Icons.more_vert));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('advanced-settings-switch')),
      findsOneWidget,
    );
    expect(find.text(strings.isoPrefix), findsNothing);
    await tester.tap(find.byKey(const ValueKey('advanced-settings-switch')));
    await tester.pumpAndSettle();
    expect(controller.settings.advanced, isTrue);
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
      controller.settings.sourceStagingPath =
          '/data/user/0/com.dvda.maker/files/linked';
      controller.settings.finalTreeUri = 'content://documents/tree/output';
      controller.settings.externalStagingPath =
          '/data/user/0/com.dvda.maker/files/external';

      final task =
          jsonDecode(controller.buildTaskProfileJson()) as Map<String, dynamic>;
      final taskValues = task['Values'] as Map<String, dynamic>;
      expect(
        taskValues['DVDA_SRC'],
        '/data/user/0/com.dvda.maker/files/linked',
      );
      expect(taskValues['DVDA_FINAL_DIR'], kDefaultTaskOutputDir);
      expect(
        taskValues['DVDA_MLP_EXTERNAL_DIR'],
        '/data/user/0/com.dvda.maker/files/external',
      );

      final saved =
          jsonDecode(controller.buildProfileJson()) as Map<String, dynamic>;
      final savedValues = saved['Values'] as Map<String, dynamic>;
      expect(savedValues['DVDA_SRC'], '/sdcard/Music');
      expect(savedValues['DVDA_FINAL_DIR'], '/sdcard/Download/DSH-Go');
    },
  );

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
