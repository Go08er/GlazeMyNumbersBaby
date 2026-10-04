/* Compiled into the x86-64-v3 copy only: it must really have FMA and AVX2
   (a packager's CFLAGS mustn't have downgraded it). */
#if !defined(__FMA__) || !defined(__AVX2__) || !defined(__BMI2__)
#error "crmath: the x86-64-v3 copy was compiled without FMA/AVX2/BMI2"
#endif
int crmath_guard_v3;
