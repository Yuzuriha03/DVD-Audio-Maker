import 'dart:convert';

import 'package:dvda_maker/src/services/app_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('TaskOutcome.fromJson', () {
    test('parses success, diagnostics, log and progress', () {
      final outcome = TaskOutcome.fromJson(<String, dynamic>{
        'Ok': true,
        'Message': 'build ok',
        'Diagnostics': <dynamic>[
          <String, dynamic>{'Code': 'E1', 'Message': 'first issue'},
          <String, dynamic>{'Code': 'E2', 'Message': 'second issue'},
          'raw diagnostic',
        ],
        'Log': 'line one\nline two',
        'ProgressDone': 5,
        'ProgressTotal': 10,
      });

      expect(outcome.ok, isTrue);
      expect(outcome.message, 'build ok');
      expect(outcome.log, 'line one\nline two');
      expect(outcome.done, 5);
      expect(outcome.total, 10);
      expect(outcome.unfinished, isFalse);
      expect(outcome.diagnostics, hasLength(3));
      expect(outcome.diagnostics[0].code, 'E1');
      expect(outcome.diagnostics[0].message, 'first issue');
      expect(outcome.diagnostics[1].code, 'E2');
      expect(outcome.diagnostics[1].message, 'second issue');
      expect(outcome.diagnostics[2].code, '');
      expect(outcome.diagnostics[2].message, 'raw diagnostic');
    });

    test('parses the unfinished status payload', () {
      final unfinished = TaskOutcome.fromJson(<String, dynamic>{
        'Ok': true,
        'Unfinished': true,
      });
      expect(unfinished.ok, isTrue);
      expect(unfinished.unfinished, isTrue);
    });

    test('tolerates missing fields', () {
      final outcome = TaskOutcome.fromJson(<String, dynamic>{});
      expect(outcome.ok, isFalse);
      expect(outcome.message, '');
      expect(outcome.log, '');
      expect(outcome.done, 0);
      expect(outcome.total, 0);
      expect(outcome.diagnostics, isEmpty);
      expect(outcome.unfinished, isFalse);
    });

    test('accepts numeric strings, doubles and non-string messages', () {
      final outcome = TaskOutcome.fromJson(<String, dynamic>{
        'Ok': 'true',
        'Message': 42,
        'Diagnostics': 'not-a-list',
        'ProgressDone': '7',
        'ProgressTotal': 9.0,
      });
      expect(outcome.ok, isFalse);
      expect(outcome.message, '42');
      expect(outcome.diagnostics, isEmpty);
      expect(outcome.done, 7);
      expect(outcome.total, 9);
    });

    test('keeps null and code-less diagnostics readable', () {
      final outcome = TaskOutcome.fromJson(<String, dynamic>{
        'Ok': false,
        'Diagnostics': <dynamic>[
          null,
          <String, dynamic>{'Message': 'no code'},
        ],
      });
      expect(outcome.diagnostics, hasLength(1));
      expect(outcome.diagnostics.single.code, '');
      expect(outcome.diagnostics.single.message, 'no code');
    });
  });

  group('TaskOutcome.parse', () {
    test('parses the JSON returned by the bridge', () {
      final raw = jsonEncode(<String, dynamic>{
        'Ok': false,
        'Message': 'verify failed',
        'Diagnostics': <dynamic>[
          <String, dynamic>{'Code': 'X1', 'Message': 'bad sector'},
        ],
        'Log': 'tail line',
        'ProgressDone': 2,
        'ProgressTotal': 4,
      });
      final outcome = TaskOutcome.parse(raw);

      expect(outcome.ok, isFalse);
      expect(outcome.message, 'verify failed');
      expect(outcome.log, 'tail line');
      expect(outcome.done, 2);
      expect(outcome.total, 4);
      expect(outcome.diagnostics, hasLength(1));
      expect(outcome.diagnostics.single.code, 'X1');
      expect(outcome.diagnostics.single.message, 'bad sector');
    });

    test('returns a readable failure for malformed JSON', () {
      final outcome = TaskOutcome.parse('not json');
      expect(outcome.ok, isFalse);
      expect(outcome.message, 'not json');
      expect(outcome.log, '');
      expect(outcome.done, 0);
      expect(outcome.total, 0);
      expect(outcome.diagnostics, isEmpty);
    });

    test('returns a readable failure for non-object JSON', () {
      final outcome = TaskOutcome.parse('[1,2,3]');
      expect(outcome.ok, isFalse);
      expect(outcome.message, '[1,2,3]');
      expect(outcome.diagnostics, isEmpty);
    });

    test('failure factory exposes the message and empty progress', () {
      final outcome = TaskOutcome.failure('boom');
      expect(outcome.ok, isFalse);
      expect(outcome.message, 'boom');
      expect(outcome.log, '');
      expect(outcome.done, 0);
      expect(outcome.total, 0);
      expect(outcome.diagnostics, isEmpty);
    });
  });
}
