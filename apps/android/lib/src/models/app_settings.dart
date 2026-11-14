import 'dart:convert';

/// Default menu font used by the classic Android build.
const String kDefaultFont = '/system/fonts/NotoSansCJK-Regular.ttc';

/// Application-scoped build directory.
///
/// The Android shell keeps the historical application id
/// `com.dvda.maker`, so the Rust core (same UID) can write this path.
const String kDefaultWorkDir = '/data/user/0/com.dvda.maker/files/build';
const String kDefaultTaskOutputDir = '/data/user/0/com.dvda.maker/files/output';

const int kDvd5Bytes = 4707319808;
const int kDvd9Bytes = 8540123136;

const List<String> kSampleRates = <String>[
  '44100',
  '48000',
  '88200',
  '96000',
  '176400',
  '192000',
];

const List<String> kSampleRateLabels = <String>[
  '44.1 kHz',
  '48 kHz',
  '88.2 kHz',
  '96 kHz',
  '176.4 kHz',
  '192 kHz',
];

const List<String> kBitDepths = <String>['16', '20', '24'];

const List<String> kAudioModes = <String>['mlp-batch', 'lpcm', 'external'];

const List<String> kVideoStandards = <String>['pal', 'secam', 'ntsc'];

const List<String> kLanguages = <String>['zh-CN', 'zh-TW', 'en', 'ja', 'ko'];

/// Mutable form state for the profile that is sent to `AppBridge.runProfile`.
class AppSettings {
  String language = 'zh-CN';
  String theme = 'system';

  String source = '/sdcard/Music';
  String sourceStagingPath = '';
  String sourceTreeUri = '';
  String finalDir = '/sdcard/Download/DSH-Go';
  String finalTreeUri = '';
  String title = 'DVD-Audio';
  int capacity = 0;
  String customBytes = '4707319808';

  bool advanced = false;
  String workDir = kDefaultWorkDir;
  String isoPrefix = '';
  String groupTrackLimit = '99';
  String titleMode = 'album';
  String albumLimit = '0';
  bool keepTmp = false;

  String audioMode = 'mlp-batch';
  String sampleRate = '48000';
  String bits = '24';
  bool dither16 = true;
  String externalDir = '';
  String externalStagingPath = '';
  String externalTreeUri = '';

  bool menu = false;
  bool stills = true;
  String tracksPerPage = '12';
  String coverDim = '35';
  String norm = 'pal';
  String fontSc = kDefaultFont;
  String fontJp = '';
  String fontKr = '';

  static String _or(String value, String fallback) {
    return value.trim().isEmpty ? fallback : value;
  }

  static String _allowed(List<String> allowed, String value, String fallback) {
    return allowed.contains(value) ? value : fallback;
  }

  int get discBytes {
    if (capacity == 1) {
      return kDvd9Bytes;
    }
    if (capacity == 2) {
      final parsed = int.tryParse(customBytes.trim());
      if (parsed != null && parsed > 0) {
        return parsed;
      }
      return kDvd5Bytes;
    }
    return kDvd5Bytes;
  }

