# DVD-Audio Maker v1.0

将 FLAC、ALAC/M4A 音乐制作成 DVD-Audio ISO，支持自动分盘、选曲菜单、专辑封面和成品验证。

## 本次更新

- 提供 **MLP 编码**与 **LPCM 编码**：MLP 无损压缩，通常更省空间；LPCM 直接保存未压缩音频。相同采样率、位深、声道及 PCM 输入下，两者音质相同。
- LPCM 当前支持 16 / 24 位，音频码率最高 9.6 Mb/s；20 位以及超出 LPCM 码率限制的多声道格式请选择 MLP。
- 界面、日志和使用说明支持 **中文、English、日本語**。
- 简化制作步骤：检查音源 → 开始制作 → 验证成品，界面不再提供预演模式。
- **导入已有 MLP 文件**适用于使用原版 SurCode MLP（surcodemlp.exe）生成的文件，直接使用已有编码结果，不会再次编码或启动原版程序。请保留对应原始音源及编码参数以便验证。
- 主程序、三语说明及许可证均放在 ZIP 根目录。

## 下载与使用

下载 **DVD-Audio-Maker-v1.0-win-x64.zip**，适用于 **Windows x64**。

1. 安装 [.NET 10 Desktop Runtime（Windows x64）](https://dotnet.microsoft.com/download/dotnet/10.0)。本包不包含 .NET 运行时。
2. 解压 ZIP，运行 `DVD-Audio-Maker.exe`。首次启动会自动准备组件，无需另装 FFmpeg、ImageMagick、eac3to 或 SurCode。
3. 选择音乐和成品目录，设置容量及音频格式，然后检查音源、开始制作并验证成品。

支持 JSON 方案保存和 `config.env` 导入。`config.env.example` 只是可选示例，不会自动加载。详细步骤见包内 README；请保留全部许可与 NOTICE 文件。

## English

Create DVD-Audio ISOs from FLAC and ALAC/M4A music with automatic disc splitting, track menus, album covers and output verification.

Choose **MLP encoding** for lossless compression or **LPCM encoding** for uncompressed audio. Both preserve the same audio quality when the PCM input, sample rate, bit depth and channel layout are identical. LPCM currently supports 16 / 24-bit audio within 9.6 Mb/s; use MLP for 20-bit audio or formats that exceed the LPCM bitrate limit.

The interface, logs and user guides now support **Chinese, English and Japanese**. The workflow is **Check sources → Build discs → Verify output**; preview mode is no longer shown. **Import existing MLP files** can use files encoded by the original SurCode MLP (surcodemlp.exe), without re-encoding or launching that program. Keep the corresponding original audio and encoding settings for verification.

Download **DVD-Audio-Maker-v1.0-win-x64.zip**. Install [.NET 10 Desktop Runtime for Windows x64](https://dotnet.microsoft.com/download/dotnet/10.0), extract the ZIP and run `DVD-Audio-Maker.exe`. The .NET runtime is not bundled. Required components are prepared automatically; no separate FFmpeg, ImageMagick, eac3to or SurCode installation is needed.

The application, guides and licenses are at the ZIP root. JSON profiles and config.env import remain available. The optional config.env.example is not loaded automatically. See README.en.md for instructions and retain all license and NOTICE files.

## 日本語

FLAC・ALAC/M4A の音源から DVD-Audio ISO を作成できます。ディスクの自動分割、選曲メニュー、アルバムアート、作成結果の検証に対応しています。

音声方式に **MLP エンコード**と **LPCM エンコード**を選べます。MLP は可逆圧縮で容量を節約し、LPCM は非圧縮で保存します。PCM 入力、サンプリング周波数、ビット深度、チャンネル構成が同じなら音質は同じです。LPCM は現在 16 / 24 ビット、最大 9.6 Mb/s に対応しています。20 ビットや LPCM のビットレート上限を超える形式には MLP を選んでください。

画面・ログ・使用説明は **中国語・英語・日本語**に対応しました。操作は「音源を確認 → ディスクを作成 → 作成結果を検証」に整理し、画面のプレビューモードを削除しました。**既存の MLP ファイルを取り込む**方式では、オリジナルの SurCode MLP（surcodemlp.exe）で生成したファイルも使用できます。再エンコードやオリジナルのプログラムの起動は行いません。検証のため、対応する元の音源とエンコード設定を保管してください。

**DVD-Audio-Maker-v1.0-win-x64.zip** をダウンロードしてください。[.NET 10 Desktop Runtime（Windows x64）](https://dotnet.microsoft.com/download/dotnet/10.0) をインストールし、ZIP を展開して `DVD-Audio-Maker.exe` を実行します。.NET ランタイムは同梱していません。必要なコンポーネントは初回起動時に準備され、FFmpeg・ImageMagick・eac3to・SurCode を別途インストールする必要はありません。

実行ファイル、説明書、ライセンスは ZIP の直下にあります。JSON 設定の保存と config.env の読み込みにも対応しています。config.env.example は任意の設定例で、自動的には読み込みません。手順は README.ja.md を参照し、ライセンスと NOTICE ファイルを保管してください。
