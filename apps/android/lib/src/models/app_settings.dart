import 'dart:convert';

import 'profile_catalog.dart';

/// Application-scoped build directory.
///
/// The Android shell keeps the historical application id `com.dvda.maker`, so
/// the Rust core (same UID) can write this path. These are platform paths, not
/// profile rules.
const String kDefaultAppFilesDir = '/data/user/0/com.dvda.maker/files';
const String kDefaultWorkDir = '$kDefaultAppFilesDir/build';

/// Mutable form state for the profile sent to `AppBridge.runProfile`.
///
/// Key defaults, allowed values, capacity bytes and validation live in Rust
/// (`dvda_core::profile`); this class only holds what the widgets edit.
class AppSettings {
  AppSettings({ProfileCatalog? catalog})
    : catalog = catalog ?? ProfileCatalog.instance {
    _applyCatalogDefaults();
  }

  final ProfileCatalog catalog;

  /// Profile keys the widgets own; every other imported key is preserved
  /// verbatim so Android profiles keep unknown/plugin keys like Windows.
  static const Set<String> guiProfileKeys = <String>{
    'DVDA_SRC',
    'DVDA_FINAL_DIR',
    'DVDA_TITLE',
    'DVDA_DISC_BYTES',
    'DVDA_BUILD_DIR',
    'DVDA_ISO_PREFIX',
    'DVDA_GROUP_TRACK_LIMIT',
    'DVDA_MLP_SOURCE',
    'DVDA_MLP_SAMPLE_RATE',
    'DVDA_MLP_BITS',
    'DVDA_DITHER_16',
    'DVDA_MLP_EXTERNAL_DIR',
    'DVDA_MENU',
    'DVDA_MENU_TRACKS_PER_PAGE',
    'DVDA_MENU_STILLPICS',
    'DVDA_MENU_COVER_DIM',
    'DVDA_MENU_NORM',
    'DVDA_MENU_FONT',
    'DVDA_MENU_FONT_JP',
    'DVDA_MENU_FONT_KR',
    'DVDA_KEEP_TMP',
    'DVDA_KEEP_INTERMEDIATE',
    'DVDA_TITLE_MODE',
    'DVDA_ALBUM_LIMIT',
    'DVDA_RESUME',
    'DVDA_MENU_INDEX_MIN_ALBUMS',
    // Session-only user-visible SAF paths; never written to the saved profile.
    'DVDA_SRC_DISPLAY_DIR',
    'DVDA_FINAL_DISPLAY_DIR',
  };

  /// Imported profile values for keys no widget owns (plugin keys included).
  ///
  /// Mirrors the desktop profile base: the GUI writes its own controls over
  /// this map instead of rebuilding the profile from scratch.
  Map<String, Object> profileExtras = <String, Object>{};

  String language = 'zh-CN';
  String theme = 'system';

  String source = '';
  String sourceStagingPath = '';
  String sourceTreeUri = '';
  String finalDir = '';
  String finalTreeUri = '';
  String title = '';
  int capacity = 0;
  String customBytes = '';

  bool advanced = false;
  String isoPrefix = '';
  String groupTrackLimit = '';
  String titleMode = '';
  String albumLimit = '';
  bool keepTmp = false;

  String audioMode = '';
  String sampleRate = '';
  String bits = '';
  bool dither16 = true;
  String externalDir = '';
  String externalStagingPath = '';
  String externalTreeUri = '';

  bool menu = false;
  bool stills = true;
  String tracksPerPage = '';
  String coverDim = '';
  String norm = '';
  String fontSc = '';
  String fontJp = '';
  String fontKr = '';

