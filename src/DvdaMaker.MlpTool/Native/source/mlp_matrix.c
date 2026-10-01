#include "mlp_matrix.h"
#include <math.h>
#include <stdlib.h>
#include <string.h>

int mlp_matrix_design_downmix(const double coefficients[2][6],
    unsigned precision,mlp_downmix_design *output)
{
    mlp_downmix_design result={0};
    double forward[2][6]={{0}},inverse[2][2]={{0}},basis[2][6]={{0}};
    unsigned r,p,j,pivot,maximum=0; double grid,step;
    if(!coefficients || !output || precision>14) return -1;
    for(r=0;r<2;++r) for(j=0;j<6;++j)
        if(!isfinite(coefficients[r][j]) || fabs(coefficients[r][j])>16) return -1;
    grid=(double)(1u<<precision); step=1.0/grid;
    for(j=0;j<6;++j) result.permutation[j]=j;
    for(r=0;r<2;++r) {
        double vector[6],largest=-1; unsigned eligible[6]={1,1,1,1,1,1};
        long double factor;
        int identity=1;
        for(j=0;j<6;++j) vector[j]=coefficients[r][result.permutation[j]];
        for(p=0;p<r;++p) {
            factor=(long double)vector[p]/forward[p][p];
            for(j=0;j<6;++j) if(j!=p)
                vector[j]=(double)((long double)vector[j]-factor*forward[p][j]);
            if(forward[p][p]<0) vector[p]=-vector[p];
            eligible[p]=0;
        }
        pivot=0;
        for(j=0;j<6;++j) if(eligible[j] && fabs(vector[j])>largest) {
            largest=fabs(vector[j]); pivot=j;
        }
        factor=vector[pivot] ? -1.0L/vector[pivot] : 1;
        for(j=0;j<6;++j) forward[r][j]=eligible[j] ? (double)(factor*vector[j]) : 0;
        forward[r][pivot]=-1;
        for(j=0;j<2;++j) inverse[r][j]=eligible[j] ? 0 : vector[j];
        inverse[r][r]=-vector[pivot];
        if(pivot!=r) {
            unsigned map=result.permutation[r];
            result.permutation[r]=result.permutation[pivot]; result.permutation[pivot]=map;
            for(p=0;p<=r;++p) {
                double swap=forward[p][r]; forward[p][r]=forward[p][pivot]; forward[p][pivot]=swap;
            }
        }
        for(j=0;j<6;++j) {
            forward[r][j]=floor((double)((long double)grid*forward[r][j]+0.5L))*step;
            if(j!=r) {
                if(forward[r][j]<=-1) forward[r][j]=step-1;
                if(forward[r][j]>=1) forward[r][j]=1-step;
            }
            if(forward[r][j]!=(j==r ? -1 : 0)) identity=0;
        }
        if(identity) { forward[r][r]=1; inverse[r][r]=-inverse[r][r]; }
    }
    basis[0][0]=basis[1][1]=1;
    for(r=0;r<2;++r) {
        double vector[6]; long double sum=0;
        for(j=0;j<6;++j) {
            vector[j]=(double)((long double)basis[0][j]*forward[r][0]+(long double)basis[1][j]*forward[r][1]);
            if(j>=2) vector[j]+=forward[r][j];
            sum+=fabs(vector[j]);
        }
        memcpy(basis[r],vector,sizeof(vector));
        while(sum>(long double)0.99) { sum*=0.5L; ++result.required_headroom[r]; }
        if(maximum<result.required_headroom[r]) maximum=result.required_headroom[r];
    }
    for(r=0;r<2;++r) if(maximum && result.required_headroom[r]<maximum-1)
        result.required_headroom[r]=maximum-1;
    for(r=2;r>0;--r) {
        unsigned target=r-1; double vector[6];
        long double sum=0,largest=fmax(fabs(inverse[target][0]),fabs(inverse[target][1]));
        double divisor;
        for(j=0;j<6;++j) {
            vector[j]=(double)((long double)basis[0][j]*inverse[target][0]+(long double)basis[1][j]*inverse[target][1]);
            sum+=fabs(vector[j]);
        }
        memcpy(basis[target],vector,sizeof(vector));
        while(sum>(long double)0.99 || largest>(long double)1.9) {
            sum*=0.5L; largest*=0.5L; ++result.post_shift[target];
        }
        if(result.post_shift[target]>15) return -1;
        divisor=(double)(1u<<result.post_shift[target]);
        inverse[target][0]/=divisor; inverse[target][1]/=divisor;
        for(p=0;p<target;++p) inverse[p][target]*=divisor;
    }
    for(r=0;r<2;++r) {
        result.forward[r].target=r; result.inverse[1-r].target=r;
        for(j=0;j<6;++j) result.forward[r].coefficient[j]=(int32_t)(forward[r][j]*16384.0);
        for(j=0;j<2;++j) {
            double value=floor((double)((long double)grid*inverse[r][j]+0.5L))*(1u<<(14-precision));
            if(value<INT32_MIN || value>INT32_MAX) return -1;
            result.inverse[1-r].coefficient[j]=(int32_t)value;
        }
    }
    *output=result; return 0;
}

