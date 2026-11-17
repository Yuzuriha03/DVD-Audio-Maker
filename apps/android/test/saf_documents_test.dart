import 'package:dvda_maker/src/services/saf_documents.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('descriptor collections preserve logical and native paths', () {
    const document = SafNativeDocument(
      uri: 'content://source/01.flac',
      relativePath: 'Album/01.flac',
      logicalPath: '/storage/Music/Album/01.flac',
      nativePath: '/proc/self/fd/42',
      size: 1,
      lastModified: null,
    );
    expect(document.nativePath, '/proc/self/fd/42');
    expect(document.logicalPath, '/storage/Music/Album/01.flac');
  });
}
