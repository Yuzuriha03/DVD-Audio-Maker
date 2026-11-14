import 'dart:convert';

import 'package:dvda_maker/src/models/app_settings.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('AppSettings defaults', () {
    test('matches the desktop-compatible profile defaults', () {
      final settings = AppSettings();

      expect(settings.language, 'zh-CN');
      expect(settings.source, '/sdcard/Music');
      expect(settings.finalDir, '/sdcard/Download/DSH-Go');
      expect(settings.title, 'DVD-Audio');
      expect(settings.capacity, 0);
      expect(settings.customBytes, '4707319808');
      expect(settings.workDir, kDefaultWorkDir);
      expect(settings.isoPrefix, '');
      expect(settings.groupTrackLimit, '99');
      expect(settings.titleMode, 'album');
      expect(settings.albumLimit, '0');
      expect(settings.keepTmp, isFalse);
      expect(settings.audioMode, 'mlp-batch');
      expect(settings.sampleRate, '48000');
      expect(settings.bits, '24');
      expect(settings.dither16, isTrue);
      expect(settings.externalDir, '');
      expect(settings.menu, isFalse);
      expect(settings.stills, isTrue);
      expect(settings.tracksPerPage, '12');
      expect(settings.coverDim, '35');
      expect(settings.norm, 'pal');
      expect(settings.fontSc, kDefaultFont);
      expect(settings.fontJp, '');
      expect(settings.fontKr, '');
      expect(settings.discBytes, kDvd5Bytes);
    });

    test('serializes Version, Language and every expected DVDA_* key', () {
      final settings = AppSettings();
      final profile = settings.toProfileJson();
      final values = settings.toProfileValues();

      expect(profile['Version'], 1);
      expect(profile['Language'], 'zh-CN');
      expect(values.length, 26);
      expect(values['DVDA_SRC'], '/sdcard/Music');
      expect(values['DVDA_FINAL_DIR'], '/sdcard/Download/DSH-Go');
      expect(values['DVDA_TITLE'], 'DVD-Audio');
      expect(values['DVDA_DISC_BYTES'], kDvd5Bytes.toString());
      expect(values['DVDA_BUILD_DIR'], kDefaultWorkDir);
      expect(values['DVDA_ISO_PREFIX'], '');
      expect(values['DVDA_GROUP_TRACK_LIMIT'], '99');
      expect(values['DVDA_MLP_SOURCE'], 'mlp-batch');
      expect(values['DVDA_MLP_SAMPLE_RATE'], '48000');
      expect(values['DVDA_MLP_BITS'], '24');
      expect(values['DVDA_DITHER_16'], 'on');
      expect(values['DVDA_MLP_EXTERNAL_DIR'], '');
      expect(values['DVDA_MENU'], 'off');
      expect(values['DVDA_MENU_TRACKS_PER_PAGE'], '12');
      expect(values['DVDA_MENU_STILLPICS'], 'on');
      expect(values['DVDA_MENU_COVER_DIM'], '35');
      expect(values['DVDA_MENU_NORM'], 'pal');
      expect(values['DVDA_MENU_FONT'], kDefaultFont);
      expect(values['DVDA_MENU_FONT_JP'], '');
      expect(values['DVDA_MENU_FONT_KR'], '');
      expect(values['DVDA_KEEP_TMP'], 'off');
      expect(values['DVDA_KEEP_INTERMEDIATE'], 'off');
      expect(values['DVDA_TITLE_MODE'], 'album');
      expect(values['DVDA_ALBUM_LIMIT'], '0');
      expect(values['DVDA_MENU_INDEX_MIN_ALBUMS'], '4');
      expect(values['DVDA_RESUME'], 'on');
    });

    test('writes string values only', () {
      final decoded =
          jsonDecode(AppSettings().toProfileJsonString())
              as Map<String, dynamic>;
      final values = decoded['Values']! as Map<String, dynamic>;
      expect(values.values.every((value) => value is String), isTrue);
    });
  });

  group('profile round-trip', () {
    test('preserves representative DVDA_* values', () {
      final source = AppSettings()
        ..language = 'ja'
        ..source = '/sdcard/Music/专辑'
        ..finalDir = '/sdcard/Download/out'
        ..title = '测试 DVD-Audio'
        ..capacity = 2
        ..customBytes = '5000000000'
        ..workDir = '/sdcard/build'
        ..isoPrefix = 'prefix'
        ..groupTrackLimit = '7'
        ..titleMode = 'one'
        ..albumLimit = '3'
        ..keepTmp = true
        ..audioMode = 'lpcm'
        ..sampleRate = '96000'
        ..bits = '16'
        ..dither16 = false
        ..externalDir = '/sdcard/mlp'
        ..menu = true
        ..stills = false
        ..tracksPerPage = '9'
        ..coverDim = '50'
        ..norm = 'ntsc'
        ..fontSc = '/fonts/sc.ttf'
        ..fontJp = '/fonts/jp.ttf'
        ..fontKr = '/fonts/kr.ttf';

      final text = source.toProfileJsonString();
      final decoded = jsonDecode(text) as Map<String, dynamic>;
      final values = decoded['Values']! as Map<String, dynamic>;

      expect(decoded['Version'], 1);
      expect(decoded['Language'], 'ja');
      expect(values['DVDA_SRC'], '/sdcard/Music/专辑');
      expect(values['DVDA_FINAL_DIR'], '/sdcard/Download/out');
      expect(values['DVDA_TITLE'], '测试 DVD-Audio');
      expect(values['DVDA_DISC_BYTES'], '5000000000');
      expect(values['DVDA_BUILD_DIR'], kDefaultWorkDir);
      expect(values['DVDA_ISO_PREFIX'], 'prefix');
      expect(values['DVDA_GROUP_TRACK_LIMIT'], '7');
      expect(values['DVDA_MLP_SOURCE'], 'lpcm');
      expect(values['DVDA_MLP_SAMPLE_RATE'], '96000');
      expect(values['DVDA_MLP_BITS'], '16');
      expect(values['DVDA_DITHER_16'], 'off');
      expect(values['DVDA_MLP_EXTERNAL_DIR'], '/sdcard/mlp');
      expect(values['DVDA_MENU'], 'on');
      expect(values['DVDA_MENU_TRACKS_PER_PAGE'], '9');
      expect(values['DVDA_MENU_STILLPICS'], 'off');
      expect(values['DVDA_MENU_COVER_DIM'], '50');
      expect(values['DVDA_MENU_NORM'], 'ntsc');
      expect(values['DVDA_MENU_FONT'], '/fonts/sc.ttf');
      expect(values['DVDA_MENU_FONT_JP'], '/fonts/jp.ttf');
      expect(values['DVDA_MENU_FONT_KR'], '/fonts/kr.ttf');
      expect(values['DVDA_KEEP_TMP'], 'off');
      expect(values['DVDA_KEEP_INTERMEDIATE'], 'off');
      expect(values['DVDA_TITLE_MODE'], 'one');
      expect(values['DVDA_ALBUM_LIMIT'], '3');
      expect(values['DVDA_MENU_INDEX_MIN_ALBUMS'], '4');
      expect(values['DVDA_RESUME'], 'on');

      final restored = AppSettings()..applyProfileJson(text);
      expect(restored.language, 'ja');
      expect(restored.source, '/sdcard/Music/专辑');
      expect(restored.finalDir, '/sdcard/Download/out');
      expect(restored.title, '测试 DVD-Audio');
      expect(restored.capacity, 2);
      expect(restored.customBytes, '5000000000');
      expect(restored.discBytes, 5000000000);
      expect(restored.workDir, kDefaultWorkDir);
      expect(restored.isoPrefix, 'prefix');
      expect(restored.groupTrackLimit, '7');
      expect(restored.titleMode, 'one');
      expect(restored.albumLimit, '3');
      expect(restored.keepTmp, isFalse);
      expect(restored.audioMode, 'lpcm');
      expect(restored.sampleRate, '96000');
      expect(restored.bits, '16');
      expect(restored.dither16, isFalse);
      expect(restored.externalDir, '/sdcard/mlp');
      expect(restored.menu, isTrue);
      expect(restored.stills, isFalse);
      expect(restored.tracksPerPage, '9');
      expect(restored.coverDim, '50');
      expect(restored.norm, 'ntsc');
      expect(restored.fontSc, '/fonts/sc.ttf');
      expect(restored.fontJp, '/fonts/jp.ttf');
      expect(restored.fontKr, '/fonts/kr.ttf');
      expect(restored.toProfileValues(), source.toProfileValues());
    });

    test('round-trips every supported language code', () {
      for (final code in kLanguages) {
        final settings = AppSettings()..language = code;
        final restored = AppSettings()
          ..applyProfileJson(settings.toProfileJsonString());
        expect(restored.language, code, reason: code);
      }
    });

    test('keeps the current language for an unsupported profile value', () {
      final settings = AppSettings()..language = 'ko';
      final text = jsonEncode(<String, dynamic>{
        'Version': 1,
        'Language': 'fr',
        'Values': <String, String>{},
      });
      settings.applyProfileJson(text);
      expect(settings.language, 'ko');
    });
  });

  group('profile decoding edge cases', () {
    test('rejects malformed documents', () {
      final settings = AppSettings();
      expect(
        () => settings.applyProfileJson('not json'),
        throwsA(isA<FormatException>()),
      );
      expect(
        () => settings.applyProfileJson('[]'),
        throwsA(isA<FormatException>()),
      );
      expect(
        () => settings.applyProfileJson('{"Version":1}'),
        throwsA(isA<FormatException>()),
      );
    });

    test('accepts a sparse profile and keeps fallbacks', () {
      final settings = AppSettings()..source = '/sdcard/keep';
      expect(
        () => settings.applyProfileJson('{"Version":1,"Values":{}}'),
        returnsNormally,
      );
      expect(settings.source, '/sdcard/keep');
      expect(settings.language, 'zh-CN');
    });

    test('maps DVD5/DVD9/custom disc bytes to capacity options', () {
      var settings = AppSettings()
        ..applyProfileJson(
          jsonEncode(<String, dynamic>{
            'Version': 1,
            'Values': <String, String>{
              'DVDA_DISC_BYTES': kDvd9Bytes.toString(),
            },
          }),
        );
      expect(settings.capacity, 1);
      expect(settings.discBytes, kDvd9Bytes);

      settings = AppSettings()
        ..applyProfileJson(
          jsonEncode(<String, dynamic>{
            'Version': 1,
            'Values': <String, String>{'DVDA_DISC_BYTES': '1234567890'},
          }),
        );
      expect(settings.capacity, 2);
      expect(settings.customBytes, '1234567890');
      expect(settings.discBytes, 1234567890);
    });

    test('falls back to DVD5 for invalid custom bytes', () {
      final settings = AppSettings()
        ..capacity = 2
        ..customBytes = 'not-a-number';
      expect(settings.discBytes, kDvd5Bytes);
      expect(
        settings.toProfileValues()['DVDA_DISC_BYTES'],
        kDvd5Bytes.toString(),
      );
    });
  });

  group('menu font fallback', () {
    test(
      'uses the system font when menu is enabled and the field is blank',
      () {
        final settings = AppSettings()
          ..menu = true
          ..fontSc = '';
        expect(settings.toProfileValues()['DVDA_MENU_FONT'], kDefaultFont);
      },
    );

    test('keeps the field empty when menu is disabled and it is blank', () {
      final settings = AppSettings()
        ..menu = false
        ..fontSc = '';
      expect(settings.toProfileValues()['DVDA_MENU_FONT'], '');
    });
  });
}