  void _applyCatalogDefaults() {
    final defaults = catalog.defaults;
    String value(String key, [String fallback = '']) =>
        defaults[key] ?? fallback;
    bool flag(String key, bool fallback) {
      final raw = defaults[key];
      if (raw == null || raw.isEmpty) {
        return fallback;
      }
      return raw == 'on';
    }

    // Android platform branch: every user folder starts empty. Without a
    // Storage Access Framework grant the app has no permission for /sdcard
    // paths, so a task must ask the user to pick source/output folders instead
    // of pre-filling a path it cannot read or write.
    source = value('DVDA_SRC');
    finalDir = value('DVDA_FINAL_DIR');
    title = value('DVDA_TITLE');
    isoPrefix = value('DVDA_ISO_PREFIX');
    groupTrackLimit = value('DVDA_GROUP_TRACK_LIMIT');
    titleMode = value('DVDA_TITLE_MODE', 'album');
    albumLimit = value('DVDA_ALBUM_LIMIT', '0');
    audioMode = value('DVDA_MLP_SOURCE');
    sampleRate = value('DVDA_MLP_SAMPLE_RATE');
    bits = value('DVDA_MLP_BITS');
    dither16 = flag('DVDA_DITHER_16', true);
    externalDir = value('DVDA_MLP_EXTERNAL_DIR');
    menu = flag('DVDA_MENU', false);
    stills = flag('DVDA_MENU_STILLPICS', true);
    tracksPerPage = value('DVDA_MENU_TRACKS_PER_PAGE');
    coverDim = value('DVDA_MENU_COVER_DIM');
    norm = value('DVDA_MENU_NORM');
    fontSc = value('DVDA_MENU_FONT');
    fontJp = value('DVDA_MENU_FONT_JP');
    fontKr = value('DVDA_MENU_FONT_KR');
    final storedBytes = int.tryParse(value('DVDA_DISC_BYTES')) ?? 0;
    capacity = catalogIndex(storedBytes);
    customBytes = storedBytes > 0
        ? storedBytes.toString()
        : catalog.dvd5Bytes.toString();
  }

  int catalogIndex(int bytes) {
    if (bytes == catalog.dvd9Bytes) {
      return catalog.dvd9Index;
    }
    if (bytes == catalog.dvd5Bytes || bytes <= 0) {
      return catalog.dvd5Index;
    }
    return catalog.customIndex;
  }

  static String _or(String value, String fallback) {
    return value.trim().isEmpty ? fallback : value.trim();
  }

  /// Legacy Android preference that pointed at the system CJK collection.
  ///
  /// It is migrated to empty so the shared runtime font collection
  /// (DvdaNotoCJK-Regular.ttc) is resolved by the same rules as Windows.
  static const String legacySystemFont =
      '/system/fonts/NotoSansCJK-Regular.ttc';

  static String migrateLegacyFont(String value) {
    return value.trim() == legacySystemFont ? '' : value;
  }

  static String portableFont(String value, String fallback) {
    final font = value.trim();
    if (font == legacySystemFont) {
      return '';
    }
    final windowsPath = RegExp(r'^[A-Za-z]:[\\/]').hasMatch(font);
    final publicAndroidPath =
        font.startsWith('/sdcard/') || font.startsWith('/storage/');
    final stalePrivatePath =
        font.startsWith('/data/user/') || font.startsWith('/data/data/');
    if (windowsPath || publicAndroidPath || stalePrivatePath) {
      return fallback;
    }
    return font;
  }

  int get discBytes => catalog.discBytes(capacity, customBytes);

  String get capacityOption {
    if (capacity == catalog.dvd9Index) {
      return 'dvd9';
    }
    if (capacity == catalog.customIndex) {
      return 'custom';
    }
    return 'dvd5';
  }

  int get coverDimValue {
    final parsed = int.tryParse(coverDim.trim());
    if (parsed == null) {
      return 35;
    }
    if (parsed < 0) {
      return 0;
    }
    if (parsed > 100) {
      return 100;
    }
    return parsed;
  }

  List<String> get sampleRates => catalog.sampleRates;
  List<String> get mlpBits => catalog.mlpBits;
  List<String> get lpcmBits => catalog.lpcmBits;
  List<String> get audioModes => catalog.audioModes;
  List<String> get videoStandards => catalog.menuNorms;
  List<String> get languages => catalog.languages;
  List<String> get bitDepthLabels =>
      catalog.mlpBits.map((bits) => '$bits bit').toList();
  List<String> get lpcmBitDepthLabels =>
      catalog.lpcmBits.map((bits) => '$bits bit').toList();
  List<String> get sampleRateLabels => catalog.sampleRates.map(_khz).toList();

