#ifndef DVDA_MENU_MEDIA_H
#define DVDA_MENU_MEDIA_H

/* Encode one DVD menu frame and, optionally, its 48 kHz stereo soundtrack
 * into a DVD-sized program stream using source-built FFmpeg in process.
 * All streams end the MPEG-2 video sequence explicitly; still_picture also
 * writes a standalone B9 sector. */
int dvda_menu_create_mpg(const char *y4m_path, const char *wav_path,
                         const char *output_path, const char *norm,
                         const char *aspect, int still_picture);

#endif
