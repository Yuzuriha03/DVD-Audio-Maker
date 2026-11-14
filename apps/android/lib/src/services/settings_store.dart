import 'package:shared_preferences/shared_preferences.dart';

import '../models/app_settings.dart';

/// Persists the editable form state with `shared_preferences`.
class SettingsStore {
  SettingsStore(this._preferences);

  final SharedPreferences _preferences;

  static Future<SettingsStore> open() async {
    return SettingsStore(await SharedPreferences.getInstance());
  }

  AppSettings load() {
    final settings = AppSettings();

    final language = _preferences.getString('language');
    if (language != null && kLanguages.contains(language)) {
      settings.language = language;
    }
    final theme = _preferences.getString('theme') ?? 'system';
    if (theme == 'system' || theme == 'light' || theme == 'dark') {
      settings.theme = theme;
    }

    settings.source = _preferences.getString('source') ?? settings.source;
    settings.sourceStagingPath =
        _preferences.getString('source_staging_path') ?? '';
    settings.sourceTreeUri = _preferences.getString('source_tree_uri') ?? '';
    settings.finalDir =
        _preferences.getString('final_dir') ?? settings.finalDir;
    settings.finalTreeUri = _preferences.getString('final_tree_uri') ?? '';
    settings.title = _preferences.getString('title') ?? settings.title;
    settings.customBytes =
        _preferences.getString('custom_bytes') ?? settings.customBytes;
    settings.advanced = _preferences.getBool('advanced') ?? settings.advanced;
    settings.workDir = _preferences.getString('work_dir') ?? settings.workDir;
    settings.isoPrefix =
        _preferences.getString('iso_prefix') ?? settings.isoPrefix;
    settings.groupTrackLimit =
        _preferences.getString('group_track_limit') ?? settings.groupTrackLimit;
    settings.titleMode =
        _preferences.getString('title_mode') ?? settings.titleMode;
    settings.albumLimit =
        _preferences.getString('album_limit') ?? settings.albumLimit;
    settings.keepTmp = _preferences.getBool('keep_tmp') ?? settings.keepTmp;

    final audioMode = _preferences.getString('audio_mode');
    if (audioMode != null && kAudioModes.contains(audioMode)) {
      settings.audioMode = audioMode;
    }
    final sampleRate = _preferences.getString('sample_rate');
    if (sampleRate != null && kSampleRates.contains(sampleRate)) {
      settings.sampleRate = sampleRate;
    }
    final bits = _preferences.getString('bits');
    if (bits != null && kBitDepths.contains(bits)) {
      settings.bits = bits;
    }
    settings.dither16 = _preferences.getBool('dither16') ?? settings.dither16;
    settings.externalDir =
        _preferences.getString('external_dir') ?? settings.externalDir;
    settings.externalStagingPath =
        _preferences.getString('external_staging_path') ?? '';
    settings.externalTreeUri =
        _preferences.getString('external_tree_uri') ?? '';

    settings.menu = _preferences.getBool('menu') ?? settings.menu;
    settings.stills = _preferences.getBool('stills') ?? settings.stills;
    settings.tracksPerPage =
        _preferences.getString('tracks_per_page') ?? settings.tracksPerPage;
    settings.coverDim =
        _preferences.getString('cover_dim') ?? settings.coverDim;
    final norm = _preferences.getString('norm');
    if (norm != null && kVideoStandards.contains(norm)) {
      settings.norm = norm;
    }
    settings.fontSc = _preferences.getString('font_sc') ?? settings.fontSc;
    settings.fontJp = _preferences.getString('font_jp') ?? settings.fontJp;
    settings.fontKr = _preferences.getString('font_kr') ?? settings.fontKr;

    final capacity = _preferences.getInt('capacity');
    if (capacity != null && capacity >= 0 && capacity <= 2) {
      settings.capacity = capacity;
    }
    return settings;
  }

  Future<void> save(AppSettings settings) async {
    await _preferences.setString('language', settings.language);
    await _preferences.setString('theme', settings.theme);
    await _preferences.setString('source', settings.source);
    await _preferences.setString(
      'source_staging_path',
      settings.sourceStagingPath,
    );
    await _preferences.setString('source_tree_uri', settings.sourceTreeUri);
    await _preferences.setString('final_dir', settings.finalDir);
    await _preferences.setString('final_tree_uri', settings.finalTreeUri);
    await _preferences.setString('title', settings.title);
    await _preferences.setInt('capacity', settings.capacity);
    await _preferences.setString('custom_bytes', settings.customBytes);
    await _preferences.setBool('advanced', settings.advanced);
    await _preferences.setString('work_dir', settings.workDir);
    await _preferences.setString('iso_prefix', settings.isoPrefix);
    await _preferences.setString('group_track_limit', settings.groupTrackLimit);
    await _preferences.setString('title_mode', settings.titleMode);
    await _preferences.setString('album_limit', settings.albumLimit);
    await _preferences.setBool('keep_tmp', settings.keepTmp);
    await _preferences.setString('audio_mode', settings.audioMode);
    await _preferences.setString('sample_rate', settings.sampleRate);
    await _preferences.setString('bits', settings.bits);
    await _preferences.setBool('dither16', settings.dither16);
    await _preferences.setString('external_dir', settings.externalDir);
    await _preferences.setString(
      'external_staging_path',
      settings.externalStagingPath,
    );
    await _preferences.setString('external_tree_uri', settings.externalTreeUri);
    await _preferences.setBool('menu', settings.menu);
    await _preferences.setBool('stills', settings.stills);
    await _preferences.setString('tracks_per_page', settings.tracksPerPage);
    await _preferences.setString('cover_dim', settings.coverDim);
    await _preferences.setString('norm', settings.norm);
    await _preferences.setString('font_sc', settings.fontSc);
    await _preferences.setString('font_jp', settings.fontJp);
    await _preferences.setString('font_kr', settings.fontKr);
  }
}