int mlp_matrix_render_stereo(const mlp_matrix_primitive *matrix,
    unsigned primitives, const unsigned qss[2], const int32_t *samples,
    size_t count, unsigned stride, const int32_t *extra0,
    const int32_t *extra1, float *stereo)
{
    float work[320], noise[2][160];
    unsigned p,ch; size_t n; int overflow=0;
    if (primitives>6 || (primitives && !matrix) || !qss ||
        qss[0]>15 || qss[1]>15 || count>160 || stride<2 || stride>6 ||
        (count && (!samples || !stereo || !extra0 || !extra1))) return -1;
    for (p=0;p<primitives;++p) {
        if (matrix[p].target>1 || matrix[p].bypass) return -1;
        for (ch=4;ch<18;++ch) if (matrix[p].coefficient[ch]) return -1;
    }
    for (n=0;n<count;++n) {
        for (ch=0;ch<2;++ch) {
            int32_t v=samples[n*stride+ch];
            if (v < -8388608 || v > 8388607) return -1;
            work[n*2+ch]=(float)v;
        }
        noise[0][n]=(float)extra0[n]; noise[1][n]=(float)extra1[n];
    }
    for (p=primitives;p>0;--p) {
        const mlp_matrix_primitive *m=matrix+p-1;
        double coefficient[4], step=(double)(1u<<qss[m->target]);
        for (ch=0;ch<4;++ch) coefficient[ch]=m->coefficient[ch]/(16384.0*step);
        for (n=0;n<count;++n) {
            long double sum=(long double)noise[1][n]*coefficient[3];
            volatile double rounded;
            long double value;
            sum+=(long double)work[n*2+1]*coefficient[1];
            sum+=(long double)work[n*2]*coefficient[0];
            sum+=(long double)noise[0][n]*coefficient[2];
            rounded=(double)sum; value=(long double)floor(rounded)*step;
            if (value < -8388608.0L || value >= 8388608.0L) overflow=1;
            work[n*2+m->target]=(float)value;
        }
    }
    if (count) memcpy(stereo,work,count*2*sizeof(*stereo));
    return overflow;
}

static int32_t sar32(uint32_t value, unsigned shift)
{
    uint32_t result;
    shift &= 31;
    result = value >> shift;
    if (shift && (value & UINT32_C(0x80000000))) result |= UINT32_MAX << (32-shift);
    return result <= INT32_MAX ? (int32_t)result : (int32_t)((int64_t)result-INT64_C(4294967296));
}

int mlp_matrix_downmix_pcm(mlp_downmix_check *state,
    const float *samples, size_t count, unsigned channels,
    const unsigned *output_shift, int32_t *clipped)
{
    mlp_downmix_check next;
    int32_t output[960],minimum=0,maximum=0;
    unsigned ch,bits; size_t n;
    if (!state || state->maximum_bits>32 || !channels || channels>6 ||
        count>160 || (count && !samples) || !output_shift) return -1;
    next=*state;
    for(ch=0;ch<channels;++ch) {
        uint32_t checksum=0;
        if(output_shift[ch]>31) return -1;
        for(n=0;n<count;++n) {
            double sample=samples[n*channels+ch]; uint32_t shifted; int32_t value;
            if(!isfinite(sample) || sample<INT32_MIN || sample>INT32_MAX) return -1;
            shifted=(uint32_t)(int32_t)sample<<output_shift[ch];
            if(shifted==UINT32_C(0x80000000)) return -1;
            value=sar32(shifted,0); checksum^=shifted;
            if(value>maximum) maximum=value;
            if(value<minimum) minimum=value;
            output[n*channels+ch]=value < -8388608 ? -8388608 : value > 8388607 ? 8388607 : value;
        }
        next.checksum^=(checksum&0xffffffu)<<ch;
    }
    if(maximum < -minimum) maximum=-minimum;
    bits=next.maximum_bits ? next.maximum_bits-1 : 0;
    while(((uint32_t)maximum>>bits)!=0) ++bits;
    next.maximum_bits=bits+1;
    if(clipped && count) memcpy(clipped,output,count*channels*sizeof(*clipped));
    *state=next;
    return minimum < -8388608 || maximum>=8388608;
}

