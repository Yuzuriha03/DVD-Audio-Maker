import 'package:saf/saf.dart';

class SafNativeDocument {
  const SafNativeDocument({
    required this.uri,
    required this.relativePath,
    required this.logicalPath,
    required this.nativePath,
    required this.size,
    required this.lastModified,
  });

  final String uri;
  final String relativePath;
  final String logicalPath;
  final String nativePath;
  final int size;
  final DateTime? lastModified;
}

class SafDocumentCollection {
  const SafDocumentCollection(this.documents, this._descriptors, this._close);

  final List<SafNativeDocument> documents;
  final List<SafOpenFd> _descriptors;
  final Future<void> Function(int fd) _close;

  Map<String, String> toProfileMap() => {
    for (final document in documents) document.logicalPath: document.nativePath,
  };

  Future<void> close() async {
    for (final descriptor in _descriptors) {
      await _close(descriptor.fd);
    }
  }
}

class SafDocumentAdapter {
  SafDocumentAdapter({Saf? saf}) : _saf = saf ?? Saf();

  final Saf _saf;

  Future<SafDocumentCollection> openTree(String root, String treeUri) async {
    final opened = <SafOpenFd>[];
    final documents = <SafNativeDocument>[];
    try {
      await for (final entry in _saf.walk(treeUri)) {
        if (entry.file.isDir) {
          continue;
        }
        final descriptor = await _saf.openFileDescriptor(entry.file.uri, 'r');
        opened.add(descriptor);
        if (opened.length > 512) {
          throw StateError('Too many SAF documents for descriptor mapping');
        }
        final relative = entry.relativePath.replaceAll('\\', '/');
        final logical = root.endsWith('/')
            ? '$root$relative'
            : '$root/$relative';
        documents.add(
          SafNativeDocument(
            uri: entry.file.uri,
            relativePath: relative,
            logicalPath: logical,
            nativePath: descriptor.path,
            size: entry.file.length,
            lastModified: DateTime.fromMillisecondsSinceEpoch(
              entry.file.lastModified,
            ),
          ),
        );
      }
      documents.sort((a, b) => a.logicalPath.compareTo(b.logicalPath));
      return SafDocumentCollection(documents, opened, _saf.closeFileDescriptor);
    } catch (_) {
      for (final descriptor in opened) {
        await _saf.closeFileDescriptor(descriptor.fd);
      }
      rethrow;
    }
  }
}
