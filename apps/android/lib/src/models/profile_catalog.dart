import 'dart:convert';

import '../../bridge.dart';

/// Shared profile schema loaded from `dvda_core::profile::gui_options()`.
///
/// Key defaults, allowed sample rates, bit depths, menu standards, languages
/// and the disc-capacity rule all come from Rust; Flutter keeps only widget
/// state and localized labels.
class ProfileCatalog {
  ProfileCatalog._({
    required this.defaults,
    required this.sampleRates,
    required this.mlpBits,
    required this.lpcmBits,
    required this.menuNorms,
    required this.audioModes,
    required this.languages,
    required this.dvd5Bytes,
    required this.dvd9Bytes,
    required this.dvd5Index,
    required this.dvd9Index,
    required this.customIndex,
    required int Function(int capacity, String customBytes) discBytesResolver,
  }) : _discBytes = discBytesResolver;

  static ProfileCatalog? _instance;

  /// Catalog installed by `main()` from the running Rust library.
  static ProfileCatalog get instance {
    final catalog = _instance;
    if (catalog == null) {
      throw StateError('ProfileCatalog is not installed');
    }
    return catalog;
  }

  static void install(ProfileCatalog catalog) => _instance = catalog;

  /// Loads the catalog through the shared Rust profile contract.
  static ProfileCatalog load() => ProfileCatalog.fromJson(
    AppBridge.profileOptions(),
    discBytes: (capacity, custom) =>
        AppBridge.profileDiscBytes(capacity: capacity, customBytes: custom),
  );

  /// Parses the JSON emitted by `dvda_core::profile::gui_options()`.
  factory ProfileCatalog.fromJson(
    String json, {
    int Function(int capacity, String customBytes)? discBytes,
  }) {
    final decoded = jsonDecode(json) as Map<String, dynamic>;
    final defaults = <String, String>{
      for (final entry in (decoded['defaults'] as Map).entries)
        entry.key.toString(): '${entry.value ?? ''}',
    };
    final capacity = decoded['capacity'] as Map<String, dynamic>;
    List<String> strings(String key) =>
        (decoded[key] as List).map((value) => '$value').toList();
    return ProfileCatalog._(
      defaults: defaults,
      sampleRates: strings('sampleRates'),
      mlpBits: strings('mlpBits'),
      lpcmBits: strings('lpcmBits'),
      menuNorms: strings('menuNorms'),
      audioModes: strings('audioModes'),
      languages: strings('languages'),
      dvd5Bytes: (capacity['dvd5'] as num).toInt(),
      dvd9Bytes: (capacity['dvd9'] as num).toInt(),
      dvd5Index: (capacity['dvd5Index'] as num).toInt(),
      dvd9Index: (capacity['dvd9Index'] as num).toInt(),
      customIndex: (capacity['customIndex'] as num).toInt(),
      discBytesResolver:
          discBytes ??
          (index, custom) => index == (capacity['dvd9Index'] as num).toInt()
              ? (capacity['dvd9'] as num).toInt()
              : (capacity['dvd5'] as num).toInt(),
    );
  }

  final Map<String, String> defaults;
  final List<String> sampleRates;
  final List<String> mlpBits;
  final List<String> lpcmBits;
  final List<String> menuNorms;
  final List<String> audioModes;
  final List<String> languages;
  final int dvd5Bytes;
  final int dvd9Bytes;
  final int dvd5Index;
  final int dvd9Index;
  final int customIndex;
  final int Function(int capacity, String customBytes) _discBytes;

  /// Shared capacity rule; never re-implemented in Dart.
  int discBytes(int capacity, String customBytes) =>
      _discBytes(capacity, customBytes.trim());

  String defaultFor(String key) => defaults[key] ?? '';
}
