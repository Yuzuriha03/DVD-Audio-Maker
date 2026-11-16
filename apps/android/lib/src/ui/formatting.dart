/// Human-readable decimal capacity, matching the classic DVD5/DVD9 labels.
String formatGigabytes(int bytes) {
  final gigabytes = bytes / 1000000000;
  return '${gigabytes.toStringAsFixed(1)} GB';
}

/// Collapses blank profile values into an em dash for summaries.
String valueOrDash(String value) {
  final trimmed = value.trim();
  return trimmed.isEmpty ? '—' : trimmed;
}

/// Keeps a dropdown usable when a loaded profile holds a value outside the
/// shared allowed list; Rust validation reports the problem when a task starts.
List<String> withCurrent(List<String> options, String current) {
  if (current.isEmpty || options.contains(current)) {
    return options;
  }
  return <String>[...options, current];
}