int mlp_matrix_analysis_init(mlp_matrix_analysis *state, unsigned channels)
{
    unsigned i;
    if (!state || !channels || channels > 6) return -1;
    memset(state,0,sizeof(*state)); state->channels = channels;
    for (i = 0; i < channels; ++i) state->energy[i] = 1;
    return 0;
}
int mlp_matrix_analysis_reset(mlp_matrix_analysis *state)
{
    unsigned i;
    if (!state || !state->channels || state->channels > 6) return -1;
    state->samples = 0; memset(state->covariance,0,sizeof(state->covariance));
    for (i = 0; i < state->channels; ++i) state->energy[i] = 1;
    return 0;
}
int mlp_matrix_analysis_add(mlp_matrix_analysis *state,
    const int32_t *samples, size_t count, unsigned stride)
{
    mlp_matrix_analysis work;
    double difference[160*6];
    unsigned ch,j; size_t n;
    if (!state || !samples || !state->channels || state->channels > 6 ||
        !count || count > 160 || stride < state->channels || stride > 6 ||
        state->samples > SIZE_MAX-count) return -1;
    work = *state;
    for (ch = 0; ch < work.channels; ++ch) {
        double energy = 0, last = work.last[ch], delta = work.difference[ch];
        if (!isfinite(last) || !isfinite(delta) || !isfinite(work.energy[ch])) return -1;
        for (n = 0; n < count; ++n) {
            int32_t v = samples[n*stride+ch]; double next;
            if (v < -8388608 || v > 8388607) return -1;
            next = v-last; difference[n*6+ch] = next-delta;
            last = v; delta = next; energy = (double)((long double)v*v+energy);
        }
        work.last[ch] = last; work.difference[ch] = delta;
        work.energy[ch] = energy+work.energy[ch];
    }
    for (ch = 0; ch < work.channels; ++ch) for (j = ch; j < work.channels; ++j) {
        long double dot = 0;
        for (n = 0; n < count; ++n) dot += (long double)difference[n*6+ch]*difference[n*6+j];
        work.covariance[ch*6+j] = work.covariance[j*6+ch] = (double)(dot+work.covariance[ch*6+j]);
        if (!isfinite(work.covariance[ch*6+j])) return -1;
    }
    work.samples += count; *state = work; return 0;
}
int mlp_matrix_decorrelate(const double covariance[36],
    const double ratio[6], unsigned channels, unsigned fixed, double threshold,
    unsigned order[6], double transform[36], double reduced[36])
{
    double c[36],t[36] = {0}; unsigned sequence[6] = {0};
    unsigned pass,i,j,k;
    if (!covariance || !ratio || !order || !transform || !reduced ||
        !channels || channels > 6 || fixed > channels || !isfinite(threshold) || threshold < 0) return -1;
    memcpy(c,covariance,sizeof(c));
    for (i = 0; i < channels; ++i) {
        if (!isfinite(ratio[i]) || ratio[i] < 0 || c[i*7] < 0) return -1;
        t[i*7] = 1;
        for (j = 0; j < channels; ++j) if (!isfinite(c[i*6+j])) return -1;
    }
    for (pass = 0; pass < 2; ++pass) {
        unsigned visited[6] = {0};
        for (i = 0; i < channels; ++i) {
            unsigned pivot = i; double diagonal;
            if (!pass) {
                if (i >= fixed) {
                    double maximum = -1e30;
                    for (j = 0; j < channels; ++j) if (!visited[j] && maximum < c[j*7]) {
                        maximum = c[j*7]; pivot = j;
                    }
                }
                sequence[i] = pivot;
            } else pivot = sequence[i];
            diagonal = c[pivot*7]; visited[pivot] = 1;
            if (!(diagonal > threshold)) continue;
            for (j = fixed; j < channels; ++j) {
                long double factor, gain;
                if (visited[j] || c[pivot*6+j] == 0 ||
                    !(ratio[pivot] < (long double)ratio[j]*100) ||
                    !(ratio[j] < (long double)ratio[pivot]*1000)) continue;
                factor = -(long double)c[pivot*6+j]/diagonal;
                gain = -(factor*c[j*6+pivot]/c[j*7]);
                if (!(gain > (long double)0.1)) continue;
                /* 1000e943 spills a double without popping the extended
                 * value used by the gain gate. Updates reload that spill. */
                factor = (double)factor;
                for (k = 0; k < channels; ++k) c[k*6+j] = (double)(factor*c[k*6+pivot]+c[k*6+j]);
                c[pivot*6+j] = 0;
                for (k = 0; k < channels; ++k) c[j*6+k] = (double)(factor*c[pivot*6+k]+c[j*6+k]);
                c[j*6+pivot] = 0;
                for (k = 0; k < channels; ++k) t[j*6+k] = (double)(factor*t[pivot*6+k]+t[j*6+k]);
            }
        }
    }
    for (i = 0; i < 36; ++i) if (!isfinite(t[i]) || !isfinite(c[i])) return -1;
    memcpy(order,sequence,sizeof(sequence)); memcpy(transform,t,sizeof(t)); memcpy(reduced,c,sizeof(c));
    return 0;
}

