@echo off
rem Called inside each launcher's setlocal; explicit component overrides win.
if not defined DVDA_MEDIA_NATIVE_DIR set "DVDA_MEDIA_NATIVE_DIR=%~dp0..\..\build\media-native-shared"
if not defined DVDA_IMAGE_NATIVE_DIR set "DVDA_IMAGE_NATIVE_DIR=%~dp0..\..\build\image-native"
if not defined DVDA_ENCODER_LIBRARY set "DVDA_ENCODER_LIBRARY=%~dp0..\..\native\mlp-encoder\win-x64\mlp_encoder.dll"
