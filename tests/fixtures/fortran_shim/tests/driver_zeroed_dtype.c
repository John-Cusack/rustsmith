#include <stdio.h>
#include <string.h>

/* NEGATIVE fixture: zeroed-dtype descriptor guess into an assumed-shape
 * dummy (ADR-027 Problem). MUST fail fast (signal/nonzero exit), never
 * go green: a guessed layout is silently-wrong-science or a crash, and
 * the shim pins the measured layout instead of guessing. */
double __shim_mod_MOD_assumed_sum(void *desc);

int main(void) {
  /* 40 + 24*2 bytes: the rank-2 descriptor size from ADR-027 §2. */
  char desc[88];
  memset(desc, 0, sizeof desc);
  printf("%f\n", __shim_mod_MOD_assumed_sum(desc));
  return 0;
}