int mlp_matrix_select_append(const double covariance[36],
    const double ratio[6], unsigned channels, unsigned fixed, size_t samples,
    const unsigned qss[6], const unsigned scale_count[6], int allow_bypass,
    mlp_matrix_primitive matrix[6], unsigned *primitives, unsigned *bypass_bits)
{
    mlp_matrix_primitive work[6] = {{0}};
    double transform[36],reduced[36]; unsigned order[6];
    unsigned count,bits=0,i,j;
    if (!qss || !scale_count || !matrix || !primitives || !bypass_bits ||
        !samples || samples > INT32_MAX || !channels || channels > 6 ||
        (allow_bypass != 0 && allow_bypass != 1)) return -1;
    count=*primitives;
    if(count>6) return -1;
    for(i=0;i<count;++i) {
        const mlp_matrix_primitive *p=matrix+i;
        if(p->target>=channels || p->bypass>1 ||
            p->coefficient[p->target]!=(p->bypass ? -32768 : -16384) ||
            (p->bypass && qss[p->target])) return -1;
        for(j=8;j<18;++j) if(p->coefficient[j]) return -1;
        bits+=p->bypass; work[i]=*p;
    }
    if(bits!=*bypass_bits) return -1;
    for (i = 0; i < channels; ++i) if (qss[i] > 15 || scale_count[i] > 8) return -1;
    if (mlp_matrix_decorrelate(covariance,ratio,channels,fixed,10,order,transform,reduced)) return -1;
    for (i = channels; i; --i) {
        unsigned target = order[i-1], bypass, useful; long double gain,relative;
        mlp_matrix_primitive p = {0};
        if(count==6) break;
        if (target < fixed || !(covariance[target*7] > 10)) continue;
        relative = (long double)reduced[target*7]/covariance[target*7]+(long double)1e-10;
        if (!(relative > 0)) return -1;
        gain = -logl(relative)/logl(4);
        if (!isfinite(gain)) return -1;
        bypass = allow_bypass && !qss[target] && scale_count[target] && gain < 2;
        useful = (gain-(long double)0.05)*samples > (bypass ? 50 : 100);
        if (useful) {
            unsigned precision = gain >= 9 ? 12 : (unsigned)gain+3;
            for (j = 0; j < channels; ++j) {
                /* The imported floor receives a double argument. */
                double rounded = floor((double)(0.5L-ldexpl(1,precision)*transform[target*6+j]));
                double coefficient = rounded*(1u<<(14-precision));
                if (coefficient < -32768 || coefficient > 32767) useful = 0;
                else p.coefficient[j] = (int32_t)coefficient;
            }
        }
        if (!useful) {
            if (!bypass) continue;
            memset(&p,0,sizeof(p));
        }
        p.target = target; p.bypass = bypass;
        p.coefficient[target] = bypass ? -32768 : -16384;
        work[count++] = p; bits += bypass;
    }
    memcpy(matrix,work,sizeof(work)); *primitives = count; *bypass_bits = bits;
    return 0;
}

int mlp_matrix_select(const double covariance[36],
    const double ratio[6], unsigned channels, unsigned fixed, size_t samples,
    const unsigned qss[6], const unsigned scale_count[6], int allow_bypass,
    mlp_matrix_primitive matrix[6], unsigned *primitives, unsigned *bypass_bits)
{
    mlp_matrix_primitive work[6]={{0}}; unsigned count=0,bits=0;
    if(!matrix || !primitives || !bypass_bits ||
        mlp_matrix_select_append(covariance,ratio,channels,fixed,samples,qss,
            scale_count,allow_bypass,work,&count,&bits)) return -1;
    memcpy(matrix,work,sizeof(work)); *primitives=count; *bypass_bits=bits;
    return 0;
}

int mlp_matrix_add_dither(mlp_matrix_primitive *matrix,
    unsigned primitives, unsigned targets, unsigned extra_source,
    const unsigned dimensions[6], unsigned *noise_shift)
{
    mlp_matrix_primitive work[16];
    unsigned i, k, common, shift;
    if (!matrix || !dimensions || !noise_shift || primitives > 15 ||
        !targets || targets > 6 || extra_source < targets || extra_source > 6) return -1;
    for (i = 0; i < targets; ++i) if (dimensions[i] > 31) return -1;
    common = dimensions[0];
    if (targets > 1 && dimensions[1] > common) common = dimensions[1];
    shift = common > 8 ? common-8 : 0;
    memcpy(work,matrix,(primitives+1)*sizeof(*work));
    for (i = 0; i < targets; ++i) {
        uint32_t amplitude = UINT32_C(1) << ((dimensions[i]-shift+6)&31);
        for (k = 0; k < primitives && work[k].target != i; ++k) {}
        work[k].coefficient[extra_source] = sar32(i ? 0-amplitude : amplitude,0);
        work[k].coefficient[extra_source+1] = sar32(amplitude,0);
    }
    memcpy(matrix,work,(primitives+1)*sizeof(*work)); *noise_shift = shift;
    return 0;
}
int mlp_matrix_noise(uint32_t *seed, size_t count, unsigned shift,
    int32_t *first, int32_t *second)
{
    uint32_t state; size_t n;
    if (!seed || count > 160 || shift > 31 || (count && (!first || !second))) return -1;
    state = *seed;
    for (n = 0; n < count; ++n) {
        uint32_t t = (uint32_t)sar32(state,7);
        int a = (int)((t>>8)&255), b = (int)(t&255);
        if (a >= 128) a -= 256;
        if (b >= 128) b -= 256;
        state = ((((state&127)<<11)^t)<<5)^t;
        first[n] = sar32((uint32_t)a << shift,0);
        second[n] = sar32((uint32_t)b << shift,0);
    }
    *seed = state; return 0;
}

