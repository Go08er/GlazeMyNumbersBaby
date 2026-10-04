/* Compiled into the baseline copy only: it must run on any x86-64 CPU. */
#if defined(__AVX__) || defined(__FMA__)
#error "crmath: the baseline copy was compiled with AVX or FMA"
#endif
int crmath_guard_baseline;
