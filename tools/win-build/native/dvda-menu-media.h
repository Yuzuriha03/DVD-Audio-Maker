#ifndef DVDA_MENU_MEDIA_H
#define DVDA_MENU_MEDIA_H

/* Encode one DVD menu frame and, optionally, its 48 kHz stereo soundtrack
 * into a DVD-sized MPEG program stream.  The implementation uses only the
 * source-built FFmpeg libraries; it never starts mpeg2enc, mp2enc or mplex. */
int dvda_menu_create_mpg(const char *y4m_path, const char *wav_path,
                         const char *output_path, const char *norm,
                         const char *aspect);

#endif
