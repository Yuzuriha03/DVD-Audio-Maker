# ライセンスと法的注意

[简体中文](LICENSING.md) | [English](LICENSING.en.md) | [日本語](LICENSING.ja.md)

> 本書にはプロジェクト初期段階の構成および配布に関する説明が含まれます。現在のリポジトリとリリース内容については、[プロジェクト README](../README.ja.md) および[第三者通知](../tools/win-build/docs/THIRD-PARTY.en.md)を参照してください。過去のワークフローに関する記述は、現在の実行時依存関係を示すものではありません。

本書は、本リポジトリのライセンス状況、第三者コンポーネントの帰属、および MLP エンコード使用時の法的制限について説明します。

---

## 1. 本リポジトリのライセンス：GPL-3.0

**本リポジトリ全体は [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html) で配布されます。** 全文はルートの `LICENSE` を参照してください。

### GPL-3.0 を採用する理由

本プロジェクトの author と LPCM パッキング規則は、改変された
[dvda-author](https://github.com/fabnicol/dvda-author) の実装を Rust へ移植したもので、
上流の帰属表示と GPL ライセンスをそのまま保持しています。現在のリポジトリには次が含まれます。

- `rust/crates/dvda-author`：Rust author、IFO/AOB 生成、ISO writer
- `rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json`：退役した
  ソースの Git リビジョン、ファイルハッシュ、著作権表示、ゴールデンデータの由来

実装範囲、受け入れ記録、過去の変更履歴（`RUST-AUTHOR-MIGRATION.md`、`DVDA-AUTHOR-CHANGES.md`、`DVDA-AUTHOR-DISABLED.md`）はローカルのソース作業コピーにのみ保持され、リポジトリには公開されません。

旧 C ミラーと `docs/dvda-author-changes.patch` は削除済みです。いずれも本リポジトリのコミット
`6c5086127590001c544373783653fe991f0ebaeb` から復元できます。ソースの整理はライセンスおよび帰属の要件を変更しません。

上流 dvda-author のライセンス記録は次のとおりです。

| 項目 | 値 |
|------|-----|
| リポジトリの `COPYING` | **GNU GPL version 3**（2007-06-29） |
| ほとんどのソースファイル冒頭 | "GNU General Public License ... either **version 2** of the License, or (at your option) any later version" |
| プログラム内の宣言文 | "either **version 3** of the License, or (at your option) any later version" |

ソース冒頭は "v2 or later" と記載しているため、GPL-3.0 での利用が可能です。
またプロジェクト自身が GPL-3.0 の `COPYING` を配布しているため、本リポジトリは **GPL-3.0** を採用します。

### 本リポジトリ自身のプログラム

現在の GUI、CLI、author、メディア／画像アダプター、MLP エンコーダーはすべて Rust で実装されています。
リポジトリは統一して GPL-3.0 を採用します。旧 C# および Python のエントリーポイントは Git 履歴に残っています。
現在の配布物はプロセス内 Rust 実装と必要な第三者ライブラリで構成され、その内訳は第三者通知を参照してください。

---

## 2. 第三者コンポーネントの帰属

### dvda-author

```
Copyright Dave Chapman 2005
Copyright Fabrice Nicol 2007-2019
Copyright Lee and Tim Feldkamp 2008-2009
License: GPL-3.0
```

上流コードの著作権は原著作者に帰属します。Rust 移植は対応する帰属表示、ゴールデンデータ、ソース由来記録を保持します。

### FFmpeg

本書で扱う初期ワークフローは、デコードと MLP エンコードにシステムの FFmpeg（8.0.1 で検証）を使用していました。

| ビルドまたはコンポーネント | ライセンス |
|------|-----|
| FFmpeg の既定ビルド | **LGPL v2.1 or later** |
| Ubuntu/Debian または `--enable-gpl` を有効にしたビルド | 一部ファイルが **GPL v2 or later**。ビルド全体を GPL として扱う必要があります |
| **MLP コーデック**（`mlpdec.c` / `mlpenc.c`） | **LGPL v2.1 or later** |

当時検討されたリンク互換性：

- dvda-author（GPL-3.0）と FFmpeg の GPL-2.0-or-later 部分：互換（"or later" により GPL-3.0 へ移行可能）
- MLP コーデック自体は LGPL-2.1+ であり、GPL-3.0 と互換

GPL 部分を完全に避けたい場合は、`--enable-gpl` と `--enable-nonfree` を付けずに
**LGPL-only** の FFmpeg を自分でビルドできます。当時のワークフローが必要としたのは
`libavcodec`、`libavformat`、`libavutil`、`libswresample` と MLP コーデックでした。

---

## 3. MLP エンコードに関する法的注意

**この点には特に注意してください。**

MLP（Meridian Lossless Packing）は **Dolby** が所有する技術です。FFmpeg は自身の MLP エンコーダーを
**experimental（実験的）** と位置付けており、当時のエンコードコマンドでは `-strict -2` が必須でした。

```bash
ffmpeg -i input.flac -c:a mlp -strict -2 output.mlp
#                          ^^^^^^^^^^^^ 指定しない場合は "Experimental feature" と報告されます
```

dvda-author 自身のヘルプにも次の記載があります。

> `--encode` Use this option to encode to MLP audio.
> This option is based on the ffmpeg encoder and **subject to the same legal
> restrictions as those applying to the MLP ffmpeg encoder**.

当時のツールチェーンは dvda-author の `--encode` を使用せず、FFmpeg CLI で事前に MLP をエンコードし、
その MLP を dvda-author に渡してパッケージングしていました。エンコーダーは同一であるため、
同じ警告が当時のワークフローにも当てはまります。

**実務上の注意：**

- お住まいの法域における MLP エンコードの規則を自身で確認してください。
- 本ワークフローと同様に音源を変更せずに保持し、必要に応じて LPCM 方式の採用を検討してください。
- 本リポジトリは技術的な実装を提供するものであり、利用者の法的遵守を保証するものではありません。

---

## 4. 音声コンテンツについて

**本リポジトリには音声、音楽、アートワークのファイルは一切含まれません。**

利用者は自身で音源を用意し、その内容を使用するために必要な権利を有している必要があります。
本ツールチェーンには復号や著作権保護の回避機能は含まれません。入力が破損している場合
（不完全な保護付きダウンロードなど）、デコード検証はその問題を報告して停止し、保護の回避は試みません。

---

## 5. 免責事項

本ソフトウェアは GPL-3.0 の条件で配布され、**いかなる保証も提供しません**。作者は、
ディスク制作の失敗、データ損失、法的紛争を含むがこれらに限らない、本ツールチェーンの使用により
生じたいかなる結果についても責任を負いません。
