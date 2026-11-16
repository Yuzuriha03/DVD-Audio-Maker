import 'package:dvda_maker/src/models/profile_catalog.dart';

/// Test-only mirror of `dvda_core::profile::gui_options()`.
///
/// Flutter tests cannot call the Rust library, so the shared contract is
/// represented here as data. The Rust side owns the production values.
ProfileCatalog testCatalog() => ProfileCatalog.fromJson(
  testCatalogJson,
  discBytes: (capacity, custom) {
    if (capacity == 1) {
      return 8540123136;
    }
    if (capacity == 2) {
      final parsed = int.tryParse(custom.trim());
      return parsed != null && parsed > 0 ? parsed : 4707319808;
    }
    return 4707319808;
  },
);

const String testCatalogJson = '''
{
  "defaults": {
    "DVDA_SRC": "",
    "DVDA_FINAL_DIR": "",
    "DVDA_TITLE": "DVD-Audio",
    "DVDA_ISO_PREFIX": "",
    "DVDA_GROUP_TRACK_LIMIT": "99",
    "DVDA_DISC_BYTES": "",
    "DVDA_MLP_SOURCE": "mlp-batch",
    "DVDA_MLP_EXTERNAL_DIR": "",
    "DVDA_MLP_METADATA_CONTEXT": "",
    "DVDA_MLP_SAMPLE_RATE": "48000",
    "DVDA_MLP_BITS": "24",
    "DVDA_MENU": "off",
    "DVDA_MENU_TRACKS_PER_PAGE": "12",
    "DVDA_MENU_INDEX_MIN_ALBUMS": "4",
    "DVDA_MENU_STILLPICS": "on",
    "DVDA_MENU_COVER_DIM": "35",
    "DVDA_MENU_NORM": "pal",
    "DVDA_MENU_FONT": "",
    "DVDA_MENU_FONT_JP": "",
    "DVDA_MENU_FONT_KR": "",
    "DVDA_AUTHOR_SRC": "",
    "DVDA_RESUME": "on",
    "DVDA_LOSS_ERROR_S": "0.05",
    "DVDA_LOSS_WARN_S": "0.005",
    "DVDA_BUILD_DIR": "/data/user/0/com.dvda.maker/files/build",
    "DVDA_TITLE_MODE": "album",
    "DVDA_ALBUM_LIMIT": "0",
    "DVDA_DITHER_16": "on",
    "DVDA_KEEP_TMP": "off",
    "DVDA_KEEP_INTERMEDIATE": "off"
  },
  "sampleRates": [44100, 48000, 88200, 96000, 176400, 192000],
  "mlpBits": [16, 20, 24],
  "lpcmBits": [16, 24],
  "menuNorms": ["pal", "secam", "ntsc"],
  "audioModes": ["mlp-batch", "lpcm", "external"],
  "languages": ["zh-CN", "zh-TW", "en", "ja", "ko"],
  "capacity": {
    "dvd5": 4707319808,
    "dvd9": 8540123136,
    "dvd5Index": 0,
    "dvd9Index": 1,
    "customIndex": 2
  }
}
''';
