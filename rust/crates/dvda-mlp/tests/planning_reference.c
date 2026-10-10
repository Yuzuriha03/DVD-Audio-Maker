#include "mlp_interval.h"
#include "mlp_scale.h"
#include "mlp_predict.h"
#include "mlp_search.h"
#include "mlp_matrix.h"
#include "mlp_restart.h"
#include "mlp_substream.h"
#include <math.h>
#include <string.h>
__declspec(dllexport) int matrix_select_analysis(const double *c,const double *r,unsigned n,unsigned f,size_t s,const unsigned *q,const unsigned *sc,int b,mlp_matrix_primitive *m,unsigned *p,unsigned *bits,int append) {return append ? mlp_matrix_select_append(c,r,n,f,s,q,sc,b,m,p,bits) : mlp_matrix_select(c,r,n,f,s,q,sc,b,m,p,bits);}
__declspec(dllexport) int design_downmix(const double c[2][6],unsigned p,mlp_downmix_design *d) {return mlp_matrix_design_downmix(c,p,d);}
__declspec(dllexport) int render_stereo(const mlp_matrix_primitive *m,unsigned p,const unsigned *q,const int32_t *s,size_t n,unsigned stride,const int32_t *a,const int32_t *b,float *out) {return mlp_matrix_render_stereo(m,p,q,s,n,stride,a,b,out);}
__declspec(dllexport) int downmix_pcm(mlp_downmix_check *state,const float *s,size_t n,unsigned c,const unsigned *sh,int32_t *out) {return mlp_matrix_downmix_pcm(state,s,n,c,sh,out);}
__declspec(dllexport) int downmix_plan(const mlp_matrix_candidate *f,const mlp_matrix_candidate *i,unsigned n,unsigned c,const unsigned *sc,const unsigned *s,const unsigned *q,const unsigned *p,mlp_downmix_plan *out) {return mlp_matrix_downmix_plan(f,i,n,c,sc,s,q,p,out);}
__declspec(dllexport) int process_downmix(mlp_downmix_state *s,const mlp_downmix_plan *p,unsigned c,const unsigned *sh,const unsigned *q,const int32_t *pcm,size_t n,unsigned flags,mlp_downmix_block *out) {return mlp_matrix_process_downmix(s,p,c,sh,q,pcm,n,flags,out);}
__declspec(dllexport) void extended_log(double x,unsigned char *out) {long double y=logl((long double)x);memcpy(out,&y,10);}
__declspec(dllexport) int analysis_init(mlp_matrix_analysis *s,unsigned c) {return mlp_matrix_analysis_init(s,c);}
__declspec(dllexport) int analysis_reset(mlp_matrix_analysis *s) {return mlp_matrix_analysis_reset(s);}
__declspec(dllexport) int analysis_add(mlp_matrix_analysis *s,const int32_t *p,size_t n,unsigned c) {return mlp_matrix_analysis_add(s,p,n,c);}
__declspec(dllexport) int decorrelate(const double *c,const double *r,unsigned n,unsigned f,double h,unsigned *o,double *t,double *d) {return mlp_matrix_decorrelate(c,r,n,f,h,o,t,d);}
__declspec(dllexport) int substream_put(const mlp_substream *s,uint32_t *words,size_t *n) { mlp_bits b; int rc; mlp_bits_init(&b,words,32768); rc=mlp_substream_put(&b,s);*n=b.count;return rc; }
__declspec(dllexport) int restart_put(const mlp_restart *h,int p,uint32_t *words,size_t *n,uint32_t *pending,unsigned *width) { mlp_bits b; int rc; mlp_bits_init(&b,words,16); rc=mlp_restart_put(&b,h,p); *n=b.count;*pending=b.pending;*width=b.pending_bits;return rc; }
__declspec(dllexport) int parameters_put(const mlp_parameters *c,const mlp_parameters *o,int r,int i,uint32_t *words,size_t *n,uint32_t *pending,unsigned *width) { mlp_bits b; int rc; mlp_bits_init(&b,words,2048); b.pending=*pending;b.pending_bits=*width;rc=mlp_parameters_put(&b,c,o,r,i);*n=b.count;*pending=b.pending;*width=b.pending_bits;return rc; }
__declspec(dllexport) int matrix_forward(const mlp_matrix_candidate *c,unsigned n,unsigned h,const unsigned *s,const unsigned *d,mlp_matrix_primitive *o,unsigned *p,unsigned *b,int *g,unsigned *f) { return mlp_matrix_build_forward(c,n,h,s,d,o,p,b,g,f); }
__declspec(dllexport) int matrix_reverse(const mlp_matrix_candidate *c,unsigned h,const int *g,const unsigned *d,mlp_matrix_primitive *o,unsigned *f) { return mlp_matrix_build_reverse(c,h,g,d,o,f); }
__declspec(dllexport) int matrix_dither(mlp_matrix_primitive *m,unsigned p,unsigned t,unsigned e,const unsigned *d,unsigned *q) { return mlp_matrix_add_dither(m,p,t,e,d,q); }
__declspec(dllexport) int matrix_noise(uint32_t *s,size_t n,unsigned q,int32_t *a,int32_t *b) { return mlp_matrix_noise(s,n,q,a,b); }
__declspec(dllexport) int matrix_interval(const mlp_matrix_primitive *m,unsigned *p,unsigned c,const unsigned *q,int32_t *s,size_t n,uint8_t *b,unsigned *k) { return mlp_matrix_apply_interval(m,p,c,q,s,n,b,k); }
__declspec(dllexport) int matrix_blocks(const mlp_matrix_primitive *m,unsigned prefix,unsigned *p,unsigned c,const unsigned q[][6],const unsigned *l,unsigned n,int32_t *s,uint8_t *b,unsigned *k) { return mlp_matrix_apply_suffix_blocks(m,prefix,p,c,q,l,n,s,b,k); }
__declspec(dllexport) int matrix_apply(const mlp_matrix_primitive *m,unsigned p,unsigned c,const unsigned *q,int32_t *s,size_t n,const int32_t *e,const int32_t *f,uint8_t *b,int i,int k) { return mlp_matrix_apply(m,p,c,q,s,n,e,f,b,i,k); }
__declspec(dllexport) int search_correlation(const int32_t *p,const size_t *l,size_t b,unsigned o,double *r) { return mlp_search_correlation_extended(p,l,b,o,r); }
__declspec(dllexport) int search_reflection(const double *r,unsigned o,double *k,double *e) { return mlp_search_reflection(r,o,k,e); }
__declspec(dllexport) int search_evaluate(const double *r,unsigned l,const double *c,unsigned o,unsigned m,double *k,double *s) { return mlp_search_iir_evaluate(r,l,c,o,m,k,s); }
__declspec(dllexport) int search_stable(const double *c,unsigned o) { return mlp_search_iir_stable(c,o); }
__declspec(dllexport) int search_interval(mlp_search_pool *p,const int32_t *s,const size_t *l,size_t n,unsigned a,unsigned b,unsigned g,unsigned f,mlp_search_plan *r) { return mlp_search_interval(p,s,l,n,a,b,g,f,r); }
__declspec(dllexport) int search_fir(const double *r,unsigned o,mlp_wire_filter *f) { return mlp_search_fir(r,o,f); }
__declspec(dllexport) int search_feedback(const int32_t *p,const mlp_wire_filter *a,const mlp_wire_filter *b,unsigned n,int32_t *s,unsigned *q) { return mlp_search_feedback_state(p,a,b,n,s,q); }
__declspec(dllexport) int search_joint(mlp_search_pool *p,const double *r,unsigned l,unsigned b,unsigned a,unsigned n,double v,mlp_wire_filter *f,mlp_wire_filter *g) { return mlp_search_joint(p,r,l,b,a,n,v,f,g); }
__declspec(dllexport) int search_pool_init(mlp_search_pool *p,uint32_t s) { return mlp_search_pool_init(p,s); }
__declspec(dllexport) int search_pool_select(mlp_search_pool *p,const double *r,unsigned l,unsigned b,unsigned a,unsigned n,mlp_wire_filter *f,double *k,double *e) { return mlp_search_pool_select(p,r,l,b,a,n,f,k,e); }
__declspec(dllexport) int search_random(uint32_t *s,unsigned r,unsigned m,unsigned a,mlp_wire_filter *f) { return mlp_search_iir_random(s,r,m,a,f); }
__declspec(dllexport) unsigned search_rand(uint32_t *s) { return mlp_search_rand(s); }
__declspec(dllexport) int predict_block(mlp_predict_filter *a,mlp_predict_filter *b,mlp_predict_state *s,unsigned q,const int32_t *in,size_t n,int32_t *out,unsigned f) { return mlp_predict_block(a,b,s,q,in,n,out,f); }
__declspec(dllexport) int interval_boundary(mlp_boundary_state *s,unsigned c,unsigned p,unsigned n) { return mlp_interval_boundary(s,c,p,n); }
__declspec(dllexport) int interval_find(mlp_interval_slot *s,unsigned w,unsigned *c) { return mlp_interval_find(s,w,c); }
__declspec(dllexport) int scale_analyze(const uint32_t *s,unsigned b,unsigned c,unsigned n,const unsigned *r,mlp_scale_plan *p) { return mlp_scale_analyze(s,b,c,n,r,p); }
