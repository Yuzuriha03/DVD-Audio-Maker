import 'dart:convert';
import 'package:dvda_maker/src/models/app_settings.dart';
import 'package:dvda_maker/src/models/profile_catalog.dart';
import 'package:dvda_maker/src/services/settings_store.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'profile_catalog_fixture.dart';

void main() {
  setUpAll(() => ProfileCatalog.install(testCatalog()));

  group('AppSettings uses the shared profile catalog', () {
    test('defaults come from the shared schema', () {
      final settings = AppSettings();
      expect(settings.source, '');
      // Android requires an explicit SAF grant, so no folder is pre-filled.
      expect(settings.finalDir, '');
      expect(settings.title, 'DVD-Audio');
      expect(settings.groupTrackLimit, '99');
      expect(settings.titleMode, 'album');
      expect(settings.albumLimit, '0');
      expect(settings.audioMode, 'mlp-batch');
      expect(settings.sampleRate, '48000');
      expect(settings.bits, '24');
      expect(settings.dither16, isTrue);
      expect(settings.menu, isFalse);
      expect(settings.stills, isTrue);
      expect(settings.tracksPerPage, '12');
      expect(settings.coverDim, '35');
      expect(settings.norm, 'pal');
      // Empty means the shared language-based font default, exactly like Windows.
      expect(settings.fontSc, '');
      expect(settings.capacity, 0);
      expect(settings.discBytes, 4707319808);
    });

    test(
      'an empty stored folder stays empty instead of being backfilled',
      () async {
        SharedPreferences.setMockInitialValues(<String, Object>{
          'source': '',
          'final_dir': '',
          'external_dir': '',
        });
        final store = await SettingsStore.open();
        final settings = store.load();
        expect(settings.source, '');
        expect(settings.finalDir, '');
        expect(settings.externalDir, '');
        expect(settings.sourceStagingPath, '');
        expect(settings.finalTreeUri, '');
        expect(settings.externalTreeUri, '');
      },
    );

    test('allowed values are exposed from the catalog, not Dart constants', () {
      final settings = AppSettings();
      expect(settings.sampleRates, <String>[
        '44100',
        '48000',
        '88200',
        '96000',
        '176400',
        '192000',
      ]);
      expect(settings.mlpBits, <String>['16', '20', '24']);
      expect(settings.lpcmBits, <String>['16', '24']);
      expect(settings.audioModes, <String>['mlp-batch', 'lpcm', 'external']);
      expect(settings.videoStandards, <String>['pal', 'secam', 'ntsc']);
      expect(settings.languages, <String>['zh-CN', 'zh-TW', 'en', 'ja', 'ko']);
      expect(settings.sampleRateLabels.first, '44.1 kHz');
      expect(settings.sampleRateLabels.last, '192 kHz');
      expect(settings.bitDepthLabels, <String>['16 bit', '20 bit', '24 bit']);
    });

    test('profile composition keeps the shared defaults and capacity rule', () {
      final settings = AppSettings();
      final values = settings.toProfileValues();
      expect(values['DVDA_SRC'], '');
      expect(values['DVDA_FINAL_DIR'], '');
      expect(values['DVDA_MLP_EXTERNAL_DIR'], '');
      expect(values['DVDA_TITLE'], 'DVD-Audio');
      expect(values['DVDA_DISC_BYTES'], '4707319808');
      expect(values['DVDA_GROUP_TRACK_LIMIT'], '99');
      expect(values['DVDA_MENU_INDEX_MIN_ALBUMS'], '4');
      expect(values['DVDA_MLP_SOURCE'], 'mlp-batch');
      expect(values['DVDA_MLP_SAMPLE_RATE'], '48000');
      expect(values['DVDA_MLP_BITS'], '24');
      expect(values['DVDA_DITHER_16'], 'on');
      expect(values['DVDA_MENU'], 'off');
      expect(values['DVDA_MENU_STILLPICS'], 'on');
      expect(values['DVDA_MENU_TRACKS_PER_PAGE'], '12');
      expect(values['DVDA_MENU_COVER_DIM'], '35');
      expect(values['DVDA_MENU_NORM'], 'pal');
      expect(values['DVDA_KEEP_TMP'], 'off');
      expect(values['DVDA_RESUME'], 'on');
      expect(values['DVDA_MENU_FONT'], '');
      final profile = settings.toProfileJson();
      expect(profile['Version'], 1);
      expect(profile['Language'], 'zh-CN');
    });

    test('capacity mapping uses the shared disc bytes', () {
      final settings = AppSettings();
      settings.capacity = 1;
      expect(settings.capacityOption, 'dvd9');
      expect(settings.discBytes, 8540123136);
      settings.capacity = 2;
      settings.customBytes = '1234567890';
      expect(settings.capacityOption, 'custom');
      expect(settings.discBytes, 1234567890);
      settings.customBytes = 'not a number';
      expect(settings.discBytes, 4707319808);
    });

    test('opening a profile keeps values instead of silently rewriting them', () {
      final settings = AppSettings();
      settings.applyProfileJson('''
{"Version":1,"Language":"en","Values":{
  "DVDA_SRC":"/sdcard/Music",
  "DVDA_FINAL_DIR":"/sdcard/Download/out",
  "DVDA_DISC_BYTES":"8540123136",
  "DVDA_MLP_SOURCE":"lpcm",
  "DVDA_MLP_BITS":"20",
  "DVDA_MLP_SAMPLE_RATE":"44100",
  "DVDA_MENU_NORM":"future-norm",
  "DVDA_MENU_FONT":"C:/Windows/Fonts/msyh.ttc"
}}''');
      expect(settings.language, 'en');
      expect(settings.source, '/sdcard/Music');
      expect(settings.finalDir, '/sdcard/Download/out');
      expect(settings.capacity, 1);
      expect(settings.discBytes, 8540123136);
      expect(settings.audioMode, 'lpcm');
      // LPCM 20-bit is NOT rewritten to 24; Rust validation reports invalid_bits.
      expect(settings.bits, '20');
      expect(settings.sampleRate, '44100');
      expect(settings.norm, 'future-norm');
      expect(settings.fontSc, '');
      expect(settings.sourceTreeUri, '');
    });

    test('malformed profile JSON is rejected, never silently replaced', () {
      final settings = AppSettings();
      expect(
        () => settings.applyProfileJson('not json'),
        throwsFormatException,
      );
      expect(
        () => settings.applyProfileJson('{"Version":1,"Values":[]}'),
        throwsFormatException,
      );
    });

    test('five-language profile round-trip keeps values and empty folders', () {
      for (final language in <String>['zh-CN', 'zh-TW', 'en', 'ja', 'ko']) {
        final settings = AppSettings();
        settings.language = language;
        settings.title = 'Title-$language';
        settings.source = '/sdcard/Music';
        final text = jsonEncode(settings.toProfileJson());
        final restored = AppSettings();
        restored.applyProfileJson(text);
        expect(restored.language, language);
        expect(restored.title, 'Title-$language');
        expect(restored.finalDir, '');
        expect(restored.finalTreeUri, '');
        expect(restored.sourceTreeUri, '');
        expect(restored.externalDir, '');
      }
    });

    test('cover dim clamps like the desktop slider', () {
      final settings = AppSettings();
      settings.coverDim = '150';
      expect(settings.coverDimValue, 100);
      settings.coverDim = '-5';
      expect(settings.coverDimValue, 0);
      settings.coverDim = 'oops';
      expect(settings.coverDimValue, 35);
    });
  });

  group('Windows profile parity', () {
    test('title and ISO prefix pass through exactly and empty stays empty', () {
      final settings = AppSettings();
      settings.title = 'My Disc 2026';
      settings.isoPrefix = '';
      var values = settings.toProfileValues();
      expect(values['DVDA_TITLE'], 'My Disc 2026');
      expect(values['DVDA_ISO_PREFIX'], '');
      settings.isoPrefix = 'MyDisc_2026';
      values = settings.toProfileValues();
      expect(values['DVDA_ISO_PREFIX'], 'MyDisc_2026');
    });

    test('unknown and non-widget profile keys survive import and save', () {
      final settings = AppSettings();
      settings.applyProfileJson(
        '{"Version":1,"Language":"zh-CN","Values":{"DVDA_TITLE":"Round Trip","DVDA_MLP_METADATA_CONTEXT":"C:/meta","DVDA_LOSS_WARN_S":"0.25","DVDA_PLUGIN_KEY":"plugin-value","PluginObject":{"nested":true}}}',
      );
      final values = settings.toProfileValues();
      expect(values['DVDA_TITLE'], 'Round Trip');
      expect(values['DVDA_MLP_METADATA_CONTEXT'], 'C:/meta');
      expect(values['DVDA_LOSS_WARN_S'], '0.25');
      expect(values['DVDA_PLUGIN_KEY'], 'plugin-value');
      // Widget-owned keys always win over the imported base.
      settings.title = 'Widget';
      expect(settings.toProfileValues()['DVDA_TITLE'], 'Widget');
    });

    test('the single keep checkbox writes both keep keys like the desktop', () {
      final settings = AppSettings();
      settings.keepTmp = true;
      expect(settings.toProfileValues()['DVDA_KEEP_TMP'], 'on');
      expect(settings.toProfileValues()['DVDA_KEEP_INTERMEDIATE'], 'on');
      settings.keepTmp = false;
      expect(settings.toProfileValues()['DVDA_KEEP_TMP'], 'off');
      expect(settings.toProfileValues()['DVDA_KEEP_INTERMEDIATE'], 'off');
    });

    test('the desktop GUI defaults for resume and album index are forced', () {
      final settings = AppSettings();
      settings.applyProfileJson(
        '{"Version":1,"Values":{"DVDA_RESUME":"off","DVDA_MENU_INDEX_MIN_ALBUMS":"9"}}',
      );
      final values = settings.toProfileValues();
      expect(values['DVDA_RESUME'], 'on');
      expect(values['DVDA_MENU_INDEX_MIN_ALBUMS'], '4');
    });

    test(
      'imported profile extras persist across a settings-store reload',
      () async {
        SharedPreferences.setMockInitialValues(<String, Object>{});
        final store = await SettingsStore.open();
        final settings = AppSettings();
        settings.applyProfileJson(
          '{"Version":1,"Values":{"DVDA_PLUGIN_KEY":"kept"}}',
        );
        await store.save(settings);
        final reloaded = store.load();
        expect(reloaded.profileExtras['DVDA_PLUGIN_KEY'], 'kept');
        expect(reloaded.toProfileValues()['DVDA_PLUGIN_KEY'], 'kept');
      },
    );
  });
}
