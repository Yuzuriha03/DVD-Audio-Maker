@echo off
setlocal
if not defined MLP_CC set "MLP_CC=i686-w64-mingw32-gcc"
pushd "%~dp0source" || exit /b 1
"%MLP_CC%" -std=c11 -Wall -Wextra -Werror -O2 -static-libgcc -DMLP_TWO_STREAM_EMBEDDED encode_file.c encode_two_stream.c mlp_bits.c mlp_cost.c mlp_entropy.c mlp_format.c mlp_interval.c mlp_matrix.c mlp_output_queue.c mlp_output_timing.c mlp_parameters.c mlp_pcm.c mlp_predict.c mlp_rate.c mlp_restart.c mlp_scale.c mlp_search.c mlp_select.c mlp_stamp.c mlp_substream.c -lm -o "..\win-x86\mlp_encode.rebuilt.exe"
set "MLP_BUILD_RESULT=%ERRORLEVEL%"
popd
exit /b %MLP_BUILD_RESULT%