  static String _khz(String rate) {
    final value = int.tryParse(rate);
    if (value == null) {
      return '$rate Hz';
    }
    final kilo = value / 1000;
    final text = kilo == kilo.roundToDouble()
        ? kilo.toStringAsFixed(0)
        : kilo.toStringAsFixed(1);
    return '$text kHz';
  }

  String appFilesDir = kDefaultAppFilesDir;
  String appWorkDir = kDefaultWorkDir;

  bool sourceMirrorDirty = false;
  bool externalMirrorDirty = false;

  String get effectiveWorkDir => appWorkDir;

  Map<String, Object> toProfileValues() {
    // Desktop parity: start from the shared catalog defaults plus the imported
    // profile base, then let the widgets override exactly the keys they own.
    final values = <String, Object>{...catalog.defaults, ...profileExtras};
    values['DVDA_SRC'] = source;
    values['DVDA_FINAL_DIR'] = finalDir;
    values['DVDA_TITLE'] = title;
    values['DVDA_DISC_BYTES'] = discBytes.toString();
    values['DVDA_BUILD_DIR'] = effectiveWorkDir;
    values['DVDA_ISO_PREFIX'] = isoPrefix;
    values['DVDA_GROUP_TRACK_LIMIT'] = _or(
      groupTrackLimit,
      catalog.defaultFor('DVDA_GROUP_TRACK_LIMIT'),
    );
    values['DVDA_MLP_SOURCE'] = audioMode;
    values['DVDA_MLP_SAMPLE_RATE'] = sampleRate;
    values['DVDA_MLP_BITS'] = bits;
    values['DVDA_DITHER_16'] = dither16 ? 'on' : 'off';
    values['DVDA_MLP_EXTERNAL_DIR'] = externalDir;
    values['DVDA_MENU'] = menu ? 'on' : 'off';
    values['DVDA_MENU_TRACKS_PER_PAGE'] = _or(
      tracksPerPage,
      catalog.defaultFor('DVDA_MENU_TRACKS_PER_PAGE'),
    );
    values['DVDA_MENU_STILLPICS'] = stills ? 'on' : 'off';
    values['DVDA_MENU_COVER_DIM'] = _or(
      coverDim,
      catalog.defaultFor('DVDA_MENU_COVER_DIM'),
    );
    values['DVDA_MENU_NORM'] = norm;
    values['DVDA_MENU_FONT'] = fontSc;
    values['DVDA_MENU_FONT_JP'] = fontJp;
    values['DVDA_MENU_FONT_KR'] = fontKr;
    values['DVDA_KEEP_TMP'] = keepTmp ? 'on' : 'off';
    values['DVDA_KEEP_INTERMEDIATE'] = keepTmp ? 'on' : 'off';
    values['DVDA_TITLE_MODE'] = _or(
      titleMode,
      catalog.defaultFor('DVDA_TITLE_MODE'),
    );
    values['DVDA_ALBUM_LIMIT'] = _or(
      albumLimit,
      catalog.defaultFor('DVDA_ALBUM_LIMIT'),
    );
    // Neither GUI exposes these; the desktop profile writer always forces the
    // shared defaults so an imported profile cannot fork them here.
    values['DVDA_RESUME'] = 'on';
    values['DVDA_MENU_INDEX_MIN_ALBUMS'] = '4';
    return values;
  }

  Map<String, Object> toProfileJson() {
    return <String, Object>{
      'Version': 1,
      'Language': language,
      'Values': toProfileValues(),
    };
  }

