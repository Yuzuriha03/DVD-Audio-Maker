# Rust Regression Fixtures

alac-broken.m4a and alac-valid.m4a are 4,678-byte synthetic ISO BMFF
containers made for the ALAC repair tests. They contain two uncompressed
512-sample, stereo, 16-bit ALAC access units at 48 kHz. The broken input has a
zero three-bit frame END marker; the expected file differs only by those marker
bits. No third-party media is included.

| File | SHA-256 |
| --- | --- |
| alac-broken.m4a | 1521B66D35B759229588E16C2E21C86A99FC8294C4C5F9C72CEC467F7AC38245 |
| alac-valid.m4a | 4DBFDBB55EEB57C2A7AC97E66413B39DB4AF1922E5BC7C7BE7DCDF2E6B6A3E23 |

The independent test decodes the repaired fixture with the pinned source-built
media component and checks its exact sample count and full output bytes.