  String get capacityOption {
    if (capacity == 1) {
      return 'dvd9';
    }
    if (capacity == 2) {
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

  String get effectiveWorkDir => kDefaultWorkDir;

  Map<String, String> toProfileValues() {
    final font = fontSc.trim().isEmpty ? (menu ? kDefaultFont : '') : fontSc;
    return <String, String>{
      'DVDA_SRC': source,
      'DVDA_FINAL_DIR': finalDir,
      'DVDA_TITLE': title,
      'DVDA_DISC_BYTES': discBytes.toString(),
      'DVDA_BUILD_DIR': effectiveWorkDir,
      'DVDA_ISO_PREFIX': isoPrefix,
      'DVDA_GROUP_TRACK_LIMIT': _or(groupTrackLimit, '99'),
      'DVDA_MLP_SOURCE': audioMode,
      'DVDA_MLP_SAMPLE_RATE': sampleRate,
      'DVDA_MLP_BITS': bits,
      'DVDA_DITHER_16': dither16 ? 'on' : 'off',
      'DVDA_MLP_EXTERNAL_DIR': externalDir,
      'DVDA_MENU': menu ? 'on' : 'off',
      'DVDA_MENU_TRACKS_PER_PAGE': _or(tracksPerPage, '12'),
      'DVDA_MENU_STILLPICS': stills ? 'on' : 'off',
      'DVDA_MENU_COVER_DIM': _or(coverDim, '35'),
      'DVDA_MENU_NORM': norm,
      'DVDA_MENU_FONT': font,
      'DVDA_MENU_FONT_JP': fontJp,
      'DVDA_MENU_FONT_KR': fontKr,
      'DVDA_KEEP_TMP': 'off',
      'DVDA_KEEP_INTERMEDIATE': 'off',
      'DVDA_TITLE_MODE': _or(titleMode, 'album'),
      'DVDA_ALBUM_LIMIT': _or(albumLimit, '0'),
      'DVDA_MENU_INDEX_MIN_ALBUMS': '4',
      'DVDA_RESUME': 'on',
    };
  }

  Map<String, Object> toProfileJson() {
    return <String, Object>{
      'Version': 1,
      'Language': language,
      'Values': toProfileValues(),
    };
  }

  String toProfileJsonString() =>
      JsonEncoder.withIndent('  ').convert(toProfileJson());

  /// Applies a desktop-compatible profile document.
  ///
  /// Throws [FormatException] when the document is not a profile object.
  void applyProfileJson(String text) {
    final dynamic decoded = jsonDecode(text);
    if (decoded is! Map) {
      throw const FormatException('Profile must be a JSON object');
    }
    final root = Map<String, dynamic>.from(decoded);
    final language = root['Language'];
    if (language is String && kLanguages.contains(language)) {
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

    source = textValue('DVDA_SRC', source);
    finalDir = textValue('DVDA_FINAL_DIR', finalDir);
    title = textValue('DVDA_TITLE', title);
    isoPrefix = textValue('DVDA_ISO_PREFIX', isoPrefix);
    groupTrackLimit = textValue('DVDA_GROUP_TRACK_LIMIT', groupTrackLimit);
    titleMode = textValue('DVDA_TITLE_MODE', titleMode);
    albumLimit = textValue('DVDA_ALBUM_LIMIT', albumLimit);
    workDir = kDefaultWorkDir;
    keepTmp = false;

    audioMode = _allowed(
      kAudioModes,
      textValue('DVDA_MLP_SOURCE', audioMode),
      audioMode,
    );
    sampleRate = _allowed(
      kSampleRates,
      textValue('DVDA_MLP_SAMPLE_RATE', sampleRate),
      sampleRate,
    );
    bits = _allowed(kBitDepths, textValue('DVDA_MLP_BITS', bits), bits);
    dither16 = onOff('DVDA_DITHER_16', dither16);
    externalDir = textValue('DVDA_MLP_EXTERNAL_DIR', externalDir);

    menu = onOff('DVDA_MENU', menu);
    stills = onOff('DVDA_MENU_STILLPICS', stills);
    tracksPerPage = textValue('DVDA_MENU_TRACKS_PER_PAGE', tracksPerPage);
    coverDim = textValue('DVDA_MENU_COVER_DIM', coverDim);
    norm = _allowed(kVideoStandards, textValue('DVDA_MENU_NORM', norm), norm);
    fontSc = textValue('DVDA_MENU_FONT', fontSc);
    fontJp = textValue('DVDA_MENU_FONT_JP', fontJp);
    fontKr = textValue('DVDA_MENU_FONT_KR', fontKr);

    if (values.containsKey('DVDA_DISC_BYTES')) {
      final raw = textValue('DVDA_DISC_BYTES', '');
      if (raw.isEmpty || raw == kDvd5Bytes.toString()) {
        capacity = 0;
      } else if (raw == kDvd9Bytes.toString()) {
        capacity = 1;
      } else {
        capacity = 2;
        customBytes = raw;
      }
    }
    sourceStagingPath = '';
    sourceTreeUri = '';
    finalTreeUri = '';
    externalStagingPath = '';
    externalTreeUri = '';
  }
}
