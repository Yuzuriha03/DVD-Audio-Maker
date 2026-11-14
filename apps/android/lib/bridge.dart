import 'src/rust/api.dart' as rust_api;
import 'src/rust/frb_generated.dart';

class BridgeProgress {
  const BridgeProgress({
    required this.done,
    required this.total,
    required this.text,
    required this.log,
  });

  final int done;
  final int total;
  final String text;
  final String log;
}

abstract final class AppBridge {
  static Future<void> init() => RustLib.init();

  static Future<String> runProfile(String profileJson, String action) {
    return rust_api.runProfile(profileJson: profileJson, action: action);
  }

  static BridgeProgress progress() {
    final value = rust_api.progressSnapshot();
    return BridgeProgress(
      done: value.done.toInt(),
      total: value.total.toInt(),
      text: value.text,
      log: value.log,
    );
  }

  static void cancel() => rust_api.cancel();
}
