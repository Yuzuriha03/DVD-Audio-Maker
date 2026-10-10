/* Build-time ABI introspection only: no bridge implementation is compiled from C. */
#include <stddef.h>
#include <stdio.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#define T(alias,type) printf("T %s %zu\n",#alias,sizeof(type));
#define F(alias,type,field) printf("F %s %s %zu\n",#alias,#field,offsetof(type,field));
#define K(name) printf("K %s %lld\n",#name,(long long)(name));
int main(void) {
T(Format,AVFormatContext) T(Stream,AVStream) T(Parameters,AVCodecParameters) T(Context,AVCodecContext) T(Frame,AVFrame) T(Packet,AVPacket) T(Layout,AVChannelLayout) T(Io,AVIOContext) T(Codec,AVCodec) T(OutputFormat,AVOutputFormat)
F(Format,AVFormatContext,streams) F(Format,AVFormatContext,nb_streams) F(Format,AVFormatContext,duration) F(Format,AVFormatContext,metadata) F(Format,AVFormatContext,interrupt_callback) F(Format,AVFormatContext,oformat) F(Format,AVFormatContext,pb) F(Format,AVFormatContext,packet_size) F(Format,AVFormatContext,priv_data)
F(Stream,AVStream,codecpar) F(Stream,AVStream,duration) F(Stream,AVStream,time_base) F(Stream,AVStream,metadata) F(Stream,AVStream,disposition) F(Stream,AVStream,attached_pic) F(Stream,AVStream,index)
F(Parameters,AVCodecParameters,codec_type) F(Parameters,AVCodecParameters,codec_id) F(Parameters,AVCodecParameters,sample_rate) F(Parameters,AVCodecParameters,ch_layout) F(Parameters,AVCodecParameters,bits_per_raw_sample) F(Parameters,AVCodecParameters,width) F(Parameters,AVCodecParameters,height)
F(Context,AVCodecContext,sample_fmt) F(Context,AVCodecContext,sample_rate) F(Context,AVCodecContext,bits_per_raw_sample) F(Context,AVCodecContext,time_base) F(Context,AVCodecContext,thread_count) F(Context,AVCodecContext,ch_layout) F(Context,AVCodecContext,flags) F(Context,AVCodecContext,frame_size) F(Context,AVCodecContext,frame_num) F(Context,AVCodecContext,err_recognition) F(Context,AVCodecContext,width) F(Context,AVCodecContext,height) F(Context,AVCodecContext,pix_fmt) F(Context,AVCodecContext,framerate) F(Context,AVCodecContext,sample_aspect_ratio) F(Context,AVCodecContext,bit_rate) F(Context,AVCodecContext,rc_min_rate) F(Context,AVCodecContext,rc_max_rate) F(Context,AVCodecContext,rc_buffer_size) F(Context,AVCodecContext,qmax) F(Context,AVCodecContext,gop_size) F(Context,AVCodecContext,max_b_frames) F(Context,AVCodecContext,profile) F(Context,AVCodecContext,codec_id)
F(Frame,AVFrame,data) F(Frame,AVFrame,extended_data) F(Frame,AVFrame,linesize) F(Frame,AVFrame,nb_samples) F(Frame,AVFrame,format) F(Frame,AVFrame,sample_rate) F(Frame,AVFrame,ch_layout) F(Frame,AVFrame,pts) F(Frame,AVFrame,width) F(Frame,AVFrame,height) F(Frame,AVFrame,pict_type) F(Frame,AVFrame,flags)
F(Packet,AVPacket,stream_index) F(Packet,AVPacket,pts) F(Packet,AVPacket,dts) F(Packet,AVPacket,duration) F(Packet,AVPacket,size) F(Packet,AVPacket,pos) F(Packet,AVPacket,data) F(Packet,AVPacket,flags)
F(Layout,AVChannelLayout,nb_channels) F(Io,AVIOContext,error) F(Codec,AVCodec,id) F(OutputFormat,AVOutputFormat,flags)
K(AV_CODEC_ID_FLAC) K(AV_CODEC_ID_PCM_S16LE) K(AV_CODEC_ID_PCM_S24LE) K(AV_CODEC_ID_PNG) K(AV_CODEC_ID_MPEG2VIDEO) K(AV_CODEC_ID_MP2) K(AV_CODEC_ID_MLP) K(AV_SAMPLE_FMT_S16) K(AV_SAMPLE_FMT_S32) K(AV_SAMPLE_FMT_DBLP) K(AV_PIX_FMT_RGB24) K(AV_PIX_FMT_YUV420P) K(AV_CODEC_FLAG_GLOBAL_HEADER) K(AV_CODEC_FLAG_CLOSED_GOP) K(AVFMT_GLOBALHEADER) K(AVFMT_NOFILE) K(AV_EF_CRCCHECK) K(AV_EF_EXPLODE) K(AV_DISPOSITION_ATTACHED_PIC)
return 0;
}