int mlp_matrix_build_forward(const mlp_matrix_candidate *candidate,
    unsigned candidates, unsigned channels, const unsigned scale_count[6],
    const unsigned shifts[6], mlp_matrix_primitive output[15],
    unsigned *primitives, unsigned *bypass_bits, int signs[6], unsigned final_shifts[6])
{
    mlp_matrix_primitive work[15] = {{0}};
    unsigned counts[6], dims[6], count = 0, bits = 0, i, j;
    int polarity[6] = {1,1,1,1,1,1};
    if (!candidate || !scale_count || !shifts || !output || !primitives ||
        !bypass_bits || !signs || !final_shifts || !channels || channels > 6 ||
        candidates > channels) return -1;
    memcpy(counts,scale_count,sizeof(counts)); memcpy(dims,shifts,sizeof(dims));
    for (i = 0; i < channels; ++i)
        if (counts[i] > 8 || dims[i] > 31) return -1;
    for (i = 0; i < candidates; ++i)
        if (candidate[i].target >= channels) return -1;
    /* Original preparatory loop is candidates, not channels. */
    for (i = 0; i < candidates; ++i) {
        while (counts[i] > 1) {
            if (count == 15 || bits == 8 || dims[i] == 31) return -1;
            work[count].target = i; work[count].bypass = 1;
            work[count++].coefficient[i] = -32768;
            polarity[i] = -polarity[i]; ++dims[i]; --counts[i]; ++bits;
        }
    }
    for (i = 0; i < candidates; ++i) {
        unsigned target = candidate[i].target, bypass = counts[target] > 0;
        uint32_t bias;
        if (candidate[i].coefficient[target] == 16384 && !bypass) continue;
        if (count == 15 || (bypass && (bits == 8 || dims[target] == 31))) return -1;
        work[count].target = target; work[count].bypass = bypass;
        bias = (uint32_t)sar32(UINT32_C(1) << dims[target],1);
        for (j = 0; j < channels; ++j) {
            uint32_t v = (uint32_t)candidate[i].coefficient[j];
            v *= (uint32_t)polarity[j]; v *= (uint32_t)polarity[target];
            v <<= dims[j]; v += bias;
            work[count].coefficient[j] = sar32(v,dims[target]);
        }
        if (work[count].coefficient[target] == 16384) polarity[target] = -polarity[target];
        work[count].coefficient[target] = bypass ? -32768 : -16384;
        if (bypass) { ++dims[target]; --counts[target]; ++bits; }
        ++count;
    }
    memcpy(output,work,sizeof(work)); memcpy(signs,polarity,sizeof(polarity));
    memcpy(final_shifts,dims,sizeof(dims)); *primitives = count; *bypass_bits = bits;
    return 0;
}

int mlp_matrix_build_reverse(const mlp_matrix_candidate *candidate,
    unsigned channels, const int signs[6], const unsigned shifts[6],
    mlp_matrix_primitive output[6], unsigned final_shifts[6])
{
    mlp_matrix_primitive work[6] = {{0}};
    unsigned dims[6], common, i, j;
    int polarity[6];
    if (!candidate || !signs || !shifts || !output || !final_shifts ||
        !channels || channels > 6) return -1;
    memcpy(dims,shifts,sizeof(dims)); memcpy(polarity,signs,sizeof(polarity));
    common = shifts[0] > shifts[1] ? shifts[0] : shifts[1];
    if (common > 31) return -1;
    for (i = 0; i < channels; ++i)
        if (dims[i] > 31 || (polarity[i] != 1 && polarity[i] != -1) ||
            candidate[i].target >= channels) return -1;
    for (i = 0; i < channels; ++i) {
        unsigned target = candidate[i].target;
        mlp_matrix_primitive *p = &work[channels-1-i];
        p->target = target;
        for (j = 0; j < channels; ++j) {
            uint32_t v = (uint32_t)candidate[i].coefficient[j]*(uint32_t)polarity[j];
            if (common > dims[j]) {
                v = (uint32_t)sar32(v,common-dims[j]-1)+1;
                v = (uint32_t)sar32(v,1);
            }
            p->coefficient[j] = sar32(v,0);
        }
        polarity[target] = 1; dims[target] = common;
    }
    memcpy(output,work,sizeof(work)); memcpy(final_shifts,dims,sizeof(dims));
    return 0;
}

