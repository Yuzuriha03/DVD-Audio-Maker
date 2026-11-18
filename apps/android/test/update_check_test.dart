import 'dart:async';

import 'package:dvda_maker/src/localization/app_strings.dart';
import 'package:dvda_maker/src/services/platform_service.dart';
import 'package:dvda_maker/src/services/update_service.dart';
import 'package:dvda_maker/src/ui/author_credits_page.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

const _download =
    'https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/download/'
    'v1.3/DVD-Audio-Maker-v1.3-android-arm64.apk';

class _ManualCheck extends UpdateService {
  _ManualCheck(this.result)
    : super(
        platform: PlatformService(),
        updateApiUrl: () => throw StateError('no network in a widget test'),
        selectUpdate: (_, _) => throw StateError('unexpected parser call'),
      );

  final UpdateCheckResult result;
  final started = Completer<void>();
  String? opened;
  bool? manual;

  @override
  AppVersion get appVersion =>
      const AppVersion(version: '1.2.0', buildNumber: 2);

  @override
  Future<String?> loadCurrentVersion() async => appVersion.version;

  @override
  Future<UpdateCheckResult> check({required bool manual}) async {
    this.manual = manual;
    await started.future;
    return result;
  }

  @override
  Future<bool> openDownload(String url) async {
    opened = url;
    return true;
  }
}

void main() {
  const strings = AppStrings.english;
  final cases = [
    (
      UpdateCheckResult.updateAvailable(
        version: 'v1.3',
        downloadUrl: _download,
      ),
      strings.updateAvailable('v1.3'),
    ),
    (UpdateCheckResult.upToDate(), strings.updateUpToDate('1.2.0 (2)')),
    (UpdateCheckResult.networkFailed('Offline'), strings.updateNetworkFailed),
    (UpdateCheckResult.failed('Invalid response'), strings.updateCheckFailed),
  ];
  for (final (result, expectedMessage) in cases) {
    testWidgets('About shows a manual dialog for ${result.outcome.name}', (
      tester,
    ) async {
      final service = _ManualCheck(result);
      await tester.pumpWidget(
        MaterialApp(
          home: AuthorCreditsPage(strings: strings, updateService: service),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('DVD-Audio Maker · v1.2.0 (2)'), findsOneWidget);
      final button = find.byTooltip(strings.updateCheck);
      await tester.scrollUntilVisible(
        button,
        160,
        scrollable: find
            .descendant(
              of: find.byType(ListView),
              matching: find.byType(Scrollable),
            )
            .first,
      );
      await tester.pumpAndSettle();
      await tester.tap(button);
      await tester.pump();
      expect(service.manual, isTrue);
      expect(find.byType(AlertDialog), findsNothing);
      service.started.complete();
      await tester.pumpAndSettle();
      final dialog = find.byType(AlertDialog);
      expect(dialog, findsOneWidget);
      expect(
        find.descendant(of: dialog, matching: find.text(expectedMessage)),
        findsOneWidget,
      );
      final download = find.descendant(
        of: dialog,
        matching: find.text(strings.updateDownload),
      );
      if (result.hasUpdate) {
        expect(download, findsOneWidget);
        expect(service.opened, isNull);
        await tester.tap(download);
        await tester.pumpAndSettle();
        expect(service.opened, _download);
      } else {
        expect(download, findsNothing);
        expect(service.opened, isNull);
      }
      await tester.pumpWidget(const SizedBox.shrink());
      service.dispose();
    });
  }
}
