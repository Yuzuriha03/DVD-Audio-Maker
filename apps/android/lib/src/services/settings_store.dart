import 'dart:convert';

import 'package:shared_preferences/shared_preferences.dart';

import '../models/app_settings.dart';
import '../models/profile_catalog.dart';

/// Persists the editable form state with `shared_preferences`.
class SettingsStore {
  SettingsStore(this._preferences);

  final SharedPreferences _preferences;

  static const String _deliveryCleanupPendingKey = 'delivery_cleanup_pending';
  static const String _deliveryCleanupDetailKey = 'delivery_cleanup_detail';

  /// Whether a delivered task still needs the shared finish retried.
  bool loadDeliveryCleanupPending() =>
      _preferences.getBool(_deliveryCleanupPendingKey) ?? false;

  String loadDeliveryCleanupDetail() =>
      _preferences.getString(_deliveryCleanupDetailKey) ?? '';

  Future<void> saveDeliveryCleanupPending(
    bool pending, {
    String detail = '',
  }) async {
    await _preferences.setBool(_deliveryCleanupPendingKey, pending);
    await _preferences.setString(
      _deliveryCleanupDetailKey,
      pending ? detail : '',
    );
  }

  static Future<SettingsStore> open() async {
    return SettingsStore(await SharedPreferences.getInstance());
  }

  AppSettings load() {
    final settings = AppSettings();

    final language = _preferences.getString('language');
    final catalog = ProfileCatalog.instance;
    if (language != null && catalog.languages.contains(language)) {
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
    if (audioMode != null && catalog.audioModes.contains(audioMode)) {
      settings.audioMode = audioMode;
    }
    final sampleRate = _preferences.getString('sample_rate');
    if (sampleRate != null && catalog.sampleRates.contains(sampleRate)) {
      settings.sampleRate = sampleRate;
    }
    final bits = _preferences.getString('bits');
    if (bits != null && catalog.mlpBits.contains(bits)) {
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
    if (norm != null && catalog.menuNorms.contains(norm)) {
      settings.norm = norm;
    }
    // Older Android builds stored the system CJK collection as the preferred
    // menu font. That preference must migrate to empty so the shared runtime
    // font (DvdaNotoCJK) is used exactly like Windows' empty default.
    settings.fontSc = AppSettings.migrateLegacyFont(
      _preferences.getString('font_sc') ?? settings.fontSc,
    );
    settings.fontJp = AppSettings.migrateLegacyFont(
      _preferences.getString('font_jp') ?? settings.fontJp,
    );
    settings.fontKr = AppSettings.migrateLegacyFont(
      _preferences.getString('font_kr') ?? settings.fontKr,
    );

    final extras = _preferences.getString('profile_extras');
    if (extras != null && extras.isNotEmpty) {
      try {
        final decoded = jsonDecode(extras);
        if (decoded is Map) {
          settings.profileExtras = <String, Object>{
            for (final entry in decoded.entries)
              if (entry.value != null)
                entry.key.toString(): entry.value as Object,
          };
        }
      } on FormatException {
        // A corrupt cache must not block startup; the next save rewrites it.
      }
    }

    final capacity = _preferences.getInt('capacity');
    if (capacity != null && capacity >= 0 && capacity <= 2) {
      settings.capacity = capacity;
    }
    return settings;
  }

  Future<void> save(AppSettings settings) async {
    final futures = <Future<bool>>[
      _preferences.setString('language', settings.language),
      _preferences.setString('theme', settings.theme),
      _preferences.setString('source', settings.source),
      _preferences.setString('source_staging_path', settings.sourceStagingPath),
      _preferences.setString('source_tree_uri', settings.sourceTreeUri),
      _preferences.setString('final_dir', settings.finalDir),
      _preferences.setString('final_tree_uri', settings.finalTreeUri),
      _preferences.setString('title', settings.title),
      _preferences.setInt('capacity', settings.capacity),
      _preferences.setString('custom_bytes', settings.customBytes),
      _preferences.setBool('advanced', settings.advanced),
      _preferences.setString('iso_prefix', settings.isoPrefix),
      _preferences.setString('group_track_limit', settings.groupTrackLimit),
      _preferences.setString('title_mode', settings.titleMode),
      _preferences.setString('album_limit', settings.albumLimit),
      _preferences.setBool('keep_tmp', settings.keepTmp),
      _preferences.setString('audio_mode', settings.audioMode),
      _preferences.setString('sample_rate', settings.sampleRate),
      _preferences.setString('bits', settings.bits),
      _preferences.setBool('dither16', settings.dither16),
      _preferences.setString('external_dir', settings.externalDir),
      _preferences.setString(
        'external_staging_path',
        settings.externalStagingPath,
      ),
      _preferences.setString('external_tree_uri', settings.externalTreeUri),
      _preferences.setBool('menu', settings.menu),
      _preferences.setBool('stills', settings.stills),
      _preferences.setString('tracks_per_page', settings.tracksPerPage),
      _preferences.setString('cover_dim', settings.coverDim),
      _preferences.setString('norm', settings.norm),
      _preferences.setString('font_sc', settings.fontSc),
      _preferences.setString('font_jp', settings.fontJp),
      _preferences.setString('font_kr', settings.fontKr),
      _preferences.setString(
        'profile_extras',
        jsonEncode(settings.profileExtras),
      ),
    ];
    await Future.wait(futures);
  }
}