int mlp_matrix_downmix_plan(const mlp_matrix_candidate forward[2],
    const mlp_matrix_candidate inverse[2], unsigned candidates, unsigned channels,
    const unsigned scale_count[6], const unsigned shifts[6],
    const unsigned qss[6], const unsigned post_shift[6], mlp_downmix_plan *plan)
{
    mlp_downmix_plan work = {0};
    unsigned dimensions[6], final[6], noise[6] = {0}, i;
    int signs[6];
    if (!forward || !inverse || !scale_count || !shifts || !qss || !post_shift || !plan ||
        !channels || channels > 6 || candidates > 2 || candidates > channels) return -1;
    for (i = 0; i < channels; ++i)
        if (shifts[i] > 15 || qss[i] > 15 || post_shift[i] > 15 || scale_count[i] > 6) return -1;
    for (i = 0; i < candidates; ++i)
        if (forward[i].target >= candidates || inverse[i].target >= candidates) return -1;
    if (mlp_matrix_build_forward(forward,candidates,channels,scale_count,shifts,
        work.forward,&work.forward_count,&work.bypass_bits,signs,dimensions)) return -1;
    if (work.forward_count > 6) return -1;
    memcpy(work.remaining_scale,scale_count,sizeof(work.remaining_scale));
    for (i = 0; i < work.forward_count; ++i)
        if (work.forward[i].bypass) --work.remaining_scale[work.forward[i].target];
    for (i = 0; i < channels; ++i) {
        noise[i] = qss[i]+dimensions[i]-shifts[i];
        if (noise[i] > 31) return -1;
    }
    work.forward_noise_shift = noise[0] > 8 ? noise[0]-8 : 0;
    work.inverse_noise_shift = qss[0] > 8 ? qss[0]-8 : 0;
    work.maximum_shift = -8;
    if (candidates) {
        if (mlp_matrix_add_dither(work.forward,work.forward_count,candidates,channels,
            noise,&work.forward_noise_shift)) return -1;
        if (mlp_matrix_build_reverse(inverse,candidates,signs,dimensions,work.inverse,final)) return -1;
        work.inverse_count = candidates;
        if (mlp_matrix_add_dither(work.inverse,candidates,candidates,candidates,qss,
            &work.inverse_noise_shift)) return -1;
        for (i = 0; i < candidates; ++i) {
            work.output_shift[i] = final[i]+post_shift[i];
            if ((int)work.output_shift[i] > work.maximum_shift) work.maximum_shift = (int)work.output_shift[i];
        }
    }
    *plan = work; return 0;
}

int mlp_matrix_process_downmix(mlp_downmix_state *state,
    const mlp_downmix_plan *plan, unsigned channels, const unsigned shifts[6],
    const unsigned qss[6], const int32_t *samples, size_t count,
    unsigned flags, mlp_downmix_block *output)
{
    mlp_downmix_state next; mlp_downmix_block work={0};
    int32_t noise[2][160]; unsigned p,ch,bits=0; size_t n; int rc;
    if(!state || !plan || !shifts || !qss || !samples || !output ||
        channels<2 || channels>6 || !count || count>160 ||
        plan->forward_count>6 || plan->inverse_count!=2 ||
        plan->forward_noise_shift>16 || plan->inverse_noise_shift>16 ||
        state->check.maximum_bits>32) return -1;
    for(ch=0;ch<channels;++ch) if(shifts[ch]>15 || qss[ch]>15) return -1;
    for(p=0;p<plan->forward_count;++p) {
        const mlp_matrix_primitive *m=plan->forward+p;
        if(m->target>=channels || m->bypass>1 ||
            m->coefficient[m->target]!=(m->bypass ? -32768 : -16384) ||
            (m->bypass && qss[m->target])) return -1;
        for(ch=0;ch<18;++ch) {
            if(m->coefficient[ch] < -32768 || m->coefficient[ch]>32767) return -1;
            if(ch>=channels+2 && m->coefficient[ch]) return -1;
        }
        bits+=m->bypass;
    }
    if(bits!=plan->bypass_bits) return -1;
    for(p=0;p<2;++p) for(ch=0;ch<18;++ch)
        if(plan->inverse[p].coefficient[ch] < -32768 || plan->inverse[p].coefficient[ch]>32767) return -1;
    for(n=0;n<count;++n) for(ch=0;ch<channels;++ch) {
        int32_t v=samples[n*channels+ch];
        if(v < -8388608 || v>8388607) return -1;
        work.transformed[n*channels+ch]=sar32((uint32_t)v,shifts[ch]);
    }
    next=*state; work.flags=flags;
    work.forward_seed=next.forward_seed; work.inverse_seed=next.inverse_seed;
    if(mlp_matrix_noise(&next.forward_seed,count,plan->forward_noise_shift,noise[0],noise[1])) return -1;
    for(p=0;p<plan->forward_count;++p) {
        uint8_t one[160]={0};
        rc=mlp_matrix_apply(plan->forward+p,1,channels,qss,work.transformed,count,noise[0],noise[1],one,0,1);
        if(rc<0) return -1;
        if(plan->forward[p].bypass) for(n=0;n<count;++n)
            work.bypass[n]=(uint8_t)((one[n]<<7)|(work.bypass[n]>>1));
        if(rc) { work.flags|=0x200; break; }
    }
    if(mlp_matrix_noise(&next.inverse_seed,count,plan->inverse_noise_shift,noise[0],noise[1])) return -1;
    rc=mlp_matrix_render_stereo(plan->inverse,2,qss,work.transformed,count,channels,noise[0],noise[1],work.stereo);
    if(rc<0) return -1;
    if(rc) work.flags|=0x200;
    if(flags&2) memset(&next.check,0,sizeof(next.check));
    rc=mlp_matrix_downmix_pcm(&next.check,work.stereo,count,2,plan->output_shift,NULL);
    if(rc<0) return -1;
    if(rc) work.flags|=0x400;
    *state=next; *output=work;
    return 0;
}