  /// Applies a desktop-compatible profile document.
  ///
  /// Throws [FormatException] when the document is not a profile object.
  /// Values are not coerced: invalid combinations are reported by the shared
  /// Rust validation when a task starts, never silently rewritten here.
  void applyProfileJson(String text) {
    final dynamic decoded = jsonDecode(text);
    if (decoded is! Map) {
      throw const FormatException('Profile must be a JSON object');
    }
    final root = Map<String, dynamic>.from(decoded);
    final language = root['Language'];
    if (language is String && catalog.languages.contains(language)) {
      this.language = language;
    }
    final dynamic rawValues = root['Values'];
    if (rawValues is! Map) {
      throw const FormatException('Profile Values must be a JSON object');
    }
    final values = Map<String, dynamic>.from(rawValues);

    String textValue(String key, String fallback) {
      final value = values[key];
      if (value is String) {
        return value;
      }
      if (value is num) {
        return value.toString();
      }
      if (value is bool) {
        return value ? 'on' : 'off';
      }
      return fallback;
    }

    bool onOff(String key, bool fallback) {
      final value = values[key];
      if (value is String) {
        return value == 'on';
      }
      if (value is bool) {
        return value;
      }
      return fallback;
    }

    final defaults = catalog.defaults;
    String defaultFor(String key, [String fallback = '']) =>
        defaults[key] ?? fallback;

    source = textValue('DVDA_SRC', defaultFor('DVDA_SRC'));
    finalDir = textValue('DVDA_FINAL_DIR', defaultFor('DVDA_FINAL_DIR'));
    title = textValue('DVDA_TITLE', defaultFor('DVDA_TITLE'));
    isoPrefix = textValue('DVDA_ISO_PREFIX', defaultFor('DVDA_ISO_PREFIX'));
    groupTrackLimit = textValue(
      'DVDA_GROUP_TRACK_LIMIT',
      defaultFor('DVDA_GROUP_TRACK_LIMIT'),
    );
    titleMode = textValue(
      'DVDA_TITLE_MODE',
      defaultFor('DVDA_TITLE_MODE', 'album'),
    );
    albumLimit = textValue(
      'DVDA_ALBUM_LIMIT',
      defaultFor('DVDA_ALBUM_LIMIT', '0'),
    );
    keepTmp = onOff('DVDA_KEEP_TMP', false);

    audioMode = textValue('DVDA_MLP_SOURCE', defaultFor('DVDA_MLP_SOURCE'));
    sampleRate = textValue(
      'DVDA_MLP_SAMPLE_RATE',
      defaultFor('DVDA_MLP_SAMPLE_RATE'),
    );
    bits = textValue('DVDA_MLP_BITS', defaultFor('DVDA_MLP_BITS'));
    dither16 = onOff('DVDA_DITHER_16', true);
    externalDir = textValue(
      'DVDA_MLP_EXTERNAL_DIR',
      defaultFor('DVDA_MLP_EXTERNAL_DIR'),
    );

    menu = onOff('DVDA_MENU', false);
    stills = onOff('DVDA_MENU_STILLPICS', true);
    tracksPerPage = textValue(
      'DVDA_MENU_TRACKS_PER_PAGE',
      defaultFor('DVDA_MENU_TRACKS_PER_PAGE'),
    );
    coverDim = textValue(
      'DVDA_MENU_COVER_DIM',
      defaultFor('DVDA_MENU_COVER_DIM'),
    );
    norm = textValue('DVDA_MENU_NORM', defaultFor('DVDA_MENU_NORM'));
    fontSc = portableFont(textValue('DVDA_MENU_FONT', fontSc), fontSc);
    fontJp = portableFont(textValue('DVDA_MENU_FONT_JP', fontJp), fontJp);
    fontKr = portableFont(textValue('DVDA_MENU_FONT_KR', fontKr), fontKr);

    if (values.containsKey('DVDA_DISC_BYTES')) {
      final raw = textValue('DVDA_DISC_BYTES', '');
      final parsed = int.tryParse(raw) ?? 0;
      capacity = catalogIndex(parsed);
      if (capacity == catalog.customIndex) {
        customBytes = raw;
      }
    }
    // Preserve every key no widget owns (including plugin keys) verbatim,
    // exactly like the desktop profile base; the widgets win for their own keys.
    profileExtras = <String, Object>{
      for (final entry in values.entries)
        if (!guiProfileKeys.contains(entry.key) && entry.value != null)
          entry.key: entry.value as Object,
    };
    sourceStagingPath = '';
    sourceTreeUri = '';
    finalTreeUri = '';
    externalStagingPath = '';
    externalTreeUri = '';
  }
}