int mlp_matrix_apply(const mlp_matrix_primitive *matrix, unsigned primitives,
    unsigned channels, const unsigned *qss, int32_t *samples, size_t count,
    const int32_t *extra0, const int32_t *extra1, uint8_t *bypass,
    int inverse, int clip)
{
    int32_t work[160*6];
    uint8_t bits[160] = {0};
    unsigned i, j, bypass_count = 0;
    unsigned remaining;
    size_t n;
    int overflow = 0;
    if (!matrix || !qss || !samples || !channels || channels > 6 ||
        !count || count > 160 || primitives > 15 ||
        (inverse != 0 && inverse != 1) || (clip != 0 && clip != 1)) return -1;
    for (j = 0; j < channels; ++j) if (qss[j] > 15) return -1;
    for (i = 0; i < primitives; ++i) {
        const mlp_matrix_primitive *p = &matrix[i];
        if (p->target >= channels || p->bypass > 1 ||
            p->coefficient[p->target] != (p->bypass ? -32768 : -16384) ||
            (p->bypass && qss[p->target])) return -1;
        bypass_count += p->bypass;
        if (bypass_count > 8 || (p->bypass && !bypass) ||
            (p->coefficient[channels] && !extra0) ||
            (p->coefficient[channels+1] && !extra1)) return -1;
    }
    for (n = 0; n < count*channels; ++n)
        if (samples[n] < -8388608 || samples[n] > 8388607) return -1;
    memcpy(work,samples,count*channels*sizeof(*work));
    if (bypass) memcpy(bits,bypass,count);
    remaining = bypass_count;
    for (i = 0; i < primitives; ++i) {
        const mlp_matrix_primitive *p = &matrix[inverse ? primitives-1-i : i];
        double step = ldexp(1.0,(int)qss[p->target]);
        for (n = 0; n < count; ++n) {
            double sum = 0, value;
            int64_t rounded;
            for (j = 0; j < channels; ++j)
                sum += work[n*channels+j]*(p->coefficient[j]/16384.0);
            if (extra0) sum += extra0[n]*(p->coefficient[channels]/16384.0);
            if (extra1) sum += extra1[n]*(p->coefficient[channels+1]/16384.0);
            if (p->bypass) {
                if (inverse) {
                    value = floor(sum)+((bits[n]>>(remaining-1))&1);
                    bits[n] &= (uint8_t)((1u<<(remaining-1))-1);
                } else {
                    /* Original floor(sum+target), (integer+1) SAR 1.
                     * Store parity at high bit, shift previous bits right. */
                    value = floor(sum+work[n*channels+p->target]);
                    if (!isfinite(value) || value < INT32_MIN || value > INT32_MAX-1.0) return -1;
                    rounded = (int64_t)value;
                    bits[n] = (uint8_t)(((uint32_t)rounded&1)*128+(bits[n]>>1));
                    value = floor((value+1)/2);
                }
            } else value = floor(sum/step)*step;
            if (!isfinite(value) || value < INT32_MIN || value > INT32_MAX) return -1;
            if (value < -8388608 || value > 8388607) {
                overflow = 1;
                if (clip) value = value < 0 ? -8388608 : 8388607;
            }
            work[n*channels+p->target] = (int32_t)value;
        }
        if (inverse && p->bypass) --remaining;
    }
    /* Forward packing places newest bit at MSB. Normalize the active bits
    * to low bits for the independent inverse API; newest is highest active
    * bit, wire packing remains a separate serialization operation. */
    if (!inverse && bypass_count)
        for (n = 0; n < count; ++n) bits[n] >>= 8-bypass_count;
    memcpy(samples,work,count*channels*sizeof(*work));
    if (bypass) memcpy(bypass,bits,count);
    return overflow;
}

static int apply_interval(const mlp_matrix_primitive *matrix,
    unsigned *primitives, unsigned channels, const unsigned qss[6],
    int32_t *samples, size_t count, uint8_t *bypass, unsigned *bypass_bits,
    const unsigned *lengths, size_t blocks, unsigned prefix)
{
    int32_t *work = NULL,*saved = NULL; uint8_t *bits = NULL;
    unsigned i,j,used = 0; size_t n,first,b; int rc = -1;
    if (!matrix || !primitives || !qss || !samples || !bypass || !bypass_bits ||
        !channels || channels > 6 || *primitives > 6 || prefix>*primitives || !count ||
        count > SIZE_MAX/sizeof(int32_t)/channels) return -1;
    for (b = 0; b < (lengths ? blocks : 1); ++b)
        for (j = 0; j < channels; ++j) if (qss[b*6+j] > 15) return -1;
    for (i = 0; i < *primitives; ++i) {
        const mlp_matrix_primitive *p = &matrix[i];
        if (p->target >= channels || p->bypass > 1 ||
            p->coefficient[p->target] != (p->bypass ? -32768 : -16384) ||
            (i>=prefix && (p->coefficient[channels] || p->coefficient[channels+1]))) return -1;
        for (b = 0; b < (lengths ? blocks : 1); ++b)
            if (p->bypass && qss[b*6+p->target]) return -1;
        if(i<prefix) used+=p->bypass;
    }
    for (n = 0; n < count*channels; ++n)
        if (samples[n] < -8388608 || samples[n] > 8388607) return -1;
    if(prefix) for(n=0;n<count;++n) if(bypass[n]>=(1u<<used)) return -1;
    work = malloc(count*channels*sizeof(*work)); saved = malloc(count*sizeof(*saved));
    bits = calloc(count,1);
    if (!work || !saved || !bits) goto done;
    memcpy(work,samples,count*channels*sizeof(*work));
    if(prefix) memcpy(bits,bypass,count);
    for (i = prefix; i < *primitives; ++i) {
        unsigned target = matrix[i].target; int overflow = 0;
        for (n = 0; n < count; ++n) saved[n] = work[n*channels+target];
        for (first = 0,b = 0; b < blocks; ++b) {
            uint8_t one[160] = {0};
            size_t block = lengths ? lengths[b] : (count-first < 160 ? count-first : 160);
            const unsigned *current_qss = qss+(lengths ? b*6 : 0);
            int result = mlp_matrix_apply(matrix+i,1,channels,current_qss,work+first*channels,block,NULL,NULL,one,0,0);
            if (result < 0) goto done;
            if (result) { overflow = 1; break; }
            if (matrix[i].bypass)
                for (n = 0; n < block; ++n) bits[first+n] |= (uint8_t)(one[n]<<used);
            first += block;
        }
        if (overflow) {
            for (n = 0; n < count; ++n) {
                work[n*channels+target] = saved[n];
                bits[n] &= (uint8_t)((1u<<used)-1);
            }
            break;
        }
        used += matrix[i].bypass;
    }
    rc = i < *primitives; *primitives = i; *bypass_bits = used;
    memcpy(samples,work,count*channels*sizeof(*work)); memcpy(bypass,bits,count);
done:
    free(work); free(saved); free(bits); return rc;
}
int mlp_matrix_apply_interval(const mlp_matrix_primitive *matrix,
    unsigned *primitives, unsigned channels, const unsigned qss[6],
    int32_t *samples, size_t count, uint8_t *bypass, unsigned *bypass_bits)
{
    return apply_interval(matrix,primitives,channels,qss,samples,count,bypass,bypass_bits,
        NULL,count/160+(count%160 != 0),0);
}
int mlp_matrix_apply_blocks(const mlp_matrix_primitive *matrix,
    unsigned *primitives, unsigned channels, const unsigned qss[][6],
    const unsigned *lengths, unsigned blocks, int32_t *samples,
    uint8_t *bypass, unsigned *bypass_bits)
{
    return mlp_matrix_apply_suffix_blocks(matrix,0,primitives,channels,qss,
        lengths,blocks,samples,bypass,bypass_bits);
}
int mlp_matrix_apply_suffix_blocks(const mlp_matrix_primitive *matrix,
    unsigned prefix, unsigned *primitives, unsigned channels,
    const unsigned qss[][6], const unsigned *lengths, unsigned blocks,
    int32_t *samples, uint8_t *bypass, unsigned *bypass_bits)
{
    size_t count = 0; unsigned b;
    if (!qss || !lengths || !blocks || blocks > 128) return -1;
    for (b = 0; b < blocks; ++b) {
        if (!lengths[b] || lengths[b] > 160) return -1;
        count += lengths[b];
    }
    return apply_interval(matrix,primitives,channels,&qss[0][0],samples,count,bypass,bypass_bits,lengths,blocks,prefix);
}
